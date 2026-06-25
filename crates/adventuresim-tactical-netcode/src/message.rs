use adventuresim_tactical_core::prelude::BodyPart;
use bevy::{ecs::entity::MapEntities, prelude::*};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub struct AttackRequest {
    #[entities]
    pub target: Entity,
    pub body_part: BodyPart,
    pub hit_precision: f32,
}

#[derive(Debug, Clone, Event, Serialize, Deserialize, MapEntities)]
pub struct SuccessfulAttackResponse {
    #[entities]
    pub attacker: Entity,
    #[entities]
    pub hit: Vec<Entity>,
    pub body_part: BodyPart,
    pub cut_damage: f32,
    pub blunt_damage: f32,
    pub flanking: f32,
}

impl SuccessfulAttackResponse {
    pub fn total_damage(&self) -> f32 {
        self.cut_damage + self.blunt_damage
    }
}
