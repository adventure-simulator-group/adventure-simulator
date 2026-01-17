use bevy::prelude::*;

use components::CustomMaterial;
use marching_cubes_plugin::MarchingCubesPlugin;
use distance_field_plugin::DistanceFieldPlugin;
use sphere_tracing_plugin::SphereTracingPlugin;

use bevy_egui::EguiPrimaryContextPass;

pub mod components;
pub mod resources;

pub struct CharacterCreatorPlugin;

impl Plugin for CharacterCreatorPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<components::SceneState>()
            .add_plugins(MaterialPlugin::<CustomMaterial>::default())
            .add_plugins(DistanceFieldPlugin)
            .add_plugins(MarchingCubesPlugin)
            .add_plugins(SphereTracingPlugin)
            .add_systems(EguiPrimaryContextPass, MarchingCubesPlugin::marching_cubes_ui.run_if(in_state(components::SceneState::Debug)))
            .add_systems(Startup, components::Scene::spawn)
            .add_systems(Update, components::AnimationPlayer::start)
            .add_systems(Startup, components::CharacterModel::spawn)
            .add_systems(Update, components::CharacterModel::update)
            .add_systems(Update, components::OrbitalCamera::update)
            .add_systems(Update, components::OrbitalCamera::gamepad_control)
            .add_systems(Update, components::OrbitalCamera::keyboard_control)
            .add_systems(Update, switch_scene)
            .add_systems(Update, (
                tag_debug_entities,
                update_visibility,
            ).chain());

        if components::Scene::DISPLAY_BONE_CYLINDERS {
            app.add_systems(Update, components::Scene::swap_mesh_for_cylinders);
        }
    }
}

fn switch_scene(
    input: Res<ButtonInput<KeyCode>>,
    state: Res<State<components::SceneState>>,
    mut next_state: ResMut<NextState<components::SceneState>>,
) {
    if input.just_pressed(KeyCode::Digit1) && *state.get() != components::SceneState::Character {
        next_state.set(components::SceneState::Character);
        info!("Switching to Character Scene");
    }
    if input.just_pressed(KeyCode::Digit0) && *state.get() != components::SceneState::Debug {
        next_state.set(components::SceneState::Debug);
        info!("Switching to Debug Scene");
    }
}

fn tag_debug_entities(
    mut commands: Commands,
    // Query for DistanceFieldComponent entities (Marching Cubes volumes)
    distance_fields: Query<Entity, (With<distance_field_plugin::DistanceFieldComponent>, Without<components::InDebugScene>)>,
    // Query for SphereTracingMaterial entities
    sphere_tracing: Query<Entity, (With<MeshMaterial3d<sphere_tracing_plugin::SphereTracingMaterial>>, Without<components::InDebugScene>)>,
) {
    for entity in distance_fields.iter() {
        commands.entity(entity).insert(components::InDebugScene);
    }
    for entity in sphere_tracing.iter() {
        commands.entity(entity).insert(components::InDebugScene);
    }
}

fn update_visibility(
    state: Res<State<components::SceneState>>,
    mut queries: ParamSet<(
        Query<&mut Visibility, With<components::InCharacterScene>>,
        Query<&mut Visibility, With<components::InDebugScene>>,
    )>,
) {
    if state.is_changed() {
        let is_character_scene = *state.get() == components::SceneState::Character;

        for mut visibility in queries.p0().iter_mut() {
            *visibility = if is_character_scene {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }

        for mut visibility in queries.p1().iter_mut() {
            *visibility = if !is_character_scene {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
