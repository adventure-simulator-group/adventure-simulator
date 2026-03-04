use bevy::prelude::*;

use distance_field_plugin::DistanceFieldPlugin;
use marching_cubes_plugin::MarchingCubesPlugin;
use sphere_tracing_plugin::SphereTracingPlugin;

use bevy_egui::EguiPrimaryContextPass;

pub mod components;
pub mod resources;
pub mod stats;

use stats::RenderingStatsPlugin;

pub struct CharacterCreatorPlugin;

impl Plugin for CharacterCreatorPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<components::SceneState>()
            .add_plugins(DistanceFieldPlugin)
            .add_plugins(MarchingCubesPlugin)
            .add_plugins(SphereTracingPlugin)
            .add_plugins(RenderingStatsPlugin)
            .add_plugins(MeshPickingPlugin)
            .add_systems(
                EguiPrimaryContextPass,
                MarchingCubesPlugin::marching_cubes_ui
                    .run_if(in_state(components::SceneState::MarchingCubes)),
            )
            .add_systems(Startup, components::Scene::spawn)
            .add_systems(Update, components::AnimationPlayer::start)
            .add_systems(Startup, components::CharacterModel::spawn)
            .add_systems(Update, components::CharacterModel::update)
            .add_systems(Update, components::OrbitalCamera::update)
            .add_systems(Update, components::OrbitalCamera::gamepad_control)
            .add_systems(Update, components::OrbitalCamera::keyboard_control)
            .add_systems(Update, switch_scene)
            .add_systems(Update, (tag_scene_entities, update_visibility).chain());

        if components::Scene::DISPLAY_BONE_CYLINDERS {
            app.add_systems(Update, components::Scene::swap_mesh_for_cylinders);
            app.add_systems(
                PostUpdate,
                (
                    components::Scene::update_bone_segments,
                    components::Scene::draw_bone_labels,
                ),
            );
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
    if input.just_pressed(KeyCode::Digit2) && *state.get() != components::SceneState::MarchingCubes
    {
        next_state.set(components::SceneState::MarchingCubes);
        info!("Switching to Marching Cubes Scene");
    }
    if input.just_pressed(KeyCode::Digit3) && *state.get() != components::SceneState::SphereTracing
    {
        next_state.set(components::SceneState::SphereTracing);
        info!("Switching to Sphere Tracing Scene");
    }
}

fn tag_scene_entities(
    mut commands: Commands,
    // Query for DistanceFieldComponent entities (Marching Cubes volumes)
    distance_fields: Query<
        Entity,
        (
            With<distance_field_plugin::DistanceFieldComponent>,
            Without<components::InMarchingCubesScene>,
        ),
    >,
    // Query for SphereTracingMaterial entities
    sphere_tracing: Query<
        Entity,
        (
            With<MeshMaterial3d<sphere_tracing_plugin::SphereTracingMaterial>>,
            Without<components::InSphereTracingScene>,
        ),
    >,
) {
    for entity in distance_fields.iter() {
        commands
            .entity(entity)
            .insert(components::InMarchingCubesScene);
    }
    for entity in sphere_tracing.iter() {
        commands
            .entity(entity)
            .insert(components::InSphereTracingScene);
    }
}

fn update_visibility(
    state: Res<State<components::SceneState>>,
    mut queries: ParamSet<(
        Query<&mut Visibility, With<components::InCharacterScene>>,
        Query<&mut Visibility, With<components::InMarchingCubesScene>>,
        Query<&mut Visibility, With<components::InSphereTracingScene>>,
    )>,
) {
    if state.is_changed() {
        let current_state = *state.get();

        for mut visibility in queries.p0().iter_mut() {
            *visibility = if current_state == components::SceneState::Character {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }

        for mut visibility in queries.p1().iter_mut() {
            *visibility = if current_state == components::SceneState::MarchingCubes {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }

        for mut visibility in queries.p2().iter_mut() {
            *visibility = if current_state == components::SceneState::SphereTracing {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
