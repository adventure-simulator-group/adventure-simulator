use bevy::prelude::*;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use distance_field::generator::Generator;
use distance_field_plugin::{
    components::{DistanceFieldComponent, BoneIndexFieldComponent, BoneWeightFieldComponent, SdfShapeComponent, StaticSdf},
    SdfBox, SdfShape,
};
use marching_cubes::MarchingCubes;

const GRID_SIZE: usize = 36;
const ISO_LEVEL: f32 = 0.0;

#[derive(Resource)]
pub struct MarchingCubesUIState {
    radius: f32,
    min_radius: f32,
    max_radius: f32,
    min_voxel_size: f32,
    max_voxel_size: f32,
    pub selected_joint: u32,
    pub joint_translation: Vec3,
    pub joint_rotation: Vec3,
    pub joint_scale: Vec3,
}

impl Default for MarchingCubesUIState {
    fn default() -> Self {
        let voxel_size = 0.12;
        let min_radius = GRID_SIZE as f32 * voxel_size * 0.1;
        let max_radius = GRID_SIZE as f32 * voxel_size * 0.6;
        let radius = GRID_SIZE as f32 * voxel_size * 0.35;

        Self {
            radius,
            min_radius,
            max_radius,
            min_voxel_size: 0.05,
            max_voxel_size: 0.5,
            selected_joint: 1, // Torso by default
            joint_translation: Vec3::ZERO,
            joint_rotation: Vec3::ZERO,
            joint_scale: Vec3::ONE,
        }
    }
}

#[derive(Resource, Clone)]
pub struct MarchingCubesMeshHandle(Handle<Mesh>);

#[derive(Component)]
pub struct SkeletonRoot;

#[derive(Component, Default, Clone)]
pub struct VirtualBindSkeleton {
    pub local_transforms: Vec<Transform>,
    pub parents: Vec<Option<usize>>,
}

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<MarchingCubesUIState>()
            .add_systems(Startup, Self::setup)
            .add_systems(Update, (
                Self::update_marching_cubes_mesh, 
                Self::apply_gltf_skinned_mesh_to_sdf,
                Self::recalculate_virtual_bind_skeleton,
            ));
    }
}

