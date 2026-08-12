use super::*;

// A 54 x 54 grid preserves the established macro-patch footprint while
// placing four times as many authored blades per square metre as the original
// 27 x 27 grid.
const GRASS_PATCH_GRID_SIDE: usize = 54;
const GRASS_PATCH_SPACING: f32 = 3.2;
const GRASS_BLADE_SPACING: f32 = 3.51 / (GRASS_PATCH_GRID_SIDE - 1) as f32;
// Keep neighbouring near-flat macro patches inside the blade footprint even
// when their deterministic centre jitter diverges in opposite directions.
const GRASS_PATCH_JITTER_FRACTION: f32 = 0.04;
const GRASS_FAR_GRID_COORDINATES: [usize; 12] = [0, 5, 10, 14, 19, 24, 29, 34, 39, 43, 48, 53];
const DRY_LEAF_PASSES_PER_SAMPLE: u64 = 3;
const TWIG_PASSES_PER_SAMPLE: u64 = 2;
const DRY_LEAF_MESH_VARIANTS: u64 = 4;
const TWIG_MESH_VARIANTS: u64 = 3;
const LOOSE_STONE_MESH_VARIANTS: u64 = 4;
const LOOSE_STONE_PASSES_PER_SAMPLE: u64 = 3;

#[derive(Resource, Default)]
pub(in crate::presentation) struct HazelPresentationCache {
    branches: Option<Handle<Mesh>>,
    cambered_leaves: Option<Handle<Mesh>>,
    leaf_cards: Option<Handle<Mesh>>,
    bark: Option<Handle<StandardMaterial>>,
    leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct GroundFoliagePresentationCache {
    forest_floor_leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
}

pub(super) fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        // Root brightness, meadow colour variation, normal up-bias, and
        // whether nearby player movement affects this material.
        shading: if ground_foliage {
            Vec4::new(0.52, 0.13, 0.76, 1.0)
        } else {
            Vec4::new(0.55, 0.08, 0.28, 0.0)
        },
        // Curved ribbon geometry, edge-on view thickening, authored lean, and
        // reserved future shaping control. Understory cards retain the older
        // crossed-plane deformation path.
        shape: Vec4::ZERO,
        ground_mask_transform: Vec4::ZERO,
        ground_mask: None,
    }
}

fn grass_material(
    wind_scale: f32,
    lod: GrassMeshLod,
    grass_density: f32,
    ground_mask: Handle<Image>,
    ground: &SceneGround,
) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        // Only the near mesh is four times denser. The far mesh retains the
        // established 144-blade topology and projected coverage rather than
        // spending geometry on subpixel blades.
        shape: Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(grass_density)),
        ground_mask_transform: Vec4::new(1.0 / ground.width(), 1.0 / ground.depth(), 0.5, 0.5),
        ground_mask: Some(ground_mask),
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

pub(super) fn update_celestial_material_lighting(
    environments: Query<&SceneEnvironment>,
    mut impostor_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
) {
    let Some(environment) = environments.iter().next() else {
        return;
    };
    let celestial = celestial_directions(
        environment.absolute_minute,
        environment.latitude_microdegrees,
        environment.longitude_microdegrees,
    );
    let sun_altitude = celestial.sun[1].asin().to_degrees();
    let moon_altitude = celestial.moon[1].asin().to_degrees();
    let light_factor =
        scene_night_factor(sun_altitude, moon_altitude, celestial.lunar_illumination);
    let (ambient_color, _) =
        scene_ambient_light(sun_altitude, moon_altitude, celestial.lunar_illumination);
    let ambient_response =
        scene_ambient_response(sun_altitude, moon_altitude, celestial.lunar_illumination);
    let direction = if sun_altitude > -6.0 {
        to_bevy_direction(celestial.sun)
    } else if moon_altitude > -2.0 {
        to_bevy_direction(celestial.moon)
    } else {
        Vec3::new(0.25, 0.92, 0.3).normalize()
    };
    for (_, material) in impostor_materials.iter_mut() {
        material.lighting = direction.extend(light_factor);
        material.ambient = ambient_color.extend(ambient_response);
    }
}

