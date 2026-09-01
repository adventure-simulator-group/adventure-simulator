//! Accelerated, deterministic headless melee combat iteration runner.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use adventuresim_core::{
    autoresolve::{
        BattleOpening, BattleResolution, MeleeIterationBuild, MeleeTimelineKind,
        melee_iteration_acceptance_evidence, melee_iteration_roster, resolve_battle,
    },
    combat::{MeleeContactClassification, MeleeContactInvalidationCause},
    item_catalog_schema::EquipmentMaterial,
    skill::{PlayerSkills, Skill},
};
use adventuresim_tactical_server::iteration::{
    TacticalDecision, TacticalDecisionStatus, TacticalDuelResolution, TacticalMeleeOutcome,
    resolve_tactical_server_melee_duel,
};
use clap::Parser;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(name = "melee-combat-iteration")]
#[command(about = "Run deterministic tactical-time and autoresolve melee duel batches")]
struct Args {
    #[arg(long, default_value = "target/melee-combat-iteration")]
    output: PathBuf,
    #[arg(long, default_value_t = 32)]
    tactical_seeds: u64,
    #[arg(long, default_value_t = 1_000)]
    autoresolve_seeds: u64,
    #[arg(long, default_value_t = 1)]
    first_seed: u64,
}

#[derive(Serialize)]
struct MatchupSummary {
    matchup: String,
    opponent: String,
    tactical_runs: u64,
    tactical_john_wins: u64,
    tactical_opponent_wins: u64,
    tactical_mutual_incapacitations: u64,
    tactical_timeouts: u64,
    tactical_causal: TacticalCausalSummary,
    tactical_total_simulated_seconds: f64,
    tactical_mean_simulated_seconds: f64,
    autoresolve_runs: u64,
    autoresolve_john_wins: u64,
    autoresolve_opponent_wins: u64,
    autoresolve_mutual_incapacitations: u64,
    autoresolve_timeouts: u64,
    autoresolve_causal: AutoresolveCausalSummary,
    john_win_rate_difference: f64,
    tactical_wall_milliseconds: f64,
    autoresolve_wall_milliseconds: f64,
    tactical_duels_per_second: f64,
    tactical_simulated_seconds_per_wall_second: f64,
    autoresolve_duels_per_second: f64,
}

#[derive(Default, Serialize)]
struct TacticalCausalSummary {
    john_attack_starts: u64,
    opponent_attack_starts: u64,
    john_resolved_attacks: u64,
    opponent_resolved_attacks: u64,
    john_first_contacts: u64,
    opponent_first_contacts: u64,
    john_canceled_for_defense: u64,
    opponent_canceled_for_defense: u64,
    john_transformed_by_defense: u64,
    opponent_transformed_by_defense: u64,
    dodge_attempts: u64,
    dodge_avoids: u64,
    dodge_contacts: u64,
    dodge_redirections: u64,
    shield_block_attempts: u64,
    shield_blocks_defended: u64,
    shield_blocks_failed: u64,
    weapon_defense_contacts: u64,
    armor_contacts: u64,
    armor_stopped: u64,
    armor_deflected: u64,
    armor_penetrated: u64,
    open_wounds: u64,
    internal_wounds: u64,
    energy_partition_failures: u64,
    minimum_contact_separation_metres: f32,
    maximum_contact_separation_metres: f32,
    maximum_oxygen_debt_joules: f32,
    maximum_local_action_fatigue: f32,
    john: TacticalCombatantCausal,
    opponent: TacticalCombatantCausal,
}

#[derive(Default, Serialize)]
struct TacticalCombatantCausal {
    attack_starts: u64,
    resolved_attacks: u64,
    contacts_dealt: u64,
    attacks_received: u64,
    block_attempts: u64,
    parry_attempts: u64,
    dodge_attempts: u64,
    effective_weapon_contacts: u64,
    shield_block_attempts: u64,
    effective_shield_blocks: u64,
    dodge_avoids: u64,
    dodge_contacts: u64,
    dodge_redirections: u64,
    committed_attacks_canceled: u64,
    committed_attacks_transformed: u64,
    armor_surface_contacts_received: u64,
    armor_gap_contacts_received: u64,
    armor_stopped: u64,
    armor_deflected: u64,
    armor_penetrated: u64,
    anatomical_subregions_received: BTreeMap<String, u64>,
    armor_layers_intersected: BTreeMap<String, u64>,
    armor_layers_missed: BTreeMap<String, u64>,
    open_wounds_received: u64,
    internal_wounds_received: u64,
    attack_interval_samples: u64,
    mean_attack_start_interval_seconds: f64,
    contact_energy_samples: u64,
    mean_contact_energy_joules: f64,
    maximum_oxygen_debt_joules: f32,
    maximum_local_action_fatigue: f32,
}

impl TacticalCausalSummary {
    fn finish_means(&mut self) {
        self.john.finish_means();
        self.opponent.finish_means();
    }
}

impl TacticalCombatantCausal {
    fn finish_means(&mut self) {
        if self.attack_interval_samples > 0 {
            self.mean_attack_start_interval_seconds /= self.attack_interval_samples as f64;
        }
        if self.contact_energy_samples > 0 {
            self.mean_contact_energy_joules /= self.contact_energy_samples as f64;
        }
    }
}

#[derive(Default, Serialize)]
struct AutoresolveCausalSummary {
    total_rounds: u64,
    melee_attacks: u64,
    hits: u64,
    blocks_or_parries: u64,
    block_attempts: u64,
    parry_attempts: u64,
    misses: u64,
    dodge_attempts: u64,
    dodge_avoids: u64,
    dodge_contacts: u64,
    shield_contacts: u64,
    weapon_defense_contacts: u64,
    armor_contacts: u64,
    armor_stopped: u64,
    armor_deflected: u64,
    armor_penetrated: u64,
    john_attacks: u64,
    opponent_attacks: u64,
    john_first_contacts: u64,
    opponent_first_contacts: u64,
    john_weapon_reach_metres: f64,
    opponent_weapon_reach_metres: f64,
    total_contact_energy_joules: f64,
    total_health_damage: f64,
    final_blood_loss_fraction: f64,
    final_wound_count: u64,
    final_open_wound_count: u64,
    final_internal_wound_count: u64,
    final_wound_flow_fraction_per_second: f64,
    final_oxygen_debt_joules: f64,
    final_local_action_fatigue: f64,
    final_acute_trauma: f64,
    final_imbalance: f64,
    john_yields: u64,
    opponent_yields: u64,
    john: AutoresolveCombatantCausal,
    opponent: AutoresolveCombatantCausal,
}

