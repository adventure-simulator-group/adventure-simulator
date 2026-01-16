use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component for a player, used as a context for
/// `bevy_enhanced_input` as well as a general identifier.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq,
)]
pub struct Player;
