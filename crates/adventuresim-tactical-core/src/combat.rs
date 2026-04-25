use avian3d::prelude::Collider;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Attack;

#[derive(Component, Default)]
pub struct AttackState {
    pub pre_hit_timer: Timer,
}

impl AttackState {
    pub fn new(pre_hit_delay: f32) -> Self {
        let mut pre_hit_timer = Timer::from_seconds(pre_hit_delay, TimerMode::Once);
        pre_hit_timer.pause();
        Self { pre_hit_timer }
    }

    pub fn is_attacking(&self) -> bool {
        !self.pre_hit_timer.is_paused() && !self.pre_hit_timer.is_finished()
    }
}

/// MVP: shared hitreg for every weapon
#[derive(Resource)]
pub struct AttackConfig {
    pub hitreg_shape: Collider,
    pub hitreg_translation: Vec3,
    pub pre_hit_delay: f32,
}

impl Default for AttackConfig {
    fn default() -> Self {
        Self {
            hitreg_shape: Collider::capsule(0.5, 0.3),
            hitreg_translation: Vec3::Z,
            pre_hit_delay: 0.3,
        }
    }
}
