use bevy::prelude::*;

use std::f32::consts::PI;

use bevy::light::CascadeShadowConfigBuilder;

use crate::plugins::animation_player::resources::{Animations, SceneHandle};

const MODEL_PATH: &str = "models/animated/Michelle.glb";

pub struct Scene;

impl Scene {
    pub fn spawn(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut graphs: ResMut<Assets<AnimationGraph>>,
    ) {
        commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.,
            ..default()
        });

        // Build the animation graph
        let (graph, node_indices) = AnimationGraph::from_clips([
            asset_server.load(GltfAssetLabel::Animation(0).from_asset(MODEL_PATH))
        ]);
    
        // Keep our animation graph in a Resource so that it can be inserted onto
        // the correct entity once the scene actually loads.
        let graph_handle = graphs.add(graph);
        commands.insert_resource(Animations {
            animations: node_indices,
            graph_handle,
        });
    
        // Camera
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 1.0, 4.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y),
        ));
    
        // Plane
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(500000.0, 500000.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        ));
    
        // Light
        commands.spawn((
            Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 1.0, -PI / 4.)),
            DirectionalLight {
                shadows_enabled: true,
                ..default()
            },
            CascadeShadowConfigBuilder {
                first_cascade_far_bound: 200.0,
                maximum_distance: 400.0,
                ..default()
            }
            .build(),
        ));
    
        // Model
        let scene_handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_PATH));
        commands.spawn(SceneRoot(scene_handle.clone()));
        commands.insert_resource(SceneHandle {
            scene: scene_handle,
        });
    }
}