use super::*;
use fabelgeist_determinism::splitmix64;

mod grass_mask;
mod streets;

use streets::UrbanGround;

/// Marker for a distant tree billboard spawned as part of a vista ring.
#[derive(Component)]
pub(crate) struct VistaTreePresentation;

/// Presentation-only scatter outside the authoritative gameplay heightfield.
#[derive(Component)]
pub(crate) struct VistaGrassPresentation;

#[derive(Component)]
pub(crate) struct VistaRockPresentation;

/// Retains nearby presentation-only heights after the event for camera-local refinement.
/// Outside the playable rectangle, quality follows camera distance, not gameplay bounds.
#[derive(Resource, Default, Clone)]
pub(super) struct ActiveVistaSurface {
    revision: u64,
    scene_digest: String,
    playable_half_extent: Vec2,
    lods: Vec<VistaLod>,
}

impl ActiveVistaSurface {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn presented_height_at(
        &self,
        scene_digest: &str,
        terrain: &SceneTerrain,
        local: Vec2,
    ) -> Option<f32> {
        if let Some(height) = terrain.height_at(local) {
            return Some(height);
        }
        if self.scene_digest != scene_digest {
            return None;
        }
        let lod = self.lods.first()?;
        let world = local
            + Vec2::new(
                lod.origin_east_metres as f32,
                lod.origin_north_metres as f32,
            );
        let vista_height = presented_height_at(lod, world, self.lods.get(1))?;
        Some(stitch_vista_height_to_playable_edge(
            terrain,
            local,
            self.playable_half_extent,
            lod.spacing_metres,
            vista_height,
        ))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects vista scene state, presentation asset stores, and the shared tree cache independently"
)]
pub(super) fn on_scene_vista_bundle(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    mut active_surface: ResMut<ActiveVistaSurface>,
    existing: Query<Entity, With<VistaTerrain>>,
    playable_scenes: Query<(&SceneTerrain, &SceneGround, &SceneEnvironment)>,
    settings: Res<TacticalGraphicsSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalVistaMaterial>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut vista_tree_cache: ResMut<VistaTreePresentationCache>,
) {
    let started = web_time::Instant::now();
    let mut presented_chunk_count = 0_usize;
    info!("Generating tactical vista presentation");
    *active_surface = ActiveVistaSurface {
        revision: active_surface.revision.wrapping_add(1),
        scene_digest: bundle.scene_digest.clone(),
        playable_half_extent: bundle.playable_half_extent_metres,
        lods: bundle.lods.iter().take(2).cloned().collect(),
    };
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let visible_lods = bundle
        .lods
        .iter()
        .take(settings.config.rendering.vista.maximum_lods)
        .collect::<Vec<_>>();
    let playable_scene = playable_scenes
        .iter()
        .find(|(_, _, environment)| environment.scene_digest == bundle.scene_digest);
    let playable_terrain = playable_scene.map(|(terrain, _, _)| terrain);
    let playable_environment = playable_scene.map(|(_, _, environment)| environment);
    let weather = playable_scene
        .map(|(_, _, environment)| environment.weather)
        .unwrap_or_else(clear_vista_weather);
    let vista_grass_color = playable_environment
        .map(grass_terminal_pigment)
        .unwrap_or(Color::srgb_u8(37, 61, 4));
    let material = materials.add(vista_material(weather, vista_grass_color));
    if playable_scene.is_none() {
        warn!(
            scene_digest = %bundle.scene_digest,
            "Tactical vista arrived before its authoritative playable terrain; edge stitching is unavailable"
        );
    }
    let mut inner_half_extent = bundle.playable_half_extent_metres;
    for (index, lod) in visible_lods.iter().copied().enumerate() {
        let meshes_for_lod = vista_lod_meshes_with_morph(
            lod,
            inner_half_extent,
            visible_lods.get(index + 1).copied(),
            (index == 0).then_some(playable_terrain).flatten(),
            (index == 0).then_some(playable_environment).flatten(),
            weather,
        );
        if meshes_for_lod.is_empty() {
            warn!(level = lod.level, "Rejected malformed tactical vista LOD");
            continue;
        }
        let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
        for (chunk, mesh) in meshes_for_lod.into_iter().enumerate() {
            presented_chunk_count += 1;
            let triangle_count = mesh_triangle_count(&mesh);
            commands.spawn((
                Name::new(format!("Tactical vista LOD {} chunk {chunk}", lod.level)),
                VistaTerrain(lod.level),
                VistaTerrainMesh(lod.level),
                TerrainTriangleCount(triangle_count),
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(
                    lod.origin_east_metres as f32,
                    0.0,
                    lod.origin_north_metres as f32,
                ),
            ));
        }
        if index <= 1 {
            spawn_vista_trees(
                &mut commands,
                lod,
                visible_lods.get(index + 1).copied(),
                inner_half_extent,
                &bundle.scene_digest,
                playable_scene.map(|(_, _, environment)| environment),
                &mut meshes,
                &mut tree_materials,
                &mut images,
                &mut vista_tree_cache,
            );
        }
        inner_half_extent = Vec2::new(
            half_extent,
            f32::from(lod.depth.saturating_sub(1)) * lod.spacing_metres * 0.5,
        );
    }
    if let (Some(lod), Some((playable_terrain, playable_ground, environment))) =
        (visible_lods.first().copied(), playable_scene)
    {
        spawn_near_vista_details(
            &mut commands,
            &bundle,
            &active_surface,
            lod,
            visible_lods.get(1).copied(),
            playable_terrain,
            playable_ground,
            environment,
            &mut meshes,
            &mut foliage_materials,
            &mut standard_materials,
            &mut images,
            &settings.config.grass,
        );
    }
    log_vista_generation(presented_chunk_count, visible_lods.len(), started);
}

