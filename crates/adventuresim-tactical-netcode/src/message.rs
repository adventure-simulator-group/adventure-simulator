use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize)]
pub struct JoinRequest {
    pub player_id: u64,
}

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize)]
pub struct PlayerInputMessage {
    pub movement: Option<Vec2>,
    pub look: Vec2,
    pub jump: bool,
}

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize)]
pub struct AttackCommand;
