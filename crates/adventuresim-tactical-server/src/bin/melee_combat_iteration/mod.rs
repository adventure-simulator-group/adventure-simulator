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
        BattleAttackKind, BattleAttackOutcome, BattleOpening, BattleResolution,
        MeleeIterationBuild, MeleeResponseChoice, MeleeTimelineKind,
        melee_iteration_acceptance_evidence, melee_iteration_roster, resolve_battle,
    },
    combat::{MeleeContactClassification, MeleeContactInvalidationCause},
    item_catalog_schema::EquipmentMaterial,
    skill::{PlayerSkills, Skill},
};
use adventuresim_tactical_server::iteration::{
    TacticalContactOutcome, TacticalCoverageContact, TacticalDecision, TacticalDecisionStatus,
    TacticalDefenseImplement, TacticalDuelResolution, TacticalMeleeOutcome,
    resolve_tactical_server_melee_duel,
};
use clap::Parser;
use serde::Serialize;

mod audit;
mod causal;
mod reviewer;
mod runner;
use reviewer::*;

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

pub(crate) fn run() -> Result<(), String> {
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
    audit::build_acceptance_audit(summaries, evidence)
}

fn run_matchup(
    args: &Args,
    john: &MeleeIterationBuild,
    opponent: &MeleeIterationBuild,
) -> Result<MatchupSummary, String> {
    let directory = args.output.join(opponent.key);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let tactical = runner::run_tactical_batch(args, john, opponent, &directory)?;

    let autoresolve = runner::run_autoresolve_batch(args, john, opponent, &directory)?;
    let tactical_win_rate = tactical.john_wins as f64 / args.tactical_seeds as f64;
    let autoresolve_win_rate = autoresolve.john_wins as f64 / args.autoresolve_seeds as f64;
    let summary = MatchupSummary {
        matchup: format!("{} vs {}", john.name, opponent.name),
        opponent: opponent.key.to_owned(),
        tactical_runs: args.tactical_seeds,
        tactical_john_wins: tactical.john_wins,
        tactical_opponent_wins: tactical.opponent_wins,
        tactical_mutual_incapacitations: tactical.mutual_incapacitations,
        tactical_timeouts: tactical.timeouts,
        tactical_causal: tactical.causal,
        tactical_total_simulated_seconds: tactical.simulated_seconds,
        tactical_mean_simulated_seconds: tactical.simulated_seconds / args.tactical_seeds as f64,
        autoresolve_runs: args.autoresolve_seeds,
        autoresolve_john_wins: autoresolve.john_wins,
        autoresolve_opponent_wins: autoresolve.opponent_wins,
        autoresolve_mutual_incapacitations: autoresolve.mutual_incapacitations,
        autoresolve_timeouts: autoresolve.timeouts,
        autoresolve_causal: autoresolve.causal,
        john_win_rate_difference: tactical_win_rate - autoresolve_win_rate,
        tactical_wall_milliseconds: tactical.wall_seconds * 1_000.0,
        autoresolve_wall_milliseconds: autoresolve.wall_seconds * 1_000.0,
        tactical_duels_per_second: args.tactical_seeds as f64
            / tactical.wall_seconds.max(f64::EPSILON),
        tactical_simulated_seconds_per_wall_second: tactical.simulated_seconds
            / tactical.wall_seconds.max(f64::EPSILON),
        autoresolve_duels_per_second: args.autoresolve_seeds as f64
            / autoresolve.wall_seconds.max(f64::EPSILON),
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
    attacker.hits_dealt += u64::from(matches!(
        entry.outcome,
        BattleAttackOutcome::HitHealth | BattleAttackOutcome::HitArmor
    ));
    defender.attacks_received += 1;
    match entry.defender_response {
        MeleeResponseChoice::Block => defender.block_attempts += 1,
        MeleeResponseChoice::Parry => defender.parry_attempts += 1,
        MeleeResponseChoice::Dodge => defender.dodge_attempts += 1,
        _ => {}
    }
    if entry.defender_response == MeleeResponseChoice::Dodge {
        let contacted = entry.outcome != BattleAttackOutcome::Missed;
        defender.dodge_avoids += u64::from(!contacted);
        defender.dodge_contacts += u64::from(contacted);
    }
    if matches!(
        entry.defender_response,
        MeleeResponseChoice::Block | MeleeResponseChoice::Parry
    ) && entry.outcome == BattleAttackOutcome::Blocked
    {
        defender.effective_weapon_contacts += 1;
    }
    if let Some(telemetry) = &entry.melee_telemetry {
        causal::record_autoresolve_telemetry(attacker, defender, telemetry);
    }
    if let Some(impact) = entry.armor_impact {
        defender.armor_surface_contacts_received += 1;
        use adventuresim_core::combat::ArmorImpactOutcome;
        match impact.outcome {
            ArmorImpactOutcome::Stopped => defender.armor_stopped += 1,
            ArmorImpactOutcome::Deflected => defender.armor_deflected += 1,
            ArmorImpactOutcome::Penetrated => defender.armor_penetrated += 1,
        }
    } else if matches!(
        entry.outcome,
        BattleAttackOutcome::HitHealth | BattleAttackOutcome::HitArmor
    ) {
        defender.armor_gap_contacts_received += 1;
    }
}

fn record_tactical_causal(
    summary: &mut TacticalCausalSummary,
    outcome: &TacticalMeleeOutcome,
    john_name: &str,
) {
    causal::record_tactical_first_contact(summary, outcome, john_name);
    causal::record_tactical_decisions(summary, outcome, john_name);
    for event in &outcome.events {
        causal::record_tactical_event(summary, event, john_name);
    }
    causal::record_tactical_wounds(summary, outcome, john_name);
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}
