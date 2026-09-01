use super::*;

pub(super) struct TacticalBatch {
    pub(super) john_wins: u64,
    pub(super) opponent_wins: u64,
    pub(super) mutual_incapacitations: u64,
    pub(super) timeouts: u64,
    pub(super) simulated_seconds: f64,
    pub(super) causal: TacticalCausalSummary,
    pub(super) wall_seconds: f64,
}

pub(super) fn run_tactical_batch(
    args: &Args,
    john: &MeleeIterationBuild,
    opponent: &MeleeIterationBuild,
    directory: &Path,
) -> Result<TacticalBatch, String> {
    let started = Instant::now();
    let mut batch = TacticalBatch {
        john_wins: 0,
        opponent_wins: 0,
        mutual_incapacitations: 0,
        timeouts: 0,
        simulated_seconds: 0.0,
        causal: TacticalCausalSummary {
            minimum_contact_separation_metres: f32::INFINITY,
            ..TacticalCausalSummary::default()
        },
        wall_seconds: 0.0,
    };
    let file = File::create(directory.join("tactical-traces.ndjson"))
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    for offset in 0..args.tactical_seeds {
        let outcome = resolve_tactical_server_melee_duel(john, opponent, args.first_seed + offset);
        batch.simulated_seconds += f64::from(outcome.simulated_seconds);
        record_tactical_causal(&mut batch.causal, &outcome, john.name);
        match &outcome.resolution {
            TacticalDuelResolution::Victory { victor } if victor == john.name => {
                batch.john_wins += 1
            }
            TacticalDuelResolution::Victory { victor } if victor == opponent.name => {
                batch.opponent_wins += 1
            }
            TacticalDuelResolution::Victory { victor } => {
                return Err(format!("unknown tactical victor {victor}"));
            }
            TacticalDuelResolution::MutualIncapacitation => batch.mutual_incapacitations += 1,
            TacticalDuelResolution::Timeout => batch.timeouts += 1,
        }
        if offset == 0 {
            write_json(&directory.join("tactical-trace.json"), &outcome)?;
        }
        serde_json::to_writer(&mut writer, &outcome).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    batch.causal.finish_means();
    batch.wall_seconds = started.elapsed().as_secs_f64();
    Ok(batch)
}

pub(super) struct AutoresolveBatch {
    pub(super) john_wins: u64,
    pub(super) opponent_wins: u64,
    pub(super) mutual_incapacitations: u64,
    pub(super) timeouts: u64,
    pub(super) causal: AutoresolveCausalSummary,
    pub(super) wall_seconds: f64,
}

pub(super) fn run_autoresolve_batch(
    args: &Args,
    john: &MeleeIterationBuild,
    opponent: &MeleeIterationBuild,
    directory: &Path,
) -> Result<AutoresolveBatch, String> {
    let started = Instant::now();
    let mut batch = AutoresolveBatch {
        john_wins: 0,
        opponent_wins: 0,
        mutual_incapacitations: 0,
        timeouts: 0,
        causal: AutoresolveCausalSummary::default(),
        wall_seconds: 0.0,
    };
    batch.causal.john_weapon_reach_metres = weapon_reach(john);
    batch.causal.opponent_weapon_reach_metres = weapon_reach(opponent);
    let file = File::create(directory.join("autoresolve-traces.ndjson"))
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    for offset in 0..args.autoresolve_seeds {
        let outcome = resolve_battle(
            vec![john.combatant.clone()],
            vec![opponent.combatant.clone()],
            args.first_seed + offset,
            BattleOpening::Normal,
        );
        record_autoresolve_outcome(&mut batch, &outcome, john, opponent);
        if offset == 0 {
            write_json(&directory.join("autoresolve-trace.json"), &outcome)?;
        }
        serde_json::to_writer(&mut writer, &outcome).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    batch.causal.john.finish_means();
    batch.causal.opponent.finish_means();
    batch.wall_seconds = started.elapsed().as_secs_f64();
    Ok(batch)
}

fn weapon_reach(build: &MeleeIterationBuild) -> f64 {
    build
        .combatant
        .equipment
        .melee_weapon
        .map_or(0.0, |weapon| f64::from(weapon.melee_reach))
}

fn record_autoresolve_outcome(
    batch: &mut AutoresolveBatch,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
    john: &MeleeIterationBuild,
    opponent: &MeleeIterationBuild,
) {
    match outcome.resolution {
        BattleResolution::AlliesVictory => batch.john_wins += 1,
        BattleResolution::EnemiesVictory => batch.opponent_wins += 1,
        BattleResolution::MutualIncapacitation => batch.mutual_incapacitations += 1,
        BattleResolution::Timeout => batch.timeouts += 1,
    }
    batch.causal.total_rounds += outcome.rounds as u64;
    batch.causal.john_yields += u64::from(outcome.allies[0].yielded);
    batch.causal.opponent_yields += u64::from(outcome.enemies[0].yielded);
    batch.causal.john.record_final(&outcome.allies[0]);
    batch.causal.opponent.record_final(&outcome.enemies[0]);
    record_final_totals(&mut batch.causal, outcome);
    record_autoresolve_log(
        &mut batch.causal,
        outcome,
        john.combatant.id,
        john.combatant.equipment.melee_weapon_id,
        opponent.combatant.equipment.melee_weapon_id,
    );
    record_autoresolve_timeline(&mut batch.causal, outcome, john.combatant.id);
}

fn record_final_totals(
    causal: &mut AutoresolveCausalSummary,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
) {
    for combatant in outcome.allies.iter().chain(&outcome.enemies) {
        causal.final_blood_loss_fraction += f64::from(combatant.blood_loss_fraction);
        causal.final_wound_count += combatant.wound_count as u64;
        causal.final_open_wound_count += combatant.open_wound_count as u64;
        causal.final_internal_wound_count += combatant.internal_wound_count as u64;
        causal.final_wound_flow_fraction_per_second +=
            f64::from(combatant.wound_flow_fraction_per_second);
        causal.final_oxygen_debt_joules += f64::from(combatant.oxygen_debt_joules);
        causal.final_local_action_fatigue += f64::from(combatant.local_action_fatigue);
        causal.final_acute_trauma += f64::from(combatant.acute_trauma);
        causal.final_imbalance += f64::from(combatant.imbalance);
    }
}

fn record_autoresolve_log(
    causal: &mut AutoresolveCausalSummary,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
    john_id: u64,
    john_weapon_id: Option<u64>,
    opponent_weapon_id: Option<u64>,
) {
    for entry in &outcome.log {
        if entry.attack_kind != BattleAttackKind::Melee {
            continue;
        }
        causal.melee_attacks += 1;
        let john_attacks = entry.attacker_id == john_id;
        causal.john_attacks += u64::from(john_attacks);
        causal.opponent_attacks += u64::from(!john_attacks);
        causal.hits += u64::from(matches!(
            entry.outcome,
            BattleAttackOutcome::HitHealth | BattleAttackOutcome::HitArmor
        ));
        causal.blocks_or_parries += u64::from(entry.outcome == BattleAttackOutcome::Blocked);
        causal.block_attempts += u64::from(entry.defender_response == MeleeResponseChoice::Block);
        causal.parry_attempts += u64::from(entry.defender_response == MeleeResponseChoice::Parry);
        causal.misses += u64::from(entry.outcome == BattleAttackOutcome::Missed);
        record_autoresolve_dodge(causal, entry);
        record_autoresolve_implement(causal, entry, john_id, john_weapon_id, opponent_weapon_id);
        record_autoresolve_armor(causal, entry);
        causal.total_contact_energy_joules += f64::from(entry.contact_stress);
        causal.total_health_damage += f64::from(entry.health_damage);
        if john_attacks {
            record_autoresolve_combatant_causal(&mut causal.john, &mut causal.opponent, entry);
        } else {
            record_autoresolve_combatant_causal(&mut causal.opponent, &mut causal.john, entry);
        }
    }
}

fn record_autoresolve_dodge(
    causal: &mut AutoresolveCausalSummary,
    entry: &adventuresim_core::autoresolve::BattleLogEntry,
) {
    if entry.defender_response != MeleeResponseChoice::Dodge {
        return;
    }
    causal.dodge_attempts += 1;
    let contacted = entry
        .melee_telemetry
        .as_ref()
        .and_then(|telemetry| telemetry.dodge_contacted_body_part)
        .is_some();
    causal.dodge_avoids += u64::from(!contacted);
    causal.dodge_contacts += u64::from(contacted);
}

fn record_autoresolve_implement(
    causal: &mut AutoresolveCausalSummary,
    entry: &adventuresim_core::autoresolve::BattleLogEntry,
    john_id: u64,
    john_weapon_id: Option<u64>,
    opponent_weapon_id: Option<u64>,
) {
    if entry.outcome != BattleAttackOutcome::Blocked || entry.defender_contact_item_id.is_none() {
        return;
    }
    let defender_weapon_id = if entry.defender_id == john_id {
        john_weapon_id
    } else {
        opponent_weapon_id
    };
    if entry.defender_contact_item_id == defender_weapon_id {
        causal.weapon_defense_contacts += 1;
    } else {
        causal.shield_contacts += 1;
    }
}

fn record_autoresolve_armor(
    causal: &mut AutoresolveCausalSummary,
    entry: &adventuresim_core::autoresolve::BattleLogEntry,
) {
    let Some(impact) = entry.armor_impact else {
        return;
    };
    causal.armor_contacts += 1;
    match impact.outcome {
        adventuresim_core::combat::ArmorImpactOutcome::Stopped => causal.armor_stopped += 1,
        adventuresim_core::combat::ArmorImpactOutcome::Deflected => causal.armor_deflected += 1,
        adventuresim_core::combat::ArmorImpactOutcome::Penetrated => causal.armor_penetrated += 1,
    }
}

fn record_autoresolve_timeline(
    causal: &mut AutoresolveCausalSummary,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
    john_id: u64,
) {
    for event in &outcome.timeline {
        let Some(id) = event.combatant_id else {
            continue;
        };
        let combatant = if id == john_id {
            &mut causal.john
        } else {
            &mut causal.opponent
        };
        causal::record_timeline_event(combatant, event);
    }
    if let Some(first) = outcome
        .timeline
        .iter()
        .find(|event| event.kind == MeleeTimelineKind::Contact)
    {
        causal.john_first_contacts += u64::from(first.combatant_id == Some(john_id));
        causal.opponent_first_contacts += u64::from(first.combatant_id != Some(john_id));
    }
}
