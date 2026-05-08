use adventuresim_tactical_core::prelude::Collider;
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

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize)]
pub struct AttackRequest;

#[derive(Debug, Clone, Event, Serialize, Deserialize, MapEntities)]
pub struct SuccessfulAttackResponse {
    pub attacker: Entity,
    pub hit: Vec<Entity>,
    pub hitreg: Collider,
    pub hitreg_transform: Transform,
}
