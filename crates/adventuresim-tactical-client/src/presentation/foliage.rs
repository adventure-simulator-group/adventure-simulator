use super::*;

pub(super) fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        lod: if ground_foliage {
            Vec4::new(24.0, 120.0, 0.18, 1.0)
        } else {
            Vec4::ZERO
        },
        // Root brightness, meadow colour variation, normal up-bias, and
        // whether nearby player movement affects this material.
        shading: if ground_foliage {
            Vec4::new(0.42, 0.13, 0.76, 1.0)
        } else {
            Vec4::new(0.55, 0.08, 0.28, 0.0)
        },
    }
}

pub(super) fn update_grass_interaction(
    time: Res<Time>,
    interactors: Query<&GlobalTransform, With<GrassInteractor>>,
    mut state: ResMut<GrassInteractionState>,
    mut materials: ResMut<Assets<TacticalFoliageMaterial>>,
) {
    let Some(position) = interactors.iter().next().map(GlobalTransform::translation) else {
        state.previous_position = None;
        state.smoothed_velocity = Vec3::ZERO;
        for (_, material) in materials.iter_mut() {
            material.interaction = Vec4::ZERO;
            material.interaction_motion = Vec4::ZERO;
        }
        return;
    };
    let delta_seconds = time.delta_secs().max(1.0 / 240.0);
    let velocity = state
        .previous_position
        .map(|previous| ((position - previous) / delta_seconds).clamp_length_max(8.0))
        .unwrap_or_default();
    let response = 1.0 - (-delta_seconds * 10.0).exp();
    state.smoothed_velocity = state.smoothed_velocity.lerp(velocity, response);
    state.previous_position = Some(position);
    let speed = state.smoothed_velocity.length();
    for (_, material) in materials.iter_mut() {
        if material.shading.w <= 0.5 {
            continue;
        }
        material.interaction = position.extend(1.35);
        material.interaction_motion = Vec4::new(
            state.smoothed_velocity.x,
            state.smoothed_velocity.y,
            state.smoothed_velocity.z,
            (0.7 + speed * 0.11).clamp(0.7, 1.35),
        );
    }
}

pub(super) fn spawn_ground_foliage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalFoliageMaterial>,
    source: Entity,
    scene_id: &SceneId,
    terrain: &SceneTerrain,
    environment: &SceneEnvironment,
) {
    let grass_color = if environment.weather.snow_cover_bps >= 5_000 {
        Color::srgb_u8(155, 164, 137)
    } else if environment.cultivation_bps >= 4_000 {
        Color::srgb_u8(142, 133, 61)
    } else {
        Color::srgb_u8(82, 119, 45)
    };
    let grass_mesh = meshes.add(grass_patch_mesh(grass_color));
    let understory_mesh = meshes.add(if environment.weather.snow_cover_bps >= 5_000 {
        foliage_clump_mesh(0.72, 0.92, Color::srgb_u8(130, 144, 119), 3)
    } else if environment.wetland_bps >= 3_000 {
        foliage_clump_mesh(0.42, 1.35, Color::srgb_u8(75, 112, 58), 4)
    } else {
        foliage_clump_mesh(0.9, 1.05, Color::srgb_u8(52, 91, 43), 3)
    });
    let grass_material = materials.add(foliage_material(
        0.16 + bps(environment.weather.wind_speed_bps) * 0.36,
        true,
    ));
    let understory_material = materials.add(foliage_material(
        0.1 + bps(environment.weather.wind_speed_bps) * 0.24,
        true,
    ));
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    let canopy = bps(environment.canopy_bps);
    let water = bps(environment.water_bps);
    let wetland = bps(environment.wetland_bps);
    let cultivation = bps(environment.cultivation_bps);
    let snow = bps(environment.weather.snow_cover_bps);
    let grass_chance = (0.96 - canopy * 0.16 - water * 0.88 + cultivation * 0.04).clamp(0.06, 0.98)
        * (1.0 - snow * 0.36);
    let understory_chance = (canopy * 0.16 + wetland * 0.22).clamp(0.0, 0.24);
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    // Each instance is a forty-nine-blade patch whose footprint overlaps its
    // neighbours. This keeps the entity count bounded while producing the
    // near-continuous oblique coverage expected from grassland.
    let spacing = 1.0;
    let count_x = (terrain.width() / spacing).floor() as i32;
    let count_z = (terrain.depth() / spacing).floor() as i32;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let choose = unit_hash(hash);
            let layer = if choose < understory_chance {
                Some(FoliageLayer::Understory)
            } else if choose < understory_chance + grass_chance {
                Some(FoliageLayer::Grass)
            } else {
                None
            };
            let Some(layer) = layer else { continue };
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.72) * spacing;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.72) * spacing;
            let Some(height) = terrain.height_at(Vec2::new(world_x, world_z)) else {
                continue;
            };
            if terrain
                .normal_at(Vec2::new(world_x, world_z))
                .is_none_or(|normal| normal.y < 0.72)
            {
                continue;
            }
            let scale = 0.72 + unit_hash(splitmix64(hash ^ 0x8c0a_3c95)) * 0.58;
            let (mesh, material) = match layer {
                FoliageLayer::Grass => (grass_mesh.clone(), grass_material.clone()),
                FoliageLayer::Understory => (understory_mesh.clone(), understory_material.clone()),
            };
            commands.spawn((
                Name::new(match layer {
                    FoliageLayer::Grass => "Tactical grass clump",
                    FoliageLayer::Understory => "Tactical understory clump",
                }),
                FoliageOf(source),
                layer,
                NotShadowCaster,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                VisibilityRange::abrupt(
                    0.0,
                    if layer == FoliageLayer::Grass {
                        130.0
                    } else {
                        92.0
                    },
                ),
                Transform::from_xyz(world_x, height, world_z)
                    .with_rotation(Quat::from_rotation_y(
                        unit_hash(hash) * core::f32::consts::TAU,
                    ))
                    .with_scale(Vec3::splat(scale)),
            ));
        }
    }
}

