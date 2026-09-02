use std::collections::HashMap;

use adventuresim_tactical_core::{inventory::ArmorLayerContact, prelude::*};
use bevy::prelude::*;
use serde::Serialize;

use super::TACTICAL_TICK_SECONDS;
use crate::combat::MeleeAttackResolved;

mod decisions;
pub(super) use decisions::*;
mod resolution;
use resolution::*;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalContactOutcome {
    Hit,
    Defended,
    Avoided,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalCoverageContact {
    None,
    ArmorSurface,
    Gap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalDefenseImplement {
    Shield,
    Weapon,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticalIncapacitationLog {
    pub pain: f32,
    pub acute_trauma: f32,
    pub blood_loss_fraction: f32,
    pub blood_loss_incapacitation: f32,
    pub imbalance: f32,
    pub fatigue: f32,
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
    pub defensive_implement_kind: Option<TacticalDefenseImplement>,
    pub defensive_item_id: Option<u64>,
    pub defense_success_probability: Option<f32>,
    pub defense_alignment_sample: Option<f32>,
    pub defense_engagement: Option<f32>,
    pub body_part: String,
    pub anatomical_subregion: String,
    pub contact_surface_coordinate: f32,
    pub armor_layer_chain: Vec<ArmorLayerContact>,
    pub scheduled_contact_measure_metres: f32,
    pub ideal_contact_measure_metres: f32,
    pub actual_contact_measure_metres: f32,
    pub contact_classification: adventuresim_core::combat::MeleeContactClassification,
    pub contact_lever_arm_metres: f32,
    pub contact_energy_fraction: f32,
    pub measure_accuracy_multiplier: f32,
    pub contact_invalidation_cause:
        Option<adventuresim_core::combat::MeleeContactInvalidationCause>,
    pub contact_material: Option<adventuresim_core::item_catalog_schema::EquipmentMaterial>,
    pub outcome: TacticalContactOutcome,
    pub coverage_contact: TacticalCoverageContact,
    pub armor_item_id: Option<u64>,
    pub armor_material: Option<adventuresim_core::item_catalog_schema::EquipmentMaterial>,
    pub armor_outcome: Option<ArmorImpactOutcome>,
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
    pub kind: adventuresim_core::combat::CombatWoundKind,
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
    dimensions: Query<&CharacterDimensions>,
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
        let weapon_reach = view.weapon_reach();
        let reach = melee_interaction_range(
            dimensions
                .get(event.attacker)
                .copied()
                .unwrap_or_default()
                .arm_reach_metres,
            weapon_reach,
        );
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
    let sequence = log.events.len() as u32 + 1;
    log.events.push(resolved_attack_entry(
        sequence,
        &clock,
        &event,
        attacker,
        defender,
        attacker_transform,
        defender_transform,
        &viewer,
        &states,
    ));
    log_new_wounds(&clock, defender, event.target, &wounds, &mut log);
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
                kind: wound.kind,
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
    );
    TacticalIncapacitationLog {
        pain: sources.pain,
        acute_trauma: state.acute_trauma,
        blood_loss_fraction: state.blood_loss_fraction,
        blood_loss_incapacitation: sources.blood_loss,
        imbalance: sources.imbalance,
        fatigue: state.fatigue,
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
                (
                    "blood_loss_fraction",
                    previous.blood_loss_fraction,
                    current.blood_loss_fraction,
                ),
                (
                    "blood_loss_incapacitation",
                    previous.blood_loss_incapacitation,
                    current.blood_loss_incapacitation,
                ),
                ("imbalance", previous.imbalance, current.imbalance),
                ("fatigue", previous.fatigue, current.fatigue),
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
