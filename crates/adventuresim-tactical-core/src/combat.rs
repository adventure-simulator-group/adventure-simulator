use bevy_enhanced_input::prelude::InputAction;

/// The attacker's body-and-arms contribution to melee interaction range.
/// Equipped weapon reach is added to this value for both client hit detection
/// and server-owned AI engagement decisions.
pub const HANDS_REACH: f32 = 1.5;
#[must_use]
pub fn melee_interaction_range(weapon_reach: f32) -> f32 {
    HANDS_REACH + weapon_reach.max(0.0)
}

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Attack;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Dodge;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Parry;
