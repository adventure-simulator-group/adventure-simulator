use super::*;

const TERMINAL_SWARD_FADE_START_METRES: f32 = 124.0;
const TERMINAL_SWARD_FADE_END_METRES: f32 = 140.0;

pub(super) fn scene_ground_color(environment: &SceneEnvironment) -> Color {
    let mut rgb = if environment.water_bps >= 5_000 {
        [52.0, 83.0, 98.0]
    } else if environment.wetland_bps >= 4_000 {
        [70.0, 62.0, 43.0]
    } else if environment.canopy_bps >= 5_000 {
        [65.0, 52.0, 32.0]
    } else if environment.cultivation_bps >= 4_000 {
        [116.0, 91.0, 49.0]
    } else {
        [101.0, 82.0, 49.0]
    };
    let snow = f32::from(environment.weather.snow_cover_bps) / 10_000.0;
    let wet = f32::from(environment.weather.ground_moisture_bps) / 10_000.0;
    for channel in &mut rgb {
        *channel *= 1.0 - wet * 0.22;
        *channel = *channel * (1.0 - snow) + 220.0 * snow;
    }
    Color::srgb(rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0)
}

#[derive(Component)]
pub(in crate::presentation) struct ScenePresentationOf(pub(in crate::presentation) Entity);

#[derive(Component)]
pub(crate) struct TerrainMaterialPresentation;

#[derive(Component)]
pub(in crate::presentation) struct PendingTerrainPresentation;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTerrainExtension {
    #[uniform(100)]
    base_color: Vec4,
    #[uniform(100)]
    grass_color: Vec4,
    #[uniform(100)]
    cover: Vec4,
    #[uniform(100)]
    weather: Vec4,
    #[uniform(100)]
    far_sward: Vec4,
    #[texture(101)]
    #[sampler(102)]
    ground_map: Handle<Image>,
}

impl MaterialExtension for TacticalTerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

pub(in crate::presentation) type TacticalTerrainMaterial =
    ExtendedMaterial<StandardMaterial, TacticalTerrainExtension>;

pub(in crate::presentation) fn on_game_scene_added(
    event: On<Add, SceneId>,
    mut commands: Commands,
) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

pub(in crate::presentation) fn present_pending_terrain(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &SceneId,
            &SceneTerrain,
            Option<&SceneEnvironment>,
            Option<&SceneGround>,
        ),
        With<PendingTerrainPresentation>,
    >,
    presentations: Query<(
        &ScenePresentationOf,
        &MeshMaterial3d<TacticalTerrainMaterial>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, id, terrain, environment, ground) in &query {
        let legacy_environment;
        let environment = if let Some(environment) = environment {
            environment
        } else {
            legacy_environment = legacy_scene_environment(id);
            &legacy_environment
        };
        let presented = if let Some((_, handle)) =
            presentations.iter().find(|(source, _)| source.0 == entity)
        {
            if let Some(mut material) = materials.get_mut(&handle.0) {
                *material = terrain_material(environment, ground, &mut images);
                true
            } else {
                false
            }
        } else {
            info!(?entity, "Spawning a scene {id:?}");
            let material = terrain_material(environment, ground, &mut images);
            commands.spawn((
                Name::new(format!("{} terrain mesh", id.0)),
                ScenePresentationOf(entity),
                TerrainMaterialPresentation,
                Mesh3d(meshes.add(terrain.mesh())),
                MeshMaterial3d(materials.add(material)),
            ));
            true
        };
        if presented {
            commands
                .entity(entity)
                .remove::<PendingTerrainPresentation>();
        }
    }
}

