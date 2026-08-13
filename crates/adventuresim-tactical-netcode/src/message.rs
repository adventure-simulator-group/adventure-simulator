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
    pub character_id: CharacterId,
}

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize, Reflect)]
pub struct PlayerInputRequest {
    pub movement: Option<Vec2>,
    pub look: Vec2,
    pub jump: bool,
    pub weapon_guard: WeaponGuardState,
}

/// Debug-build request to run the tactical simulation at normal or quarter
/// speed. Production servers intentionally do not install a handler for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Event, Serialize, Deserialize)]
pub struct DebugGameTimeScaleRequest {
    pub quarter_speed: bool,
}

/// Debug-build request to serialize the server's entire world (every entity's
/// reflected components, plus reflected resources) to a `.scn.ron` file for
/// offline inspection or replay as a test fixture. Production servers
/// intentionally do not install a handler for it.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize)]
pub struct DebugDumpWorldRequest;

impl DebugGameTimeScaleRequest {
    pub const fn relative_speed(self) -> f32 {
        if self.quarter_speed { 0.25 } else { 1.0 }
    }
}

#[cfg(test)]
mod debug_game_time_scale_tests {
    use super::DebugGameTimeScaleRequest;

    #[test]
    fn request_maps_to_normal_or_quarter_speed() {
        assert_eq!(
            DebugGameTimeScaleRequest {
                quarter_speed: false
            }
            .relative_speed(),
            1.0
        );
        assert_eq!(
            DebugGameTimeScaleRequest {
                quarter_speed: true
            }
            .relative_speed(),
            0.25
        );
    }
}

/// Both melee phases share one mapped ordered stream, so a completion cannot
/// overtake its server-observed start.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub enum MeleeActionRequest {
    Start,
    Complete {
        #[entities]
        target: Entity,
        body_part: BodyPart,
        reported_precision: f32,
    },
}

/// Both ranged phases share one mapped ordered stream. A completion may omit
/// a target when the client-fired shot missed, but it still consumes ammo.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub enum RangedActionRequest {
    Start,
    CompleteMiss,
    CompleteHit {
        #[entities]
        target: Entity,
        body_part: BodyPart,
        reported_precision: f32,
    },
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