#[derive(Default, Serialize)]
struct AutoresolveCombatantCausal {
    attack_starts: u64,
    attacks: u64,
    hits_dealt: u64,
    attacks_received: u64,
    block_attempts: u64,
    parry_attempts: u64,
    dodge_attempts: u64,
    effective_weapon_contacts: u64,
    dodge_avoids: u64,
    dodge_contacts: u64,
    dodge_redirections: u64,
    committed_attacks_canceled: u64,
    committed_attacks_transformed: u64,
    movement_actions: BTreeMap<String, u64>,
    movement_segments: u64,
    movement_elapsed_seconds: f64,
    movement_absolute_displacement_metres: f64,
    maximum_movement_speed_metres_per_second: f64,
    maximum_movement_segment_seconds: f64,
    movement_displacement_limit_failures: u64,
    movement_nonzero_delta_zero_elapsed: u64,
    response_availability: BTreeMap<String, u64>,
    response_choices: BTreeMap<String, u64>,
    phase_adaptation_events: u64,
    phase_adaptation_delay_seconds: f64,
    simultaneous_contacts: u64,
    armor_surface_contacts_received: u64,
    armor_gap_contacts_received: u64,
    armor_stopped: u64,
    armor_deflected: u64,
    armor_penetrated: u64,
    anatomical_subregions_received: BTreeMap<String, u64>,
    armor_layers_intersected: BTreeMap<String, u64>,
    armor_layers_missed: BTreeMap<String, u64>,
    attack_samples: u64,
    contact_classifications: BTreeMap<String, u64>,
    contact_measure_samples: u64,
    mean_scheduled_contact_measure_metres: f64,
    mean_actual_contact_measure_metres: f64,
    minimum_actual_contact_measure_metres: f64,
    maximum_actual_contact_measure_metres: f64,
    minimum_actual_center_separation_metres: f64,
    full_energy_intended_contacts_inside_ten_centimetres: u64,
    mean_attack_interval_seconds: f64,
    mean_attack_power_multiplier: f64,
    minimum_attack_fatigue_performance: f64,
    final_pain_incapacitation: f64,
    final_blood_loss_fraction: f64,
    final_acute_trauma: f64,
    final_oxygen_debt_joules: f64,
    final_local_action_fatigue: f64,
    final_imbalance: f64,
    final_open_wounds: u64,
    final_internal_wounds: u64,
    incapacitated_runs: u64,
    yielded_runs: u64,
    terminal_causes: BTreeMap<String, u64>,
}

impl AutoresolveCombatantCausal {
    fn record_final(&mut self, outcome: &adventuresim_core::autoresolve::CombatantOutcome) {
        self.final_pain_incapacitation += f64::from(outcome.pain_incapacitation);
        self.final_blood_loss_fraction += f64::from(outcome.blood_loss_fraction);
        self.final_acute_trauma += f64::from(outcome.acute_trauma);
        self.final_oxygen_debt_joules += f64::from(outcome.oxygen_debt_joules);
        self.final_local_action_fatigue += f64::from(outcome.local_action_fatigue);
        self.final_imbalance += f64::from(outcome.imbalance);
        self.final_open_wounds += outcome.open_wound_count as u64;
        self.final_internal_wounds += outcome.internal_wound_count as u64;
        self.incapacitated_runs += u64::from(outcome.incapacitated);
        self.yielded_runs += u64::from(outcome.yielded);
        if let Some(cause) = outcome.terminal_cause {
            *self
                .terminal_causes
                .entry(format!("{cause:?}"))
                .or_default() += 1;
        }
    }

    fn finish_means(&mut self) {
        if self.attack_samples > 0 {
            let samples = self.attack_samples as f64;
            self.mean_attack_interval_seconds /= samples;
            self.mean_attack_power_multiplier /= samples;
        }
        if self.contact_measure_samples > 0 {
            let samples = self.contact_measure_samples as f64;
            self.mean_scheduled_contact_measure_metres /= samples;
            self.mean_actual_contact_measure_metres /= samples;
        }
    }
}

#[derive(Serialize)]
struct ReviewerCombatant<'a> {
    name: &'a str,
    build: &'a str,
    equipment: &'a str,
    attributes: AttributeContext,
    skills: SkillContext,
}

#[derive(Serialize)]
struct AttributeContext {
    endurance: f32,
    strength: f32,
    agility: f32,
    instinct: f32,
}

#[derive(Serialize)]
struct SkillContext {
    primary_weapon_rank: f32,
    dodge_rank: f32,
    block_rank: f32,
    will_rank: f32,
    balance_rank: f32,
}

#[derive(Serialize)]
struct ReviewerPacket<'a> {
    balance_concept: &'static str,
    scale_context: &'static str,
    matchup: String,
    combatants: [ReviewerCombatant<'a>; 2],
    tactical_trace_file: &'static str,
    all_tactical_traces_file: &'static str,
    autoresolve_trace_file: &'static str,
    all_autoresolve_traces_file: &'static str,
    aggregate_summary_file: &'static str,
    acceptance_evidence_file: &'static str,
    acceptance_audit_file: &'static str,
}

#[derive(Serialize)]
struct ReviewerIndex<'a> {
    gate: &'static str,
    tactical_seeds_per_matchup: u64,
    autoresolve_seeds_per_matchup: u64,
    summary_file: &'static str,
    acceptance_evidence_file: &'static str,
    acceptance_audit_file: &'static str,
    matchup_directories: Vec<&'a str>,
}