pub(super) fn spawn_ground_foliage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalFoliageMaterial>,
    standard_materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    hazel_cache: &mut HazelPresentationCache,
    ground_foliage_cache: &mut GroundFoliagePresentationCache,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
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
    let grass_mask = images.add(grass_cover_mask_image(
        ground,
        stable_text_seed(&environment.scene_digest),
    ));
    let grass_near_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Near,
        grass_density,
        grass_mask.clone(),
        ground,
    ));
    let grass_far_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Far,
        grass_density,
        grass_mask,
        ground,
    ));
    let dry_leaf_meshes = (0..DRY_LEAF_MESH_VARIANTS)
        .map(|variant| meshes.add(dry_leaf_patch_mesh(variant)))
        .collect::<Vec<_>>();
    let twig_meshes = (0..TWIG_MESH_VARIANTS)
        .map(|variant| meshes.add(twig_patch_mesh(variant)))
        .collect::<Vec<_>>();
    let dry_leaf_material = ground_foliage_cache
        .forest_floor_leaves
        .get_or_insert_with(|| leaf_materials.add(forest_floor_leaf_material(asset_server)))
        .clone();
    let twig_material = materials.add(foliage_material(0.004, false));
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    // Grass uses a macro patch whose internal blade spacing matches the old
    // one-metre patch. A roughly ten-times larger footprint therefore retains
    // density while cutting extraction, visibility, and instance entities by
    // an order of magnitude. Macro patches stay unit-scale and nearly gridded:
    // randomly shrinking/rotating the square footprint opened visible seams.
    // Aligning each patch to the sampled terrain normal keeps the shared plane
    // seated on slopes while its blades retain deterministic local variation.
    let count_x = (terrain.width() / GRASS_PATCH_SPACING).ceil() as i32;
    let count_z = (terrain.depth() / GRASS_PATCH_SPACING).ceil() as i32;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let eligibility_world_x =
                -half_x + (x as f32 + 0.5 + jitter_x * 0.24) * GRASS_PATCH_SPACING;
            let eligibility_world_z =
                -half_z + (z as f32 + 0.5 + jitter_z * 0.24) * GRASS_PATCH_SPACING;
            let world_x = -half_x
                + (x as f32 + 0.5 + jitter_x * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let world_z = -half_z
                + (z as f32 + 0.5 + jitter_z * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let Some(transform) = grass_patch_placement(
                terrain,
                ground,
                Vec2::new(eligibility_world_x, eligibility_world_z),
                Vec2::new(world_x, world_z),
            ) else {
                continue;
            };
            commands.spawn((
                Name::new("Tactical grass near ribbons"),
                GroundScatterLayer::Grass,
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
                GroundScatterLayer::Understory,
                Mesh3d(hazel_cache.branches.as_ref().unwrap().clone()),
                MeshMaterial3d(hazel_cache.bark.as_ref().unwrap().clone()),
                VisibilityRange::abrupt(0.0, 92.0),
                transform,
            ));
            commands.spawn((
                Name::new("Shared common hazel cambered leaves"),
                GroundScatterLayer::Understory,
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
                GroundScatterLayer::Understory,
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
            spawn_forest_floor_leaf_patch(
                commands,
                terrain,
                ground,
                cell_origin,
                hash,
                "Tactical dry-leaf patch",
                GroundScatterLayer::DryLeaves,
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
                GroundScatterLayer::Twigs,
                &twig_meshes[(hash % twig_meshes.len() as u64) as usize],
                &twig_material,
                0.72,
                0.02,
            );
        }
    }

    let loose_stone_recipes = (0..LOOSE_STONE_MESH_VARIANTS)
        .map(loose_stone_recipe)
        .collect::<Vec<_>>();
    let loose_stone_meshes = loose_stone_recipes
        .iter()
        .map(|recipe| meshes.add(procedural_rock_mesh(*recipe)))
        .collect::<Vec<_>>();
    let loose_stone_materials = loose_stone_recipes
        .iter()
        .map(|recipe| {
            standard_materials.add(StandardMaterial {
                base_color: rock_color(recipe.lithology),
                perceptual_roughness: 1.0,
                ..default()
            })
        })
        .collect::<Vec<_>>();
    for (index, sample) in ground.samples().iter().enumerate() {
        if sample.cover != GroundCover::LooseStone {
            continue;
        }
        let grid_x = index % ground.grid_width();
        let grid_z = index / ground.grid_width();
        let cell_origin = Vec2::new(
            grid_x as f32 * ground.grid_scale() - ground.width() * 0.5,
            grid_z as f32 * ground.grid_scale() - ground.depth() * 0.5,
        );
        let density = bps(sample.cover_density_bps);
        for pass in 0..LOOSE_STONE_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(17) ^ 0x7374_6f6e_655f_7363);
            if unit_hash(hash) >= density {
                continue;
            }
            let jitter = ground.grid_scale() * 0.72;
            let position = cell_origin
                + Vec2::new(
                    unit_hash(splitmix64(hash ^ 0x672a_1f04)) - 0.5,
                    unit_hash(splitmix64(hash ^ 0xeeb0_31cd)) - 0.5,
                ) * jitter;
            if ground
                .ground_at(position)
                .is_none_or(|surface| surface.cover != GroundCover::LooseStone)
            {
                continue;
            }
            let Some(mut transform) = foliage_transform(terrain, position.x, position.y, hash)
            else {
                continue;
            };
            let variant = (hash % LOOSE_STONE_MESH_VARIANTS) as usize;
            let scale = 0.075 + unit_hash(splitmix64(hash ^ 0x51d2_9ec4)) * 0.085;
            transform.scale *= scale;
            transform.translation.y += scale * 0.46;
            commands.spawn((
                Name::new("Tactical loose-stone scatter"),
                GroundScatterLayer::LooseStone,
                NotShadowCaster,
                Mesh3d(loose_stone_meshes[variant].clone()),
                MeshMaterial3d(loose_stone_materials[variant].clone()),
                VisibilityRange::abrupt(0.0, 58.0),
                transform,
            ));
        }
    }
}

