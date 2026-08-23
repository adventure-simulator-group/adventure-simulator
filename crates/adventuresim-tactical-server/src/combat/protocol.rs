use adventuresim_tactical_core::prelude::{AttackHand, BodyPart, StrikeFamily};
use adventuresim_tactical_netcode::message::DefendRequest;
use bevy::prelude::*;

use super::{CombatDuration, CombatInstant, ReportedPrecision};

/// Stores the defender's most recent dodge, downed roll, or parry choice along with the
/// server timestamp when it was received. Consumed on each attack resolution.
#[derive(Component)]
pub(crate) struct PendingDefenderResponse {
    pub(crate) choice: DefendRequest,
    pub(crate) set_at: CombatInstant,
}

/// Both network clients and server-owned AI enter melee through this seam.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Entity,
    pub(crate) body_part: BodyPart,
    pub(crate) reported_precision: ReportedPrecision,
    pub(crate) strike_family: StrikeFamily,
    pub(crate) hand: AttackHand,
}

#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackStartedIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Entity,
    pub(crate) windup: CombatDuration,
    pub(crate) strike_family: StrikeFamily,
    pub(crate) hand: AttackHand,
}

/// `target == None` is an authoritative miss that still consumes a projectile.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct RangedAttackIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Option<Entity>,
    pub(crate) body_part: BodyPart,
    pub(crate) reported_precision: ReportedPrecision,
}

#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct RangedAttackStartedIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Option<Entity>,
    pub(crate) animation_windup: CombatDuration,
    pub(crate) minimum_windup: CombatDuration,
}