#[derive(Serialize)]
struct AcceptanceAudit {
    no_tactical_timeouts: bool,
    no_autoresolve_timeouts: bool,
    tactical_energy_conservation_holds: bool,
    side_swap_nonterminal_timeline_equal: bool,
    simultaneous_contacts_preserved: bool,
    canceled_attacks_emit_no_ghost_contacts: bool,
    autoresolve_movement_elapsed_matches_distance_delta: bool,
    autoresolve_movement_respects_tick_and_speed_limits: bool,
    polearm_contact_revalidation_holds: bool,
    all_weapon_swept_contact_contract_holds: bool,
    matchups: Vec<MatchupCausalAudit>,
}

#[derive(Serialize)]
struct MatchupCausalAudit {
    opponent: String,
    tactical_first_contacts: [u64; 2],
    autoresolve_first_contacts: [u64; 2],
    autoresolve_attack_starts: [u64; 2],
    autoresolve_resolved_attacks: [u64; 2],
    autoresolve_cancellations: [u64; 2],
    autoresolve_response_events: [u64; 2],
    response_event_for_every_incoming_contact: bool,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    if args.tactical_seeds == 0 || args.autoresolve_seeds == 0 {
        return Err("seed counts must both be positive".into());
    }
    fs::create_dir_all(&args.output).map_err(|error| error.to_string())?;
    let acceptance_evidence = melee_iteration_acceptance_evidence()?;
    write_json(
        &args.output.join("acceptance-evidence.json"),
        &acceptance_evidence,
    )?;
    let (john, opponents) = melee_iteration_roster()?;
    let mut summaries = Vec::new();
    for opponent in &opponents {
        summaries.push(run_matchup(&args, &john, opponent)?);
    }
    write_json(&args.output.join("summary.json"), &summaries)?;
    write_json(
        &args.output.join("acceptance-audit.json"),
        &acceptance_audit(&summaries, &acceptance_evidence),
    )?;
    write_json(
        &args.output.join("reviewer-index.json"),
        &ReviewerIndex {
            gate: "bounded causal mechanics review before scale acceptance",
            tactical_seeds_per_matchup: args.tactical_seeds,
            autoresolve_seeds_per_matchup: args.autoresolve_seeds,
            summary_file: "summary.json",
            acceptance_evidence_file: "acceptance-evidence.json",
            acceptance_audit_file: "acceptance-audit.json",
            matchup_directories: opponents.iter().map(|build| build.key).collect(),
        },
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summaries).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn acceptance_audit(
    summaries: &[MatchupSummary],
    evidence: &adventuresim_core::autoresolve::MeleeIterationAcceptanceEvidence,
) -> AcceptanceAudit {
    let polearm_contact_revalidation_holds = evidence.polearm_contact_revalidation.len() == 6
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::IntendedSurface
                && contact.contact_material == Some(EquipmentMaterial::RoughSteel)
                && contact.transformed_energy_joules == contact.incident_energy_joules
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::Haft
                && contact.contact_material == Some(EquipmentMaterial::Hardwood)
                && contact.transformed_energy_joules < contact.incident_energy_joules
                && !contact.edge_contact
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::Pommel
                && contact.contact_material == Some(EquipmentMaterial::Hardwood)
                && contact.transformed_energy_joules < contact.incident_energy_joules
                && !contact.edge_contact
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::InvalidatedMiss
                && contact.transformed_energy_joules == 0.0
                && contact.invalidation_cause == Some(MeleeContactInvalidationCause::OutsideReach)
        });
    let matchups = summaries
        .iter()
        .map(|summary| {
            let john_responses = summary
                .autoresolve_causal
                .john
                .response_availability
                .values()
                .sum();
            let opponent_responses = summary
                .autoresolve_causal
                .opponent
                .response_availability
                .values()
                .sum();
            MatchupCausalAudit {
                opponent: summary.opponent.clone(),
                tactical_first_contacts: [
                    summary.tactical_causal.john_first_contacts,
                    summary.tactical_causal.opponent_first_contacts,
                ],
                autoresolve_first_contacts: [
                    summary.autoresolve_causal.john_first_contacts,
                    summary.autoresolve_causal.opponent_first_contacts,
                ],
                autoresolve_attack_starts: [
                    summary.autoresolve_causal.john.attack_starts,
                    summary.autoresolve_causal.opponent.attack_starts,
                ],
                autoresolve_resolved_attacks: [
                    summary.autoresolve_causal.john_attacks,
                    summary.autoresolve_causal.opponent_attacks,
                ],
                autoresolve_cancellations: [
                    summary.autoresolve_causal.john.committed_attacks_canceled,
                    summary
                        .autoresolve_causal
                        .opponent
                        .committed_attacks_canceled,
                ],
                autoresolve_response_events: [john_responses, opponent_responses],
                response_event_for_every_incoming_contact: john_responses
                    == summary.autoresolve_causal.opponent_attacks
                    && opponent_responses == summary.autoresolve_causal.john_attacks,
            }
        })
        .collect();
    AcceptanceAudit {
        no_tactical_timeouts: summaries
            .iter()
            .all(|summary| summary.tactical_timeouts == 0),
        no_autoresolve_timeouts: summaries
            .iter()
            .all(|summary| summary.autoresolve_timeouts == 0),
        tactical_energy_conservation_holds: summaries
            .iter()
            .all(|summary| summary.tactical_causal.energy_partition_failures == 0),
        side_swap_nonterminal_timeline_equal: evidence
            .autoresolve_timeline
            .normalized_nonterminal_sequences_equal,
        simultaneous_contacts_preserved: evidence.autoresolve_timeline.simultaneous_contacts.len()
            == 2,
        canceled_attacks_emit_no_ghost_contacts: evidence
            .autoresolve_timeline
            .canceled_attack_ids_that_contacted
            .is_empty(),
        autoresolve_movement_elapsed_matches_distance_delta: summaries.iter().all(|summary| {
            summary
                .autoresolve_causal
                .john
                .movement_nonzero_delta_zero_elapsed
                == 0
                && summary
                    .autoresolve_causal
                    .opponent
                    .movement_nonzero_delta_zero_elapsed
                    == 0
        }),
        autoresolve_movement_respects_tick_and_speed_limits: summaries.iter().all(|summary| {
            summary
                .autoresolve_causal
                .john
                .movement_displacement_limit_failures
                == 0
                && summary
                    .autoresolve_causal
                    .opponent
                    .movement_displacement_limit_failures
                    == 0
                && summary
                    .autoresolve_causal
                    .john
                    .maximum_movement_segment_seconds
                    <= f64::from(1.0_f32 / 64.0 + 1.0e-6)
                && summary
                    .autoresolve_causal
                    .opponent
                    .maximum_movement_segment_seconds
                    <= f64::from(1.0_f32 / 64.0 + 1.0e-6)
        }),
        polearm_contact_revalidation_holds,
        all_weapon_swept_contact_contract_holds: evidence.all_weapon_contact_bands.iter().all(
            |contact| {
                contact.center_separation_metres
                    >= adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES
                    && contact.transformed_energy_joules <= contact.incident_energy_joules + 1.0e-4
            },
        ) && evidence.all_weapon_contact_bands.iter().any(
            |contact| {
                contact.weapon == "war_hammer"
                    && contact.classification == MeleeContactClassification::Pommel
            },
        ) && summaries.iter().all(|summary| {
            let minimum_center = f64::from(
                adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
            );
            summary
                .autoresolve_causal
                .john
                .full_energy_intended_contacts_inside_ten_centimetres
                == 0
                && summary
                    .autoresolve_causal
                    .opponent
                    .full_energy_intended_contacts_inside_ten_centimetres
                    == 0
                && (summary.autoresolve_causal.john.contact_measure_samples == 0
                    || summary
                        .autoresolve_causal
                        .john
                        .minimum_actual_center_separation_metres
                        >= minimum_center)
                && (summary.autoresolve_causal.opponent.contact_measure_samples == 0
                    || summary
                        .autoresolve_causal
                        .opponent
                        .minimum_actual_center_separation_metres
                        >= minimum_center)
        }),
        matchups,
    }
}

