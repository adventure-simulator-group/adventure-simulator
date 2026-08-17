use adventuresim_tactical_core::prelude::{BodyPart, StrikeFamily};
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

/// Transient allegiance is independent from connectivity and bot control.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub(crate) enum TacticalCombatSide {
    Party,
    Enemy,
}

/// Emitted once per transition from active to incapacitated.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct TacticalCombatantDefeated(pub(crate) Entity);

/// Both network clients and server-owned AI enter melee through this seam.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Entity,
    pub(crate) body_part: BodyPart,
    pub(crate) reported_precision: ReportedPrecision,
    pub(crate) strike_family: StrikeFamily,
}

#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackStartedIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Entity,
    pub(crate) windup: CombatDuration,
    pub(crate) strike_family: StrikeFamily,
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
    pub(crate) windup: CombatDuration,
}