pub(in crate::presentation) fn terrain_material(
    environment: &SceneEnvironment,
    ground: Option<&SceneGround>,
    images: &mut Assets<Image>,
) -> TacticalTerrainMaterial {
    TacticalTerrainMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalTerrainExtension {
            base_color: color_vec4(scene_ground_color(environment)),
            // Match the rendered optical average of the sward rather than its
            // brighter pre-lighting blade pigment.
            grass_color: color_vec4(grass_terminal_pigment(environment)),
            cover: Vec4::new(
                bps(environment.canopy_bps),
                bps(environment.wetland_bps),
                bps(environment.cultivation_bps),
                bps(environment.water_bps),
            ),
            weather: Vec4::new(
                bps(environment.weather.ground_moisture_bps),
                bps(environment.weather.snow_cover_bps),
                bps(environment.hilly_bps),
                bps(environment.weather.wind_speed_bps),
            ),
            // Beyond the geometric grass range, retain a band-limited sward
            // response in the terrain material instead of paying for blades
            // that project to less than a pixel. x/y are the fade interval;
            // z is environment-dependent coverage and w is reserved.
            far_sward: Vec4::new(
                TERMINAL_SWARD_FADE_START_METRES,
                TERMINAL_SWARD_FADE_END_METRES,
                (1.0 - bps(environment.water_bps) * 0.9
                    - bps(environment.weather.snow_cover_bps) * 0.8)
                    .clamp(0.0, 1.0),
                0.0,
            ),
            ground_map: images.add(ground_map_image(
                ground,
                stable_text_seed(&environment.scene_digest),
            )),
        },
    }
}

const GROUND_PRESENTATION_SAMPLES_PER_CELL: usize = 6;

fn ground_surface_pixel(sample: GroundSurface) -> [u8; 4] {
    let cover = match sample.cover {
        GroundCover::Bare => 0,
        GroundCover::TallGrass => 1,
        GroundCover::LeafLitter => 2,
        GroundCover::LooseStone => 3,
        GroundCover::Reeds => 4,
    };
    let substrate = match sample.substrate {
        GroundSubstrate::Soil => 0,
        GroundSubstrate::Stone => 1,
        GroundSubstrate::Gravel => 2,
        GroundSubstrate::Mud => 3,
        GroundSubstrate::Road => 4,
        GroundSubstrate::Water => 5,
    };
    [
        cover,
        substrate,
        (u32::from(sample.cover_density_bps) * 255 / 10_000) as u8,
        sample.cover_height_cm.min(255) as u8,
    ]
}