pub(super) fn on_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    scenes: Query<(&SceneId, &SceneTerrain)>,
    foliage: Query<&FoliageOf>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
) -> Result {
    if foliage.iter().any(|source| source.0 == event.entity) {
        return Ok(());
    }
    let environment = environments.get(event.entity)?;
    let (scene_id, terrain) = scenes.get(event.entity)?;
    spawn_ground_foliage(
        &mut commands,
        &mut meshes,
        &mut foliage_materials,
        event.entity,
        scene_id,
        terrain,
        environment,
    );
    Ok(())
}

pub(super) fn grass_patch_mesh(color: Color) -> Mesh {
    let blades = (0..49)
        .map(|index| {
            let row = index / 7;
            let column = index % 7;
            let hash = splitmix64(index as u64 ^ 0x8d12_6f4a_0bc3_7791);
            let jitter_x = (unit_hash(hash) - 0.5) * 0.07;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * 0.07;
            let scale = 0.68 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.36;
            (
                (column as f32 - 3.0) * 0.18 + jitter_x,
                (row as f32 - 3.0) * 0.18 + jitter_z,
                scale,
            )
        })
        .collect::<Vec<_>>();
    foliage_patch_mesh(0.045, 0.82, color, 2, &blades)
}

pub(super) fn foliage_clump_mesh(width: f32, height: f32, color: Color, planes: usize) -> Mesh {
    foliage_patch_mesh(width, height, color, planes, &[(0.0, 0.0, 1.0)])
}

