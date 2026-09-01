use adventuresim_core::combat::WeaponDefenseAlignment;

use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "outcome emission carries the complete authoritative contact"
)]
pub(super) fn emit_melee_outcome(
    cmd: &mut Commands,
    attack: &AuthorizedMeleeAttack,
    attack_key: u64,
    entity: Entity,
    hand: AttackHand,
    attacker_side: BodySide,
    response: DefenderResponse,
    alignment: Option<WeaponDefenseAlignment>,
    dodge: Option<MeleeDodgeGeometry>,
    redirected_body_part: Option<BodyPart>,
    contact_at_time: MeleeContactAtTime,
    contact: MeleeContactLocation,
    result: AttackResult,
    flanking: f32,
    attacker_has_weapon: bool,
    attacker: &TacticalPlayerView<'_, '_, '_>,
    defender: &TacticalPlayerView<'_, '_, '_>,
    attacker_transform: &Transform,
    defender_transform: &Transform,
    viewer: &TacticalPlayerViewer<'_, '_>,
    config: &TacticalCombatConfig,
) {
    let Some(attacker_weapon_slot) = weapon_slot_for_side(Some(attacker_side)) else {
        info!(attack_key, attacker = ?attack.attacker(), target = ?attack.target(), body_part = ?attack.body_part(), outcome = "failed", reason = "ambiguous_striking_side", "melee_attack_resolved");
        return;
    };
    let defender_blocking_slot = defender_blocking_slot(
        response,
        defender.shield_holding_side(),
        defender.weapon_holding_side(),
    );
    let (recipient, velocity_change, point, normal) = authoritative_impact(
        result,
        attack.attacker(),
        attacker_transform.translation,
        attacker.body_weight() + attacker.inventory_weight(),
        attack.target(),
        defender_transform,
        defender.body_weight() + defender.inventory_weight(),
        contact.body_part,
        config,
    );
    let effects = authoritative_impact_effects(&viewer.inventory, entity, hand, result);
    cmd.trigger(ApplyMeleeAttackResult {
        attacker: attack.attacker(),
        target: attack.target(),
        body_part: contact.body_part,
        anatomical_subregion: contact.anatomical_subregion,
        surface_coordinate: contact.surface_coordinate,
        result,
        defender_response: response,
        defense_success_probability: alignment.map(|value| value.success_probability),
        defense_alignment_sample: alignment.map(|value| value.alignment_sample),
        defense_engagement: alignment.map(|value| value.engagement),
        attacker_weapon_slot,
        defender_blocking_slot,
        attacker_weapon_contact: attacker_has_weapon,
        impact_recipient: recipient,
        impact_velocity_change: velocity_change,
        closest_approach_metres: dodge.map(|geometry| geometry.closest_approach_metres),
        redirected_from: redirected_body_part
            .filter(|body_part| *body_part != attack.body_part())
            .map(|_| attack.body_part()),
        contact_at_time,
    });
    log_melee_result(
        attack_key,
        entity,
        attack.target(),
        contact.body_part,
        result,
    );
    cmd.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SuccessfulAttackResponse {
            attacker: attack.attacker(),
            hit: vec![attack.target()],
            body_part: contact.body_part,
            result,
            flanking,
            defender_response: response,
            impact_recipient: recipient,
            impact_velocity_change: velocity_change,
            impact_point: point,
            impact_normal: normal,
            impact_effects: effects,
        },
    });
}

fn log_melee_result(
    attack_key: u64,
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    result: AttackResult,
) {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => info!(
            attack_key,
            ?attacker,
            ?target,
            ?body_part,
            outcome = "failed",
            balance_damage,
            "melee_attack_resolved"
        ),
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => info!(
            attack_key,
            ?attacker,
            ?target,
            ?body_part,
            outcome = "connected",
            total_damage = cut_damage + blunt_damage,
            cut_damage,
            blunt_damage,
            balance_damage,
            "melee_attack_resolved"
        ),
    }
}
