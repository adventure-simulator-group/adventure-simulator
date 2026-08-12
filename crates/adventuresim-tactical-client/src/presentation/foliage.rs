use super::*;

const GRASS_PATCH_GRID_SIDE: usize = 27;
const GRASS_PATCH_SPACING: f32 = 3.2;
const GRASS_BLADE_SPACING: f32 = 0.135;
const GRASS_FAR_GRID_COORDINATES: [usize; 12] = [0, 2, 5, 7, 9, 12, 14, 17, 19, 21, 24, 26];
const DRY_LEAF_PASSES_PER_SAMPLE: u64 = 3;
const TWIG_PASSES_PER_SAMPLE: u64 = 2;
const DRY_LEAF_MESH_VARIANTS: u64 = 4;
const TWIG_MESH_VARIANTS: u64 = 3;

#[derive(Resource, Default)]
pub(in crate::presentation) struct HazelPresentationCache {
    branches: Option<Handle<Mesh>>,
    cambered_leaves: Option<Handle<Mesh>>,
    leaf_cards: Option<Handle<Mesh>>,
    bark: Option<Handle<StandardMaterial>>,
    leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
}

pub(super) fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        // Root brightness, meadow colour variation, normal up-bias, and
        // whether nearby player movement affects this material.
        shading: if ground_foliage {
            Vec4::new(0.42, 0.13, 0.76, 1.0)
        } else {
            Vec4::new(0.55, 0.08, 0.28, 0.0)
        },
        // Curved ribbon geometry, edge-on view thickening, authored lean, and
        // reserved future shaping control. Understory cards retain the older
        // crossed-plane deformation path.
        shape: Vec4::ZERO,
    }
}

