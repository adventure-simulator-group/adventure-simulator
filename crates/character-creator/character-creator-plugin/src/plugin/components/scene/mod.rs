use bevy::asset::RenderAssetUsages;
use bevy::ecs::hierarchy::ChildOf;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::{skinning::SkinnedMesh, Indices};
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use crate::plugin::components::InCharacterScene;
use crate::plugin::components::OrbitalCamera;
use crate::plugin::resources::{Animations, SceneHandle};

const MODEL_PATH: &str = "models/animated/Michelle.glb";
const BONE_RADIUS: f32 = 0.05;
const BONE_SEGMENT_RESOLUTION: usize = 8;
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

        // Model
        let scene_handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_PATH));
        let model = commands
            .spawn((
                SceneRoot(scene_handle.clone()),
                Transform::default(),
                GlobalTransform::default(),
                InCharacterScene,
            ))
            .id();
        commands.insert_resource(SceneHandle {
            scene: scene_handle,
        });

        // Camera
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 1.0, 4.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y),
            OrbitalCamera {
                focus: Some(model),
                ..default()
            },
        ));

        // Plane
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(500000.0, 500000.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
            crate::plugin::components::Floor,
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
    }

    pub fn swap_mesh_for_cylinders(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        skinned_meshes: Query<(Entity, &SkinnedMesh)>,
        parents: Query<&ChildOf>,
        mut already_replaced: Local<bool>,
    ) {
        if !Self::DISPLAY_BONE_CYLINDERS || *already_replaced {
            return;
        }

        for (entity, _skinned) in skinned_meshes.iter() {
            info!("swap_mesh_for_cylinders called for entity {:?}", entity);
        }

        let Ok((entity, skinned)) = skinned_meshes.iter().next().map(|(e, s)| (e, s)).ok_or(())
        else {
            return;
        };

        info!(
            "Swapping mesh for cylinders for entity {:?} with {} joints",
            entity,
            skinned.joints.iter().len()
        );

        let mut joint_indices = HashMap::new();
        for (index, joint) in skinned.joints.iter().enumerate() {
            joint_indices.insert(*joint, index);
        }

        // Unit cylinder mesh (Y=0 to Y=1)
        let mesh = Self::build_unit_cylinder_mesh(BONE_RADIUS, BONE_SEGMENT_RESOLUTION);
        let mesh_handle = meshes.add(mesh);

        for (child_index, joint_entity) in skinned.joints.iter().enumerate() {
            let Ok(parent) = parents.get(*joint_entity) else {
                continue;
            };
            let parent_entity = parent.0;
            let Some(&_parent_index) = joint_indices.get(&parent_entity) else {
                continue;
            };

            // Generate a color based on child_index
            let r = ((child_index * 13) % 256) as f32 / 255.0;
            let g = ((child_index * 57) % 256) as f32 / 255.0;
            let b = ((child_index * 91) % 256) as f32 / 255.0;
            let material_handle = materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                ..default()
            });

            commands
                .spawn((
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    BoneSegment {
                        joint: *joint_entity,
                        parent: parent_entity,
                        index: child_index,
                    },
                    InCharacterScene,
                ))
                .observe(|trigger: On<Pointer<Over>>, mut commands: Commands| {
                    let entity = trigger.entity;
                    info!("Hovered bone entity {:?}", entity);
                    commands.entity(entity).insert(HoveredBone);
                })
                .observe(|trigger: On<Pointer<Out>>, mut commands: Commands| {
                    let entity = trigger.entity;
                    info!("Unhovered bone entity {:?}", entity);
                    commands.entity(entity).remove::<HoveredBone>();
                });
        }

        commands
            .entity(entity)
            .insert(Visibility::Hidden)
            .insert(Name::new("OriginalCharacterMesh"));

        *already_replaced = true;
    }

    fn build_unit_cylinder_mesh(radius: f32, resolution: usize) -> Mesh {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let resolution = resolution.max(3);
        for ring in 0..resolution {
            let angle = TAU * (ring as f32 / resolution as f32);
            let (sin, cos) = angle.sin_cos();
            let normal = Vec3::new(cos, 0.0, sin);

            for step in 0..=1 {
                let t = step as f32;
                let position = Vec3::new(radius * cos, t, radius * sin);
                positions.push(position.to_array());
                normals.push(normal.to_array());
            }
        }

        for ring in 0..resolution {
            let next = (ring + 1) % resolution;
            let bottom_current = (ring * 2) as u32;
            let top_current = bottom_current + 1;
            let bottom_next = (next * 2) as u32;
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

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    pub fn update_bone_segments(
        globals: Query<&GlobalTransform>,
        mut segments: Query<(&mut Transform, &BoneSegment)>,
    ) {
        for (mut transform, segment) in segments.iter_mut() {
            let Ok(parent_global) = globals.get(segment.parent) else {
                continue;
            };
            let Ok(joint_global) = globals.get(segment.joint) else {
                continue;
            };

            let start = parent_global.translation();
            let end = joint_global.translation();
            let direction = end - start;
            let length = direction.length();

            if length < MIN_BONE_LENGTH {
                transform.scale = Vec3::ZERO;
                continue;
            }

            let axis = direction / length;
            let rotation = if axis.length_squared() < f32::EPSILON {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_arc(Vec3::Y, axis)
            };

            transform.translation = start;
            transform.rotation = rotation;
            transform.scale = Vec3::new(1.0, length, 1.0);
        }
    }

    pub fn draw_bone_labels(
        mut commands: Commands,
        globals: Query<&GlobalTransform>,
        segments: Query<(&GlobalTransform, &BoneSegment), With<HoveredBone>>,
        labels: Query<Entity, With<BoneIndexLabel>>,
    ) {
        if !Self::DISPLAY_BONE_CYLINDERS {
            return;
        }

        // Despawn old labels
        for entity in labels.iter() {
            commands.entity(entity).despawn();
        }

        for (global, segment) in segments.iter() {
            let Ok(parent_global) = globals.get(segment.parent) else {
                continue;
            };
            let Ok(joint_global) = globals.get(segment.joint) else {
                continue;
            };

            let start = parent_global.translation();
            let end = joint_global.translation();
            let midpoint = (start + end) * 0.5;
            let pos = midpoint + Vec3::Y * 0.05;

            commands.spawn((
                Text(segment.index.to_string()),
                TextColor(Color::WHITE),
                TextFont::from_font_size(30.0),
                Transform::from_translation(pos),
                BoneIndexLabel,
                InCharacterScene,
            ));
        }
    }
}

#[derive(Component)]
pub struct BoneSegment {
    pub joint: Entity,
    pub parent: Entity,
    pub index: usize,
}

#[derive(Component)]
pub struct BoneIndexLabel;

#[derive(Component)]
pub struct HoveredBone;
