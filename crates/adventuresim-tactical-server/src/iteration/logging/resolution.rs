use super::*;

pub(super) struct ResolvedEnergyLog {
    pub(super) outcome: TacticalContactOutcome,
    pub(super) coverage_contact: TacticalCoverageContact,
    pub(super) armor_impact: Option<ArmorImpact>,
    pub(super) contact: f32,
    pub(super) cut: f32,
    pub(super) blunt: f32,
}

pub(super) fn resolved_energy_log(result: AttackResult) -> ResolvedEnergyLog {
    match result {
        AttackResult::ToAttacker {
            contact_force,
            physical_contact,
            ..
        } => ResolvedEnergyLog {
            outcome: if physical_contact {
                TacticalContactOutcome::Defended
            } else {
                TacticalContactOutcome::Avoided
            },
            coverage_contact: TacticalCoverageContact::None,
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
            outcome: TacticalContactOutcome::Hit,
            coverage_contact: if armor_impact.is_some() {
                TacticalCoverageContact::ArmorSurface
            } else {
                TacticalCoverageContact::Gap
            },
            armor_impact,
            contact: contact_force,
            cut: cut_damage,
            blunt: blunt_damage,
        },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the log entry joins immutable event projections"
)]
pub(super) fn resolved_attack_entry(
    sequence: u32,
    clock: &IterationClock,
    event: &MeleeAttackResolved,
    attacker: &Player,
    defender: &Player,
    attacker_transform: &Transform,
    defender_transform: &Transform,
    viewer: &TacticalPlayerViewer<'_, '_>,
    states: &Query<(&TacticalCombatState, &Limbs)>,
) -> TacticalMeleeLogEntry {
    let energy = resolved_energy_log(event.result);
    let armor_impact = energy.armor_impact;
    TacticalMeleeLogEntry {
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
        defender_decision: defender_decision(event.defender_response),
        defensive_implement: event.defender_blocking_slot.and_then(|slot| {
            viewer
                .inventory
                .get(event.target)
                .item_at_slot(slot)
                .map(|(catalog_id, _)| catalog_id.to_owned())
        }),
        defensive_implement_kind: defensive_implement_kind(event, viewer),
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
        anatomical_subregion: serialized_name(event.anatomical_subregion),
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
        contact_classification: event.contact_at_time.classification,
        contact_lever_arm_metres: event.contact_at_time.lever_arm_metres,
        contact_energy_fraction: event.contact_at_time.energy_fraction,
        contact_invalidation_cause: event.contact_at_time.invalidation_cause,
        contact_material: event.contact_at_time.contact_material,
        outcome: energy.outcome,
        coverage_contact: energy.coverage_contact,
        armor_item_id: armor_impact.and_then(|impact| impact.surface.inventory_item_id),
        armor_material: armor_impact.and_then(|impact| impact.surface.material),
        armor_outcome: armor_impact.map(|impact| impact.outcome),
        resisted_energy_joules: armor_impact.map_or(0.0, |impact| impact.resisted_energy_joules),
        transmitted_energy_joules: armor_impact
            .map_or(0.0, |impact| impact.transmitted_energy_joules),
        penetrated_energy_joules: armor_impact
            .map_or(0.0, |impact| impact.penetrated_energy_joules),
        contact_energy_joules: energy.contact,
        cut_damage_joules: energy.cut,
        blunt_damage_joules: energy.blunt,
        attacker_incapacitation: incapacitation_log(viewer, states, event.attacker),
        defender_incapacitation: incapacitation_log(viewer, states, event.target),
    }
}

fn defensive_implement_kind(
    event: &MeleeAttackResolved,
    viewer: &TacticalPlayerViewer<'_, '_>,
) -> Option<TacticalDefenseImplement> {
    let slot = event.defender_blocking_slot?;
    let defender = viewer.get(event.target).ok()?;
    let side_slot = |side| match side {
        BodySide::Left => Some(EquipSlot::HoldingLeft),
        BodySide::Right => Some(EquipSlot::HoldingRight),
        BodySide::Both => None,
    };
    if defender.shield_holding_side().and_then(side_slot) == Some(slot) {
        Some(TacticalDefenseImplement::Shield)
    } else if defender.weapon_holding_side().and_then(side_slot) == Some(slot) {
        Some(TacticalDefenseImplement::Weapon)
    } else {
        Some(TacticalDefenseImplement::Other)
    }
}

fn defender_decision(response: DefenderResponse) -> TacticalDecision {
    match response {
        DefenderResponse::None => TacticalDecision::NoDefense,
        DefenderResponse::Block { .. } => TacticalDecision::Block,
        DefenderResponse::Parry { .. } => TacticalDecision::Parry,
        DefenderResponse::Dodge { .. } => TacticalDecision::Dodge,
    }
}

fn serialized_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("combat diagnostic value is serializable")
        .as_str()
        .expect("combat diagnostic value serializes as a string")
        .to_owned()
}