pub(super) fn foliage_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    planes: usize,
    tufts: &[(f32, f32, f32)],
) -> Mesh {
    let mut positions = Vec::with_capacity(tufts.len() * planes * 5);
    let mut normals = Vec::with_capacity(tufts.len() * planes * 5);
    let mut uvs = Vec::with_capacity(tufts.len() * planes * 5);
    let mut blade_roots = Vec::with_capacity(tufts.len() * planes * 5);
    let mut colors = Vec::with_capacity(tufts.len() * planes * 5);
    let mut indices = Vec::with_capacity(tufts.len() * planes * 9);
    let linear = color.to_linear().to_f32_array();
    for (tuft_index, &(offset_x, offset_z, tuft_scale)) in tufts.iter().enumerate() {
        let centre = Vec3::new(offset_x, 0.0, offset_z);
        let blade_threshold = unit_hash(splitmix64(tuft_index as u64 ^ 0x3d91_02ea_61b8_7c45));
        let blade_color = [linear[0], linear[1], linear[2], blade_threshold];
        for plane in 0..planes {
            let angle = plane as f32 * core::f32::consts::PI / planes as f32;
            let direction = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * tuft_scale * 0.5;
            let shoulder = direction * 0.48;
            let tip = Vec3::Y * height * tuft_scale;
            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                (centre - direction).to_array(),
                (centre + direction).to_array(),
                (centre - shoulder + tip * 0.72).to_array(),
                (centre + shoulder + tip * 0.72).to_array(),
                (centre + tip).to_array(),
            ]);
            let normal = Vec3::Y.cross(direction).normalize_or_zero().to_array();
            normals.extend_from_slice(&[normal; 5]);
            uvs.extend_from_slice(&[
                [0.0, 0.0],
                [1.0, 0.0],
                [0.25, 0.72],
                [0.75, 0.72],
                [0.5, 1.0],
            ]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 5]);
            colors.extend_from_slice(&[blade_color; 5]);
            indices.extend_from_slice(&[
                base,
                base + 1,
                base + 3,
                base,
                base + 3,
                base + 2,
                base + 2,
                base + 3,
                base + 4,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_roots);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalFoliageMaterial {
    #[uniform(0)]
    wind: Vec4,
    #[uniform(0)]
    interaction: Vec4,
    #[uniform(0)]
    interaction_motion: Vec4,
    #[uniform(0)]
    lod: Vec4,
    #[uniform(0)]
    shading: Vec4,
}

impl Material for TacticalFoliageMaterial {
    fn vertex_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FoliageLayer {
    Grass,
    Understory,
}

/// Marks the locally controlled character whose movement bends nearby grass.
#[derive(Component)]
pub(crate) struct GrassInteractor;

#[derive(Resource, Default)]
pub(in crate::presentation) struct GrassInteractionState {
    previous_position: Option<Vec3>,
    smoothed_velocity: Vec3,
}

#[derive(Component)]
pub(in crate::presentation) struct FoliageOf(pub(in crate::presentation) Entity);

const FOLIAGE_SHADER: &str = "shaders/tactical_foliage.wgsl";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_clumps_carry_root_to_tip_wind_weights() {
        let mesh = foliage_clump_mesh(0.5, 0.8, Color::WHITE, 3);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("foliage mesh must carry float2 UV wind weights");
        };
        assert!(uvs.iter().any(|uv| uv[1] == 0.0));
        assert!(uvs.iter().any(|uv| uv[1] == 1.0));
        assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    #[test]
    fn grass_patches_pack_forty_nine_thin_blades_into_each_instance() {
        let mesh = grass_patch_mesh(Color::WHITE);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(positions.len(), 49 * 2 * 5);
        let Some(VertexAttributeValues::Float32x2(roots)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry stable blade roots");
        };
        assert_eq!(roots.len(), positions.len());
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.lod, Vec4::new(24.0, 120.0, 0.18, 1.0));
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.lod, Vec4::ZERO);
        assert_eq!(crown.shading.w, 0.0);
    }

    #[test]
    fn local_interactor_position_reaches_only_ground_foliage_materials() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<Assets<TacticalFoliageMaterial>>();
        app.init_resource::<GrassInteractionState>();
        app.add_systems(Update, update_grass_interaction);
        let (grass, crown) = {
            let mut materials = app
                .world_mut()
                .resource_mut::<Assets<TacticalFoliageMaterial>>();
            (
                materials.add(foliage_material(0.3, true)),
                materials.add(foliage_material(0.3, false)),
            )
        };
        app.world_mut().spawn((
            GrassInteractor,
            GlobalTransform::from_translation(Vec3::new(3.0, 1.0, -2.0)),
        ));
        app.update();

        let materials = app.world().resource::<Assets<TacticalFoliageMaterial>>();
        assert_eq!(
            materials.get(&grass).unwrap().interaction,
            Vec4::new(3.0, 1.0, -2.0, 1.35)
        );
        assert_eq!(materials.get(&crown).unwrap().interaction, Vec4::ZERO);
    }
}
