use bevy::asset::RenderAssetUsages;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_ecs::hierarchy::ChildOf;
use bevy_mesh::{skinning::SkinnedMesh, Indices, VertexAttributeValues};

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use crate::plugins::animation_player::resources::{Animations, SceneHandle};

const MODEL_PATH: &str = "models/animated/Michelle.glb";
const BONE_RADIUS: f32 = 0.01;
const BONE_SEGMENT_RESOLUTION: usize = 12;
const MIN_BONE_LENGTH: f32 = 0.001;

pub struct Scene;

impl Scene {
    /// Set to `false` to re-enable rendering the original mesh imported from the GLB.
    pub const DISPLAY_BONE_CYLINDERS: bool = true;

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

    pub fn swap_mesh_for_cylinders(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        skinned_meshes: Query<(Entity, &SkinnedMesh), Added<SkinnedMesh>>,
        globals: Query<&GlobalTransform>,
        parents: Query<&ChildOf>,
        mut already_replaced: Local<bool>,
    ) {
        if !Self::DISPLAY_BONE_CYLINDERS || *already_replaced {
            return;
        }

        let Some((entity, skinned)) = skinned_meshes.iter().next() else {
            return;
        };

        let Some(mesh) = Self::build_bone_cylinder_mesh(skinned, &globals, &parents) else {
            return;
        };

        let mesh_handle = meshes.add(mesh);
        let material_handle = materials.add(Color::srgb(0.85, 0.4, 0.2));

        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            skinned.clone(),
            Name::new("BoneCylinderDebugMesh"),
        ));

        commands
            .entity(entity)
            .insert(Visibility::Hidden)
            .insert(Name::new("OriginalCharacterMesh"));

        *already_replaced = true;
    }

    fn build_bone_cylinder_mesh(
        skinned_mesh: &SkinnedMesh,
        globals: &Query<&GlobalTransform>,
        parents: &Query<&ChildOf>,
    ) -> Option<Mesh> {
        let mut joint_indices = HashMap::new();
        for (index, joint) in skinned_mesh.joints.iter().enumerate() {
            joint_indices.insert(*joint, index as u16);
        }
    
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut joint_index_data = Vec::new();
        let mut joint_weight_data = Vec::new();
        let mut indices = Vec::new();
    
        for (child_index, joint_entity) in skinned_mesh.joints.iter().enumerate() {
            let Ok(parent) = parents.get(*joint_entity) else {
                continue;
            };
            let parent_entity = parent.0;
            let Some(&parent_index) = joint_indices.get(&parent_entity) else {
                continue;
            };
            let Ok(parent_transform) = globals.get(parent_entity) else {
                continue;
            };
            let Ok(child_transform) = globals.get(*joint_entity) else {
                continue;
            };
    
            let start = parent_transform.translation();
            let end = child_transform.translation();
            let direction = end - start;
            let length = direction.length();
            if length < MIN_BONE_LENGTH {
                continue;
            }
            let axis = direction / length;
            let rotation = if axis.length_squared() < f32::EPSILON {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_arc(Vec3::Y, axis)
            };
    
            let base_index = positions.len() as u32;
            let resolution = BONE_SEGMENT_RESOLUTION.max(3);
            for ring in 0..resolution {
                let angle = TAU * (ring as f32 / resolution as f32);
                let (sin, cos) = angle.sin_cos();
                let normal = rotation * Vec3::new(cos, 0.0, sin);
    
                for step in 0..=1 {
                    let t = step as f32;
                    let local = Vec3::new(BONE_RADIUS * cos, t * length, BONE_RADIUS * sin);
                    let position = rotation * local + start;
    
                    positions.push(position.to_array());
                    normals.push(normal.to_array());
    
                    let child_index = child_index as u16;
                    joint_index_data.push([parent_index, child_index, 0, 0]);
                    joint_weight_data.push([1.0 - t, t, 0.0, 0.0]);
                }
            }
    
            for ring in 0..resolution {
                let next = (ring + 1) % resolution;
                let bottom_current = base_index + (ring * 2) as u32;
                let top_current = bottom_current + 1;
                let bottom_next = base_index + (next * 2) as u32;
                let top_next = bottom_next + 1;
    
                indices.extend_from_slice(&[
                    bottom_current,
                    top_current,
                    bottom_next,
                    top_current,
                    top_next,
                    bottom_next,
                ]);
            }
        }
    
        if positions.is_empty() {
            return None;
        }
    
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(joint_index_data),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            VertexAttributeValues::Float32x4(joint_weight_data),
        );
        mesh.insert_indices(Indices::U32(indices));
    
        Some(mesh)
    }    
}