fn grass_material(
    wind_scale: f32,
    lod: GrassMeshLod,
    grass_density: f32,
) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        // The far mesh retains 144 of the near mesh's 729 blades.
        // Widening by the square root of that density ratio preserves roughly
        // the same projected coverage without submitting collapsed geometry.
        shape: Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(grass_density)),
        ..foliage_material(wind_scale, true)
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
    standard_materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    hazel_cache: &mut HazelPresentationCache,
    asset_server: &AssetServer,
    scene_id: &SceneId,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    environment: &SceneEnvironment,
) {
    let canopy = bps(environment.canopy_bps);
    let water = bps(environment.water_bps);
    let wetland = bps(environment.wetland_bps);
    let cultivation = bps(environment.cultivation_bps);
    let snow = bps(environment.weather.snow_cover_bps);
    let grass_density = (0.96 - canopy * 0.16 - water * 0.88 + cultivation * 0.04)
        .clamp(0.06, 0.98)
        * (1.0 - snow * 0.36);
    // A mature shared hazel occupies far more space than the former crossed
    // card placeholder. Scatter on a wider lattice so woodland contains
    // legible individual shrubs and traversable openings instead of a wall of
    // overlapping coppice stems.
    let understory_chance = (canopy * 0.52 + wetland * 0.3).clamp(0.0, 0.52);
    let grass_color = if environment.weather.snow_cover_bps >= 5_000 {
        Color::srgb_u8(155, 164, 137)
    } else if environment.cultivation_bps >= 4_000 {
        Color::srgb_u8(142, 133, 61)
    } else {
        Color::srgb_u8(82, 119, 45)
    };
    let grass_near_mesh = meshes.add(grass_patch_mesh(
        grass_color,
        GrassMeshLod::Near,
        grass_density,
    ));
    let grass_far_mesh = meshes.add(grass_patch_mesh(
        grass_color,
        GrassMeshLod::Far,
        grass_density,
    ));
    ensure_hazel_presentation(
        meshes,
        standard_materials,
        leaf_materials,
        hazel_cache,
        asset_server,
    );
    let grass_wind_scale = 0.16 + bps(environment.weather.wind_speed_bps) * 0.36;
    let grass_near_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Near,
        grass_density,
    ));
    let grass_far_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Far,
        grass_density,
    ));
    let dry_leaf_meshes = (0..DRY_LEAF_MESH_VARIANTS)
        .map(|variant| meshes.add(dry_leaf_patch_mesh(variant)))
        .collect::<Vec<_>>();
    let twig_meshes = (0..TWIG_MESH_VARIANTS)
        .map(|variant| meshes.add(twig_patch_mesh(variant)))
        .collect::<Vec<_>>();
    let dry_leaf_material = materials.add(foliage_material(0.008, false));
    let twig_material = materials.add(foliage_material(0.004, false));
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    // Grass uses a macro patch whose internal blade spacing matches the old
    // one-metre patch. A roughly ten-times larger footprint therefore retains
    // density while cutting extraction, visibility, and instance entities by
    // an order of magnitude. Aligning each patch to the sampled terrain normal
    // keeps the larger shared plane seated on slopes.
    let count_x = (terrain.width() / GRASS_PATCH_SPACING).ceil() as i32;
    let count_z = (terrain.depth() / GRASS_PATCH_SPACING).ceil() as i32;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.24) * GRASS_PATCH_SPACING;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.24) * GRASS_PATCH_SPACING;
            let ground_position = Vec2::new(world_x, world_z);
            if !ground_allows_grass_patch(ground, ground_position) {
                continue;
            }
            let Some(transform) = foliage_transform(terrain, world_x, world_z, hash) else {
                continue;
            };
            commands.spawn((
                Name::new("Tactical grass near ribbons"),
                FoliageLayer::Grass,
                NotShadowCaster,
                Mesh3d(grass_near_mesh.clone()),
                MeshMaterial3d(grass_near_material.clone()),
                grass_lod_visibility(GrassMeshLod::Near),
                transform,
            ));
            commands.spawn((
                Name::new("Tactical grass far ribbons"),
                NotShadowCaster,
                Mesh3d(grass_far_mesh.clone()),
                MeshMaterial3d(grass_far_material.clone()),
                grass_lod_visibility(GrassMeshLod::Far),
                transform,
            ));
        }
    }

    let understory_spacing = 3.2;
    let understory_count_x = (terrain.width() / understory_spacing).floor() as i32;
    let understory_count_z = (terrain.depth() / understory_spacing).floor() as i32;
    for z in 0..understory_count_z {
        for x in 0..understory_count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell ^ 0xa04f_63d2_719b_e850);
            if unit_hash(hash) >= understory_chance {
                continue;
            }
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.72) * understory_spacing;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.72) * understory_spacing;
            if ground
                .ground_at(Vec2::new(world_x, world_z))
                .is_none_or(|sample| sample.cover == GroundCover::LeafLitter)
            {
                continue;
            }
            let Some(transform) = foliage_transform(terrain, world_x, world_z, hash) else {
                continue;
            };
            commands.spawn((
                Name::new("Shared common hazel shrub wood"),
                FoliageLayer::Understory,
                Mesh3d(hazel_cache.branches.as_ref().unwrap().clone()),
                MeshMaterial3d(hazel_cache.bark.as_ref().unwrap().clone()),
                VisibilityRange::abrupt(0.0, 92.0),
                transform,
            ));
            commands.spawn((
                Name::new("Shared common hazel cambered leaves"),
                FoliageLayer::Understory,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(hazel_cache.cambered_leaves.as_ref().unwrap().clone()),
                MeshMaterial3d(hazel_cache.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: 26.0..34.0,
                    use_aabb: true,
                },
                transform,
            ));
            commands.spawn((
                Name::new("Shared common hazel alpha-card leaves"),
                FoliageLayer::Understory,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(hazel_cache.leaf_cards.as_ref().unwrap().clone()),
                MeshMaterial3d(hazel_cache.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 26.0..34.0,
                    end_margin: 84.0..96.0,
                    use_aabb: true,
                },
                transform,
            ));
        }
    }

    for (index, sample) in ground.samples().iter().enumerate() {
        if sample.cover != GroundCover::LeafLitter {
            continue;
        }
        let grid_x = index % ground.grid_width();
        let grid_z = index / ground.grid_width();
        let cell_origin = Vec2::new(
            grid_x as f32 * ground.grid_scale() - ground.width() * 0.5,
            grid_z as f32 * ground.grid_scale() - ground.depth() * 0.5,
        );
        let density = bps(sample.cover_density_bps);
        for pass in 0..DRY_LEAF_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(19) ^ 0x2b6f_5dd9_81aa_9135);
            if unit_hash(hash) >= density * 0.92 {
                continue;
            }
            spawn_forest_floor_patch(
                commands,
                terrain,
                ground,
                cell_origin,
                hash,
                "Tactical dry-leaf patch",
                FoliageLayer::DryLeaves,
                &dry_leaf_meshes[(hash % dry_leaf_meshes.len() as u64) as usize],
                &dry_leaf_material,
                0.8,
                0.012,
            );
        }
        for pass in 0..TWIG_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(23) ^ 0xc41b_b83e_3a70_f965);
            if unit_hash(hash) >= density * 0.62 {
                continue;
            }
            spawn_forest_floor_patch(
                commands,
                terrain,
                ground,
                cell_origin,
                hash,
                "Tactical twig patch",
                FoliageLayer::Twigs,
                &twig_meshes[(hash % twig_meshes.len() as u64) as usize],
                &twig_material,
                0.72,
                0.02,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_forest_floor_patch(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cell_origin: Vec2,
    hash: u64,
    name: &'static str,
    layer: FoliageLayer,
    mesh: &Handle<Mesh>,
    material: &Handle<TacticalFoliageMaterial>,
    scale: f32,
    height_offset: f32,
) {
    let jitter = ground.grid_scale() * 0.78;
    let position = cell_origin
        + Vec2::new(
            unit_hash(splitmix64(hash ^ 0x672a_1f04)) - 0.5,
            unit_hash(splitmix64(hash ^ 0xeeb0_31cd)) - 0.5,
        ) * jitter;
    if ground
        .ground_at(position)
        .is_none_or(|sample| sample.cover != GroundCover::LeafLitter)
    {
        return;
    }
    let Some(mut transform) = foliage_transform(terrain, position.x, position.y, hash) else {
        return;
    };
    transform.translation.y += height_offset;
    transform.scale *= scale;
    commands.spawn((
        Name::new(name),
        layer,
        NotShadowCaster,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        VisibilityRange::abrupt(0.0, 72.0),
        transform,
    ));
}

fn ground_allows_grass_patch(ground: &SceneGround, centre: Vec2) -> bool {
    ground
        .ground_at(centre)
        .is_some_and(|sample| sample.cover == GroundCover::TallGrass)
        && !ground.cover_intersects_square(
            centre,
            GRASS_PATCH_SPACING * 0.58,
            GroundCover::LeafLitter,
        )
}

fn foliage_transform(
    terrain: &SceneTerrain,
    world_x: f32,
    world_z: f32,
    hash: u64,
) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    let height = terrain.height_at(sample)?;
    let normal = terrain.normal_at(sample)?;
    if normal.y < 0.72 {
        return None;
    }
    let terrain_rotation = Quat::from_rotation_arc(Vec3::Y, normal);
    let yaw = Quat::from_rotation_y(unit_hash(hash) * core::f32::consts::TAU);
    let scale = 0.72 + unit_hash(splitmix64(hash ^ 0x8c0a_3c95)) * 0.58;
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(terrain_rotation * yaw)
            .with_scale(Vec3::splat(scale)),
    )
}

