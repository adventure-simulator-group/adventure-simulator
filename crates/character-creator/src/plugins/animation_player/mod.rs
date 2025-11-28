use bevy::prelude::*;

use crate::plugins::animation_player::components::CustomMaterial;

pub mod components;
pub mod resources;

pub struct AnimationPlayerPlugin;

impl Plugin for AnimationPlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<CustomMaterial>::default());
        app.add_systems(Startup, components::Scene::spawn)
            .add_systems(Startup, components::AnimationPlayer::spawn)
            .add_systems(Update, components::AnimationPlayer::keyboard_control)
            .add_systems(Update, components::AnimationPlayer::start)
            .add_systems(Update, components::AnimationPlayer::gamepad_control)
            .add_systems(Startup, components::CharacterModel::spawn)
            .add_systems(Update, components::CharacterModel::update);

        if components::Scene::DISPLAY_BONE_CYLINDERS {
            app.add_systems(Update, components::Scene::swap_mesh_for_cylinders);
        }
    }
}
