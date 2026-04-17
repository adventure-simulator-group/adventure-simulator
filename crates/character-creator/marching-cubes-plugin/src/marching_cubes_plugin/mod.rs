use bevy::prelude::{
    info, error, default, App, Assets, Color, Commands, Component, Entity, Handle, Mesh, Mesh3d, MeshMaterial3d, Plugin, PointLight, Query, Res, ResMut, Resource, StandardMaterial, Startup, Update, Without, Quat, EulerRot, ChildOf, With, Changed, Local,
    Mat4 as BevyMat4, Vec3 as BevyVec3, Transform as BevyTransform,
};
use bevy_mesh::{Indices, VertexAttributeValues};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use distance_field::generator::Generator;
use distance_field_plugin::{
    components::{DistanceFieldComponent, BoneIndexFieldComponent, BoneWeightFieldComponent, SdfShapeComponent, StaticSdf},
    SdfShape,
};
use gpu_runtime::prelude::*;
use gpu_runtime::data::gpu::compute::marching_cubes::{MarchingCubesDefinition, MarchingCubes as GpuMarchingCubes};

const GRID_SIZE: usize = 36;
const ISO_LEVEL: f32 = 0.0;
const MAX_VERTICES: u32 = 1000000;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub indices: [u32; 4],
    pub weights: [f32; 4],
}

#[derive(Resource)]
pub struct GpuMarchingCubesResources {
    pub context: WgpuContext,
    pub mc_definition: MarchingCubesDefinition,
    pub skin_map_def: MapDefinition,
    pub sdf_tex: Texture3D,
    pub idx_tex: Texture3D,
    pub weight_tex: Texture3D,
    pub sampler: Sampler,
    pub output_vertices: Buffer,
    pub output_indirect: Buffer,
    pub output_skinned: Buffer,
}

#[derive(Resource)]
pub struct MarchingCubesUIState {
    radius: f32,
    min_radius: f32,
    max_radius: f32,
    min_voxel_size: f32,
    max_voxel_size: f32,
    pub selected_joint: u32,
    pub joint_translation: BevyVec3,
    pub joint_rotation: BevyVec3,
    pub joint_scale: BevyVec3,
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
            joint_translation: BevyVec3::ZERO,
            joint_rotation: BevyVec3::ZERO,
            joint_scale: BevyVec3::ONE,
        }
    }
}

#[derive(Resource, Clone)]
pub struct MarchingCubesMeshHandle(Handle<Mesh>);

#[derive(Component)]
pub struct SkeletonRoot;

#[derive(Component, Default, Clone)]
pub struct VirtualBindSkeleton {
    pub local_transforms: Vec<BevyTransform>,
    pub parents: Vec<Option<usize>>,
}

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<MarchingCubesUIState>()
            .add_systems(Startup, Self::setup)
            .add_systems(Update, (
                Self::update_marching_cubes_mesh_gpu, 
                Self::apply_gltf_skinned_mesh_to_sdf,
                Self::recalculate_virtual_bind_skeleton,
            ));
    }
}