pub(super) fn present_ground_scatter(
    scenes: Query<
        (
            Entity,
            &SceneId,
            &SceneTerrain,
            &SceneGround,
            &SceneEnvironment,
        ),
        Without<GroundScatterPresented>,
    >,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut leaf_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut hazel_cache: ResMut<HazelPresentationCache>,
    asset_server: Res<AssetServer>,
) {
    for (entity, scene_id, terrain, ground, environment) in &scenes {
        spawn_ground_foliage(
            &mut commands,
            &mut meshes,
            &mut foliage_materials,
            &mut standard_materials,
            &mut leaf_materials,
            &mut hazel_cache,
            &asset_server,
            scene_id,
            terrain,
            ground,
            environment,
        );
        commands.entity(entity).insert(GroundScatterPresented);
    }
}

fn ensure_hazel_presentation(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    cache: &mut HazelPresentationCache,
    asset_server: &AssetServer,
) {
    if cache.branches.is_some() {
        return;
    }
    // One deterministic specimen is shared by every scattered shrub. Instance
    // transforms still vary placement, rotation, and scale without generating
    // unique botanical geometry per occurrence.
    let seed = 0xc0a1_5a2e_11_u64;
    let branches = procedural_woody_plant_skeleton(seed, 0.0, COMMON_HAZEL_PARAMETERS);
    let leaves = procedural_woody_plant_leaves(seed, &branches, 0.0, COMMON_HAZEL_PARAMETERS);
    cache.branches = Some(meshes.add(procedural_tree_branch_mesh(&branches, 3)));
    cache.cambered_leaves = Some(meshes.add(procedural_woody_cambered_leaf_mesh(&leaves)));
    cache.leaf_cards = Some(meshes.add(procedural_woody_leaf_card_mesh(&leaves)));
    cache.bark = Some(materials.add(StandardMaterial {
        base_color: Color::srgb_u8(118, 104, 78),
        perceptual_roughness: 0.96,
        ..default()
    }));
    cache.leaves = Some(leaf_materials.add(hazel_leaf_material(asset_server)));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GrassMeshLod {
    Near,
    Far,
}

impl GrassMeshLod {
    fn row_heights(self) -> &'static [f32] {
        match self {
            // Seven paired rows plus a shared tip: the same fifteen-vertex
            // near ribbon used by Ghost of Tsushima's published grass design.
            Self::Near => &[0.0, 0.14, 0.29, 0.45, 0.61, 0.76, 0.9],
            // Three paired rows plus a shared tip: seven vertices at distance.
            Self::Far => &[0.0, 0.45, 0.82],
        }
    }

    fn blade_grid_indices(self, grass_density: f32) -> impl Iterator<Item = usize> {
        let coordinates: &[usize] = match self {
            Self::Near => &[],
            Self::Far => &GRASS_FAR_GRID_COORDINATES,
        };
        (0..GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE).filter(move |index| {
            let selected_for_lod = if coordinates.is_empty() {
                true
            } else {
                let row = index / GRASS_PATCH_GRID_SIDE;
                let column = index % GRASS_PATCH_GRID_SIDE;
                coordinates.contains(&row) && coordinates.contains(&column)
            };
            selected_for_lod
                && (grass_density >= 1.0
                    || unit_hash(splitmix64(*index as u64 ^ 0x24e8_51c6_9a37_b40d)) < grass_density)
        })
    }

    fn blade_count(self, grass_density: f32) -> usize {
        self.blade_grid_indices(grass_density).count()
    }

    fn width_compensation(self, grass_density: f32) -> f32 {
        let near_count = Self::Near.blade_count(grass_density).max(1) as f32;
        let lod_count = self.blade_count(grass_density).max(1) as f32;
        (near_count / lod_count).sqrt()
    }
}

