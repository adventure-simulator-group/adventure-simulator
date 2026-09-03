use super::*;

/// Commit the same resolved ranged contact to gameplay and client feedback.
pub(super) fn publish(
    commands: &mut Commands,
    target: Entity,
    attacker_weapon_slot: EquipSlot,
    defender_blocking_slot: Option<EquipSlot>,
    response: SuccessfulAttackResponse,
) {
    commands.trigger(ApplyMeleeAttackResult {
        attacker: response.attacker,
        target,
        body_part: response.body_part,
        anatomical_subregion: anatomical_subregion(response.body_part, 0.5),
        surface_coordinate: 0.5,
        result: response.result,
        defender_response: response.defender_response,
        defense_success_probability: None,
        defense_alignment_sample: None,
        defense_engagement: None,
        attacker_weapon_slot,
        defender_blocking_slot,
        attacker_weapon_contact: false,
        impact_recipient: response.impact_recipient,
        impact_velocity_change: response.impact_velocity_change,
        contact_at_time: MeleeContactAtTime::intended(0.0),
    });
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: response,
    });
}