fn log_vista_generation(chunks: usize, lods: usize, started: web_time::Instant) {
    info!(
        chunks,
        lods,
        elapsed_ms = started.elapsed().as_millis(),
        "Generated tactical vista presentation"
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "near-vista details share the scene stores already injected at the observer boundary"
)]
fn spawn_near_vista_details(
    commands: &mut Commands,
    bundle: &SceneVistaBundle,
    active_surface: &ActiveVistaSurface,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: &SceneTerrain,
    playable_ground: &SceneGround,
    environment: &SceneEnvironment,
    meshes: &mut Assets<Mesh>,
    foliage_materials: &mut Assets<TacticalFoliageMaterial>,
    standard_materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    grass: &crate::presentation::config::GrassConfig,
) {
    streets::spawn(
        commands,
        &bundle.streets,
        &bundle.yards,
        active_surface,
        &bundle.scene_digest,
        playable_terrain,
        meshes,
        standard_materials,
    );
    let urban_ground = UrbanGround::new(&bundle.streets, &bundle.yards);
    spawn_near_vista_scatter(
        commands,
        lod,
        coarser_lod,
        bundle.playable_half_extent_metres,
        playable_terrain,
        playable_ground,
        environment,
        meshes,
        foliage_materials,
        images,
        grass,
        urban_ground,
    );
    spawn_vista_rocks(
        commands,
        lod,
        coarser_lod,
        bundle.playable_half_extent_metres,
        playable_terrain,
        stable_text_seed(&environment.scene_digest),
        meshes,
        standard_materials,
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn spawn_near_vista_scatter(
    commands: &mut Commands,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_half_extent: Vec2,
    playable_terrain: &SceneTerrain,
    playable_ground: &SceneGround,
    environment: &SceneEnvironment,
    meshes: &mut Assets<Mesh>,
    foliage_materials: &mut Assets<TacticalFoliageMaterial>,
    images: &mut Assets<Image>,
    grass: &crate::presentation::config::GrassConfig,
    urban_ground: UrbanGround<'_>,
) {
    let scene_seed = stable_text_seed(&environment.scene_digest);
    let (grass_color, grass_dryness) = grass_pigment(environment);
    let wind_scale = 0.16 + bps(environment.weather.wind_speed_bps) * 0.36;
    let grass_seed = scene_seed ^ 0x6772_6173_735f_6c6f;
    let grass_profile = GrassCommunityProfile::from_environment(environment);
    let (coverage_mask, coverage_transform) = grass_mask::vista_grass_cover_mask_image(
        lod,
        playable_half_extent,
        playable_ground,
        scene_seed,
        150.0,
        urban_ground,
    );
    let coverage_mask = images.add(coverage_mask);

    // The playable boundary selects the height/cover data source, never the
    // representation. The same globally aligned lattice and distance ranges
    // continue across it, so crossing the boundary cannot introduce an LOD
    // edge or replace blank space with a close representation.
    if grass.enabled {
        for grass_lod in [GrassMeshLod::Near, GrassMeshLod::Far] {
            let community_meshes = GrassCommunity::ALL.map(|community| {
                GrassTopology::ALL.map(|topology| {
                    meshes.add(configured_grass_patch_mesh(
                        grass_color,
                        grass_lod,
                        topology.density() * grass.density_scale,
                        community,
                        grass,
                    ))
                })
            });
            let material = foliage_materials.add(configured_vista_grass_material(
                wind_scale,
                grass_dryness,
                coverage_mask.clone(),
                coverage_transform,
                grass_lod,
                grass.density_scale,
                grass,
            ));
            spawn_vista_grass_lattice(
                commands,
                lod,
                coarser_lod,
                playable_half_extent,
                playable_terrain,
                playable_ground,
                grass_seed,
                grass_seed,
                grass.placement.playable_patch_spacing_m,
                80.0,
                &community_meshes,
                grass_profile,
                &material,
                configured_grass_lod_visibility(grass_lod, grass),
                urban_ground,
            );
        }

        let vista_meshes = GrassCommunity::ALL.map(|community| {
            GrassTopology::ALL.map(|topology| {
                meshes.add(configured_grass_patch_mesh(
                    grass_color,
                    GrassMeshLod::Vista,
                    topology.density() * grass.density_scale,
                    community,
                    grass,
                ))
            })
        });
        let vista_material = foliage_materials.add(configured_vista_grass_material(
            wind_scale,
            grass_dryness,
            coverage_mask,
            coverage_transform,
            GrassMeshLod::Vista,
            grass.density_scale,
            grass,
        ));
        spawn_vista_grass_lattice(
            commands,
            lod,
            coarser_lod,
            playable_half_extent,
            playable_terrain,
            playable_ground,
            grass_seed ^ 0x7669_7374_615f_6c6f,
            grass_seed,
            grass.placement.vista_patch_spacing_m,
            150.0,
            &vista_meshes,
            grass_profile,
            &vista_material,
            configured_grass_lod_visibility(GrassMeshLod::Vista, grass),
            urban_ground,
        );
    }
}

const VISTA_GRASS_BOUNDARY_STITCH_METRES: f32 = 12.0;
fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn vista_grass_patch_topology(
    lod: &VistaLod,
    playable_half_extent: Vec2,
    playable_ground: &SceneGround,
    centre: Vec2,
    half_extent: f32,
    urban_ground: UrbanGround<'_>,
) -> Option<GrassTopology> {
    let mut total = 0.0;
    let mut samples = 0;
    for z in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let point = centre + Vec2::new(x, z) * half_extent;
            total += stitched_vista_topology_coverage(
                lod,
                playable_half_extent,
                playable_ground,
                point,
                urban_ground,
            );
            samples += 1;
        }
    }
    GrassTopology::for_local_coverage(total / samples as f32)
}

fn stitched_vista_topology_coverage(
    lod: &VistaLod,
    playable_half_extent: Vec2,
    playable_ground: &SceneGround,
    point: Vec2,
    urban_ground: UrbanGround<'_>,
) -> f32 {
    if urban_ground.suppresses_grass(point) {
        return 0.0;
    }
    let boundary = point.clamp(-playable_half_extent, playable_half_extent);
    let playable_coverage = playable_ground
        .ground_at(boundary)
        .filter(|sample| sample.cover == GroundCover::TallGrass)
        .map_or(0.0, |sample| bps(sample.cover_density_bps));
    let outside = (point.abs() - playable_half_extent)
        .max(Vec2::ZERO)
        .max_element();
    if outside <= 0.0 {
        return playable_coverage;
    }
    let vista_coverage = sample_vista_environment(lod, point)
        .map(vista_sward_coverage)
        .unwrap_or(0.0);
    playable_coverage.lerp(
        vista_coverage,
        smoothstep01(outside / VISTA_GRASS_BOUNDARY_STITCH_METRES),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn spawn_vista_grass_lattice(
    commands: &mut Commands,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_half_extent: Vec2,
    playable_terrain: &SceneTerrain,
    playable_ground: &SceneGround,
    seed: u64,
    community_seed: u64,
    spacing: f32,
    outer_collar: f32,
    meshes: &[[Handle<Mesh>; GrassTopology::COUNT]; 3],
    profile: GrassCommunityProfile,
    material: &Handle<TacticalFoliageMaterial>,
    visibility: VisibilityRange,
    urban_ground: UrbanGround<'_>,
) {
    let outer = playable_half_extent + Vec2::splat(outer_collar);
    let minimum = (-outer / spacing).floor().as_ivec2();
    let maximum = (outer / spacing).ceil().as_ivec2();
    for z in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(seed ^ cell);
            let jitter = Vec2::new(
                unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5,
                unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5,
            ) * spacing
                * 0.04;
            let point = Vec2::new(x as f32, z as f32) * spacing + jitter;
            if point.x.abs() <= playable_half_extent.x && point.y.abs() <= playable_half_extent.y {
                continue;
            }
            let Some(sample) = sample_vista_environment(lod, point) else {
                continue;
            };
            let Some(topology) = vista_grass_patch_topology(
                lod,
                playable_half_extent,
                playable_ground,
                point,
                spacing * 0.58,
                urban_ground,
            ) else {
                continue;
            };
            let local_profile = profile.localized(sample);
            let mesh = &meshes[grass_community_at(point, community_seed, local_profile) as usize]
                [topology.index()];
            let Some(transform) = vista_scatter_transform(
                lod,
                coarser_lod,
                playable_terrain,
                playable_half_extent,
                point,
                hash,
                0.0,
            ) else {
                continue;
            };
            commands.spawn((
                Name::new("Tactical vista grass patch"),
                VistaTerrain(lod.level),
                VistaGrassPresentation,
                GroundScatterLayer::Grass,
                NotShadowCaster,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                visibility.clone(),
                transform,
            ));
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn spawn_vista_rocks(
    commands: &mut Commands,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_half_extent: Vec2,
    playable_terrain: &SceneTerrain,
    scene_seed: u64,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let recipe = RockRecipe {
        seed: 0x7669_7374_615f_726f,
        archetype: RockArchetype::Rounded,
        lithology: RockLithology::Granite,
        dimensions_cm: [120, 92, 108],
        collision_radius_cm: 80,
    };
    let near_mesh = meshes.add(super::obstacles::rock::procedural_rock_mesh(recipe));
    let far_mesh = meshes.add(vista_rock_mesh());
    let material = materials.add(StandardMaterial {
        base_color: super::obstacles::rock::rock_color(RockLithology::Granite),
        perceptual_roughness: 0.92,
        ..default()
    });
    let spacing = 24.0;
    let outer = playable_half_extent + Vec2::splat(420.0);
    let minimum = (-outer / spacing).floor().as_ivec2();
    let maximum = (outer / spacing).ceil().as_ivec2();
    for z in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(scene_seed ^ cell ^ 0x726f_636b_5f76_6973);
            let jitter = Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash)) - 0.5)
                * spacing
                * 0.72;
            let point = Vec2::new(x as f32, z as f32) * spacing + jitter;
            if point.x.abs() <= playable_half_extent.x + 2.0
                && point.y.abs() <= playable_half_extent.y + 2.0
            {
                continue;
            }
            let Some(sample) = sample_vista_environment(lod, point) else {
                continue;
            };
            let exposed = bps(sample.hilly_bps)
                * (1.0 - bps(sample.water_bps))
                * (1.0 - bps(sample.wetland_bps) * 0.75)
                * (1.0 - bps(sample.canopy_bps) * 0.42);
            if unit_hash(splitmix64(hash ^ 0xa880_2dd1)) > exposed * 0.46 {
                continue;
            }
            let lift = 0.08;
            let Some(mut transform) = vista_scatter_transform(
                lod,
                coarser_lod,
                playable_terrain,
                playable_half_extent,
                point,
                hash,
                lift,
            ) else {
                continue;
            };
            let scale = 0.55 + unit_hash(splitmix64(hash ^ 0x9137_b22c)) * 1.35;
            transform.scale = Vec3::new(scale, scale * 0.72, scale * 0.9);
            for (name, mesh, visibility) in [
                (
                    "Tactical vista rock mesh",
                    near_mesh.clone(),
                    VisibilityRange {
                        start_margin: 0.0..0.0,
                        end_margin: 90.0..112.0,
                        use_aabb: false,
                    },
                ),
                (
                    "Tactical vista rock low LOD",
                    far_mesh.clone(),
                    VisibilityRange {
                        start_margin: 88.0..110.0,
                        end_margin: 360.0..430.0,
                        use_aabb: false,
                    },
                ),
            ] {
                commands.spawn((
                    Name::new(name),
                    VistaTerrain(lod.level),
                    VistaRockPresentation,
                    NotShadowCaster,
                    Mesh3d(mesh),
                    MeshMaterial3d(material.clone()),
                    visibility,
                    transform,
                ));
            }
        }
    }
}