pub(super) fn grass_lod_visibility(lod: GrassMeshLod) -> VisibilityRange {
    match lod {
        GrassMeshLod::Near => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 34.0..44.0,
            use_aabb: false,
        },
        GrassMeshLod::Far => VisibilityRange {
            start_margin: 34.0..44.0,
            end_margin: 124.0..132.0,
            use_aabb: false,
        },
    }
}

pub(super) fn grass_patch_mesh(color: Color, lod: GrassMeshLod, grass_density: f32) -> Mesh {
    let grid_side = GRASS_PATCH_GRID_SIDE;
    let centre = (grid_side - 1) as f32 * 0.5;
    let blade_spacing = GRASS_BLADE_SPACING;
    let blades = lod
        .blade_grid_indices(grass_density)
        .map(|index| {
            let row = index / grid_side;
            let column = index % grid_side;
            let hash = splitmix64(index as u64 ^ 0x8d12_6f4a_0bc3_7791);
            let jitter_x = (unit_hash(hash) - 0.5) * blade_spacing * 0.39;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * blade_spacing * 0.39;
            let scale = 0.68 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.36;
            (
                (column as f32 - centre) * blade_spacing + jitter_x,
                (row as f32 - centre) * blade_spacing + jitter_z,
                scale,
                index as u64,
            )
        })
        .collect::<Vec<_>>();
    grass_ribbon_patch_mesh(0.045, 0.82, color, lod, &blades)
}

