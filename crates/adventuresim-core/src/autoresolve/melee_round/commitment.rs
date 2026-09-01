use super::*;

pub(super) fn phase_adaptation_delay(
    phase: MeleeDefenderPhase,
    incoming: ScheduledMeleeTiming,
    defender: &Combatant,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    let gap = match phase {
        MeleeDefenderPhase::CommittedAttack(timing) => {
            (timing.contact_at_seconds - incoming.contact_at_seconds).max(0.0)
        }
        MeleeDefenderPhase::NeutralGuard | MeleeDefenderPhase::OccupiedRecovery { .. } => 0.0,
    };
    gap.min(
        defender
            .equipment
            .melee_weapon
            .map_or(parameters.melee_windup_seconds, |weapon| {
                weapon.attack_interval_seconds
            }),
    )
}

pub(super) fn timeline_phase(phase: MeleeDefenderPhase) -> MeleeTimelinePhase {
    match phase {
        MeleeDefenderPhase::NeutralGuard => MeleeTimelinePhase::NeutralGuard,
        MeleeDefenderPhase::CommittedAttack(_) => MeleeTimelinePhase::Windup,
        MeleeDefenderPhase::OccupiedRecovery { .. } => MeleeTimelinePhase::Recovery,
    }
}

pub(super) fn timeline_phase_after_commitment(
    commitment: MeleeDefenseCommitmentKind,
    phase: MeleeDefenderPhase,
) -> MeleeTimelinePhase {
    match commitment {
        MeleeDefenseCommitmentKind::CanceledSameWeapon
        | MeleeDefenseCommitmentKind::DefenseRecovery => MeleeTimelinePhase::Recovery,
        MeleeDefenseCommitmentKind::TransformedOffhand => MeleeTimelinePhase::Windup,
        MeleeDefenseCommitmentKind::None | MeleeDefenseCommitmentKind::NeutralGuardRecovery => {
            timeline_phase(phase)
        }
    }
}

pub(super) fn response_availability(
    defender: &Combatant,
    response: DefenderResponse,
    phase: MeleeDefenderPhase,
    incoming: ScheduledMeleeTiming,
) -> MeleeResponseAvailability {
    if matches!(response, DefenderResponse::Dodge { .. }) {
        return MeleeResponseAvailability::DodgeChosen;
    }
    match phase {
        MeleeDefenderPhase::CommittedAttack(timing)
            if timing.started_at_seconds > incoming.started_at_seconds
                && timing.started_at_seconds <= incoming.contact_at_seconds =>
        {
            MeleeResponseAvailability::ReciprocalWindup
        }
        MeleeDefenderPhase::CommittedAttack(_) => {
            MeleeResponseAvailability::OccupiedByEarlierAttack
        }
        MeleeDefenderPhase::OccupiedRecovery { .. } => MeleeResponseAvailability::OccupiedRecovery,
        MeleeDefenderPhase::NeutralGuard
            if defender.equipment.melee_weapon.is_none()
                && defender.equipment.shield_block_bonus <= 0.0 =>
        {
            MeleeResponseAvailability::NoImplement
        }
        MeleeDefenderPhase::NeutralGuard => MeleeResponseAvailability::NeutralGuard,
    }
}

pub(in crate::autoresolve) fn defender_phase_at_contact(
    defender: &Combatant,
    attacker_id: u64,
    incoming: ScheduledMeleeTiming,
) -> MeleeDefenderPhase {
    if defender
        .melee_engagement_target
        .is_some_and(|target| target != attacker_id)
    {
        return MeleeDefenderPhase::NeutralGuard;
    }
    if let (Some(started_at_seconds), Some(contact_at_seconds)) = (
        defender.melee_attack_started_at_seconds,
        defender.melee_attack_contact_at_seconds,
    ) {
        return MeleeDefenderPhase::CommittedAttack(ScheduledMeleeTiming {
            started_at_seconds,
            contact_at_seconds,
            recovery_until_seconds: defender.melee_recovery_until_seconds,
        });
    }
    if defender.melee_recovery_until_seconds > incoming.contact_at_seconds {
        return MeleeDefenderPhase::OccupiedRecovery {
            until_seconds: defender.melee_recovery_until_seconds,
        };
    }
    MeleeDefenderPhase::NeutralGuard
}

pub(super) fn response_name(response: DefenderResponse) -> &'static str {
    match response {
        DefenderResponse::None => "none",
        DefenderResponse::Block { .. } => "block",
        DefenderResponse::Parry { .. } => "parry",
        DefenderResponse::Dodge { .. } => "dodge",
    }
}