fn forest_floor_leaf_material(asset_server: &AssetServer) -> TacticalTreeLeafCardMaterial {
    let mut material = oak_leaf_material(asset_server);
    // Fallen leaves reuse the oak surface maps/PBR response but do not inherit
    // canopy wind displacement. NotShadowCaster on every litter entity keeps
    // their dense alpha geometry out of the shadow pass.
    material.parameters.z = 0.0;
    material.surface_parameters.z = 0.0;
    material.surface_parameters.w = 0.035;
    material.physical_parameters.x = 0.96;
    material.physical_parameters.y = 0.00035;
    material.physical_parameters.z = 1.0;
    material
}

fn loose_stone_recipe(variant: u64) -> RockRecipe {
    let archetype = match variant % 3 {
        0 => RockArchetype::Rounded,
        1 => RockArchetype::Angular,
        _ => RockArchetype::Slab,
    };
    RockRecipe {
        seed: splitmix64(0x7065_6262_6c65_0000 ^ variant),
        archetype,
        lithology: match variant % 3 {
            0 => RockLithology::Granite,
            1 => RockLithology::Limestone,
            _ => RockLithology::Sandstone,
        },
        dimensions_cm: match archetype {
            RockArchetype::Rounded => [126, 96, 116],
            RockArchetype::Angular => [132, 104, 120],
            RockArchetype::Slab => [140, 66, 128],
        },
        collision_radius_cm: 75,
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
    layer: GroundScatterLayer,
    mesh: &Handle<Mesh>,
    material: &Handle<TacticalFoliageMaterial>,
    scale: f32,
    height_offset: f32,
) {
    let Some(transform) =
        forest_floor_patch_transform(terrain, ground, cell_origin, hash, scale, height_offset)
    else {
        return;
    };
    commands.spawn((
        Name::new(name),
        layer,
        NotShadowCaster,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        VisibilityRange::abrupt(0.0, 35.0),
        transform,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_forest_floor_leaf_patch(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cell_origin: Vec2,
    hash: u64,
    name: &'static str,
    layer: GroundScatterLayer,
    mesh: &Handle<Mesh>,
    material: &Handle<TacticalTreeLeafCardMaterial>,
    scale: f32,
    height_offset: f32,
) {
    let Some(transform) =
        forest_floor_patch_transform(terrain, ground, cell_origin, hash, scale, height_offset)
    else {
        return;
    };
    commands.spawn((
        Name::new(name),
        layer,
        NotShadowCaster,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        VisibilityRange::abrupt(0.0, 35.0),
        transform,
    ));
}

fn forest_floor_patch_transform(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cell_origin: Vec2,
    hash: u64,
    scale: f32,
    height_offset: f32,
) -> Option<Transform> {
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
        return None;
    }
    let mut transform = foliage_transform(terrain, position.x, position.y, hash)?;
    transform.translation.y += height_offset;
    transform.scale *= scale;
    Some(transform)
}

fn ground_allows_grass_patch(ground: &SceneGround, centre: Vec2) -> bool {
    let half_extent = GRASS_PATCH_SPACING * 0.58;
    [-1.0, 0.0, 1.0].into_iter().any(|z| {
        [-1.0, 0.0, 1.0].into_iter().any(|x| {
            ground
                .ground_at(centre + Vec2::new(x, z) * half_extent)
                .is_some_and(|sample| sample.cover == GroundCover::TallGrass)
        })
    })
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

fn grass_patch_transform(terrain: &SceneTerrain, world_x: f32, world_z: f32) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    let height = terrain.height_at(sample)?;
    let normal = terrain.normal_at(sample)?;
    if normal.y < 0.72 {
        return None;
    }
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal)),
    )
}

fn grass_patch_placement(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    legacy_predicate_centre: Vec2,
    render_centre: Vec2,
) -> Option<Transform> {
    // The legacy centre remains a one-way count-invariance guard: a formerly
    // rejected patch stays rejected. The actual rendered centre must also be
    // legal, so reducing jitter cannot move grass into leaf litter or outside
    // a usable terrain anchor.
    if !ground_allows_grass_patch(ground, legacy_predicate_centre)
        || !ground_allows_grass_patch(ground, render_centre)
    {
        return None;
    }
    grass_patch_transform(terrain, render_centre.x, render_centre.y)
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
    mut images: ResMut<Assets<Image>>,
    mut hazel_cache: ResMut<HazelPresentationCache>,
    mut ground_foliage_cache: ResMut<GroundFoliagePresentationCache>,
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
            &mut ground_foliage_cache,
            &asset_server,
            &mut images,
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
        if self == Self::Near {
            return 1.0;
        }
        // Keep the far representation calibrated to the original 27 x 27
        // near field. The additional 54 x 54 density is intentionally local.
        let near_count = (27 * 27) as f32 * grass_density.clamp(0.0, 1.0);
        let lod_count = self.blade_count(grass_density).max(1) as f32;
        (near_count.max(1.0) / lod_count).sqrt()
    }
}

