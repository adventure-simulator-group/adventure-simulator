use bevy::prelude::*;

use components::CustomMaterial;
use marching_cubes_plugin::MarchingCubesPlugin;
use distance_field_plugin::DistanceFieldPlugin;
use sphere_tracing_plugin::SphereTracingPlugin;

pub mod components;
pub mod resources;

pub struct CharacterCreatorPlugin;

impl Plugin for CharacterCreatorPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(MaterialPlugin::<CustomMaterial>::default())
            .add_plugins(DistanceFieldPlugin)
            .add_plugins(MarchingCubesPlugin)
            .add_plugins(SphereTracingPlugin)
            .add_systems(Startup, components::Scene::spawn)
            .add_systems(Update, components::AnimationPlayer::start)
            .add_systems(Startup, components::CharacterModel::spawn)
            .add_systems(Update, components::CharacterModel::update)
            .add_systems(Update, components::OrbitalCamera::update)
            .add_systems(Update, components::OrbitalCamera::gamepad_control)
            .add_systems(Update, components::OrbitalCamera::keyboard_control);

        if components::Scene::DISPLAY_BONE_CYLINDERS {
            app.add_systems(Update, components::Scene::swap_mesh_for_cylinders);
        }
    }
}