pub(super) fn response_choice(response: DefenderResponse) -> MeleeResponseChoice {
    match response {
        DefenderResponse::None => MeleeResponseChoice::None,
        DefenderResponse::Block { .. } => MeleeResponseChoice::Block,
        DefenderResponse::Parry { .. } => MeleeResponseChoice::Parry,
        DefenderResponse::Dodge { .. } => MeleeResponseChoice::Dodge,
    }
}

pub(super) fn autoresolve_armor_layer_chain(
    equipment: &CombatEquipment,
    contact: MeleeContactLocation,
) -> Vec<ArmorLayerTelemetry> {
    let armor = equipment.armor[body_part_index(contact.body_part)];
    let geometry = armor.coverage_geometry;
    let span = geometry
        .map(|geometry| geometry.span)
        .or(armor.coverage_span)
        .unwrap_or_else(|| ArmorCoverageSpan::centered(armor.coverage));
    let intersected = span.contains(contact.surface_coordinate);
    vec![ArmorLayerTelemetry {
        inventory_item_id: armor.inventory_item_id,
        material: armor.material,
        geometry,
        intersected,
        selected: intersected,
    }]
}

pub(super) fn charge_defensive_work(defender: &mut Combatant, response: DefenderResponse) {
    match response {
        DefenderResponse::None => {}
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. } => {
            defender.charge_action_work(CombatActionWork::WeaponDefense, 0.5)
        }
        DefenderResponse::Dodge { .. } => {
            defender.charge_action_work(CombatActionWork::ExplosiveDodge, 0.5)
        }
    }
}

pub(in crate::autoresolve) fn commit_defensive_action(
    defender: &mut Combatant,
    attempted: DefenderResponse,
    effective: DefenderResponse,
    phase: MeleeDefenderPhase,
) -> DefenseCommitment {
    let defense_seconds = match attempted {
        DefenderResponse::None => return DefenseCommitment::NONE,
        DefenderResponse::Block { .. }
        | DefenderResponse::Parry { .. }
        | DefenderResponse::Dodge { .. } => 0.5,
    };
    if let DefenderResponse::Block { effectiveness } = effective
        && defender.equipment.shield_block_bonus > 0.0
        && matches!(phase, MeleeDefenderPhase::CommittedAttack(_))
    {
        defender.melee_attack_power_multiplier *=
            (1.0 - 0.4 * effectiveness.clamp(0.0, 1.0)).clamp(0.2, 1.0);
        return DefenseCommitment {
            kind: MeleeDefenseCommitmentKind::TransformedOffhand,
            retained_power: Some(defender.melee_attack_power_multiplier),
            recovery_seconds_after_contact: 0.0,
        };
    }
    if matches!(phase, MeleeDefenderPhase::NeutralGuard) {
        return DefenseCommitment {
            kind: MeleeDefenseCommitmentKind::NeutralGuardRecovery,
            retained_power: None,
            recovery_seconds_after_contact: if matches!(attempted, DefenderResponse::Dodge { .. }) {
                defense_seconds
            } else {
                0.0
            },
        };
    }
    let canceled =
        attempted.is_weapon_contact() && matches!(phase, MeleeDefenderPhase::CommittedAttack(_));
    DefenseCommitment {
        kind: if canceled {
            MeleeDefenseCommitmentKind::CanceledSameWeapon
        } else {
            MeleeDefenseCommitmentKind::DefenseRecovery
        },
        retained_power: None,
        recovery_seconds_after_contact: defense_seconds,
    }
}

#[derive(Clone, Copy)]
pub(in crate::autoresolve) struct DefenseCommitment {
    pub kind: MeleeDefenseCommitmentKind,
    pub retained_power: Option<f32>,
    pub recovery_seconds_after_contact: f32,
}
impl DefenseCommitment {
    const NONE: Self = Self {
        kind: MeleeDefenseCommitmentKind::None,
        retained_power: None,
        recovery_seconds_after_contact: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::autoresolve) enum MeleeDefenseCommitmentKind {
    None,
    NeutralGuardRecovery,
    CanceledSameWeapon,
    TransformedOffhand,
    DefenseRecovery,
}
impl MeleeDefenseCommitmentKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NeutralGuardRecovery => "neutral_guard_recovery",
            Self::CanceledSameWeapon => "canceled_for_same_weapon_defense",
            Self::TransformedOffhand => "transformed_by_offhand_defense",
            Self::DefenseRecovery => "defense_recovery",
        }
    }
}