fn run_matchup(
    args: &Args,
    john: &MeleeIterationBuild,
    opponent: &MeleeIterationBuild,
) -> Result<MatchupSummary, String> {
    let directory = args.output.join(opponent.key);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let tactical_started = Instant::now();
    let mut tactical_john_wins = 0;
    let mut tactical_opponent_wins = 0;
    let mut tactical_mutual_incapacitations = 0;
    let mut tactical_timeouts = 0;
    let mut simulated_seconds = 0.0_f64;
    let mut tactical_causal = TacticalCausalSummary {
        minimum_contact_separation_metres: f32::INFINITY,
        ..TacticalCausalSummary::default()
    };
    let trace_file = File::create(directory.join("tactical-traces.ndjson"))
        .map_err(|error| error.to_string())?;
    let mut trace_writer = BufWriter::new(trace_file);
    for offset in 0..args.tactical_seeds {
        let outcome = resolve_tactical_server_melee_duel(john, opponent, args.first_seed + offset);
        simulated_seconds += f64::from(outcome.simulated_seconds);
        record_tactical_causal(&mut tactical_causal, &outcome, john.name);
        match &outcome.resolution {
            TacticalDuelResolution::Victory { victor } if victor == john.name => {
                tactical_john_wins += 1;
            }
            TacticalDuelResolution::Victory { victor } if victor == opponent.name => {
                tactical_opponent_wins += 1;
            }
            TacticalDuelResolution::Victory { victor } => {
                return Err(format!("unknown tactical victor {victor}"));
            }
            TacticalDuelResolution::MutualIncapacitation => {
                tactical_mutual_incapacitations += 1;
            }
            TacticalDuelResolution::Timeout => tactical_timeouts += 1,
        }
        if offset == 0 {
            write_json(&directory.join("tactical-trace.json"), &outcome)?;
        }
        serde_json::to_writer(&mut trace_writer, &outcome).map_err(|error| error.to_string())?;
        trace_writer
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
    }
    trace_writer.flush().map_err(|error| error.to_string())?;
    tactical_causal.finish_means();
    let tactical_wall = tactical_started.elapsed().as_secs_f64();

    let autoresolve_started = Instant::now();
    let mut autoresolve_john_wins = 0;
    let mut autoresolve_opponent_wins = 0;
    let mut autoresolve_mutual_incapacitations = 0;
    let mut autoresolve_timeouts = 0;
    let mut autoresolve_causal = AutoresolveCausalSummary::default();
    autoresolve_causal.john_weapon_reach_metres = john
        .combatant
        .equipment
        .melee_weapon
        .map_or(0.0, |weapon| f64::from(weapon.melee_reach));
    autoresolve_causal.opponent_weapon_reach_metres = opponent
        .combatant
        .equipment
        .melee_weapon
        .map_or(0.0, |weapon| f64::from(weapon.melee_reach));
    let autoresolve_trace_file = File::create(directory.join("autoresolve-traces.ndjson"))
        .map_err(|error| error.to_string())?;
    let mut autoresolve_trace_writer = BufWriter::new(autoresolve_trace_file);
    for offset in 0..args.autoresolve_seeds {
        let outcome = resolve_battle(
            vec![john.combatant.clone()],
            vec![opponent.combatant.clone()],
            args.first_seed + offset,
            BattleOpening::Normal,
        );
        match outcome.resolution {
            BattleResolution::AlliesVictory => autoresolve_john_wins += 1,
            BattleResolution::EnemiesVictory => autoresolve_opponent_wins += 1,
            BattleResolution::MutualIncapacitation => {
                autoresolve_mutual_incapacitations += 1;
            }
            BattleResolution::Timeout => autoresolve_timeouts += 1,
        }
        autoresolve_causal.total_rounds += outcome.rounds as u64;
        autoresolve_causal.john_yields += u64::from(outcome.allies[0].yielded);
        autoresolve_causal.opponent_yields += u64::from(outcome.enemies[0].yielded);
        autoresolve_causal.john.record_final(&outcome.allies[0]);
        autoresolve_causal
            .opponent
            .record_final(&outcome.enemies[0]);
        for combatant in outcome.allies.iter().chain(&outcome.enemies) {
            autoresolve_causal.final_blood_loss_fraction +=
                f64::from(combatant.blood_loss_fraction);
            autoresolve_causal.final_wound_count += combatant.wound_count as u64;
            autoresolve_causal.final_open_wound_count += combatant.open_wound_count as u64;
            autoresolve_causal.final_internal_wound_count += combatant.internal_wound_count as u64;
            autoresolve_causal.final_wound_flow_fraction_per_second +=
                f64::from(combatant.wound_flow_fraction_per_second);
            autoresolve_causal.final_oxygen_debt_joules += f64::from(combatant.oxygen_debt_joules);
            autoresolve_causal.final_local_action_fatigue +=
                f64::from(combatant.local_action_fatigue);
            autoresolve_causal.final_acute_trauma += f64::from(combatant.acute_trauma);
            autoresolve_causal.final_imbalance += f64::from(combatant.imbalance);
        }
        for entry in &outcome.log {
            if entry.attack_kind != "melee" {
                continue;
            }
            autoresolve_causal.melee_attacks += 1;
            if entry.attacker_id == john.combatant.id {
                autoresolve_causal.john_attacks += 1;
            } else {
                autoresolve_causal.opponent_attacks += 1;
            }
            autoresolve_causal.hits += u64::from(entry.outcome.starts_with("hit"));
            autoresolve_causal.blocks_or_parries += u64::from(entry.outcome == "blocked");
            autoresolve_causal.block_attempts += u64::from(entry.defender_response == "block");
            autoresolve_causal.parry_attempts += u64::from(entry.defender_response == "parry");
            autoresolve_causal.misses += u64::from(entry.outcome == "missed");
            if entry.defender_response == "dodge" {
                autoresolve_causal.dodge_attempts += 1;
                let contacted = entry
                    .melee_telemetry
                    .as_ref()
                    .and_then(|telemetry| telemetry.dodge_contacted_body_part)
                    .is_some();
                autoresolve_causal.dodge_avoids += u64::from(!contacted);
                autoresolve_causal.dodge_contacts += u64::from(contacted);
            }
            if entry.outcome == "blocked" && entry.defender_contact_item_id.is_some() {
                let defender_weapon_id = if entry.defender_id == john.combatant.id {
                    john.combatant.equipment.melee_weapon_id
                } else {
                    opponent.combatant.equipment.melee_weapon_id
                };
                if entry.defender_contact_item_id == defender_weapon_id {
                    autoresolve_causal.weapon_defense_contacts += 1;
                } else {
                    autoresolve_causal.shield_contacts += 1;
                }
            }
            if let Some(impact) = entry.armor_impact {
                autoresolve_causal.armor_contacts += 1;
                use adventuresim_core::combat::ArmorImpactOutcome;
                match impact.outcome {
                    ArmorImpactOutcome::Stopped => autoresolve_causal.armor_stopped += 1,
                    ArmorImpactOutcome::Deflected => autoresolve_causal.armor_deflected += 1,
                    ArmorImpactOutcome::Penetrated => autoresolve_causal.armor_penetrated += 1,
                }
            }
            autoresolve_causal.total_contact_energy_joules += f64::from(entry.contact_stress);
            autoresolve_causal.total_health_damage += f64::from(entry.health_damage);
            if entry.attacker_id == john.combatant.id {
                record_autoresolve_combatant_causal(
                    &mut autoresolve_causal.john,
                    &mut autoresolve_causal.opponent,
                    entry,
                );
            } else {
                record_autoresolve_combatant_causal(
                    &mut autoresolve_causal.opponent,
                    &mut autoresolve_causal.john,
                    entry,
                );
            }
        }
        for event in &outcome.timeline {
            let Some(combatant_id) = event.combatant_id else {
                continue;
            };
            let combatant = if combatant_id == john.combatant.id {
                &mut autoresolve_causal.john
            } else {
                &mut autoresolve_causal.opponent
            };
            match event.kind {
                MeleeTimelineKind::Movement => {
                    if let Some(action) = event.movement_action {
                        *combatant
                            .movement_actions
                            .entry(format!("{action:?}"))
                            .or_default() += 1;
                    }
                    combatant.movement_elapsed_seconds +=
                        f64::from(event.movement_elapsed_seconds.unwrap_or_default());
                    combatant.movement_segments += 1;
                    let elapsed = event.movement_elapsed_seconds.unwrap_or_default();
                    let displacement = event.movement_displacement_metres.unwrap_or_default().abs();
                    let maximum_velocity = event
                        .movement_velocity_before_metres_per_second
                        .unwrap_or_default()
                        .abs()
                        .max(
                            event
                                .movement_velocity_after_metres_per_second
                                .unwrap_or_default()
                                .abs(),
                        );
                    combatant.movement_absolute_displacement_metres += f64::from(displacement);
                    combatant.maximum_movement_speed_metres_per_second = combatant
                        .maximum_movement_speed_metres_per_second
                        .max(f64::from(maximum_velocity));
                    combatant.maximum_movement_segment_seconds = combatant
                        .maximum_movement_segment_seconds
                        .max(f64::from(elapsed));
                    combatant.movement_displacement_limit_failures +=
                        u64::from(displacement > maximum_velocity * elapsed + 1.0e-5);
                    let distance_delta = event
                        .engagement_distance_before_metres
                        .zip(event.engagement_distance_after_metres)
                        .map_or(0.0, |(before, after)| (after - before).abs());
                    if distance_delta > f32::EPSILON
                        && event.movement_elapsed_seconds.unwrap_or_default() <= 0.0
                    {
                        combatant.movement_nonzero_delta_zero_elapsed += 1;
                    }
                }
                MeleeTimelineKind::AttackStarted => combatant.attack_starts += 1,
                MeleeTimelineKind::Response => {
                    if let Some(availability) = event.response_availability {
                        *combatant
                            .response_availability
                            .entry(format!("{availability:?}"))
                            .or_default() += 1;
                    }
                    if let Some(choice) = event.response_choice {
                        *combatant
                            .response_choices
                            .entry(format!("{choice:?}"))
                            .or_default() += 1;
                    }
                    if let Some(delay) = event.phase_adaptation_delay_seconds
                        && delay > 0.0
                    {
                        combatant.phase_adaptation_events += 1;
                        combatant.phase_adaptation_delay_seconds += f64::from(delay);
                    }
                }
                MeleeTimelineKind::AttackCanceled => {
                    combatant.committed_attacks_canceled += 1;
                }
                MeleeTimelineKind::AttackTransformed => {
                    combatant.committed_attacks_transformed += 1;
                }
                MeleeTimelineKind::Contact if event.simultaneous_members.len() > 1 => {
                    combatant.simultaneous_contacts += 1;
                }
                MeleeTimelineKind::Contact | MeleeTimelineKind::Terminal => {}
            }
        }
        if let Some(first_contact) = outcome
            .timeline
            .iter()
            .find(|event| event.kind == MeleeTimelineKind::Contact)
        {
            if first_contact.combatant_id == Some(john.combatant.id) {
                autoresolve_causal.john_first_contacts += 1;
            } else {
                autoresolve_causal.opponent_first_contacts += 1;
            }
        }
        if offset == 0 {
            write_json(&directory.join("autoresolve-trace.json"), &outcome)?;
        }
        serde_json::to_writer(&mut autoresolve_trace_writer, &outcome)
            .map_err(|error| error.to_string())?;
        autoresolve_trace_writer
            .write_all(b"\n")
            .map_err(|error| error.to_string())?;
    }
    autoresolve_trace_writer
        .flush()
        .map_err(|error| error.to_string())?;
    autoresolve_causal.john.finish_means();
    autoresolve_causal.opponent.finish_means();
    let autoresolve_wall = autoresolve_started.elapsed().as_secs_f64();
    let tactical_win_rate = tactical_john_wins as f64 / args.tactical_seeds as f64;
    let autoresolve_win_rate = autoresolve_john_wins as f64 / args.autoresolve_seeds as f64;
    let summary = MatchupSummary {
        matchup: format!("{} vs {}", john.name, opponent.name),
        opponent: opponent.key.to_owned(),
        tactical_runs: args.tactical_seeds,
        tactical_john_wins,
        tactical_opponent_wins,
        tactical_mutual_incapacitations,
        tactical_timeouts,
        tactical_causal,
        tactical_total_simulated_seconds: simulated_seconds,
        tactical_mean_simulated_seconds: simulated_seconds / args.tactical_seeds as f64,
        autoresolve_runs: args.autoresolve_seeds,
        autoresolve_john_wins,
        autoresolve_opponent_wins,
        autoresolve_mutual_incapacitations,
        autoresolve_timeouts,
        autoresolve_causal,
        john_win_rate_difference: tactical_win_rate - autoresolve_win_rate,
        tactical_wall_milliseconds: tactical_wall * 1_000.0,
        autoresolve_wall_milliseconds: autoresolve_wall * 1_000.0,
        tactical_duels_per_second: args.tactical_seeds as f64 / tactical_wall.max(f64::EPSILON),
        tactical_simulated_seconds_per_wall_second: simulated_seconds
            / tactical_wall.max(f64::EPSILON),
        autoresolve_duels_per_second: args.autoresolve_seeds as f64
            / autoresolve_wall.max(f64::EPSILON),
    };
    write_json(&directory.join("summary.json"), &summary)?;
    let packet = ReviewerPacket {
        balance_concept: "Physically based gameplay: equipment, anatomy, force, skill, positioning, pain, blood loss, imbalance, and exertion should produce plausible causal outcomes rather than symmetric game abstractions.",
        scale_context: "Attributes and skill ranks use a 0-5 practical scale. About 3 describes a healthy adult or competent practitioner; 4 is exceptional or expert; 5 approaches Olympic or world-class human performance.",
        matchup: summary.matchup.clone(),
        combatants: [reviewer_combatant(john), reviewer_combatant(opponent)],
        tactical_trace_file: "tactical-trace.json",
        all_tactical_traces_file: "tactical-traces.ndjson",
        autoresolve_trace_file: "autoresolve-trace.json",
        all_autoresolve_traces_file: "autoresolve-traces.ndjson",
        aggregate_summary_file: "summary.json",
        acceptance_evidence_file: "../acceptance-evidence.json",
        acceptance_audit_file: "../acceptance-audit.json",
    };
    write_json(&directory.join("reviewer-packet.json"), &packet)?;
    Ok(summary)
}