impl MarchingCubesPlugin {
    fn setup(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        _inverse_bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        _ui_state: Res<MarchingCubesUIState>,
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

        // Start async generation on GPU
        let generated = pollster::block_on(Generator::generate_textures(GRID_SIZE as u32));

        match generated {
            Ok((context, sdf_tex, idx_tex, weight_tex)) => {
                info!("GPU SDF Generation Successful: {:?}", sdf_tex.size);

                let mc_definition = MarchingCubesDefinition::new(&context).expect("Failed to create MC definition");
                
                let skin_map_wgsl = r#"
                    struct Vertex {
                        position: vec4<f32>,
                        normal: vec4<f32>,
                    }

                    struct SkinnedVertex {
                        position: vec4<f32>,
                        normal: vec4<f32>,
                        indices: vec4<u32>,
                        weights: vec4<f32>,
                    }

                    @group(0) @binding(3) var bone_indices: texture_3d<u32>;
                    @group(0) @binding(4) var bone_weights: texture_3d<f32>;
                    @group(0) @binding(5) var samp: sampler;

                    fn map(v: Vertex, voxel_size: f32, grid_size: f32) -> SkinnedVertex {
                        let grid_pos = v.position.xyz;
                        
                        // Nearest neighbor for indices
                        let idx_coord = vec3<u32>(grid_pos + 0.5);
                        let indices = textureLoad(bone_indices, idx_coord, 0);
                        
                        // Linear interpolation for weights
                        let tex_dim = vec3<f32>(textureDimensions(bone_weights));
                        let norm_coord = (grid_pos + 0.5) / tex_dim;
                        let weights = textureSampleLevel(bone_weights, samp, norm_coord, 0.0);
                        
                        // Center the mesh around (0,0,0) in local space
                        let local_pos = (grid_pos - (grid_size / 2.0)) * voxel_size;
                        
                        return SkinnedVertex(
                            vec4<f32>(local_pos, 1.0),
                            v.normal,
                            indices,
                            weights
                        );
                    }
                "#;

                let skin_map_def = MapDefinition::new(skin_map_wgsl.to_string()).expect("Failed to create Skin Map definition");
                let sampler = Sampler::new(&context, None, None, None, None, None).expect("Failed to create sampler");

                let output_vertices = Buffer::new(
                    &context,
                    (MAX_VERTICES as usize * 32) as u64, // Vertex is 32 bytes (vec4 pos, vec4 norm)
                    BufferDefinition::storage().with_label("mc_output_vertices")
                ).expect("Failed to create output_vertices buffer");

                let output_indirect = Buffer::new(
                    &context,
                    16, // Indirect draw is 4 * u32
                    BufferDefinition::storage().with_label("mc_output_indirect").with_copy_src()
                ).expect("Failed to create output_indirect buffer");

                let output_skinned = Buffer::new(
                    &context,
                    (MAX_VERTICES as usize * 64) as u64, // SkinnedVertex is 64 bytes
                    BufferDefinition::storage().with_label("mc_output_skinned")
                ).expect("Failed to create output_skinned buffer");

                commands.insert_resource(GpuMarchingCubesResources {
                    context: context.clone(),
                    mc_definition,
                    skin_map_def,
                    sdf_tex,
                    idx_tex,
                    weight_tex,
                    sampler,
                    output_vertices,
                    output_indirect,
                    output_skinned,
                });

                // Spawn Volume
                commands.spawn((
                    DistanceFieldComponent::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 2.0 / (GRID_SIZE as f32)),
                    BoneIndexFieldComponent(distance_field::BoneIndexField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12, [0, 0, 0, 0])),
                    BoneWeightFieldComponent(distance_field::BoneWeightField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12, [0.0, 0.0, 0.0, 0.0])),
                    StaticSdf,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle.clone()),
                    BevyTransform::from_xyz(0.0, 2.0, 0.0),
                ));
            }
            Err(e) => {
                error!("GPU SDF Generation Failed: {:?}", e);
            }
        }

        commands.spawn((
            PointLight {
                intensity: 4000.0,
                range: 40.0,
                shadows_enabled: true,
                ..default()
            },
            BevyTransform::from_xyz(8.0, 12.0, 8.0),
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

    fn update_marching_cubes_mesh_gpu(
        mut meshes: ResMut<Assets<Mesh>>,
        gpu_resources: Option<Res<GpuMarchingCubesResources>>,
        query: Query<(
            &DistanceFieldComponent,
            &Mesh3d,
            &BevyTransform
        ), Changed<DistanceFieldComponent>>,
    ) {
        let Some(res) = gpu_resources else { return; };
        
        for (distance_field, mesh3d, transform) in query.iter() {
            if let Some(existing) = meshes.get_mut(&mesh3d.0) {
                let grid = distance_field.0.dimensions();
                let threshold = ISO_LEVEL;
                
                // 1. Prepare resources
                let sdf_res = GpuResource::Texture3D(res.sdf_tex.clone());
                let output_vertices = GpuResource::Buffer(res.output_vertices.clone());
                let output_indirect = GpuResource::Buffer(res.output_indirect.clone());
                let output_skinned = GpuResource::Buffer(res.output_skinned.clone());

                // 2. Execute Marching Cubes
                GpuMarchingCubes::execute(
                    &res.context,
                    &res.mc_definition,
                    &sdf_res,
                    &output_vertices,
                    &output_indirect,
                    (grid.0 as u32, grid.1 as u32, grid.2 as u32),
                    threshold,
                    MAX_VERTICES,
                ).expect("Failed to execute GpuMarchingCubes");

                let mut map_params = PassParameters::new();
                map_params.insert("bone_indices", res.idx_tex.clone());
                map_params.insert("bone_weights", res.weight_tex.clone());
                map_params.insert("samp", res.sampler.clone());
                map_params.insert("voxel_size", distance_field.voxel_size);
                map_params.insert("grid_size", grid.0 as f32);

                Map::execute_with_parameters(
                    &res.context,
                    &res.skin_map_def,
                    Some(&output_vertices),
                    &output_skinned,
                    Some(map_params),
                ).expect("Failed to execute skin map");

                // 4. Readback
                let skinned_verts = pollster::block_on(output_skinned.read::<SkinnedVertex>(&res.context)).expect("Failed to readback skinned vertices");
                let indirect_data = pollster::block_on(output_indirect.read::<u32>(&res.context)).expect("Failed to readback indirect data");
                let vertex_count = indirect_data[0] as usize;

                // 5. Update Bevy Mesh
                let mut positions = Vec::with_capacity(vertex_count);
                let mut normals = Vec::with_capacity(vertex_count);
                let mut indices_attr = Vec::with_capacity(vertex_count);
                let mut weights_attr = Vec::with_capacity(vertex_count);

                for i in 0..vertex_count {
                    let v = skinned_verts[i];
                    positions.push([v.position[0], v.position[1], v.position[2]]);
                    normals.push([v.normal[0], v.normal[1], v.normal[2]]);
                    indices_attr.push([v.indices[0] as u16, v.indices[1] as u16, v.indices[2] as u16, v.indices[3] as u16]);
                    weights_attr.push([v.weights[0], v.weights[1], v.weights[2], v.weights[3]]);
                }

                existing.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                existing.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                existing.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(indices_attr));
                existing.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights_attr);
                
                // Indices (simple triangle list)
                let indices: Vec<u32> = (0..vertex_count as u32).collect();
                existing.insert_indices(Indices::U32(indices));
            }
        }
    }

    fn apply_gltf_skinned_mesh_to_sdf(
        mut commands: Commands,
        gltf_meshes: Query<&SkinnedMesh, Without<BoneWeightFieldComponent>>,
        sdf_meshes: Query<Entity, With<BoneWeightFieldComponent>>,
        mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
        parents: Query<&ChildOf>,
        mut done: Local<bool>,
    ) {
        if *done { return; }

        if let Some(gltf_skinned_mesh) = gltf_meshes.iter().next() {
            let Some(orig_ibp) = bindposes.get(&gltf_skinned_mesh.inverse_bindposes) else { return; };
            let orig_ibp_vec: Vec<BevyMat4> = orig_ibp.iter().copied().collect();
            
            // Clone the inverse bind poses so our SDF has its own set of matrices to mutate
            let new_ibp_handle = bindposes.add(SkinnedMeshInverseBindposes::from(orig_ibp_vec.clone()));

            let mut vs = VirtualBindSkeleton {
                local_transforms: vec![BevyTransform::IDENTITY; gltf_skinned_mesh.joints.len()],
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
                    vs.local_transforms[i] = BevyTransform::from_matrix(parent_global_inv * global);
                } else {
                    vs.local_transforms[i] = BevyTransform::from_matrix(global);
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
                let mut globals = vec![BevyMat4::IDENTITY; vs.local_transforms.len()];
                let mut new_ibps = vec![BevyMat4::IDENTITY; vs.local_transforms.len()];

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