fn vista_scatter_transform(
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: &SceneTerrain,
    playable_half_extent: Vec2,
    point: Vec2,
    hash: u64,
    lift: f32,
) -> Option<Transform> {
    let origin = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let local = point - origin;
    let height = presented_vista_vertex_height(
        lod,
        coarser_lod,
        Some(playable_terrain),
        local,
        playable_half_extent,
    )?;
    let delta = 2.0;
    let at = |offset: Vec2| {
        presented_vista_vertex_height(
            lod,
            coarser_lod,
            Some(playable_terrain),
            local + offset,
            playable_half_extent,
        )
        .unwrap_or(height)
    };
    let tangent_x = Vec3::new(delta * 2.0, at(Vec2::X * delta) - at(-Vec2::X * delta), 0.0);
    let tangent_z = Vec3::new(0.0, at(Vec2::Y * delta) - at(-Vec2::Y * delta), delta * 2.0);
    let normal = tangent_z.cross(tangent_x).normalize_or_zero();
    if normal.y < 0.72 {
        return None;
    }
    Some(
        Transform::from_xyz(point.x, height + lift, point.y).with_rotation(
            Quat::from_rotation_arc(Vec3::Y, normal)
                * Quat::from_rotation_y(
                    unit_hash(splitmix64(hash ^ 0x55d8_093b)) * core::f32::consts::TAU,
                ),
        ),
    )
}

fn sample_vista_environment(lod: &VistaLod, world: Vec2) -> Option<EnvironmentalSample> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let origin = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let coordinate = (world - origin) / lod.spacing_metres
        + Vec2::new((width - 1) as f32 * 0.5, (depth - 1) as f32 * 0.5);
    if coordinate.x < 0.0
        || coordinate.y < 0.0
        || coordinate.x > (width - 1) as f32
        || coordinate.y > (depth - 1) as f32
    {
        return None;
    }
    let nearest = coordinate.round().as_uvec2();
    lod.environment
        .get(nearest.y as usize * width + nearest.x as usize)
        .copied()
}

