use adventuresim_tactical_core::avian3d::debug_render::PhysicsDebugRenderConfig;
use bevy::prelude::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle_debug_render);
    }
}

fn toggle_debug_render(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<PhysicsDebugRenderConfig>,
) {
    if keyboard.just_pressed(KeyCode::F3) {
        config.enable_colliders = !config.enable_colliders;
        config.enable_axes = !config.enable_axes;
    }
}