fn record_autoresolve_combatant_causal(
    attacker: &mut AutoresolveCombatantCausal,
    defender: &mut AutoresolveCombatantCausal,
    entry: &adventuresim_core::autoresolve::BattleLogEntry,
) {
    attacker.attacks += 1;
    attacker.hits_dealt += u64::from(entry.outcome.starts_with("hit"));
    defender.attacks_received += 1;
    match entry.defender_response {
        "block" => defender.block_attempts += 1,
        "parry" => defender.parry_attempts += 1,
        "dodge" => defender.dodge_attempts += 1,
        _ => {}
    }
    if entry.defender_response == "dodge" {
        let contacted = entry
            .melee_telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.dodge_contacted_body_part)
            .is_some();
        defender.dodge_avoids += u64::from(!contacted);
        defender.dodge_contacts += u64::from(contacted);
    }
    if matches!(entry.defender_response, "block" | "parry") && entry.outcome == "blocked" {
        defender.effective_weapon_contacts += 1;
    }
    if let Some(telemetry) = &entry.melee_telemetry {
        *attacker
            .contact_classifications
            .entry(format!("{:?}", telemetry.contact_classification))
            .or_default() += 1;
        attacker.contact_measure_samples += 1;
        attacker.mean_scheduled_contact_measure_metres +=
            f64::from(telemetry.scheduled_contact_measure_metres);
        attacker.mean_actual_contact_measure_metres +=
            f64::from(telemetry.actual_contact_measure_metres);
        if attacker.contact_measure_samples == 1 {
            attacker.minimum_actual_contact_measure_metres =
                f64::from(telemetry.actual_contact_measure_metres);
            attacker.maximum_actual_contact_measure_metres =
                f64::from(telemetry.actual_contact_measure_metres);
            attacker.minimum_actual_center_separation_metres =
                f64::from(telemetry.actual_center_separation_metres);
        } else {
            attacker.minimum_actual_contact_measure_metres = attacker
                .minimum_actual_contact_measure_metres
                .min(f64::from(telemetry.actual_contact_measure_metres));
            attacker.maximum_actual_contact_measure_metres = attacker
                .maximum_actual_contact_measure_metres
                .max(f64::from(telemetry.actual_contact_measure_metres));
            attacker.minimum_actual_center_separation_metres = attacker
                .minimum_actual_center_separation_metres
                .min(f64::from(telemetry.actual_center_separation_metres));
        }
        attacker.full_energy_intended_contacts_inside_ten_centimetres += u64::from(
            telemetry.actual_contact_measure_metres < 0.1
                && telemetry.contact_classification == MeleeContactClassification::IntendedSurface
                && telemetry.contact_energy_fraction >= 0.999,
        );
        *defender
            .anatomical_subregions_received
            .entry(format!("{:?}", telemetry.anatomical_subregion))
            .or_default() += 1;
        defender.dodge_redirections += u64::from(telemetry.redirected_from.is_some());
        for layer in &telemetry.armor_layer_chain {
            let key = format!("{:?}:{:?}", layer.inventory_item_id, layer.material);
            let distribution = if layer.intersected {
                &mut defender.armor_layers_intersected
            } else {
                &mut defender.armor_layers_missed
            };
            *distribution.entry(key).or_default() += 1;
        }
        attacker.attack_samples += 1;
        attacker.mean_attack_interval_seconds += f64::from(telemetry.attack_interval_seconds);
        attacker.mean_attack_power_multiplier += f64::from(telemetry.attack_power_multiplier);
        let performance = f64::from(telemetry.attacker_fatigue_performance);
        if attacker.attack_samples == 1 {
            attacker.minimum_attack_fatigue_performance = performance;
        } else {
            attacker.minimum_attack_fatigue_performance =
                attacker.minimum_attack_fatigue_performance.min(performance);
        }
    }
    if let Some(impact) = entry.armor_impact {
        defender.armor_surface_contacts_received += 1;
        use adventuresim_core::combat::ArmorImpactOutcome;
        match impact.outcome {
            ArmorImpactOutcome::Stopped => defender.armor_stopped += 1,
            ArmorImpactOutcome::Deflected => defender.armor_deflected += 1,
            ArmorImpactOutcome::Penetrated => defender.armor_penetrated += 1,
        }
    } else if entry.outcome.starts_with("hit") {
        defender.armor_gap_contacts_received += 1;
    }
}