pub(super) fn grass_lod_visibility(lod: GrassMeshLod) -> VisibilityRange {
    match lod {
        GrassMeshLod::Near => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 18.0..26.0,
            use_aabb: false,
        },
        GrassMeshLod::Far => VisibilityRange {
            start_margin: 18.0..26.0,
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
            let clump_x = ((row as f32 * 0.47 + column as f32 * 0.19).sin()) * blade_spacing * 0.24;
            let clump_z = ((column as f32 * 0.41 - row as f32 * 0.23).sin()) * blade_spacing * 0.24;
            let jitter_x = (unit_hash(hash) - 0.5) * blade_spacing * 0.46;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * blade_spacing * 0.46;
            let clump_vigor = 0.5 + 0.5 * (row as f32 * 0.31 + column as f32 * 0.17 + 0.8).sin();
            let height_scale =
                (0.50 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.62 + clump_vigor * 0.20)
                    .clamp(0.50, 1.30);
            let width_scale = 0.62 + unit_hash(splitmix64(hash ^ 0x91e2_57a4)) * 0.76;
            let base_x = (column as f32 - centre) * blade_spacing;
            let base_z = (row as f32 - centre) * blade_spacing;
            let mut offset_x = base_x + jitter_x + clump_x;
            let mut offset_z = base_z + jitter_z + clump_z;
            // Boundary rows may wander outward but never inward. This retains
            // organic clumping inside the patch while mitigating gaps along
            // near-flat and ordinary sloped shared edges.
            if column == 0 {
                offset_x = offset_x.min(base_x);
            } else if column + 1 == grid_side {
                offset_x = offset_x.max(base_x);
            }
            if row == 0 {
                offset_z = offset_z.min(base_z);
            } else if row + 1 == grid_side {
                offset_z = offset_z.max(base_z);
            }
            GrassBlade {
                offset_x,
                offset_z,
                height_scale,
                width_scale,
                seed: index as u64,
            }
        })
        .collect::<Vec<_>>();
    grass_ribbon_patch_mesh(0.026, 0.82, color, lod, &blades)
}

#[derive(Clone, Copy)]
struct GrassBlade {
    offset_x: f32,
    offset_z: f32,
    height_scale: f32,
    width_scale: f32,
    seed: u64,
}