impl MarchingCubesPlugin {
    fn setup(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        inverse_bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        ui_state: Res<MarchingCubesUIState>,
    ) {
        // Create Mesh Handle
        let mesh = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        let mesh_handle = meshes.add(mesh);

        let material_handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            metallic: 0.05,
            perceptual_roughness: 0.4,
            ..default()
        });

        // Start async generation
        let generated = pollster::block_on(Generator::generate());

        // Use Generated SDF or Fallback
        let (df_comp, b_idx_comp, b_weight_comp) = match generated {
            Ok((df, idx, weight)) => {
                info!("SDF Generation Successful: {:?}", df.dimensions());
                (
                    DistanceFieldComponent(df),
                    BoneIndexFieldComponent(idx),
                    BoneWeightFieldComponent(weight),
                )
            }
            Err(e) => {
                error!("SDF Generation Failed: {:?}", e);
                // Fallback
                (
                    DistanceFieldComponent::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12),
                    BoneIndexFieldComponent(distance_field::BoneIndexField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12, [0, 0, 0, 0])),
                    BoneWeightFieldComponent(distance_field::BoneWeightField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12, [0.0, 0.0, 0.0, 0.0])),
                )
            }
        };

        // The actual SkinnedMesh will be assigned once `Michelle.glb` loads via `apply_gltf_skinned_mesh_to_sdf`.

        // Spawn Volume
        let _volume_entity = commands
            .spawn((
                df_comp,
                b_idx_comp,
                b_weight_comp,
                StaticSdf,
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material_handle.clone()),
                Transform::from_xyz(0.0, 2.0, 0.0), // Raised up
            ))
            .id();

        // Spawn a SECOND independent SDF volume to verify multi-SDF support
        let volume_entity_2 = commands
            .spawn((
                DistanceFieldComponent::new(GRID_SIZE / 2, GRID_SIZE / 2, GRID_SIZE / 2, 0.15),
                BoneIndexFieldComponent(distance_field::BoneIndexField::new(GRID_SIZE / 2, GRID_SIZE / 2, GRID_SIZE / 2, 0.15, [0, 0, 0, 0])),
                BoneWeightFieldComponent(distance_field::BoneWeightField::new(GRID_SIZE / 2, GRID_SIZE / 2, GRID_SIZE / 2, 0.15, [0.0, 0.0, 0.0, 0.0])),
                Mesh3d(mesh_handle.clone()),
                Transform::from_xyz(5.0, 3.0, 0.0), // Raised up and offset
            ))
            .id();

        // We need to create a new mesh asset for the second volume
        let mesh_2 = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        let mesh_handle_2 = meshes.add(mesh_2);

        // Re-spawn volume 2 with correct mesh handle
        commands.entity(volume_entity_2).insert((
            Mesh3d(mesh_handle_2),
            MeshMaterial3d(material_handle.clone()),
        ));

        // Add shapes to second volume
        commands.entity(volume_entity_2).with_children(|parent| {
            parent.spawn((
                SdfShapeComponent::from(SdfBox {
                    size: Vec3::splat(0.6),
                }),
                Transform::IDENTITY,
            ));
            parent.spawn((
                SdfShapeComponent::from(SdfBox {
                    size: Vec3::new(0.2, 1.0, 0.2),
                }), // Make it smaller to fit
                Transform::from_xyz(0.5, 0.0, 0.5),
            ));
        });

        commands.spawn((
            PointLight {
                intensity: 4000.0,
                range: 40.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_xyz(8.0, 12.0, 8.0),
        ));
    }

    pub fn marching_cubes_ui(
        mut contexts: EguiContexts,
        mut ui_state: ResMut<MarchingCubesUIState>,
        sk_query: Query<&SkinnedMesh>,
        mut shapes: Query<&mut SdfShapeComponent>,
        mut fields: Query<&mut DistanceFieldComponent>,
        mut virtual_skeletons: Query<&mut VirtualBindSkeleton>,
        mut frames: Local<u32>,
    ) {
        if *frames < 5 {
            *frames += 1;
            return;
        }

        if let Ok(ctx) = contexts.ctx_mut() {
            egui::Window::new("Marching Cubes")
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.label("Sphere Radius");
                    let mut current = ui_state.radius;
                    let slider =
                        egui::Slider::new(&mut current, ui_state.min_radius..=ui_state.max_radius)
                            .text("meters");
                    if ui.add(slider).changed() {
                        ui_state.radius = current;
                        for mut shape in shapes.iter_mut() {
                            if let SdfShapeComponent(SdfShape::Sphere(sphere)) = &mut *shape {
                                sphere.radius = current;
                            }
                        }
                    }

                    ui.separator();

                    ui.label("Voxel Size");
                    let mut current_voxel =
                        fields.iter().next().map(|c| c.voxel_size).unwrap_or(0.12);

                    let slider_voxel = egui::Slider::new(
                        &mut current_voxel,
                        ui_state.min_voxel_size..=ui_state.max_voxel_size,
                    )
                    .text("meters");
                    if ui.add(slider_voxel).changed() {
                        for mut field in fields.iter_mut() {
                            field.voxel_size = current_voxel;
                        }
                    }

                    ui.separator();
                    // Just cleanly check if the SDf mesh has been bound to a skinned mesh yet.
                    let mut is_bound = false;
                    for _ in sk_query.iter() {
                        is_bound = true;
                    }

                    if is_bound {
                        ui.separator();
                        ui.label("✅ SDF Bound to Michelle.glb Rig!");
                        ui.separator();

                        if let Some(mut vs) = virtual_skeletons.iter_mut().next() {
                            ui.label("Hierarchical Bind Pose Adjust");

                            ui.horizontal(|ui| {
                                ui.label("Joint Index:");
                                egui::ComboBox::from_id_salt("joint_selector")
                                    .selected_text(format!("{}", ui_state.selected_joint))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut ui_state.selected_joint, 3, "Left Arm (3)");
                                        ui.selectable_value(&mut ui_state.selected_joint, 33, "Right Arm (33)");
                                        ui.selectable_value(&mut ui_state.selected_joint, 56, "Left Leg (56)");
                                        ui.selectable_value(&mut ui_state.selected_joint, 61, "Right Leg (61)");
                                        ui.selectable_value(&mut ui_state.selected_joint, 1, "Torso (1)");
                                        ui.selectable_value(&mut ui_state.selected_joint, 6, "Head (6)");
                                    });
                            });

                            let index = ui_state.selected_joint as usize;
                            if index < vs.local_transforms.len() {
                                let mut local = vs.local_transforms[index];
                                let mut changed = false;

                                ui.horizontal(|ui| {
                                    ui.label("Local Pos:");
                                    changed |= ui.add(egui::DragValue::new(&mut local.translation.x).speed(0.1)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut local.translation.y).speed(0.1)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut local.translation.z).speed(0.1)).changed();
                                });
                                
                                let mut euler = local.rotation.to_euler(EulerRot::XYZ);
                                ui.horizontal(|ui| {
                                    ui.label("Local Rot (rad):");
                                    changed |= ui.add(egui::DragValue::new(&mut euler.0).speed(0.01)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut euler.1).speed(0.01)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut euler.2).speed(0.01)).changed();
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Local Scl:");
                                    changed |= ui.add(egui::DragValue::new(&mut local.scale.x).speed(0.01)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut local.scale.y).speed(0.01)).changed();
                                    changed |= ui.add(egui::DragValue::new(&mut local.scale.z).speed(0.01)).changed();
                                });

                                if changed {
                                    local.rotation = Quat::from_euler(EulerRot::XYZ, euler.0, euler.1, euler.2);
                                    vs.local_transforms[index] = local;
                                }
                            }
                        }

                    } else {
                        ui.label("⏳ Waiting for Michelle.glb bones to attach...");
                    }
                });
        }
    }

    fn update_marching_cubes_mesh(
        mut meshes: ResMut<Assets<Mesh>>,
        query: Query<(
            &DistanceFieldComponent,
            &BoneIndexFieldComponent,
            &BoneWeightFieldComponent,
            &Mesh3d
        ), Changed<DistanceFieldComponent>>,
    ) {
        for (distance_field, bone_index_field, bone_weight_field, mesh3d) in query.iter() {
            if let Some(existing) = meshes.get_mut(&mesh3d.0) {
                let mesh_builder = MarchingCubes::generate_mesh(
                    &distance_field.0,
                    &bone_index_field.0,
                    &bone_weight_field.0,
                    ISO_LEVEL,
                    distance_field.voxel_size,
                );
                *existing = mesh_builder.build();
            }
        }
    }

    fn apply_gltf_skinned_mesh_to_sdf(
        mut commands: Commands,
        gltf_meshes: Query<&SkinnedMesh, Without<BoneWeightFieldComponent>>,
        sdf_meshes: Query<Entity, With<BoneWeightFieldComponent>>,
        mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
        parents: Query<&bevy::ecs::hierarchy::ChildOf>,
        mut done: Local<bool>,
    ) {
        if *done { return; }

        if let Some(gltf_skinned_mesh) = gltf_meshes.iter().next() {
            let Some(orig_ibp) = bindposes.get(&gltf_skinned_mesh.inverse_bindposes) else { return; };
            let orig_ibp_vec: Vec<Mat4> = orig_ibp.iter().copied().collect();
            
            // Clone the inverse bind poses so our SDF has its own set of matrices to mutate
            let new_ibp_handle = bindposes.add(SkinnedMeshInverseBindposes::from(orig_ibp_vec.clone()));

            let mut vs = VirtualBindSkeleton {
                local_transforms: vec![Transform::IDENTITY; gltf_skinned_mesh.joints.len()],
                parents: vec![None; gltf_skinned_mesh.joints.len()],
            };

            let joint_map: std::collections::HashMap<Entity, usize> = gltf_skinned_mesh.joints.iter().enumerate().map(|(i, &e)| (e, i)).collect();

            for (i, &joint_entity) in gltf_skinned_mesh.joints.iter().enumerate() {
                if let Ok(parent_comp) = parents.get(joint_entity) {
                    if let Some(&parent_idx) = joint_map.get(&parent_comp.0) {
                        vs.parents[i] = Some(parent_idx);
                    }
                }

                // Initial local transforms calculation: Local = Parent_Global_Inv * Global
                // Global = InverseBindPose.inverse()
                let global = orig_ibp_vec[i].inverse();
                if let Some(p_idx) = vs.parents[i] {
                    let parent_global_inv = orig_ibp_vec[p_idx]; // InverseBindPose IS the parent global inverse!
                    vs.local_transforms[i] = Transform::from_matrix(parent_global_inv * global);
                } else {
                    vs.local_transforms[i] = Transform::from_matrix(global);
                }
            }

            for sdf_entity in sdf_meshes.iter() {
                commands.entity(sdf_entity).insert((
                    SkinnedMesh {
                        joints: gltf_skinned_mesh.joints.clone(),
                        inverse_bindposes: new_ibp_handle.clone(),
                    },
                    vs.clone(),
                ));
            }
            *done = true;
            info!("Successfully bound SDF mesh to Michelle.glb Mixamo rig hierarchically!");
        }
    }

    fn recalculate_virtual_bind_skeleton(
        mut virtual_skeletons: Query<(&VirtualBindSkeleton, &SkinnedMesh), Changed<VirtualBindSkeleton>>,
        mut inverse_bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    ) {
        for (vs, sk) in virtual_skeletons.iter_mut() {
            if let Some(ibp) = inverse_bindposes.get_mut(&sk.inverse_bindposes) {
                let mut globals = vec![Mat4::IDENTITY; vs.local_transforms.len()];
                let mut new_ibps = vec![Mat4::IDENTITY; vs.local_transforms.len()];

                for i in 0..vs.local_transforms.len() {
                    let local = vs.local_transforms[i].to_matrix();
                    if let Some(p_idx) = vs.parents[i] {
                        globals[i] = globals[p_idx] * local;
                    } else {
                        globals[i] = local;
                    }
                    new_ibps[i] = globals[i].inverse();
                }

                *ibp = SkinnedMeshInverseBindposes::from(new_ibps);
            }
        }
    }
}