fn record_tactical_causal(
    summary: &mut TacticalCausalSummary,
    outcome: &TacticalMeleeOutcome,
    john_name: &str,
) {
    if let Some(first_contact) = outcome.events.first() {
        if first_contact.attacker == john_name {
            summary.john_first_contacts += 1;
        } else {
            summary.opponent_first_contacts += 1;
        }
    }
    let mut john_previous_attack_start = None;
    let mut opponent_previous_attack_start = None;
    for decision in &outcome.decision_events {
        let john = decision.combatant == john_name;
        match (decision.decision, decision.status) {
            (TacticalDecision::Attack, TacticalDecisionStatus::Started) if john => {
                summary.john_attack_starts += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::Started) => {
                summary.opponent_attack_starts += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::CanceledForDefense) if john => {
                summary.john_canceled_for_defense += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::CanceledForDefense) => {
                summary.opponent_canceled_for_defense += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::TransformedByDefense) if john => {
                summary.john_transformed_by_defense += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::TransformedByDefense) => {
                summary.opponent_transformed_by_defense += 1;
            }
            _ => {}
        }
        let combatant = if john {
            &mut summary.john
        } else {
            &mut summary.opponent
        };
        match (decision.decision, decision.status) {
            (TacticalDecision::Attack, TacticalDecisionStatus::Started) => {
                combatant.attack_starts += 1;
                let previous = if john {
                    &mut john_previous_attack_start
                } else {
                    &mut opponent_previous_attack_start
                };
                if let Some(previous_seconds) = previous.replace(decision.elapsed_seconds) {
                    combatant.attack_interval_samples += 1;
                    combatant.mean_attack_start_interval_seconds +=
                        f64::from(decision.elapsed_seconds - previous_seconds);
                }
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::CanceledForDefense) => {
                combatant.committed_attacks_canceled += 1;
            }
            (TacticalDecision::Attack, TacticalDecisionStatus::TransformedByDefense) => {
                combatant.committed_attacks_transformed += 1;
            }
            _ => {}
        }
    }
    for event in &outcome.events {
        let attacker_is_john = event.attacker == john_name;
        if attacker_is_john {
            summary.john_resolved_attacks += 1;
        } else {
            summary.opponent_resolved_attacks += 1;
        }
        {
            let attacker = if attacker_is_john {
                &mut summary.john
            } else {
                &mut summary.opponent
            };
            attacker.resolved_attacks += 1;
            attacker.contacts_dealt += u64::from(event.contact_energy_joules > 0.0);
            attacker.contact_energy_samples += 1;
            attacker.mean_contact_energy_joules += f64::from(event.contact_energy_joules);
            attacker.maximum_oxygen_debt_joules = attacker
                .maximum_oxygen_debt_joules
                .max(event.attacker_incapacitation.oxygen_debt_joules);
            attacker.maximum_local_action_fatigue = attacker
                .maximum_local_action_fatigue
                .max(event.attacker_incapacitation.local_action_fatigue);
        }
        let defender = if attacker_is_john {
            &mut summary.opponent
        } else {
            &mut summary.john
        };
        defender.attacks_received += 1;
        defender.maximum_oxygen_debt_joules = defender
            .maximum_oxygen_debt_joules
            .max(event.defender_incapacitation.oxygen_debt_joules);
        defender.maximum_local_action_fatigue = defender
            .maximum_local_action_fatigue
            .max(event.defender_incapacitation.local_action_fatigue);
        *defender
            .anatomical_subregions_received
            .entry(event.anatomical_subregion.clone())
            .or_default() += 1;
        match event.defender_decision {
            TacticalDecision::Block => defender.block_attempts += 1,
            TacticalDecision::Parry => defender.parry_attempts += 1,
            TacticalDecision::Dodge => defender.dodge_attempts += 1,
            _ => {}
        }
        if event.defender_decision == TacticalDecision::Dodge {
            defender.dodge_avoids += u64::from(event.outcome == "avoided");
            defender.dodge_contacts += u64::from(event.outcome != "avoided");
            defender.dodge_redirections += u64::from(event.redirected_from_body_part.is_some());
        }
        if event.defensive_implement.as_deref() == Some("buckler") {
            defender.shield_block_attempts += 1;
            defender.effective_shield_blocks += u64::from(event.outcome == "defended");
        } else if matches!(
            event.defender_decision,
            TacticalDecision::Block | TacticalDecision::Parry
        ) && event.outcome == "defended"
        {
            defender.effective_weapon_contacts += 1;
        }
        if event.coverage_contact == "armor_surface" {
            defender.armor_surface_contacts_received += 1;
            match event.armor_outcome.as_deref() {
                Some("stopped") => defender.armor_stopped += 1,
                Some("deflected") => defender.armor_deflected += 1,
                Some("penetrated") => defender.armor_penetrated += 1,
                _ => {}
            }
        } else if event.coverage_contact == "gap" {
            defender.armor_gap_contacts_received += 1;
        }
        for layer in &event.armor_layer_chain {
            let key = format!("{}:{:?}", layer.item_id, layer.material);
            let distribution = if layer.intersected {
                &mut defender.armor_layers_intersected
            } else {
                &mut defender.armor_layers_missed
            };
            *distribution.entry(key).or_default() += 1;
        }
        summary.minimum_contact_separation_metres = summary
            .minimum_contact_separation_metres
            .min(event.center_separation_metres);
        summary.maximum_contact_separation_metres = summary
            .maximum_contact_separation_metres
            .max(event.center_separation_metres);
        summary.maximum_oxygen_debt_joules = summary
            .maximum_oxygen_debt_joules
            .max(event.attacker_incapacitation.oxygen_debt_joules)
            .max(event.defender_incapacitation.oxygen_debt_joules);
        summary.maximum_local_action_fatigue = summary
            .maximum_local_action_fatigue
            .max(event.attacker_incapacitation.local_action_fatigue)
            .max(event.defender_incapacitation.local_action_fatigue);
        if event.defender_decision == TacticalDecision::Dodge {
            summary.dodge_attempts += 1;
            if event.outcome == "avoided" {
                summary.dodge_avoids += 1;
            } else {
                summary.dodge_contacts += 1;
            }
            summary.dodge_redirections += u64::from(event.redirected_from_body_part.is_some());
        }
        if event.defensive_implement.as_deref() == Some("buckler") {
            summary.shield_block_attempts += 1;
            if event.outcome == "defended" {
                summary.shield_blocks_defended += 1;
            } else {
                summary.shield_blocks_failed += 1;
            }
        } else if event.outcome == "defended" {
            summary.weapon_defense_contacts += 1;
        }
        if event.coverage_contact == "armor_surface" {
            summary.armor_contacts += 1;
            match event.armor_outcome.as_deref() {
                Some("stopped") => summary.armor_stopped += 1,
                Some("deflected") => summary.armor_deflected += 1,
                Some("penetrated") => summary.armor_penetrated += 1,
                _ => {}
            }
            let partition = event.resisted_energy_joules
                + event.transmitted_energy_joules
                + event.penetrated_energy_joules;
            summary.energy_partition_failures +=
                u64::from((partition - event.contact_energy_joules).abs() > 0.01);
        }
    }
    for wound in &outcome.wound_events {
        let combatant = if wound.combatant == john_name {
            &mut summary.john
        } else {
            &mut summary.opponent
        };
        match wound.kind.as_str() {
            "open" => {
                summary.open_wounds += 1;
                combatant.open_wounds_received += 1;
            }
            "internal" => {
                summary.internal_wounds += 1;
                combatant.internal_wounds_received += 1;
            }
            _ => {}
        }
    }
}

fn reviewer_combatant(build: &MeleeIterationBuild) -> ReviewerCombatant<'_> {
    let attributes = &build.combatant.attributes;
    let skills = &build.combatant.skills;
    let primary_hours = build
        .combatant
        .equipment
        .melee_weapon
        .map_or(0.0, |weapon| {
            weapon
                .skills
                .weighted_check(|skill| skills.skill_hours_trained(skill))
        });
    ReviewerCombatant {
        name: build.name,
        build: build.description,
        equipment: build.equipment_description,
        attributes: AttributeContext {
            endurance: attributes.endurance,
            strength: (attributes.left_arm_strength + attributes.right_arm_strength) * 0.5,
            agility: (attributes.left_arm_agility + attributes.right_arm_agility) * 0.5,
            instinct: attributes.instinct,
        },
        skills: SkillContext {
            primary_weapon_rank: Skill::Sword.training_rank(primary_hours),
            dodge_rank: Skill::Dodge.training_rank(skills.dodge_hours),
            block_rank: Skill::Block.training_rank(skills.block_hours),
            will_rank: Skill::Will.training_rank(skills.will_hours),
            balance_rank: Skill::Balance.training_rank(skills.balance_hours),
        },
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}
