use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component for a player entity, for both client-controlled
/// active player and other players.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq,
)]
#[require(PlayerId)]
pub struct Player;

/// Player's client ID usable to distinguish the active player
/// from other connected players.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq,
)]
#[component(immutable)]
pub struct PlayerId(pub u64);
