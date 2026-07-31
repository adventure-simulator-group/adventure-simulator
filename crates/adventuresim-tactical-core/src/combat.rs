use bevy_enhanced_input::prelude::InputAction;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Attack;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Dodge;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Parry;