fn chamfer_distance_to(
    width: usize,
    height: usize,
    sources: impl Fn(usize) -> bool,
    maximum: usize,
) -> Vec<usize> {
    let mut distance = (0..width * height)
        .map(|index| if sources(index) { 0 } else { maximum + 1 })
        .collect::<Vec<_>>();
    for z in 0..height {
        for x in 0..width {
            let index = z * width + x;
            for (dx, dz) in [(-1_isize, 0_isize), (0, -1), (-1, -1), (1, -1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx >= 0 && nz >= 0 && nx < width as isize && nz < height as isize {
                    distance[index] = distance[index]
                        .min(distance[nz as usize * width + nx as usize].saturating_add(1));
                }
            }
        }
    }
    for z in (0..height).rev() {
        for x in (0..width).rev() {
            let index = z * width + x;
            for (dx, dz) in [(1_isize, 0_isize), (0, 1), (1, 1), (-1, 1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx >= 0 && nz >= 0 && nx < width as isize && nz < height as isize {
                    distance[index] = distance[index]
                        .min(distance[nz as usize * width + nx as usize].saturating_add(1));
                }
            }
        }
    }
    distance
}

fn encode_canopy_floor_distance(
    ground: &SceneGround,
    width: usize,
    height: usize,
    pixels: &mut [u8],
) {
    let metres_per_pixel = ground.grid_scale() / GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    let inner_radius = (2.2 / metres_per_pixel).ceil().max(1.0) as usize;
    let outer_radius = (4.8 / metres_per_pixel).ceil().max(1.0) as usize;
    let litter = pixels
        .chunks_exact(4)
        .map(|pixel| pixel[0] == GroundCover::LeafLitter as u8)
        .collect::<Vec<_>>();
    let distance_to_litter =
        chamfer_distance_to(width, height, |index| litter[index], outer_radius);
    let distance_to_other =
        chamfer_distance_to(width, height, |index| !litter[index], inner_radius);
    for index in 0..width * height {
        let encoded = if litter[index] {
            let depth = (distance_to_other[index] as f32 / inner_radius as f32).clamp(0.0, 1.0);
            128.0 + depth * 127.0
        } else {
            let proximity =
                (1.0 - distance_to_litter[index] as f32 / outer_radius as f32).clamp(0.0, 1.0);
            proximity * 127.0
        };
        // Alpha is presentation-only. It carries signed distance from the
        // organic litter boundary instead of duplicating gameplay cover
        // height, which remains authoritative in SceneGround.
        pixels[index * 4 + 3] = encoded.round() as u8;
    }
}

fn ground_mask_noise(seed: u64, point: Vec2) -> f32 {
    let cell = point.floor();
    let local = point - cell;
    let curve = local * local * (Vec2::splat(3.0) - local * 2.0);
    let hash = |offset: Vec2| {
        let coordinate = cell + offset;
        let x = i64::from(coordinate.x as i32) as u64;
        let y = i64::from(coordinate.y as i32) as u64;
        unit_hash(splitmix64(
            seed ^ x.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ y.wrapping_mul(0xbf58_476d_1ce4_e5b9),
        ))
    };
    let bottom = hash(Vec2::ZERO).lerp(hash(Vec2::X), curve.x);
    let top = hash(Vec2::Y).lerp(hash(Vec2::ONE), curve.x);
    bottom.lerp(top, curve.y)
}

pub(super) fn organic_ground_pixels(ground: &SceneGround, seed: u64) -> (u32, u32, Vec<u8>) {
    let source_width = ground.grid_width();
    let source_depth = ground.grid_depth();
    let width = (source_width - 1) * GROUND_PRESENTATION_SAMPLES_PER_CELL + 1;
    let depth = (source_depth - 1) * GROUND_PRESENTATION_SAMPLES_PER_CELL + 1;
    let mut pixels = Vec::with_capacity(width * depth * 4);
    let scale = GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    for z in 0..depth {
        for x in 0..width {
            let point = Vec2::new(x as f32 / scale, z as f32 / scale);
            let broad_point = point * 0.38;
            let fine_point = point * 0.93;
            let broad_warp = Vec2::new(
                ground_mask_noise(seed ^ 0x2f31_9a87, broad_point),
                ground_mask_noise(seed ^ 0x91b7_43cd, broad_point + Vec2::new(17.3, -9.1)),
            ) * 2.0
                - Vec2::ONE;
            let fine_warp = Vec2::new(
                ground_mask_noise(seed ^ 0x6d25_e9f1, fine_point + Vec2::new(31.7, 5.9)),
                ground_mask_noise(seed ^ 0xc4ab_1283, fine_point + Vec2::new(-7.7, 23.1)),
            ) * 2.0
                - Vec2::ONE;
            let warped = point + broad_warp * 0.78 + fine_warp * 0.22;
            let source_x = (warped.x.round() as isize).clamp(0, source_width as isize - 1) as usize;
            let source_z = (warped.y.round() as isize).clamp(0, source_depth as isize - 1) as usize;
            pixels.extend_from_slice(&ground_surface_pixel(
                ground.samples()[source_z * source_width + source_x],
            ));
        }
    }
    encode_canopy_floor_distance(ground, width, depth, &mut pixels);
    (width as u32, depth as u32, pixels)
}

pub(super) fn grass_cover_mask_pixels(ground: &SceneGround, seed: u64) -> (u32, u32, Vec<u8>) {
    let (width, height, ground_pixels) = organic_ground_pixels(ground, seed);
    let mut mask = vec![0_u8; width as usize * height as usize];
    let metres_per_pixel = ground.grid_scale() / GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    let radius = (4.8 / metres_per_pixel).ceil().max(1.0) as usize;
    let width_usize = width as usize;
    let height_usize = height as usize;
    // The playable rectangle is a data-authority boundary, not a vegetation
    // boundary. Only authored non-grass pixels seed this distance field.
    let distance = chamfer_distance_to(
        width_usize,
        height_usize,
        |index| ground_pixels[index * 4] != GroundCover::TallGrass as u8,
        radius,
    );
    for z in 0..height as usize {
        for x in 0..width as usize {
            let pixel = (z * width as usize + x) * 4;
            if ground_pixels[pixel] != GroundCover::TallGrass as u8 {
                continue;
            }
            let density = ground_pixels[pixel + 2];
            let feather = (distance[z * width_usize + x] as f32 / radius as f32)
                .clamp(0.0, 1.0)
                .powf(1.28);
            mask[z * width as usize + x] = (f32::from(density) * feather) as u8;
        }
    }
    (width, height, mask)
}

pub(super) fn grass_cover_mask_image(ground: &SceneGround, seed: u64) -> Image {
    let (width, height, mask) = grass_cover_mask_pixels(ground, seed);
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        mask,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn ground_map_image(ground: Option<&SceneGround>, seed: u64) -> Image {
    let (width, height, pixels) = ground.map_or_else(
        || (1, 1, vec![0, 0, 0, 0]),
        |ground| organic_ground_pixels(ground, seed),
    );
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The shader rounds the interpolated enum channel back to one exact cover
    // kind before selecting a material. Linear filtering therefore smooths
    // the baked contour itself without producing a visible colour gradient.
    image.sampler = ImageSampler::linear();
    image
}

pub(in crate::presentation) fn legacy_scene_environment(id: &SceneId) -> SceneEnvironment {
    let (canopy_bps, hilly_bps, cultivation_bps) = match id.0.as_str() {
        "hills" => (1_200, 7_000, 0),
        "desert" => (0, 1_500, 0),
        value => {
            warn!("Unknown legacy scene: {value}");
            (0, 0, 0)
        }
    };
    SceneEnvironment {
        scene_digest: id.0.clone(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: 12 * 60,
        absolute_elevation_metres: 20,
        weather: WeatherSnapshot {
            rules_version: WEATHER_RULES_VERSION,
            interval_start_minute: 0,
            cell_latitude: 0,
            cell_longitude: 0,
            temperature_deci_c: 100,
            wind_speed_bps: 1_500,
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
        },
        canopy_bps,
        wetland_bps: 0,
        cultivation_bps,
        water_bps: 0,
        hilly_bps,
    }
}

pub(super) fn on_environment_added(event: On<Add, SceneEnvironment>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

pub(super) fn on_ground_added(event: On<Add, SceneGround>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use bevy::asset::{AssetApp, AssetPlugin};

    #[test]
    fn ground_shader_uses_solid_palette_colors_without_surface_texture_detail() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_terrain.wgsl"
        ));
        assert!(shader.contains("var ground_map: texture_2d<f32>"));
        assert!(shader.contains("pbr_input.material.base_color = vec4<f32>(color, 1.0)"));
        assert!(shader.contains("distance(position, view.lod_view_world_position.xyz)"));
        assert!(!shader.contains("distance(position.xz, view.lod_view_world_position.xz)"));
        assert!(!shader.contains("select(color, terrain.grass_color.rgb, tall_grass > 0.5)"));
        assert!(shader.contains("let shaded_substrate = select("));
        assert!(shader.contains("let canopy_floor = ground_sample.a"));
        assert!(shader.contains("canopy_floor >= 0.78"));
        assert!(shader.contains("let sward_color = terrain.grass_color.rgb"));
        assert!(!shader.contains("sward_color = color *"));
        assert!(shader.contains("sward_dither < sward_amount"));
        assert!(!shader.contains("sward_amount >= 0.5"));
        for forbidden in [
            "dirt_diffuse",
            "dirt_normal_gl",
            "dirt_arm",
            "procedural_normal",
            "mapped_dirt_normal",
        ] {
            assert!(!shader.contains(forbidden), "found {forbidden}");
        }
    }

    #[test]
    fn terminal_terrain_sward_starts_when_the_final_grass_lod_fades() {
        let vista = grass_lod_visibility(GrassMeshLod::Vista);
        assert_eq!(vista.end_margin.start, TERMINAL_SWARD_FADE_START_METRES);
        assert_eq!(vista.end_margin.end, TERMINAL_SWARD_FADE_END_METRES);
    }

    use bevy::prelude::TaskPoolPlugin;

    #[test]
    fn terrain_presentation_remains_pending_until_terrain_arrives() {
        let mut app = App::new();
        app.add_observer(on_game_scene_added);
        let scene = app.world_mut().spawn(SceneId("add-order".into())).id();
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );

        app.world_mut()
            .entity_mut(scene)
            .insert(SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).expect("valid terrain"));
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
    }

    #[test]
    fn terrain_reconcile_updates_once_and_retries_a_stale_material_handle() {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<Image>();
        app.init_asset::<Mesh>();
        app.init_asset::<TacticalTerrainMaterial>();
        app.add_observer(on_game_scene_added);
        app.add_observer(on_environment_added);
        app.add_observer(on_ground_added);
        app.add_systems(Update, present_pending_terrain);
        let ground = SceneGround::from_samples(2, 2, 1.0, vec![GroundSurface::default(); 4])
            .expect("valid ground");
        let scene = app
            .world_mut()
            .spawn((
                SceneId("lifecycle".into()),
                SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).expect("valid terrain"),
                legacy_scene_environment(&SceneId("lifecycle".into())),
                ground,
            ))
            .id();
        app.update();

        assert!(
            !app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
        let mut presentation_query = app.world_mut().query::<(
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        )>();
        let presentations = presentation_query
            .iter(app.world())
            .filter(|(source, _)| source.0 == scene)
            .map(|(_, handle)| handle.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(presentations.len(), 1);
        let handle = presentations[0].clone();

        app.world_mut()
            .entity_mut(scene)
            .insert(PendingTerrainPresentation);
        app.update();
        let mut presentation_query = app.world_mut().query::<(
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        )>();
        let refreshed = presentation_query
            .iter(app.world())
            .filter(|(source, _)| source.0 == scene)
            .map(|(_, material)| material.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(refreshed, vec![handle.clone()]);

        app.world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .remove(&handle);
        app.update();
        let image_count = app.world().resource::<Assets<Image>>().len();
        app.world_mut()
            .entity_mut(scene)
            .insert(PendingTerrainPresentation);
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
        assert_eq!(app.world().resource::<Assets<Image>>().len(), image_count);
    }

    #[test]
    fn presentation_ground_mask_is_deterministic_discrete_and_not_grid_aligned() {
        let mut samples = vec![GroundSurface::default(); 7 * 7];
        for z in 0..7 {
            for x in 3..7 {
                samples[z * 7 + x] = GroundSurface {
                    cover: GroundCover::LeafLitter,
                    cover_density_bps: 9_200,
                    cover_height_cm: 6,
                    ..default()
                };
            }
        }
        let ground = SceneGround::from_samples(7, 7, 2.0, samples).expect("valid ground");
        let (width, depth, pixels) = organic_ground_pixels(&ground, 42);
        let repeated = organic_ground_pixels(&ground, 42);
        let changed = organic_ground_pixels(&ground, 43);
        assert_eq!((width, depth), (37, 37));
        assert_eq!((width, depth, pixels.clone()), repeated);
        assert_ne!(pixels, changed.2);

        let valid_surfaces = [
            ground_surface_pixel(GroundSurface::default()),
            ground_surface_pixel(GroundSurface {
                cover: GroundCover::LeafLitter,
                cover_density_bps: 9_200,
                cover_height_cm: 6,
                ..default()
            }),
        ];
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| valid_surfaces.iter().any(|valid| pixel[..3] == valid[..3]))
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] == GroundCover::LeafLitter as u8 && pixel[3] > 180),
            "deep litter must encode an interior loam zone"
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] != GroundCover::LeafLitter as u8 && pixel[3] > 0),
            "open cover must retain an exterior canopy transition"
        );

        let first_leaf_litter_x = (0..depth as usize)
            .filter_map(|z| {
                pixels
                    .chunks_exact(4)
                    .skip(z * width as usize)
                    .take(width as usize)
                    .position(|pixel| pixel[0] == 2)
            })
            .collect::<BTreeSet<_>>();
        assert!(
            first_leaf_litter_x.len() >= 5,
            "organic boundary should cross rows at varied positions: {first_leaf_litter_x:?}"
        );
    }

    #[test]
    fn grass_cover_mask_is_deterministic_and_feathers_authoritative_edges() {
        let grass = GroundSurface {
            cover: GroundCover::TallGrass,
            cover_density_bps: 10_000,
            cover_height_cm: 82,
            ..default()
        };
        let mut samples = vec![grass; 17 * 17];
        for z in 7..10 {
            for x in 7..10 {
                samples[z * 17 + x].cover = GroundCover::LeafLitter;
            }
        }
        let ground = SceneGround::from_samples(17, 17, 2.0, samples).unwrap();
        let image = grass_cover_mask_image(&ground, 91);
        let repeated = grass_cover_mask_image(&ground, 91);
        let values = image.data.as_deref().unwrap();
        assert_eq!(image.data, repeated.data);
        assert!(values.contains(&0), "non-grass must reject every blade");
        assert!(
            values.iter().copied().max().unwrap_or_default() > 200,
            "deep grass must retain most blades"
        );
        assert!(
            values.iter().any(|value| (1..255).contains(value)),
            "grass-side boundary must be progressively sparse"
        );
        assert!(values[0] > 200 && values[values.len() - 1] > 200);
    }
}
