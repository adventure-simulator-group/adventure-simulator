use bevy::prelude::*;
use bevy_enhanced_input::prelude::Actions;
use serde::{Deserialize, Serialize};

/// BEI Component alias to mark players that are controlled by the present client.
pub type ControlledPlayer = Actions<Player>;

/// Component for a player entity, for both client-controlled
/// active player and other players.
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq, Eq)]
#[require(PlayerId)]
#[component(immutable)]
pub struct Player {
    pub name: String,
}

/// Player's client ID usable to distinguish the active player
/// from other connected players.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq,
)]
#[component(immutable)]
pub struct PlayerId(pub u64);

impl PlayerId {
    /// Get associated color of this player.
    pub fn color(&self) -> Color {
        // SplitMix64-style mixing for good bit diffusion
        let mut x = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;

        let hue = (x % 360) as f32;
        let saturation = 0.28 + ((x >> 8) & 0xFF) as f32 / 255.0 * 0.18;
        let value = 0.90 + ((x >> 16) & 0xFF) as f32 / 255.0 * 0.08;

        Color::hsv(hue, saturation, value)
    }
}