fn vista_rock_mesh() -> Mesh {
    let vertices = [
        Vec3::new(-0.58, -0.36, -0.48),
        Vec3::new(0.52, -0.36, -0.44),
        Vec3::new(0.61, -0.28, 0.42),
        Vec3::new(-0.49, -0.31, 0.55),
        Vec3::new(-0.37, 0.31, -0.33),
        Vec3::new(0.32, 0.42, -0.29),
        Vec3::new(0.39, 0.27, 0.31),
        Vec3::new(-0.31, 0.36, 0.38),
    ];
    let faces = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    let mut positions = Vec::with_capacity(faces.len() * 3);
    let mut normals = Vec::with_capacity(faces.len() * 3);
    for [a, b, c] in faces {
        let normal = (vertices[b] - vertices[a])
            .cross(vertices[c] - vertices[a])
            .normalize_or_zero();
        positions.extend([
            vertices[a].to_array(),
            vertices[b].to_array(),
            vertices[c].to_array(),
        ]);
        normals.extend([normal.to_array(); 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn spawn_vista_trees(
    commands: &mut Commands,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_half_extent: Vec2,
    scene_digest: &str,
    environment: Option<&SceneEnvironment>,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    cache: &mut VistaTreePresentationCache,
) {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let center = Vec2::new((width - 1) as f32, (depth - 1) as f32) * 0.5;
    let scene_seed = stable_text_seed(scene_digest);
    for z in 0..depth - 1 {
        for x in 0..width - 1 {
            let sample = lod.environment[z * width + x];
            let canopy = bps(sample.canopy_bps)
                * (1.0 - bps(sample.water_bps))
                * (1.0 - bps(sample.cultivation_bps) * 0.85);
            // A regional source cell represents a stand, not individual
            // stems. Keep a physical-area-scaled silhouette sample; the
            // terrain material carries the remaining aggregate canopy.
            let cell_key = ((x as u64) << 32) | z as u64;
            let candidate_count = vista_tree_candidate_count(
                canopy,
                lod.spacing_metres,
                splitmix64(scene_seed ^ cell_key ^ 0x74c3_019d),
            )
            .min(if lod.spacing_metres <= 250.0 { 24 } else { 3 });
            if candidate_count == 0 {
                continue;
            }
            let cell_min = (Vec2::new(x as f32, z as f32) - center) * lod.spacing_metres;
            for candidate in 0..candidate_count {
                let hash = splitmix64(
                    scene_seed ^ cell_key ^ (candidate as u64).wrapping_mul(0x9e37_79b9),
                );
                let local = cell_min
                    + Vec2::new(unit_hash(hash), unit_hash(splitmix64(hash ^ 0x51b7_2d8a)))
                        * lod.spacing_metres;
                if local.x.abs() <= playable_half_extent.x + 7.0
                    && local.y.abs() <= playable_half_extent.y + 7.0
                {
                    continue;
                }
                let world = local
                    + Vec2::new(
                        lod.origin_east_metres as f32,
                        lod.origin_north_metres as f32,
                    );
                let Some(height) = presented_height_at(lod, world, coarser_lod) else {
                    continue;
                };
                // Vista stands share one calibrated whole-tree atlas. Scale,
                // rotation-independent view selection, and placement still
                // break repetition without baking during every source cell.
                let variant_seed = splitmix64(0x6f61_6b00);
                let species =
                    environment.map_or(TreePresentationSpecies::EnglishOak, |environment| {
                        tree_species_for_site(Vec3::new(local.x, 0.0, local.y), environment)
                    });
                let cached = ensure_vista_tree_variant(
                    variant_seed,
                    0.5,
                    species,
                    meshes,
                    tree_materials,
                    images,
                    cache,
                );
                // Each atlas represents the visible crown mass of a small
                // stand at regional distance, not a survey-accurate stem.
                let scale = vista_tree_scale(
                    lod.spacing_metres,
                    unit_hash(splitmix64(hash ^ 0xa29c_413d)),
                );
                let card_height = cached
                    .provenance
                    .records
                    .first()
                    .map_or(12.0, |record| record.projected_bounds.w);
                commands.spawn((
                    Name::new(format!("Distant vista {} billboard", species.name())),
                    VistaTerrain(lod.level),
                    VistaTreePresentation,
                    // The impostor shader yaws the card toward the camera, so
                    // the mesh's static bounds would mis-cull near screen
                    // edges. This rotation-safe box restores frustum culling:
                    // off-screen stands previously always rendered through
                    // `NoFrustumCulling`, roughly half the vista vertex cost.
                    bevy::camera::primitives::Aabb {
                        center: bevy::math::Vec3A::new(0.0, card_height * 0.5, 0.0),
                        half_extents: bevy::math::Vec3A::new(
                            card_height * 0.8,
                            card_height * 0.6,
                            card_height * 0.8,
                        ),
                    },
                    NotShadowCaster,
                    Mesh3d(cached.mesh.clone()),
                    MeshMaterial3d(cached.material.clone()),
                    cached.provenance.clone(),
                    vista_tree_visibility(lod.spacing_metres, card_height, scale),
                    Transform::from_xyz(local.x, height, local.y).with_scale(Vec3::splat(scale)),
                ));
            }
        }
    }
}

const VISTA_TREE_MAX_ANGULAR_HEIGHT_RADIANS: f32 = 16.0_f32.to_radians();

fn vista_tree_scale(spacing_metres: f32, variation: f32) -> f32 {
    // Candidate count already represents regional stand density. Each card
    // must remain one plausible tree; scaling a single trunk into a 32-metre
    // stand creates the near-ring columns seen from the playable scene.
    let coarse_scale = if spacing_metres <= 250.0 { 1.0 } else { 1.25 };
    (0.85 + variation.clamp(0.0, 1.0) * 0.4) * coarse_scale
}

fn vista_tree_visibility(
    spacing_metres: f32,
    unscaled_card_height: f32,
    scale: f32,
) -> VisibilityRange {
    let scaled_height = unscaled_card_height.max(0.0) * scale.max(0.0);
    // Keep regional stand cards out of the near field. The first visible
    // sample is at most 16 degrees; the fade completes farther away. Using
    // this distance as the end of the fade admitted partially visible 17-19
    // degree cards, contradicting the background-size contract.
    let first_visible_distance =
        scaled_height / (2.0 * (VISTA_TREE_MAX_ANGULAR_HEIGHT_RADIANS * 0.5).tan());
    VisibilityRange {
        start_margin: first_visible_distance..(first_visible_distance * 1.12),
        end_margin: if spacing_metres <= 250.0 {
            1_600.0..1_900.0
        } else {
            4_600.0..5_200.0
        },
        use_aabb: false,
    }
}

fn vista_tree_candidate_count(canopy: f32, spacing_metres: f32, seed: u64) -> usize {
    let expected = canopy.clamp(0.0, 1.0) * spacing_metres * spacing_metres / 3_200.0;
    expected.floor() as usize + usize::from(unit_hash(seed) < expected.fract())
}

#[cfg(test)]
pub(super) fn vista_lod_meshes(lod: &VistaLod, inner_half_extent: Vec2) -> Vec<Mesh> {
    vista_lod_meshes_with_morph(
        lod,
        inner_half_extent,
        None,
        None,
        None,
        clear_vista_weather(),
    )
}

fn vista_lod_meshes_with_morph(
    lod: &VistaLod,
    inner_half_extent: Vec2,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: Option<&SceneTerrain>,
    playable_environment: Option<&SceneEnvironment>,
    weather: WeatherSnapshot,
) -> Vec<Mesh> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    if width < 2
        || depth < 2
        || width.checked_mul(depth).is_none_or(|samples| {
            lod.heights_metres.len() != samples || lod.environment.len() != samples
        })
        || !lod.spacing_metres.is_finite()
        || lod.spacing_metres <= 0.0
    {
        return Vec::new();
    }
    let center_x = (width - 1) as f32 * 0.5;
    let center_z = (depth - 1) as f32 * 0.5;
    // Keep at least two chunks across the longer axis of small coarse rings so
    // they retain useful frustum-culling granularity.
    let chunk_cells = VISTA_CHUNK_CELLS.min((width.max(depth) - 1).div_ceil(2).max(1));
    let mut meshes = Vec::new();
    for chunk_z in (0..depth - 1).step_by(chunk_cells) {
        for chunk_x in (0..width - 1).step_by(chunk_cells) {
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut colors = Vec::new();
            let mut indices = Vec::new();
            for z in chunk_z..(chunk_z + chunk_cells).min(depth - 1) {
                for x in chunk_x..(chunk_x + chunk_cells).min(width - 1) {
                    let cell_min = Vec2::new(
                        (x as f32 - center_x) * lod.spacing_metres,
                        (z as f32 - center_z) * lod.spacing_metres,
                    );
                    let cell_max = cell_min + Vec2::splat(lod.spacing_metres);
                    for rectangle in cell_rectangles_outside_inner_rectangle(
                        cell_min,
                        cell_max,
                        inner_half_extent,
                    ) {
                        for [minimum_x, maximum_x, minimum_z, maximum_z] in
                            subdivide_playable_boundary_rectangle(
                                rectangle,
                                inner_half_extent,
                                playable_terrain,
                            )
                        {
                            let vertex = |local: Vec2| {
                                let height = presented_vista_vertex_height(
                                    lod,
                                    coarser_lod,
                                    playable_terrain,
                                    local,
                                    inner_half_extent,
                                )
                                .expect("clipped vista vertex remains inside its source LOD");
                                let delta = lod.spacing_metres.min(100.0);
                                let height_offset = |offset: Vec2| {
                                    presented_vista_vertex_height(
                                        lod,
                                        coarser_lod,
                                        playable_terrain,
                                        local + offset,
                                        inner_half_extent,
                                    )
                                    .unwrap_or(height)
                                };
                                let tangent_x = Vec3::new(
                                    delta * 2.0,
                                    height_offset(Vec2::X * delta)
                                        - height_offset(-Vec2::X * delta),
                                    0.0,
                                );
                                let tangent_z = Vec3::new(
                                    0.0,
                                    height_offset(Vec2::Y * delta)
                                        - height_offset(-Vec2::Y * delta),
                                    delta * 2.0,
                                );
                                (
                                    [local.x, height, local.y],
                                    tangent_z.cross(tangent_x).normalize().to_array(),
                                    presented_vista_vertex_color(
                                        lod,
                                        coarser_lod,
                                        playable_environment,
                                        local,
                                        inner_half_extent,
                                        weather,
                                    )
                                    .expect("clipped vista color remains inside its source LOD"),
                                )
                            };
                            let base = positions.len() as u32;
                            let vertices = [
                                vertex(Vec2::new(minimum_x, minimum_z)),
                                vertex(Vec2::new(maximum_x, minimum_z)),
                                vertex(Vec2::new(maximum_x, maximum_z)),
                                vertex(Vec2::new(minimum_x, maximum_z)),
                            ];
                            positions.extend(vertices.map(|vertex| vertex.0));
                            normals.extend(vertices.map(|vertex| vertex.1));
                            colors.extend(vertices.map(|vertex| vertex.2));
                            indices.extend_from_slice(&[
                                base,
                                base + 2,
                                base + 1,
                                base,
                                base + 3,
                                base + 2,
                            ]);
                        }
                    }
                }
            }
            if positions.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh);
        }
    }
    meshes
}

fn presented_vista_vertex_color(
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_environment: Option<&SceneEnvironment>,
    local: Vec2,
    playable_half_extent: Vec2,
    weather: WeatherSnapshot,
) -> Option<[f32; 4]> {
    let world = local
        + Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let vista_color = Vec4::from_array(presented_color_at(lod, world, coarser_lod, weather)?);
    Some(
        playable_environment
            .map(|environment| {
                stitch_vista_color_to_playable_edge(
                    local,
                    playable_half_extent,
                    // Ground-cover proportions summarize a wider ecological
                    // patch than height samples. Ease pigment over several
                    // vista cells so the playable rectangle cannot read as a
                    // terrain tile from an overhead or grazing camera.
                    lod.spacing_metres * 4.0,
                    vista_color,
                    Vec4::from_array(scene_ground_color(environment).to_linear().to_f32_array()),
                )
            })
            .unwrap_or(vista_color)
            .to_array(),
    )
}

fn presented_vista_vertex_height(
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: Option<&SceneTerrain>,
    local: Vec2,
    playable_half_extent: Vec2,
) -> Option<f32> {
    let world = local
        + Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let vista_height = presented_height_at(lod, world, coarser_lod)?;
    Some(playable_terrain.map_or(vista_height, |terrain| {
        stitch_vista_height_to_playable_edge(
            terrain,
            local,
            playable_half_extent,
            lod.spacing_metres,
            vista_height,
        )
    }))
}

fn subdivide_playable_boundary_rectangle(
    rectangle: [f32; 4],
    playable_half_extent: Vec2,
    terrain: Option<&SceneTerrain>,
) -> Vec<[f32; 4]> {
    let Some(terrain) = terrain else {
        return vec![rectangle];
    };
    let [minimum_x, maximum_x, minimum_z, maximum_z] = rectangle;
    let epsilon = terrain.grid_scale() * 0.01;
    if (minimum_x - playable_half_extent.x).abs() <= epsilon
        || (maximum_x + playable_half_extent.x).abs() <= epsilon
    {
        return split_rectangle_axis(
            rectangle,
            1,
            terrain.depth() * -0.5,
            terrain.grid_scale(),
            terrain.grid_depth(),
        );
    }
    if (minimum_z - playable_half_extent.y).abs() <= epsilon
        || (maximum_z + playable_half_extent.y).abs() <= epsilon
    {
        return split_rectangle_axis(
            rectangle,
            0,
            terrain.width() * -0.5,
            terrain.grid_scale(),
            terrain.grid_width(),
        );
    }
    vec![rectangle]
}

fn split_rectangle_axis(
    rectangle: [f32; 4],
    axis: usize,
    terrain_minimum: f32,
    spacing: f32,
    sample_count: usize,
) -> Vec<[f32; 4]> {
    let (minimum, maximum) = if axis == 0 {
        (rectangle[0], rectangle[1])
    } else {
        (rectangle[2], rectangle[3])
    };
    let mut boundaries = vec![minimum, maximum];
    boundaries.extend((0..sample_count).filter_map(|index| {
        let coordinate = terrain_minimum + index as f32 * spacing;
        (coordinate > minimum && coordinate < maximum).then_some(coordinate)
    }));
    boundaries.sort_by(f32::total_cmp);
    boundaries.dedup_by(|left, right| (*left - *right).abs() < spacing * 0.001);
    boundaries
        .windows(2)
        .map(|interval| {
            let mut split = rectangle;
            if axis == 0 {
                split[0] = interval[0];
                split[1] = interval[1];
            } else {
                split[2] = interval[0];
                split[3] = interval[1];
            }
            split
        })
        .collect()
}

fn stitch_vista_height_to_playable_edge(
    terrain: &SceneTerrain,
    local: Vec2,
    playable_half_extent: Vec2,
    transition_width: f32,
    vista_height: f32,
) -> f32 {
    let boundary = local.clamp(-playable_half_extent, playable_half_extent);
    let Some(playable_height) = terrain.height_at(boundary) else {
        return vista_height;
    };
    let outside_distance = (local.abs() - playable_half_extent)
        .max(Vec2::ZERO)
        .max_element();
    let vista_weight = (outside_distance / transition_width.max(f32::EPSILON)).clamp(0.0, 1.0);
    playable_height.lerp(vista_height, vista_weight)
}

fn stitch_vista_color_to_playable_edge(
    local: Vec2,
    playable_half_extent: Vec2,
    transition_width: f32,
    vista_color: Vec4,
    playable_color: Vec4,
) -> Vec4 {
    let outside_distance = (local.abs() - playable_half_extent)
        .max(Vec2::ZERO)
        .max_element();
    let vista_weight = (outside_distance / transition_width.max(f32::EPSILON)).clamp(0.0, 1.0);
    let mut stitched = playable_color.lerp(vista_color, vista_weight);
    // Alpha carries distant geometric-sward coverage, not material opacity.
    // Preserve it while blending only the molded substrate pigment.
    stitched.w = vista_color.w;
    stitched
}

fn cell_rectangles_outside_inner_rectangle(
    minimum: Vec2,
    maximum: Vec2,
    inner_half_extent: Vec2,
) -> Vec<[f32; 4]> {
    if inner_half_extent.x <= 0.0
        || inner_half_extent.y <= 0.0
        || maximum.x <= -inner_half_extent.x
        || minimum.x >= inner_half_extent.x
        || maximum.y <= -inner_half_extent.y
        || minimum.y >= inner_half_extent.y
    {
        return vec![[minimum.x, maximum.x, minimum.y, maximum.y]];
    }
    if minimum.x >= -inner_half_extent.x
        && maximum.x <= inner_half_extent.x
        && minimum.y >= -inner_half_extent.y
        && maximum.y <= inner_half_extent.y
    {
        return Vec::new();
    }

    let mut rectangles = Vec::with_capacity(4);
    if minimum.x < -inner_half_extent.x {
        rectangles.push([
            minimum.x,
            maximum.x.min(-inner_half_extent.x),
            minimum.y,
            maximum.y,
        ]);
    }
    if maximum.x > inner_half_extent.x {
        rectangles.push([
            minimum.x.max(inner_half_extent.x),
            maximum.x,
            minimum.y,
            maximum.y,
        ]);
    }
    let middle_minimum_x = minimum.x.max(-inner_half_extent.x);
    let middle_maximum_x = maximum.x.min(inner_half_extent.x);
    if middle_minimum_x < middle_maximum_x {
        if minimum.y < -inner_half_extent.y {
            rectangles.push([
                middle_minimum_x,
                middle_maximum_x,
                minimum.y,
                maximum.y.min(-inner_half_extent.y),
            ]);
        }
        if maximum.y > inner_half_extent.y {
            rectangles.push([
                middle_minimum_x,
                middle_maximum_x,
                minimum.y.max(inner_half_extent.y),
                maximum.y,
            ]);
        }
    }
    rectangles
}

#[cfg(test)]
fn presented_height(
    lod: &VistaLod,
    x: usize,
    z: usize,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
) -> f32 {
    let own = lod.heights_metres[z * usize::from(lod.width) + x];
    let Some(coarser) = coarser_lod else {
        return own;
    };
    let weight = lod_transition_weight(lod, coarser, world);
    sample_vista_height(coarser, world)
        .map(|height| own.lerp(height, weight))
        .unwrap_or(own)
}

fn presented_height_at(lod: &VistaLod, world: Vec2, coarser_lod: Option<&VistaLod>) -> Option<f32> {
    let own = sample_vista_height(lod, world)?;
    let Some(coarser) = coarser_lod else {
        return Some(own);
    };
    let weight = lod_transition_weight(lod, coarser, world);
    if weight <= 0.0 {
        return Some(own);
    }
    Some(
        sample_vista_height(coarser, world)
            .map(|height| own.lerp(height, weight))
            .unwrap_or(own),
    )
}

#[cfg(test)]
fn presented_color(
    lod: &VistaLod,
    x: usize,
    z: usize,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
    weather: WeatherSnapshot,
) -> [f32; 4] {
    let own = vista_sample_color(lod.environment[z * usize::from(lod.width) + x], weather);
    let Some(coarser) = coarser_lod else {
        return own.to_array();
    };
    let weight = lod_transition_weight(lod, coarser, world);
    sample_vista_color(coarser, world, weather)
        .map(|color| own.lerp(color, weight))
        .unwrap_or(own)
        .to_array()
}

fn presented_color_at(
    lod: &VistaLod,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
    weather: WeatherSnapshot,
) -> Option<[f32; 4]> {
    let own = sample_vista_color(lod, world, weather)?;
    let Some(coarser) = coarser_lod else {
        return Some(own.to_array());
    };
    let weight = lod_transition_weight(lod, coarser, world);
    Some(
        sample_vista_color(coarser, world, weather)
            .map(|color| own.lerp(color, weight))
            .unwrap_or(own)
            .to_array(),
    )
}

fn lod_transition_weight(lod: &VistaLod, coarser: &VistaLod, world: Vec2) -> f32 {
    let center = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
    // Begin morphing one coarse sample before the boundary. A one-fine-cell
    // band still exposes the square footprint whenever adjacent LOD spacing
    // grows rapidly (50 m -> 250 m -> 1 km).
    let transition_width = coarser
        .spacing_metres
        .min(half_extent)
        .max(lod.spacing_metres);
    let radius = (world - center).abs().max_element();
    ((radius - (half_extent - transition_width)) / transition_width).clamp(0.0, 1.0)
}

fn sample_vista_height(lod: &VistaLod, world: Vec2) -> Option<f32> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let local = world
        - Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let coordinate =
        local / lod.spacing_metres + Vec2::new((width - 1) as f32 * 0.5, (depth - 1) as f32 * 0.5);
    if coordinate.x < 0.0
        || coordinate.y < 0.0
        || coordinate.x > (width - 1) as f32
        || coordinate.y > (depth - 1) as f32
    {
        return None;
    }
    let lower = coordinate.floor().as_uvec2();
    let upper = (lower + UVec2::ONE).min(UVec2::new(width as u32 - 1, depth as u32 - 1));
    let fraction = coordinate.fract();
    let at = |x: u32, z: u32| lod.heights_metres[z as usize * width + x as usize];
    let near = at(lower.x, lower.y).lerp(at(upper.x, lower.y), fraction.x);
    let far = at(lower.x, upper.y).lerp(at(upper.x, upper.y), fraction.x);
    Some(near.lerp(far, fraction.y))
}

fn vista_sample_color(sample: EnvironmentalSample, weather: WeatherSnapshot) -> Vec4 {
    let environment = SceneEnvironment {
        scene_digest: String::new(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: 12 * 60,
        lunar_phase_minute: 12 * 60,
        absolute_elevation_metres: 20,
        weather,
        canopy_bps: sample.canopy_bps,
        wetland_bps: sample.wetland_bps,
        cultivation_bps: sample.cultivation_bps,
        water_bps: sample.water_bps,
        hilly_bps: sample.hilly_bps,
    };
    let mut color = Vec4::from_array(scene_ground_color(&environment).to_linear().to_f32_array());
    let hills = bps(sample.hilly_bps);
    let snow = bps(weather.snow_cover_bps);
    let exposed_rock = hills
        * (1.0 - bps(sample.water_bps))
        * (1.0 - bps(sample.wetland_bps) * 0.8)
        * (1.0 - bps(sample.canopy_bps) * 0.45)
        * (1.0 - snow);
    let rock = Color::srgb_u8(104, 101, 91).to_linear().to_f32_array();
    color = color.lerp(Vec4::from_array(rock), exposed_rock * 0.62);
    color.w = vista_sward_coverage(sample) * (1.0 - snow * 0.92);
    color
}

fn vista_sward_coverage(sample: EnvironmentalSample) -> f32 {
    let surface = match sample.surface {
        TacticalSurface::Open | TacticalSurface::SparseWoods => 1.0,
        TacticalSurface::DeepWoods => 0.28,
        TacticalSurface::Wetland => 0.42,
        TacticalSurface::Road | TacticalSurface::Water => 0.0,
    };
    (surface
        * (1.0 - bps(sample.water_bps))
        * (1.0 - bps(sample.cultivation_bps) * 0.72)
        * (1.0 - bps(sample.hilly_bps) * 0.82))
        .clamp(0.0, 1.0)
}

fn sample_vista_color(lod: &VistaLod, world: Vec2, weather: WeatherSnapshot) -> Option<Vec4> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let local = world
        - Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let coordinate =
        local / lod.spacing_metres + Vec2::new((width - 1) as f32 * 0.5, (depth - 1) as f32 * 0.5);
    if coordinate.x < 0.0
        || coordinate.y < 0.0
        || coordinate.x > (width - 1) as f32
        || coordinate.y > (depth - 1) as f32
    {
        return None;
    }
    let lower = coordinate.floor().as_uvec2();
    let upper = (lower + UVec2::ONE).min(UVec2::new(width as u32 - 1, depth as u32 - 1));
    let fraction = coordinate.fract();
    let at = |x: u32, z: u32| {
        vista_sample_color(lod.environment[z as usize * width + x as usize], weather)
    };
    let near = at(lower.x, lower.y).lerp(at(upper.x, lower.y), fraction.x);
    let far = at(lower.x, upper.y).lerp(at(upper.x, upper.y), fraction.x);
    Some(near.lerp(far, fraction.y))
}

fn clear_vista_weather() -> WeatherSnapshot {
    WeatherSnapshot {
        rules_version: WEATHER_RULES_VERSION,
        interval_start_minute: 0,
        cell_latitude: 0,
        cell_longitude: 0,
        temperature_deci_c: 100,
        wind_speed_bps: 0,
        precipitation: Precipitation::Clear,
        intensity_bps: 0,
        ground_moisture_bps: 0,
        snow_cover_bps: 0,
        atmosphere: Default::default(),
    }
}

#[derive(Component)]
pub(crate) struct VistaTerrain(pub(crate) u8);

/// A terrain-surface chunk, excluding vista grass, rocks, and tree cards that
/// also carry [`VistaTerrain`] for broad visibility isolation.
#[derive(Component)]
pub(crate) struct VistaTerrainMesh(pub(crate) u8);

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalVistaExtension {
    #[uniform(100)]
    weather: Vec4,
    #[uniform(100)]
    grass_color: Vec4,
}

impl MaterialExtension for TacticalVistaExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/tactical_vista.wgsl".into()
    }
}

