use std::collections::HashMap;

use adventuresim_tactical_core::{inventory::ArmorLayerContact, prelude::*};
use bevy::prelude::*;
use serde::Serialize;

use super::TACTICAL_TICK_SECONDS;
use crate::combat::MeleeAttackResolved;

mod decisions;
pub(super) use decisions::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalDecision {
    Attack,
    Block,
    Parry,
    Dodge,
    Withdraw,
    Yield,
    NoDefense,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalDecisionStatus {
    Started,
    Attempted,
    Accepted,
    Rejected,
    CanceledForDefense,
    TransformedByDefense,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalIncapacitationLog {
    pub pain: f32,
    pub acute_trauma: f32,
    pub blood_loss: f32,
    pub imbalance: f32,
    pub oxygen_debt_joules: f32,
    pub oxygen_debt_incapacitation: f32,
    pub local_action_fatigue: f32,
    pub total: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalMeleeLogEntry {
    pub sequence: u32,
    pub tick: u64,
    pub elapsed_seconds: f32,
    pub attacker: String,
    pub defender: String,
    pub center_separation_metres: f32,
    pub attacker_decision: TacticalDecision,
    pub defender_decision: TacticalDecision,
    pub defensive_implement: Option<String>,
    pub defensive_item_id: Option<u64>,
    pub defense_success_probability: Option<f32>,
    pub defense_alignment_sample: Option<f32>,
    pub defense_engagement: Option<f32>,
    pub body_part: String,
    pub anatomical_subregion: String,
    pub contact_surface_coordinate: f32,
    pub armor_layer_chain: Vec<ArmorLayerContact>,
    pub redirected_from_body_part: Option<String>,
    pub closest_approach_metres: Option<f32>,
    pub scheduled_contact_measure_metres: f32,
    pub actual_contact_measure_metres: f32,
    pub contact_classification: String,
    pub contact_lever_arm_metres: f32,
    pub contact_energy_fraction: f32,
    pub contact_invalidation_cause: Option<String>,
    pub contact_material: Option<String>,
    pub outcome: String,
    pub coverage_contact: String,
    pub armor_item_id: Option<u64>,
    pub armor_material: Option<String>,
    pub armor_outcome: Option<String>,
    pub resisted_energy_joules: f32,
    pub transmitted_energy_joules: f32,
    pub penetrated_energy_joules: f32,
    pub contact_energy_joules: f32,
    pub cut_damage_joules: f32,
    pub blunt_damage_joules: f32,
    pub attacker_incapacitation: TacticalIncapacitationLog,
    pub defender_incapacitation: TacticalIncapacitationLog,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalConditionLogEntry {
    pub tick: u64,
    pub elapsed_seconds: f32,
    pub combatant: String,
    pub cause: String,
    pub previous: f32,
    pub current: f32,
    pub delta: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalWoundLogEntry {
    pub tick: u64,
    pub elapsed_seconds: f32,
    pub combatant: String,
    pub body_part: String,
    pub kind: String,
    pub blood_fraction_per_second: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalDecisionLogEntry {
    pub tick: u64,
    pub elapsed_seconds: f32,
    pub combatant: String,
    pub decision: TacticalDecision,
    pub status: TacticalDecisionStatus,
    pub target: Option<String>,
    pub center_separation_metres: Option<f32>,
    pub preferred_melee_measure_metres: Option<f32>,
    pub attack_key: Option<u64>,
    pub cause: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TacticalDuelResolution {
    Victory { victor: String },
    MutualIncapacitation,
    Timeout,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalMeleeOutcome {
    pub seed: u64,
    pub resolution: TacticalDuelResolution,
    pub simulated_ticks: u64,
    pub simulated_seconds: f32,
    pub initial_center_separation_metres: f32,
    pub final_center_separation_metres: f32,
    pub attack_starts: u32,
    pub resolved_attacks: u32,
    pub events: Vec<TacticalMeleeLogEntry>,
    pub decision_events: Vec<TacticalDecisionLogEntry>,
    pub condition_events: Vec<TacticalConditionLogEntry>,
    pub wound_events: Vec<TacticalWoundLogEntry>,
}

struct ResolvedEnergyLog {
    outcome: &'static str,
    coverage_contact: &'static str,
    armor_impact: Option<ArmorImpact>,
    contact: f32,
    cut: f32,
    blunt: f32,
}

#[derive(Resource)]
pub(super) struct IterationClock {
    pub(super) tick: u64,
}

#[derive(Resource, Default)]
pub(super) struct IterationLog {
    pub(super) attack_starts: u32,
    pub(super) events: Vec<TacticalMeleeLogEntry>,
    pub(super) decision_events: Vec<TacticalDecisionLogEntry>,
    pub(super) condition_events: Vec<TacticalConditionLogEntry>,
    pub(super) wound_events: Vec<TacticalWoundLogEntry>,
    last_conditions: HashMap<Entity, TacticalIncapacitationLog>,
    logged_wound_counts: HashMap<Entity, usize>,
}

pub(super) fn record_attack_start(
    event: On<crate::combat::MeleeAttackStartedIntent>,
    clock: Res<IterationClock>,
    players: Query<(&Player, &Transform)>,
    viewer: TacticalPlayerViewer,
    config: Res<TacticalCombatConfig>,
    mut log: ResMut<IterationLog>,
) {
    log.attack_starts += 1;
    let Ok((attacker, attacker_transform)) = players.get(event.attacker) else {
        return;
    };
    let target = event.target.and_then(|entity| players.get(entity).ok());
    let preferred_melee_measure_metres = viewer.get(event.attacker).ok().map(|view| {
        let reach = view.weapon_reach();
        let grip = view.weapon_grip_to_tip();
        let head = view.weapon_striking_head_length();
        let distal = adventuresim_core::combat::has_distal_striking_surface(
            grip,
            head,
            view.weapon_body_material(),
            view.weapon_striking_material(),
        );
        adventuresim_core::combat::preferred_melee_striking_measure(
            reach,
            grip,
            head,
            distal,
            config.ai.ordinary.offense.melee_measure_reach_fraction,
        )
    });
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: attacker.name.clone(),
        decision: TacticalDecision::Attack,
        status: TacticalDecisionStatus::Started,
        target: target.map(|(player, _)| player.name.clone()),
        center_separation_metres: target.map(|(_, transform)| {
            attacker_transform
                .translation
                .xz()
                .distance(transform.translation.xz())
        }),
        preferred_melee_measure_metres,
        attack_key: None,
        cause: None,
    });
}

pub(super) fn record_defense_resolution(
    event: On<crate::combat::DefendIntentResolved>,
    clock: Res<IterationClock>,
    players: Query<&Player>,
    mut log: ResMut<IterationLog>,
) {
    let Ok(defender) = players.get(event.defender) else {
        return;
    };
    let decision = match event.choice {
        adventuresim_tactical_netcode::message::DefendRequest::Dodge { .. }
        | adventuresim_tactical_netcode::message::DefendRequest::Roll => TacticalDecision::Dodge,
    };
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: defender.name.clone(),
        decision,
        status: TacticalDecisionStatus::Attempted,
        target: None,
        center_separation_metres: None,
        preferred_melee_measure_metres: None,
        attack_key: None,
        cause: None,
    });
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: defender.name.clone(),
        decision,
        status: if event.accepted {
            TacticalDecisionStatus::Accepted
        } else {
            TacticalDecisionStatus::Rejected
        },
        target: None,
        center_separation_metres: None,
        preferred_melee_measure_metres: None,
        attack_key: None,
        cause: None,
    });
}

pub(super) fn record_resolved_attack(
    event: On<MeleeAttackResolved>,
    clock: Res<IterationClock>,
    players: Query<(&Player, &Transform)>,
    viewer: TacticalPlayerViewer,
    states: Query<(&TacticalCombatState, &Limbs)>,
    wounds: Query<&crate::combat::TacticalWounds>,
    mut log: ResMut<IterationLog>,
) {
    let Ok((attacker, attacker_transform)) = players.get(event.attacker) else {
        return;
    };
    let Ok((defender, defender_transform)) = players.get(event.target) else {
        return;
    };
    let ResolvedEnergyLog {
        outcome,
        coverage_contact,
        armor_impact,
        contact,
        cut,
        blunt,
    } = resolved_energy_log(event.result);
    let sequence = log.events.len() as u32 + 1;
    log.events.push(TacticalMeleeLogEntry {
        sequence,
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        attacker: attacker.name.clone(),
        defender: defender.name.clone(),
        center_separation_metres: attacker_transform
            .translation
            .xz()
            .distance(defender_transform.translation.xz()),
        attacker_decision: TacticalDecision::Attack,
        defender_decision: match event.defender_response {
            DefenderResponse::None => TacticalDecision::NoDefense,
            DefenderResponse::Block { .. } => TacticalDecision::Block,
            DefenderResponse::Parry { .. } => TacticalDecision::Parry,
            DefenderResponse::Dodge { .. } => TacticalDecision::Dodge,
        },
        defensive_implement: event.defender_blocking_slot.and_then(|slot| {
            viewer
                .inventory
                .get(event.target)
                .item_at_slot(slot)
                .map(|(catalog_id, _)| catalog_id.to_owned())
        }),
        defensive_item_id: event.defender_blocking_slot.and_then(|slot| {
            viewer
                .inventory
                .get(event.target)
                .item_at_slot(slot)
                .and_then(|(_, inventory_id)| inventory_id)
        }),
        defense_success_probability: event.defense_success_probability,
        defense_alignment_sample: event.defense_alignment_sample,
        defense_engagement: event.defense_engagement,
        body_part: format!("{:?}", event.body_part).to_lowercase(),
        anatomical_subregion: serde_json::to_value(event.anatomical_subregion)
            .expect("anatomical subregion is serializable")
            .as_str()
            .expect("anatomical subregion serializes as a string")
            .to_owned(),
        contact_surface_coordinate: event.surface_coordinate,
        armor_layer_chain: viewer
            .inventory
            .get(event.target)
            .armor_layer_chain(event.body_part, event.surface_coordinate),
        redirected_from_body_part: event
            .redirected_from
            .map(|part| format!("{part:?}").to_lowercase()),
        closest_approach_metres: event.closest_approach_metres,
        scheduled_contact_measure_metres: event.contact_at_time.scheduled_measure_metres,
        actual_contact_measure_metres: event.contact_at_time.actual_measure_metres,
        contact_classification: serde_json::to_value(event.contact_at_time.classification)
            .expect("contact classification is serializable")
            .as_str()
            .expect("contact classification serializes as a string")
            .to_owned(),
        contact_lever_arm_metres: event.contact_at_time.lever_arm_metres,
        contact_energy_fraction: event.contact_at_time.energy_fraction,
        contact_invalidation_cause: event.contact_at_time.invalidation_cause.map(|cause| {
            serde_json::to_value(cause)
                .expect("contact invalidation cause is serializable")
                .as_str()
                .expect("contact invalidation cause serializes as a string")
                .to_owned()
        }),
        contact_material: event
            .contact_at_time
            .contact_material
            .map(|material| format!("{material:?}").to_lowercase()),
        outcome: outcome.into(),
        coverage_contact: coverage_contact.into(),
        armor_item_id: armor_impact.and_then(|impact| impact.surface.inventory_item_id),
        armor_material: armor_impact
            .and_then(|impact| impact.surface.material)
            .map(|material| format!("{material:?}").to_lowercase()),
        armor_outcome: armor_impact.map(|impact| format!("{:?}", impact.outcome).to_lowercase()),
        resisted_energy_joules: armor_impact.map_or(0.0, |impact| impact.resisted_energy_joules),
        transmitted_energy_joules: armor_impact
            .map_or(0.0, |impact| impact.transmitted_energy_joules),
        penetrated_energy_joules: armor_impact
            .map_or(0.0, |impact| impact.penetrated_energy_joules),
        contact_energy_joules: contact,
        cut_damage_joules: cut,
        blunt_damage_joules: blunt,
        attacker_incapacitation: incapacitation_log(&viewer, &states, event.attacker),
        defender_incapacitation: incapacitation_log(&viewer, &states, event.target),
    });
    log_new_wounds(&clock, defender, event.target, &wounds, &mut log);
}

fn resolved_energy_log(result: AttackResult) -> ResolvedEnergyLog {
    match result {
        AttackResult::ToAttacker {
            contact_force,
            physical_contact,
            ..
        } => ResolvedEnergyLog {
            outcome: if physical_contact {
                "defended"
            } else {
                "avoided"
            },
            coverage_contact: "none",
            armor_impact: None,
            contact: contact_force,
            cut: 0.0,
            blunt: 0.0,
        },
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            contact_force,
            armor_impact,
            ..
        } => ResolvedEnergyLog {
            outcome: "hit",
            coverage_contact: if armor_impact.is_some() {
                "armor_surface"
            } else {
                "gap"
            },
            armor_impact,
            contact: contact_force,
            cut: cut_damage,
            blunt: blunt_damage,
        },
    }
}

fn log_new_wounds(
    clock: &IterationClock,
    defender: &Player,
    target: Entity,
    wounds: &Query<&crate::combat::TacticalWounds>,
    log: &mut IterationLog,
) {
    if let Ok(wounds) = wounds.get(target) {
        let already_logged = log
            .logged_wound_counts
            .get(&target)
            .copied()
            .unwrap_or(0)
            .min(wounds.0.len());
        for wound in &wounds.0[already_logged..] {
            log.wound_events.push(TacticalWoundLogEntry {
                tick: clock.tick,
                elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
                combatant: defender.name.clone(),
                body_part: format!("{:?}", wound.body_part).to_lowercase(),
                kind: format!("{:?}", wound.kind).to_lowercase(),
                blood_fraction_per_second: wound.blood_fraction_per_second,
            });
        }
        log.logged_wound_counts.insert(target, wounds.0.len());
    }
}

fn incapacitation_log(
    viewer: &TacticalPlayerViewer<'_, '_>,
    states: &Query<(&TacticalCombatState, &Limbs)>,
    entity: Entity,
) -> TacticalIncapacitationLog {
    let view = viewer
        .get(entity)
        .expect("logged combatant remains projected");
    let (state, limbs) = states
        .get(entity)
        .expect("logged combatant has condition state");
    let sources = state.incapacitation_sources(
        limbs.total_damage(),
        view.skill_check(Skill::Will, LimbWeights::all_equal()),
        view.raw_single_body_part_attr(SimpleAttribute::Endurance),
    );
    TacticalIncapacitationLog {
        pain: sources.pain,
        acute_trauma: state.acute_trauma,
        blood_loss: sources.blood_loss,
        imbalance: sources.imbalance,
        oxygen_debt_joules: state.oxygen_debt_joules,
        oxygen_debt_incapacitation: sources.oxygen_debt,
        local_action_fatigue: state.local_action_fatigue,
        total: state.incapacitation,
    }
}

pub(super) fn record_condition_changes(
    clock: Res<IterationClock>,
    players: Query<(Entity, &Player)>,
    states: Query<(&TacticalCombatState, &Limbs)>,
    viewer: TacticalPlayerViewer,
    mut log: ResMut<IterationLog>,
) {
    for (entity, player) in &players {
        let current = incapacitation_log(&viewer, &states, entity);
        if let Some(previous) = log.last_conditions.get(&entity).cloned() {
            for (cause, before, after) in [
                ("pain", previous.pain, current.pain),
                ("acute_trauma", previous.acute_trauma, current.acute_trauma),
                ("blood_loss", previous.blood_loss, current.blood_loss),
                ("imbalance", previous.imbalance, current.imbalance),
                (
                    "oxygen_debt_joules",
                    previous.oxygen_debt_joules,
                    current.oxygen_debt_joules,
                ),
                (
                    "oxygen_debt_incapacitation",
                    previous.oxygen_debt_incapacitation,
                    current.oxygen_debt_incapacitation,
                ),
                (
                    "local_action_fatigue",
                    previous.local_action_fatigue,
                    current.local_action_fatigue,
                ),
                ("total", previous.total, current.total),
            ] {
                if (after - before).abs() > 1.0e-6 {
                    log.condition_events.push(TacticalConditionLogEntry {
                        tick: clock.tick,
                        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
                        combatant: player.name.clone(),
                        cause: cause.into(),
                        previous: before,
                        current: after,
                        delta: after - before,
                    });
                }
            }
        }
        log.last_conditions.insert(entity, current);
    }
}