fn grass_ribbon_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    lod: GrassMeshLod,
    blades: &[(f32, f32, f32, u64)],
) -> Mesh {
    let rows = lod.row_heights();
    let vertices_per_blade = rows.len() * 2 + 1;
    let triangles_per_blade = (rows.len() - 1) * 2 + 1;
    let mut positions = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut normals = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut uvs = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut blade_roots = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut colors = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut indices = Vec::with_capacity(blades.len() * triangles_per_blade * 3);
    let linear = color.to_linear().to_f32_array();

    for &(offset_x, offset_z, blade_scale, blade_seed) in blades {
        let root = Vec3::new(offset_x, 0.0, offset_z);
        let hash = splitmix64(blade_seed ^ 0x6c8e_9cf5_701a_d30b);
        let angle = unit_hash(hash) * core::f32::consts::TAU;
        let half_width = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * blade_scale * 0.5;
        let normal = Vec3::Y.cross(half_width).normalize_or_zero().to_array();
        let blade_threshold = unit_hash(splitmix64(hash ^ 0x3d91_02ea_61b8_7c45));
        let blade_color = [linear[0], linear[1], linear[2], blade_threshold];
        let base = positions.len() as u32;

        for &height_fraction in rows {
            let taper = (1.0 - height_fraction).powf(0.72);
            let side = half_width * taper;
            let centre = root + Vec3::Y * height * blade_scale * height_fraction;
            positions.extend_from_slice(&[(centre - side).to_array(), (centre + side).to_array()]);
            normals.extend_from_slice(&[normal; 2]);
            uvs.extend_from_slice(&[[0.0, height_fraction], [1.0, height_fraction]]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 2]);
            colors.extend_from_slice(&[blade_color; 2]);
        }
        positions.push((root + Vec3::Y * height * blade_scale).to_array());
        normals.push(normal);
        uvs.push([0.5, 1.0]);
        blade_roots.push([offset_x, offset_z]);
        colors.push(blade_color);

        for row in 0..rows.len() - 1 {
            let lower = base + (row * 2) as u32;
            let upper = lower + 2;
            indices.extend_from_slice(&[lower, lower + 1, upper + 1, lower, upper + 1, upper]);
        }
        let shoulder = base + ((rows.len() - 1) * 2) as u32;
        let tip = base + (vertices_per_blade - 1) as u32;
        indices.extend_from_slice(&[shoulder, shoulder + 1, tip]);
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

/// Dense dry-leaf carpet with enough colour, size, and orientation variation
/// to avoid reading as a repeated decal when its shared mesh is instanced.
fn dry_leaf_patch_mesh(variant: u64) -> Mesh {
    let mut data = GroundLitterMeshData::default();
    let leaf_colors = [
        Color::srgb_u8(151, 82, 30),
        Color::srgb_u8(184, 116, 38),
        Color::srgb_u8(116, 67, 30),
        Color::srgb_u8(201, 145, 54),
        Color::srgb_u8(128, 91, 42),
    ];
    for leaf in 0..24_u64 {
        let hash = splitmix64(leaf ^ variant.rotate_left(29) ^ 0x5ec4_57d2_bf90_1c37);
        let centre = Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash ^ 1)) - 0.5) * 1.08;
        let angle = unit_hash(splitmix64(hash ^ 2)) * core::f32::consts::TAU;
        let long =
            Vec2::new(angle.cos(), angle.sin()) * (0.065 + unit_hash(splitmix64(hash ^ 3)) * 0.045);
        let side = Vec2::new(-long.y, long.x) * (0.34 + unit_hash(splitmix64(hash ^ 4)) * 0.16);
        data.append_diamond(
            centre,
            long,
            side,
            0.003 + (leaf % 7) as f32 * 0.00045,
            leaf_colors[leaf as usize % leaf_colors.len()],
        );
    }
    data.into_mesh()
}

/// Independent twig mesh. Longer, thinner pieces and a lower spawn density
/// let twigs form irregular accents over the denser dry-leaf carpet.
fn twig_patch_mesh(variant: u64) -> Mesh {
    let mut data = GroundLitterMeshData::default();
    let twig_colors = [
        Color::srgb_u8(79, 47, 24),
        Color::srgb_u8(102, 62, 29),
        Color::srgb_u8(62, 40, 25),
    ];
    for twig in 0..9_u64 {
        let hash = splitmix64(twig ^ variant.rotate_left(31) ^ 0xa773_9fe2_410c_862d);
        let centre = Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash ^ 1)) - 0.5) * 1.02;
        let angle = unit_hash(splitmix64(hash ^ 2)) * core::f32::consts::TAU;
        let long =
            Vec2::new(angle.cos(), angle.sin()) * (0.14 + unit_hash(splitmix64(hash ^ 3)) * 0.13);
        let side = Vec2::new(-long.y, long.x) * (0.045 + unit_hash(splitmix64(hash ^ 4)) * 0.025);
        data.append_quad(
            centre,
            long,
            side,
            0.008 + (twig % 5) as f32 * 0.0008,
            twig_colors[twig as usize % twig_colors.len()],
        );
    }
    data.into_mesh()
}

impl GroundLitterMeshData {
    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.roots);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