pub(in crate::presentation) type TacticalVistaMaterial =
    ExtendedMaterial<StandardMaterial, TacticalVistaExtension>;

fn vista_material(weather: WeatherSnapshot, grass_color: Color) -> TacticalVistaMaterial {
    TacticalVistaMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.94,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalVistaExtension {
            weather: Vec4::new(
                bps(weather.ground_moisture_bps),
                bps(weather.snow_cover_bps),
                bps(weather.wind_speed_bps),
                0.0,
            ),
            grass_color: color_vec4(grass_color),
        },
    }
}

// Chunking is a CPU/ECS submission boundary, not a visual tessellation
// boundary. Thirty-two cells retains the exact terrain vertices while cutting
// a 50 km three-ring vista to about one sixteenth as many render entities.
const VISTA_CHUNK_CELLS: usize = 32;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn vista_ground_uses_continuous_palette_colors_and_geometry_normals() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_vista.wgsl"
        ));
        assert!(!shader.contains("texture_2d"));
        assert!(!shader.contains("textureSample"));
        assert!(!shader.contains("composed_normal"));
        assert!(shader.contains("let sward_color = vista.grass_color.rgb"));
        assert!(shader.contains("color = mix(color, sward_target, sward)"));
        assert!(!shader.contains("sward_color = color *"));
        assert!(shader.contains("let molded_rock = vec3<f32>(0.31, 0.30, 0.275)"));
    }

    #[test]
    fn vista_terminal_sward_uses_the_optically_compensated_grass_pigment() {
        let pigment = Color::srgb_u8(91, 126, 47);
        let material = vista_material(clear_vista_weather(), pigment);
        assert_eq!(material.extension.grass_color, color_vec4(pigment));
    }

    #[test]
    fn vista_grass_reuses_the_playable_terminal_sward_handoff() {
        // `spawn_near_vista_scatter` uses this same range for its globally
        // aligned lattice, so the playable-to-vista seam cannot extend the
        // physical-grass budget beyond the terrain handoff.
        let vista = grass_lod_visibility(GrassMeshLod::Vista);
        assert_eq!(vista.end_margin, 42.0..50.0);
        assert_eq!(vista.end_margin.start, TERMINAL_SWARD_FADE_START_METRES);
        assert_eq!(vista.end_margin.end, TERMINAL_SWARD_FADE_END_METRES);
    }

    #[test]
    fn retained_near_vista_surface_continues_detail_patch_across_playable_bounds() {
        let terrain =
            SceneTerrain::from_heightmap(3, 3, 2.0, vec![10.0; 9]).expect("playable terrain");
        let lod = VistaLod {
            level: 0,
            spacing_metres: 2.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![20.0; 25],
            environment: vec![EnvironmentalSample::default(); 25],
        };
        let retained = ActiveVistaSurface {
            revision: 1,
            scene_digest: "boundary".into(),
            playable_half_extent: Vec2::splat(2.0),
            lods: vec![lod],
        };
        assert_eq!(
            retained.presented_height_at("boundary", &terrain, Vec2::new(2.0, 0.0)),
            Some(10.0)
        );
        let outside = retained
            .presented_height_at("boundary", &terrain, Vec2::new(3.0, 0.0))
            .unwrap();
        assert!((outside - 15.0).abs() < 0.0001, "{outside}");
        assert_eq!(
            retained.presented_height_at("different", &terrain, Vec2::new(3.0, 0.0)),
            None
        );
    }

    #[test]
    fn vista_rock_lod_is_a_bounded_twelve_face_silhouette() {
        let mesh = vista_rock_mesh();
        assert_eq!(mesh.count_vertices(), 12 * 3);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("vista rock positions");
        assert!(
            positions
                .iter()
                .all(|position| Vec3::from_array(*position).length() < 1.0)
        );
    }

    #[test]
    fn vista_lods_build_independent_overlapping_rings() {
        let input = TacticalSceneInput::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/tactical-scenes/valley-distant-ridge.json"),
        )
        .unwrap();
        let mut inner = Vec2::splat(55.0);
        for (index, lod) in input.vista.lods.iter().enumerate() {
            let meshes = vista_lod_meshes(lod, inner);
            assert!(!meshes.is_empty());
            assert!(meshes.iter().all(|mesh| mesh.count_vertices() > 0));
            assert!(meshes.iter().all(|mesh| {
                mesh.count_vertices() <= VISTA_CHUNK_CELLS * VISTA_CHUNK_CELLS * 4 * 4
            }));
            if index > 0 {
                assert!(
                    meshes.len() > 1,
                    "regional LODs must be independently culled"
                );
            }
            inner = Vec2::new(
                f32::from(lod.width - 1) * lod.spacing_metres * 0.5,
                f32::from(lod.depth - 1) * lod.spacing_metres * 0.5,
            );
        }
    }

    #[test]
    fn coarse_vista_cells_are_clipped_to_the_playable_hole() {
        let lod = VistaLod {
            level: 0,
            spacing_metres: 250.0,
            width: 9,
            depth: 9,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![8.0; 81],
            environment: vec![EnvironmentalSample::default(); 81],
        };
        let inner_half_extent = Vec2::new(55.0, 42.0);
        let meshes = vista_lod_meshes(&lod, inner_half_extent);
        let mut touches_boundary = false;
        for mesh in meshes {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("vista mesh must expose Float32x3 positions");
            };
            for quad in positions.as_chunks::<4>().0 {
                let outside = quad
                    .iter()
                    .all(|position| position[0] <= -inner_half_extent.x)
                    || quad
                        .iter()
                        .all(|position| position[0] >= inner_half_extent.x)
                    || quad
                        .iter()
                        .all(|position| position[2] <= -inner_half_extent.y)
                    || quad
                        .iter()
                        .all(|position| position[2] >= inner_half_extent.y);
                assert!(
                    outside,
                    "vista quad overlaps the playable terrain: {quad:?}"
                );
                touches_boundary |= quad.iter().any(|position| {
                    (position[0].abs() - inner_half_extent.x).abs() < 0.001
                        || (position[2].abs() - inner_half_extent.y).abs() < 0.001
                });
            }
        }
        assert!(
            touches_boundary,
            "coarse cells must be split at the exact playable boundary"
        );
    }

    #[test]
    fn first_vista_ring_stitches_to_playable_height_then_blends_outward() {
        let terrain = SceneTerrain::from_heightmap(
            3,
            3,
            50.0,
            vec![12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0],
        )
        .unwrap();
        let half_extent = Vec2::splat(50.0);

        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(50.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            12.0
        );
        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(175.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            62.0
        );
        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(300.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            112.0
        );
    }

    #[test]
    fn first_vista_ring_stitches_substrate_color_without_changing_sward_coverage() {
        let playable = Vec4::new(0.08, 0.12, 0.04, 1.0);
        let vista = Vec4::new(0.24, 0.31, 0.14, 0.37);
        let half_extent = Vec2::splat(50.0);

        let boundary = stitch_vista_color_to_playable_edge(
            Vec2::new(50.0, 10.0),
            half_extent,
            250.0,
            vista,
            playable,
        );
        let midpoint = stitch_vista_color_to_playable_edge(
            Vec2::new(175.0, 10.0),
            half_extent,
            250.0,
            vista,
            playable,
        );
        let outside = stitch_vista_color_to_playable_edge(
            Vec2::new(300.0, 10.0),
            half_extent,
            250.0,
            vista,
            playable,
        );

        assert_eq!(boundary.truncate(), playable.truncate());
        assert_eq!(midpoint.truncate(), playable.lerp(vista, 0.5).truncate());
        assert_eq!(outside.truncate(), vista.truncate());
        assert_eq!(boundary.w, vista.w);
        assert_eq!(midpoint.w, vista.w);
        assert_eq!(outside.w, vista.w);
    }

    #[test]
    fn first_vista_ring_reuses_every_playable_boundary_sample() {
        let heights = (0..5)
            .flat_map(|z| (0..5).map(move |_| z as f32 * 7.0))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(5, 5, 25.0, heights).unwrap();
        let lod = VistaLod {
            level: 0,
            spacing_metres: 250.0,
            width: 3,
            depth: 3,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![100.0; 9],
            environment: vec![EnvironmentalSample::default(); 9],
        };
        let half_extent = Vec2::splat(50.0);
        let meshes = vista_lod_meshes_with_morph(
            &lod,
            half_extent,
            None,
            Some(&terrain),
            None,
            clear_vista_weather(),
        );
        let mut east_edge = Vec::new();
        for mesh in meshes {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("vista mesh must expose Float32x3 positions");
            };
            east_edge.extend(
                positions
                    .iter()
                    .copied()
                    .filter(|position| (position[0] - half_extent.x).abs() < 0.001),
            );
        }

        for sample in 0..terrain.grid_depth() {
            let z = -half_extent.y + sample as f32 * terrain.grid_scale();
            let expected_height = terrain.height_at(Vec2::new(half_extent.x, z)).unwrap();
            assert!(
                east_edge.iter().any(|position| {
                    (position[2] - z).abs() < 0.001 && (position[1] - expected_height).abs() < 0.001
                }),
                "vista edge omitted playable boundary sample z={z}, height={expected_height}"
            );
        }
    }

    #[test]
    fn finer_ring_morphs_onto_the_coarse_surface_at_its_outer_boundary() {
        let sample = EnvironmentalSample::default();
        let finer = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![12.0; 25],
            environment: vec![sample; 25],
        };
        let coarse = VistaLod {
            level: 1,
            spacing_metres: 20.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![38.0; 25],
            environment: vec![sample; 25],
        };
        assert_eq!(
            presented_height(&finer, 4, 2, Vec2::new(20.0, 0.0), Some(&coarse)),
            38.0
        );
        assert_eq!(
            presented_height(&finer, 2, 2, Vec2::ZERO, Some(&coarse)),
            12.0
        );
        assert_eq!(
            presented_height(&finer, 3, 2, Vec2::new(15.0, 0.0), Some(&coarse)),
            31.5
        );
    }

    #[test]
    fn vista_vertex_colors_reuse_ground_palette_in_linear_space() {
        let open = EnvironmentalSample::default();
        let expected = scene_ground_color(&SceneEnvironment {
            scene_digest: String::new(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 12 * 60,
            lunar_phase_minute: 12 * 60,
            absolute_elevation_metres: 20,
            weather: clear_vista_weather(),
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        })
        .to_linear()
        .to_f32_array();
        assert_eq!(
            vista_sample_color(open, clear_vista_weather()).to_array(),
            expected
        );

        let lod = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 3,
            depth: 3,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 9],
            environment: vec![open; 9],
        };
        assert!(
            vista_lod_meshes(&lod, Vec2::ZERO)
                .iter()
                .all(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some())
        );
    }

    #[test]
    fn distant_sward_respects_surface_and_land_cover() {
        let open = EnvironmentalSample::default();
        let deep_woods = EnvironmentalSample {
            surface: TacticalSurface::DeepWoods,
            ..open
        };
        let mountain = EnvironmentalSample {
            hilly_bps: 10_000,
            ..open
        };
        let road = EnvironmentalSample {
            surface: TacticalSurface::Road,
            ..open
        };
        assert_eq!(vista_sward_coverage(open), 1.0);
        assert!(vista_sward_coverage(deep_woods) < 0.3);
        assert!(vista_sward_coverage(mountain) < 0.2);
        assert_eq!(vista_sward_coverage(road), 0.0);
    }

    #[test]
    fn grass_coverage_stitches_continuously_across_the_playable_boundary() {
        let ground = SceneGround::from_samples(
            3,
            3,
            10.0,
            vec![
                GroundSurface {
                    cover: GroundCover::TallGrass,
                    cover_density_bps: 10_000,
                    cover_height_cm: 82,
                    ..default()
                };
                9
            ],
        )
        .unwrap();
        let deep_woods = EnvironmentalSample {
            surface: TacticalSurface::DeepWoods,
            ..default()
        };
        let lod = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 7,
            depth: 7,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 49],
            environment: vec![deep_woods; 49],
        };
        let (width, depth, mask) = grass_cover_mask_pixels(&ground, 42);
        let coverage = |x| {
            grass_mask::stitched_vista_grass_coverage(
                &lod,
                Vec2::splat(10.0),
                &ground,
                &mask,
                width,
                depth,
                Vec2::new(x, 0.0),
                UrbanGround::new(&[], &[]),
            )
        };
        let boundary = coverage(10.0);
        let just_outside = coverage(10.5);
        let midpoint = coverage(16.0);
        let vista = coverage(22.0);
        assert!(boundary > 0.99);
        assert!((boundary - just_outside).abs() < 0.02);
        assert!(just_outside > midpoint && midpoint > vista);
        assert!((vista - vista_sward_coverage(deep_woods)).abs() < 0.01);

        let street = [CityStreetPatch::Corridor {
            start_metres: Vec2::new(12.0, 0.0),
            end_metres: Vec2::new(28.0, 0.0),
            half_width_metres: 2.0,
            surface: CityStreetSurface::CompactedEarth,
        }];
        assert_eq!(
            grass_mask::stitched_vista_grass_coverage(
                &lod,
                Vec2::splat(10.0),
                &ground,
                &mask,
                width,
                depth,
                Vec2::new(22.0, 0.0),
                UrbanGround::new(&street, &[]),
            ),
            0.0
        );
    }

    #[test]
    fn snow_palette_carries_into_vista_and_suppresses_sward() {
        let open = EnvironmentalSample::default();
        let clear = vista_sample_color(open, clear_vista_weather());
        let snow = vista_sample_color(
            open,
            WeatherSnapshot {
                snow_cover_bps: 10_000,
                precipitation: Precipitation::Snow,
                ..clear_vista_weather()
            },
        );
        assert!(snow.x > clear.x && snow.y > clear.y && snow.z > clear.z);
        assert!(snow.w < 0.1);
    }

    #[test]
    fn vista_tree_density_scales_with_physical_cell_area() {
        let small = (0..64_u64)
            .map(|seed| vista_tree_candidate_count(1.0, 50.0, splitmix64(seed)))
            .sum::<usize>();
        let large = (0..64_u64)
            .map(|seed| vista_tree_candidate_count(1.0, 100.0, splitmix64(seed)))
            .sum::<usize>();
        assert!(small > 0);
        assert!(large >= small * 3);
        assert_eq!(vista_tree_candidate_count(0.0, 250.0, 0), 0);
    }

    #[test]
    fn vista_tree_cards_enter_only_at_background_angular_size() {
        let card_height = 10.8;
        for spacing in [50.0, 250.0, 1_000.0] {
            for variation in [0.0, 0.5, 1.0] {
                let scale = vista_tree_scale(spacing, variation);
                let range = vista_tree_visibility(spacing, card_height, scale);
                let angular_height =
                    2.0 * ((card_height * scale * 0.5) / range.start_margin.start).atan();
                assert!(
                    angular_height <= VISTA_TREE_MAX_ANGULAR_HEIGHT_RADIANS + 0.0001,
                    "spacing={spacing} scale={scale} angle={}",
                    angular_height.to_degrees()
                );
                assert!(range.start_margin.start > 0.0);
                assert!(range.start_margin.end > range.start_margin.start);
                assert!(range.start_margin.end < range.end_margin.start);
            }
        }
        assert!((vista_tree_scale(250.0, 1.0) - 1.25).abs() < 0.0001);
        assert!((vista_tree_scale(1_000.0, 1.0) - 1.5625).abs() < 0.0001);
    }

    #[test]
    fn finer_color_morph_matches_coarse_color_at_outer_boundary() {
        let forest = EnvironmentalSample {
            canopy_bps: 8_000,
            ..default()
        };
        let cultivated = EnvironmentalSample {
            cultivation_bps: 8_000,
            ..default()
        };
        let finer = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 25],
            environment: vec![forest; 25],
        };
        let coarse = VistaLod {
            level: 1,
            spacing_metres: 20.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 25],
            environment: vec![cultivated; 25],
        };
        assert_eq!(
            presented_color(
                &finer,
                4,
                2,
                Vec2::new(20.0, 0.0),
                Some(&coarse),
                clear_vista_weather(),
            ),
            vista_sample_color(cultivated, clear_vista_weather()).to_array()
        );
        assert_eq!(
            presented_color(
                &finer,
                2,
                2,
                Vec2::ZERO,
                Some(&coarse),
                clear_vista_weather(),
            ),
            vista_sample_color(forest, clear_vista_weather()).to_array()
        );
    }
}