fn grass_ribbon_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    lod: GrassMeshLod,
    blades: &[GrassBlade],
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

    for &GrassBlade {
        offset_x,
        offset_z,
        height_scale,
        width_scale,
        seed: blade_seed,
    } in blades
    {
        let root = Vec3::new(offset_x, 0.0, offset_z);
        let hash = splitmix64(blade_seed ^ 0x6c8e_9cf5_701a_d30b);
        let angle = unit_hash(hash) * core::f32::consts::TAU;
        let half_width = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * width_scale * 0.5;
        let normal = Vec3::Y.cross(half_width).normalize_or_zero().to_array();
        let blade_threshold = unit_hash(splitmix64(hash ^ 0x3d91_02ea_61b8_7c45));
        let pigment = unit_hash(splitmix64(hash ^ 0x76b3_144d));
        let warmth = unit_hash(splitmix64(hash ^ 0xa52d_98c7));
        let brightness = 0.82 + pigment * 0.30;
        let blade_color = [
            (linear[0] * brightness * (0.94 + warmth * 0.12)).clamp(0.0, 1.0),
            (linear[1] * brightness * (1.04 - warmth * 0.08)).clamp(0.0, 1.0),
            (linear[2] * brightness * (0.88 + warmth * 0.10)).clamp(0.0, 1.0),
            blade_threshold,
        ];
        let base = positions.len() as u32;

        for &height_fraction in rows {
            let taper = (1.0 - height_fraction).powf(0.72);
            let side = half_width * taper;
            let centre = root + Vec3::Y * height * height_scale * height_fraction;
            positions.extend_from_slice(&[(centre - side).to_array(), (centre + side).to_array()]);
            normals.extend_from_slice(&[normal; 2]);
            uvs.extend_from_slice(&[[0.0, height_fraction], [1.0, height_fraction]]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 2]);
            colors.extend_from_slice(&[blade_color; 2]);
        }
        positions.push((root + Vec3::Y * height * height_scale).to_array());
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
        Color::srgb_u8(174, 132, 78),
        Color::srgb_u8(151, 111, 68),
        Color::srgb_u8(190, 153, 96),
        Color::srgb_u8(128, 96, 66),
        Color::srgb_u8(165, 140, 99),
    ];
    for leaf in 0..24_u64 {
        let hash = splitmix64(leaf ^ variant.rotate_left(29) ^ 0x5ec4_57d2_bf90_1c37);
        let centre = Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash ^ 1)) - 0.5) * 1.08;
        let angle = unit_hash(splitmix64(hash ^ 2)) * core::f32::consts::TAU;
        let long =
            Vec2::new(angle.cos(), angle.sin()) * (0.065 + unit_hash(splitmix64(hash ^ 3)) * 0.045);
        let side = Vec2::new(-long.y, long.x) * (0.34 + unit_hash(splitmix64(hash ^ 4)) * 0.16);
        data.append_cambered_leaf(
            centre,
            long,
            side,
            0.003 + (leaf % 7) as f32 * 0.00045,
            hash,
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
        let start = Vec3::new(centre.x - long.x, 0.008, centre.y - long.y);
        let end = Vec3::new(
            centre.x + long.x,
            0.011 + unit_hash(splitmix64(hash ^ 4)) * 0.018,
            centre.y + long.y,
        );
        let sides = 3 + (hash % 3) as u32;
        let radius = 0.006 + unit_hash(splitmix64(hash ^ 5)) * 0.005;
        let color = twig_colors[twig as usize % twig_colors.len()];
        data.append_tapered_twig(
            start,
            end,
            radius,
            radius * 0.42,
            sides,
            true,
            true,
            centre,
            color,
        );
        if twig < 2 && unit_hash(splitmix64(hash ^ 6)) > 0.46 {
            let attach = start.lerp(end, 0.58);
            let direction = (end - start).normalize();
            let lateral = Vec3::new(-direction.z, 0.12, direction.x).normalize();
            let fork_end = attach
                + (direction * 0.38 + lateral * if hash & 1 == 0 { 0.62 } else { -0.62 })
                    .normalize()
                    * long.length()
                    * 0.72;
            data.append_tapered_twig(
                attach,
                fork_end,
                radius * 0.55,
                radius * 0.18,
                sides,
                false,
                true,
                centre,
                color,
            );
        }
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
    fn append_cambered_leaf(
        &mut self,
        centre: Vec2,
        long: Vec2,
        side: Vec2,
        height: f32,
        seed: u64,
        color: Color,
    ) {
        let base = self.positions.len() as u32;
        // Fallen leaves should curl without becoming little tents. Build the
        // varied plate first, then seat its lowest vertex just below the local
        // patch ground plane so every instance visibly makes contact.
        let long_slope = (unit_hash(splitmix64(seed ^ 0x11)) - 0.5) * 0.12;
        let side_slope = (unit_hash(splitmix64(seed ^ 0x12)) - 0.5) * 0.08;
        let camber = 0.0035 + unit_hash(splitmix64(seed ^ 0x13)) * 0.0065;
        let curl = (unit_hash(splitmix64(seed ^ 0x14)) - 0.5) * 0.004;
        let burial = 0.0007 + height.min(0.006) * 0.15 + unit_hash(splitmix64(seed ^ 0x15)) * 0.001;
        let long3 = Vec3::new(long.x, long_slope * long.length(), long.y);
        let side3 = Vec3::new(side.x, side_slope * side.length(), side.y);
        let centre3 = Vec3::new(centre.x, 0.0, centre.y);
        let outline = [
            (0.0, -1.0),
            (0.82, -0.55),
            (1.0, 0.0),
            (0.74, 0.58),
            (0.0, 1.0),
            (-0.74, 0.58),
            (-1.0, 0.0),
            (-0.82, -0.55),
        ];
        let mut leaf_positions = Vec::with_capacity(9);
        leaf_positions.push(centre3 + Vec3::Y * camber);
        for (u, v) in outline {
            let lift = camber * (1.0 - u * u) * (1.0 - v * v) + curl * v * v;
            leaf_positions.push(centre3 + long3 * v + side3 * u + Vec3::Y * lift);
        }
        let minimum_y = leaf_positions
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        for point in &mut leaf_positions {
            point.y -= minimum_y + burial;
        }
        let mut leaf_normals = vec![Vec3::ZERO; 9];
        for outline_index in 0..8_usize {
            let left = 1 + outline_index;
            let right = 1 + (outline_index + 1) % 8;
            let face = (leaf_positions[right] - leaf_positions[0])
                .cross(leaf_positions[left] - leaf_positions[0]);
            leaf_normals[0] += face;
            leaf_normals[left] += face;
            leaf_normals[right] += face;
            self.indices
                .extend_from_slice(&[base, base + right as u32, base + left as u32]);
        }
        for (index, point) in leaf_positions.into_iter().enumerate() {
            self.positions.push(point.to_array());
            self.normals
                .push(leaf_normals[index].normalize().to_array());
            self.roots.push(centre.to_array());
        }
        self.uvs.push([0.5, 0.5]);
        self.uvs
            .extend(outline.map(|(u, v)| [0.5 + u * 0.5, 0.5 + v * 0.5]));
        let color = color.to_linear().to_f32_array();
        self.colors.extend_from_slice(&[color; 9]);
    }

    #[allow(clippy::too_many_arguments)]
    fn append_tapered_twig(
        &mut self,
        start: Vec3,
        end: Vec3,
        start_radius: f32,
        end_radius: f32,
        sides: u32,
        cap_start: bool,
        cap_end: bool,
        root: Vec2,
        color: Color,
    ) {
        let base = self.positions.len() as u32;
        let direction = (end - start).normalize();
        let reference = if direction.y.abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::X
        };
        let right = direction.cross(reference).normalize();
        let forward = right.cross(direction).normalize();
        let linear_color = color.to_linear().to_f32_array();
        for (ring, (centre, radius)) in [(start, start_radius), (end, end_radius)]
            .into_iter()
            .enumerate()
        {
            for side_index in 0..sides {
                let phase = side_index as f32 * core::f32::consts::TAU / sides as f32;
                let normal = right * phase.cos() + forward * phase.sin();
                self.positions.push((centre + normal * radius).to_array());
                self.normals.push(normal.to_array());
                self.uvs
                    .push([side_index as f32 / sides as f32, ring as f32]);
                self.roots.push(root.to_array());
                self.colors.push(linear_color);
            }
        }
        for side_index in 0..sides {
            let next = (side_index + 1) % sides;
            self.indices.extend_from_slice(&[
                base + side_index,
                base + sides + side_index,
                base + sides + next,
                base + side_index,
                base + sides + next,
                base + next,
            ]);
        }
        for (at_start, centre, normal) in [(true, start, -direction), (false, end, direction)] {
            if (at_start && !cap_start) || (!at_start && !cap_end) {
                continue;
            }
            let cap = self.positions.len() as u32;
            self.positions.push(centre.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push([0.5, if at_start { 0.0 } else { 1.0 }]);
            self.roots.push(root.to_array());
            self.colors.push(linear_color);
            let ring = if at_start { base } else { base + sides };
            for side_index in 0..sides {
                let next = (side_index + 1) % sides;
                if at_start {
                    self.indices
                        .extend_from_slice(&[cap, ring + next, ring + side_index]);
                } else {
                    self.indices
                        .extend_from_slice(&[cap, ring + side_index, ring + next]);
                }
            }
        }
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
    #[uniform(0)]
    ground_mask_transform: Vec4,
    #[texture(1)]
    #[sampler(2)]
    ground_mask: Option<Handle<Image>>,
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
pub(crate) enum GroundScatterLayer {
    Grass,
    Understory,
    DryLeaves,
    Twigs,
    LooseStone,
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
    use std::collections::BTreeSet;

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
        assert_eq!(near_positions.len(), 2_916 * 15);
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
        let Some(VertexAttributeValues::Float32x4(far_colors)) =
            far.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("far grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
        for (far_root, far_color) in far_roots.chunks_exact(7).zip(far_colors.chunks_exact(7)) {
            let matching_near_blade = near_roots
                .chunks_exact(15)
                .position(|near_root| near_root[0] == far_root[0])
                .expect("every far blade must retain its exact near-LOD root");
            assert_eq!(
                colors[matching_near_blade * 15][3],
                far_color[0][3],
                "near and far LODs must apply the same ground-mask threshold"
            );
        }

        let blade_heights = near_positions
            .chunks_exact(15)
            .map(|blade| {
                blade
                    .iter()
                    .map(|position| position[1])
                    .fold(0.0_f32, f32::max)
            })
            .collect::<Vec<_>>();
        let minimum_height = blade_heights.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_height = blade_heights
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            minimum_height < 0.52,
            "short blades should break the curtain silhouette"
        );
        assert!(
            maximum_height > 0.95,
            "mature blades should remain visibly taller"
        );
        assert!(maximum_height - minimum_height > 0.45);

        let blade_widths = near_positions
            .chunks_exact(15)
            .map(|blade| Vec3::from_array(blade[0]).distance(Vec3::from_array(blade[1])))
            .collect::<Vec<_>>();
        let minimum_width = blade_widths.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_width = blade_widths
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(maximum_width / minimum_width > 2.0);

        let distinct_pigments = colors
            .iter()
            .map(|color| [color[0].to_bits(), color[1].to_bits(), color[2].to_bits()])
            .collect::<BTreeSet<_>>();
        assert!(distinct_pigments.len() > 100);
    }

    #[test]
    fn unit_scale_macro_patch_footprints_overlap_at_worst_case_near_flat_jitter() {
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::INFINITY, f32::min);
        let max_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let worst_adjacent_centre_distance =
            GRASS_PATCH_SPACING * (1.0 + GRASS_PATCH_JITTER_FRACTION);
        assert!(max_x - min_x > worst_adjacent_centre_distance);
        assert!(max_z - min_z > worst_adjacent_centre_distance);

        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let transform = grass_patch_transform(&terrain, 0.0, 0.0).unwrap();
        assert_eq!(transform.scale, Vec3::ONE);
        assert_eq!(transform.rotation, Quat::IDENTITY);
    }

    #[test]
    fn boundary_patch_is_retained_for_per_blade_ground_masking() {
        let width = 81;
        let depth = 41;
        let mut samples = vec![GroundSurface::default(); width * depth];
        // x=1.9 m: outside the legacy footprint centred at -0.32 m, but
        // inside the actual footprint centred at 0.0 m.
        let leaf_x = 59;
        let leaf_z = 20;
        samples[leaf_z * width + leaf_x].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(width, depth, 0.1, samples).unwrap();
        let terrain = SceneTerrain::from_heightmap(9, 9, 1.0, vec![0.0; 81]).unwrap();
        let legacy = Vec2::new(-0.32, 0.0);
        let rendered = Vec2::ZERO;
        assert!(ground_allows_grass_patch(&ground, legacy));
        assert!(ground_allows_grass_patch(&ground, rendered));
        assert!(grass_patch_placement(&terrain, &ground, legacy, rendered).is_some());
    }

    #[test]
    fn invalid_render_anchor_is_skipped_without_legacy_fallback() {
        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let ground =
            SceneGround::from_samples(81, 81, 0.1, vec![GroundSurface::default(); 81 * 81])
                .unwrap();
        assert!(grass_patch_transform(&terrain, 0.0, 0.0).is_some());
        assert!(
            grass_patch_placement(&terrain, &ground, Vec2::ZERO, Vec2::new(2.0, 0.0)).is_none()
        );
    }

    #[test]
    fn representative_slope_keeps_adjacent_boundary_rows_overlapping() {
        let heights = (0..3)
            .flat_map(|_| (0..9).map(|x| x as f32 * 0.25))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(9, 3, 1.0, heights).unwrap();
        let left = grass_patch_transform(&terrain, -1.6, 0.0).unwrap();
        let right = grass_patch_transform(&terrain, 1.6, 0.0).unwrap();
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let direction = (right.translation - left.translation).normalize();
        let left_edge = left.transform_point(Vec3::new(max_x, 0.0, 0.0));
        let right_edge = right.transform_point(Vec3::new(min_x, 0.0, 0.0));
        assert!((right_edge - left_edge).dot(direction) <= 0.0);
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
    fn grass_composition_reuses_existing_mask_fetch_and_preserves_topology() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_foliage.wgsl"
        ));
        assert_eq!(shader.matches("textureSampleLevel(").count(), 1);
        assert!(shader.contains("let effective_coverage = ground_coverage * clump_coverage"));
        assert!(shader.contains("let edge_growth = mix(0.58, 1.0"));

        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let far = grass_patch_mesh(Color::WHITE, GrassMeshLod::Far, 1.0);
        assert_eq!(near.count_vertices(), 2_916 * 15);
        assert_eq!(far.count_vertices(), 144 * 7);
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.shading.w, 0.0);
        assert_eq!(grass.shape, Vec4::ZERO);
        assert_eq!(GrassMeshLod::Near.width_compensation(1.0), 1.0);
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Near.width_compensation(1.0)),
            Vec4::new(1.0, 0.88, 0.09, 1.0)
        );
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Far.width_compensation(1.0)),
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
    fn only_deep_leaf_litter_omits_a_grass_patch() {
        let mut samples = vec![GroundSurface::default(); 81];
        samples[40].cover = GroundCover::LeafLitter;
        let boundary = SceneGround::from_samples(9, 9, 1.0, samples).unwrap();
        assert!(ground_allows_grass_patch(&boundary, Vec2::ZERO));
        let litter = SceneGround::from_samples(
            9,
            9,
            1.0,
            vec![
                GroundSurface {
                    cover: GroundCover::LeafLitter,
                    ..default()
                };
                81
            ],
        )
        .unwrap();
        assert!(!ground_allows_grass_patch(&litter, Vec2::ZERO));
    }

    #[test]
    fn forest_floor_meshes_are_deterministic_bounded_and_volumetric() {
        let leaves = dry_leaf_patch_mesh(0);
        let repeated_leaves = dry_leaf_patch_mesh(0);
        let alternate_leaves = dry_leaf_patch_mesh(1);
        let twigs = twig_patch_mesh(0);
        let repeated_twigs = twig_patch_mesh(0);
        let leaf_positions = leaves
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let twig_positions = twigs
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let leaf_normals = leaves
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let twig_normals = twigs
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(leaf_positions.len(), 24 * 9);
        assert_eq!(leaves.indices().unwrap().len() / 3, 24 * 8);
        assert!((72..=130).contains(&twig_positions.len()));
        assert!((108..=210).contains(&(twigs.indices().unwrap().len() / 3)));
        assert_eq!(
            leaves.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_leaves.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            twigs.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_twigs.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_ne!(
            leaves.attribute(Mesh::ATTRIBUTE_POSITION),
            alternate_leaves.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        for mesh in [&leaves, &twigs] {
            assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
            assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
        }
        assert!(
            leaf_normals
                .iter()
                .all(|normal| Vec3::from_array(*normal).is_normalized())
        );
        assert!(
            twig_normals
                .iter()
                .all(|normal| Vec3::from_array(*normal).is_normalized())
        );
        assert!(
            leaf_normals
                .iter()
                .any(|normal| Vec3::from_array(*normal).distance(Vec3::Y) > 0.03)
        );
        let leaf_spans = leaf_positions
            .chunks_exact(9)
            .map(|leaf| {
                let minimum = leaf
                    .iter()
                    .map(|point| point[1])
                    .fold(f32::INFINITY, f32::min);
                let maximum = leaf
                    .iter()
                    .map(|point| point[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(minimum <= -0.0006, "leaf must contact/bury: {minimum}");
                assert!(maximum <= 0.025, "leaf lift must stay bounded: {maximum}");
                maximum - minimum
            })
            .collect::<Vec<_>>();
        assert!(leaf_spans.iter().all(|span| *span > 0.003));
        assert!(
            leaf_spans
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 0.001)
        );
        let Some(VertexAttributeValues::Float32x2(leaf_uvs)) =
            leaves.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("fallen-leaf UVs must use Float32x2 storage");
        };
        assert!(leaf_uvs.contains(&[0.5, 0.0]));
        assert!(leaf_uvs.contains(&[1.0, 0.5]));
        assert!(leaf_uvs.contains(&[0.5, 1.0]));
        assert!(leaf_uvs.contains(&[0.0, 0.5]));
        assert!(!leaf_uvs.contains(&[0.0, 0.0]));
        assert!(!leaf_uvs.contains(&[1.0, 1.0]));
        let leaf_indices = leaves.indices().unwrap().iter().collect::<Vec<_>>();
        for triangle in leaf_indices.chunks_exact(3) {
            let a = Vec3::from_array(leaf_positions[triangle[0] as usize]);
            let b = Vec3::from_array(leaf_positions[triangle[1] as usize]);
            let c = Vec3::from_array(leaf_positions[triangle[2] as usize]);
            let average_normal = (Vec3::from_array(leaf_normals[triangle[0] as usize])
                + Vec3::from_array(leaf_normals[triangle[1] as usize])
                + Vec3::from_array(leaf_normals[triangle[2] as usize]))
            .normalize();
            assert!((b - a).cross(c - a).dot(average_normal) > 0.0);
        }
        for positions in [leaf_positions, twig_positions] {
            assert!(positions.iter().flatten().all(|value| value.is_finite()));
        }
        assert!(
            leaf_positions
                .iter()
                .all(|point| point[0].abs() < 0.7 && point[2].abs() < 0.7)
        );
        assert!(
            twig_positions
                .iter()
                .all(|point| point[0].abs() < 0.9 && point[2].abs() < 0.9)
        );
        let leaf_height_bounds = leaf_positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), point| (minimum.min(point[1]), maximum.max(point[1])),
        );
        assert!(
            leaf_height_bounds.0 >= -0.05 && leaf_height_bounds.1 <= 0.06,
            "fallen leaf height bounds: {leaf_height_bounds:?}"
        );
        assert!(
            twig_positions
                .iter()
                .all(|point| (-0.02..0.20).contains(&point[1]))
        );
    }

    #[test]
    fn forest_floor_leaves_reuse_oak_pbr_texture_and_surface_contract() {
        let mut app = App::new();
        app.add_plugins(TaskPoolPlugin::default());
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>();
        let floor = forest_floor_leaf_material(asset_server);
        let oak = oak_leaf_material(asset_server);
        assert_eq!(floor.opacity, oak.opacity);
        assert_eq!(floor.front_albedo, oak.front_albedo);
        assert_eq!(floor.back_albedo, oak.back_albedo);
        assert_eq!(floor.front_normal, oak.front_normal);
        assert_eq!(floor.back_normal, oak.back_normal);
        assert_eq!(floor.parameters.z, 0.0);
        assert_eq!(floor.surface_parameters.z, 0.0);
        assert!(floor.surface_parameters.w < oak.surface_parameters.w * 0.2);
        assert!(floor.physical_parameters.x > oak.physical_parameters.x);
        assert!(floor.physical_parameters.y < oak.physical_parameters.y);
        assert_eq!(floor.physical_parameters.z, 1.0);
        let shader = include_str!("../../../../assets/shaders/tactical_tree_leaf_card.wgsl");
        assert!(shader.contains("dry_texture * mix(vec3<f32>(1.0), in.color.rgb, 0.72)"));
        assert!(shader.contains("select(albedo * vec3<f32>(in.color.r), dry_pigment, dry_leaf)"));
    }

    #[test]
    fn fallen_leaf_vertex_pigments_are_dry_warm_and_varied() {
        let leaves = dry_leaf_patch_mesh(0);
        let Some(VertexAttributeValues::Float32x4(colors)) =
            leaves.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("fallen-leaf pigments must use Float32x4 storage");
        };
        assert!(colors.iter().all(|color| {
            color[0] > color[1] && color[1] > color[2] && color[0] - color[2] < 0.5
        }));
        let pigments = colors
            .chunks_exact(9)
            .map(|leaf| leaf[0])
            .collect::<Vec<_>>();
        assert!(pigments.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn twig_variants_have_exact_bounded_topology_and_only_fork_base_boundaries() {
        for variant in 0..TWIG_MESH_VARIANTS {
            let mesh = twig_patch_mesh(variant);
            let mut expected_vertices = 0;
            let mut expected_triangles = 0;
            let mut expected_boundaries = 0;
            for twig in 0..9_u64 {
                let hash = splitmix64(twig ^ variant.rotate_left(31) ^ 0xa773_9fe2_410c_862d);
                let sides = 3 + (hash % 3) as usize;
                expected_vertices += sides * 2 + 2;
                expected_triangles += sides * 4;
                if twig < 2 && unit_hash(splitmix64(hash ^ 6)) > 0.46 {
                    expected_vertices += sides * 2 + 1;
                    expected_triangles += sides * 3;
                    expected_boundaries += sides;
                }
            }
            assert_eq!(mesh.count_vertices(), expected_vertices);
            assert_eq!(mesh.indices().unwrap().len() / 3, expected_triangles);
            let mut edges = std::collections::BTreeMap::new();
            let indices = mesh.indices().unwrap().iter().collect::<Vec<_>>();
            for triangle in indices.chunks_exact(3) {
                for edge in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    *edges
                        .entry(if edge.0 < edge.1 {
                            edge
                        } else {
                            (edge.1, edge.0)
                        })
                        .or_insert(0) += 1;
                }
            }
            assert!(edges.values().all(|count| *count <= 2));
            assert_eq!(
                edges.values().filter(|count| **count == 1).count(),
                expected_boundaries
            );
        }
    }
}
