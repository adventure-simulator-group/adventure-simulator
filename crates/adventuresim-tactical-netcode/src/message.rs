use adventuresim_tactical_core::prelude::*;
use bevy::{ecs::entity::MapEntities, prelude::*};
use serde::{Deserialize, Serialize};

/// Sent by the client whenever the player presses dodge or parry.
///
/// This is a simplified version of [`DefenderResponse`] that omits
/// `input_reflex` — the server computes reflex from timestamp delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Event, Serialize, Deserialize)]
pub enum DefendRequest {
    Dodge,
    Parry,
}

/// Requests enrollment of a strategic character in the tactical mission.
/// The sender remains the authoritative network identity.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize)]
pub struct JoinRequest {
    pub player_id: u64,
}

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize)]
pub struct PlayerInputRequest {
    pub movement: Option<Vec2>,
    pub look: Vec2,
    pub jump: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeleeActionPhase {
    Start,
    Complete,
}

/// Both melee phases share one mapped ordered stream, so a completion cannot
/// overtake its server-observed start.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub struct MeleeActionRequest {
    pub phase: MeleeActionPhase,
    #[entities]
    pub target: Option<Entity>,
    pub body_part: BodyPart,
    pub hit_precision: f32,
}

impl MeleeActionRequest {
    pub fn start() -> Self {
        Self {
            phase: MeleeActionPhase::Start,
            target: None,
            body_part: BodyPart::Chest,
            hit_precision: 0.0,
        }
    }

    pub fn complete(target: Entity, body_part: BodyPart, hit_precision: f32) -> Self {
        Self {
            phase: MeleeActionPhase::Complete,
            target: Some(target),
            body_part,
            hit_precision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RangedActionPhase {
    Start,
    Complete,
}

/// Both ranged phases share one mapped ordered stream. A completion may omit
/// a target when the client-fired shot missed, but it still consumes ammo.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub struct RangedActionRequest {
    pub phase: RangedActionPhase,
    #[entities]
    pub target: Option<Entity>,
    pub body_part: BodyPart,
    pub hit_precision: f32,
}

impl RangedActionRequest {
    pub fn start() -> Self {
        Self {
            phase: RangedActionPhase::Start,
            target: None,
            body_part: BodyPart::Chest,
            hit_precision: 0.0,
        }
    }

    pub fn complete(target: Option<Entity>, body_part: BodyPart, hit_precision: f32) -> Self {
        Self {
            phase: RangedActionPhase::Complete,
            target,
            body_part,
            hit_precision,
        }
    }
}

#[derive(Debug, Clone, Event, Serialize, Deserialize, MapEntities)]
pub struct SuccessfulAttackResponse {
    #[entities]
    pub attacker: Entity,
    #[entities]
    pub hit: Vec<Entity>,
    pub body_part: BodyPart,
    pub result: AttackResult,
    pub flanking: f32,
    pub defender_response: DefenderResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalOutcome {
    Victory,
    Defeat,
}

/// Broadcast only after strategic authority accepts the terminal tactical
/// submission. It is presentation state, not a second outcome authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Event, Serialize, Deserialize)]
pub struct TacticalOutcomeResponse {
    pub outcome: TacticalOutcome,
}

impl SuccessfulAttackResponse {
    pub fn total_damage(&self) -> f32 {
        match self.result {
            AttackResult::ToAttacker { .. } => 0.0,
            AttackResult::ToDefender {
                cut_damage,
                blunt_damage,
                ..
            } => cut_damage + blunt_damage,
        }
    }
}
