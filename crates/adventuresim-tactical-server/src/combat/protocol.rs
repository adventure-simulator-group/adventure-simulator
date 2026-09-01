use adventuresim_tactical_core::prelude::{AttackHand, BodyPart, StrikeFamily};
use adventuresim_tactical_netcode::message::DefendRequest;
use bevy::prelude::*;

use super::{CombatDuration, CombatInstant, ReportedPrecision};

/// Stores the defender's most recent dodge or downed roll along with the
/// server timestamp when it was received. Consumed on each attack resolution.
#[derive(Component)]
pub(crate) struct PendingDefenderResponse {
    pub(crate) choice: DefendRequest,
    pub(crate) set_at: CombatInstant,
}

/// Authoritative defensive action requested by either an authenticated client
/// or a server-owned behavior package. Source-specific code may choose the
/// actor, but all validation, animation state, and combat timing happens after
/// this seam.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct DefendIntent {
    pub(crate) defender: Entity,
    pub(crate) choice: DefendRequest,
}

/// Result of authoritative validation for a requested defensive action.
/// Iteration diagnostics observe this seam so an attempted reaction cannot be
/// mistaken for a defense that actually entered combat state.
#[derive(Event, Clone, Copy, Debug)]
pub struct DefendIntentResolved {
    pub defender: Entity,
    pub choice: DefendRequest,
    pub accepted: bool,
}

/// Both network clients and server-owned AI enter melee through this seam.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Entity,
    pub(crate) body_part: BodyPart,
    pub(crate) contact_sample: f32,
    pub(crate) defense_alignment_sample: f32,
    pub(crate) reported_precision: ReportedPrecision,
    pub(crate) strike_family: StrikeFamily,
    pub(crate) hand: AttackHand,
}

#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct MeleeAttackStartedIntent {
    pub(crate) attacker: Entity,
    pub(crate) target: Option<Entity>,
    pub(crate) windup: CombatDuration,
    pub(crate) reported_precision: ReportedPrecision,
    pub(crate) strike_family: StrikeFamily,
    pub(crate) hand: AttackHand,
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PendingMeleeContact {
    pub(crate) attack_key: u64,
    pub(crate) target: Option<Entity>,
    pub(crate) body_part: Option<BodyPart>,
    pub(crate) contact_sample: f32,
    pub(crate) defense_alignment_sample: f32,
    pub(crate) resolve_at: CombatInstant,
    pub(crate) reported_precision: ReportedPrecision,
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