#[derive(Default)]
struct GroundLitterMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    roots: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl GroundLitterMeshData {
    fn append_quad(&mut self, centre: Vec2, long: Vec2, side: Vec2, height: f32, color: Color) {
        let base = self.positions.len() as u32;
        for point in [
            centre - long - side,
            centre + long - side,
            centre + long + side,
            centre - long + side,
        ] {
            self.positions.push([point.x, height, point.y]);
            self.normals.push(Vec3::Y.to_array());
            self.roots.push(centre.to_array());
        }
        self.uvs
            .extend_from_slice(&[[0.0, 0.85], [1.0, 0.85], [1.0, 1.0], [0.0, 1.0]]);
        let color = color.to_linear().to_f32_array();
        self.colors.extend_from_slice(&[color; 4]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn append_diamond(&mut self, centre: Vec2, long: Vec2, side: Vec2, height: f32, color: Color) {
        let base = self.positions.len() as u32;
        for point in [centre - long, centre + side, centre + long, centre - side] {
            self.positions.push([point.x, height, point.y]);
            self.normals.push(Vec3::Y.to_array());
            self.roots.push(centre.to_array());
        }
        self.uvs
            .extend_from_slice(&[[0.5, 0.85], [1.0, 0.925], [0.5, 1.0], [0.0, 0.925]]);
        let color = color.to_linear().to_f32_array();
        self.colors.extend_from_slice(&[color; 4]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
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
    shading: Vec4,
    #[uniform(0)]
    shape: Vec4,
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
    DryLeaves,
    Twigs,
}

#[derive(Component)]
pub(in crate::presentation) struct GroundScatterPresented;

/// Marks the locally controlled character whose movement bends nearby grass.
#[derive(Component)]
pub(crate) struct GrassInteractor;

#[derive(Resource, Default)]
pub(in crate::presentation) struct GrassInteractionState {
    previous_position: Option<Vec3>,
    smoothed_velocity: Vec3,
}

const FOLIAGE_SHADER: &str = "shaders/tactical_foliage.wgsl";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_patches_use_a_stable_reduced_far_subset() {
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let far = grass_patch_mesh(Color::WHITE, GrassMeshLod::Far, 1.0);
        let sparse = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 0.25);
        let near_positions = near
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let far_positions = far
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(near_positions.len(), 729 * 15);
        assert_eq!(far_positions.len(), 144 * 7);
        assert!(near_positions.len() > far_positions.len());
        let sparse_positions = sparse
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert!(!sparse_positions.is_empty());
        assert!(sparse_positions.len() < near_positions.len());
        let Some(VertexAttributeValues::Float32x2(near_roots)) =
            near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry stable blade roots");
        };
        let Some(VertexAttributeValues::Float32x2(far_roots)) = far.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("far grass mesh must carry stable blade roots");
        };
        assert_eq!(near_roots.len(), near_positions.len());
        assert_eq!(far_roots.len(), far_positions.len());
        assert!(far_roots.iter().all(|root| near_roots.contains(root)));
        let Some(VertexAttributeValues::Float32x4(colors)) = near.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
    }

    #[test]
    fn grass_lods_crossfade_across_the_same_distance_interval() {
        let near = grass_lod_visibility(GrassMeshLod::Near);
        let far = grass_lod_visibility(GrassMeshLod::Far);
        assert_eq!(near.end_margin, far.start_margin);
        assert!(!near.is_abrupt());
        assert!(!far.is_abrupt());
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.shading.w, 0.0);
        assert_eq!(grass.shape, Vec4::ZERO);
        assert_eq!(
            grass_material(0.3, GrassMeshLod::Near, 1.0).shape,
            Vec4::new(1.0, 0.88, 0.09, 1.0)
        );
        assert_eq!(
            grass_material(0.3, GrassMeshLod::Far, 1.0).shape,
            Vec4::new(1.0, 0.88, 0.09, 2.25)
        );
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

    #[test]
    fn leaf_litter_conservatively_excludes_overlapping_grass_patches() {
        let mut samples = vec![GroundSurface::default(); 81];
        samples[40].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(9, 9, 1.0, samples).unwrap();
        assert!(!ground_allows_grass_patch(&ground, Vec2::ZERO));
        assert!(ground_allows_grass_patch(&ground, Vec2::new(-4.0, -4.0)));
    }

    #[test]
    fn proof_litter_uses_separate_dense_leaf_and_twig_meshes() {
        let leaves = dry_leaf_patch_mesh(0);
        let alternate_leaves = dry_leaf_patch_mesh(1);
        let twigs = twig_patch_mesh(0);
        let leaf_positions = leaves
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let twig_positions = twigs
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(leaf_positions.len(), 24 * 4);
        assert_eq!(twig_positions.len(), 9 * 4);
        assert_ne!(
            leaves.attribute(Mesh::ATTRIBUTE_POSITION),
            alternate_leaves.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        for mesh in [&leaves, &twigs] {
            assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
            assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
        }
    }
}
