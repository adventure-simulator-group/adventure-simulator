use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Message, Serialize, Deserialize)]
pub struct JoinRequest {
    pub player_id: u64,
}

#[derive(Debug, Clone, Copy, Default, Message, Serialize, Deserialize)]
pub struct PlayerInputMessage {
    pub movement: Vec2,
    pub look: Vec2,
    pub jump: bool,
}

#[derive(Debug, Clone, Copy, Default, Message, Serialize, Deserialize)]
pub struct AttackCommand;
