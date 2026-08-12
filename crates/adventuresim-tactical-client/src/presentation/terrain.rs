use super::*;

pub(super) fn scene_ground_color(environment: &SceneEnvironment) -> Color {
    let mut rgb = if environment.water_bps >= 5_000 {
        [52.0, 83.0, 98.0]
    } else if environment.wetland_bps >= 4_000 {
        [73.0, 86.0, 58.0]
    } else if environment.canopy_bps >= 5_000 {
        [55.0, 82.0, 43.0]
    } else if environment.cultivation_bps >= 4_000 {
        [126.0, 116.0, 66.0]
    } else {
        [96.0, 108.0, 56.0]
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

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTerrainExtension {
    #[uniform(100)]
    base_color: Vec4,
    #[uniform(100)]
    cover: Vec4,
    #[uniform(100)]
    weather: Vec4,
    #[uniform(100)]
    variation: Vec4,
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
    query: Query<(
        &SceneId,
        &SceneTerrain,
        Option<&SceneEnvironment>,
        Option<&SceneGround>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) -> Result {
    let (id, terrain, environment, ground) = query.get(event.entity)?;
    info!(entity = ?event.entity, "Spawning a scene {id:?}");

    let legacy_environment;
    let environment = if let Some(environment) = environment {
        environment
    } else {
        legacy_environment = legacy_scene_environment(id);
        &legacy_environment
    };

    commands.spawn((
        Name::new(format!("{} terrain mesh", id.0)),
        ScenePresentationOf(event.entity),
        TerrainMaterialPresentation,
        Mesh3d(meshes.add(terrain.mesh())),
        MeshMaterial3d(materials.add(terrain_material(environment, ground, &mut images))),
    ));
    Ok(())
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
            variation: Vec4::new(
                digest_unit(&environment.scene_digest),
                0.055,
                0.032,
                environment.generation_version as f32,
            ),
            // Beyond the geometric grass range, retain a band-limited sward
            // response in the terrain material instead of paying for blades
            // that project to less than a pixel. x/y are the fade interval;
            // z is environment-dependent coverage and w is reserved.
            far_sward: Vec4::new(
                104.0,
                132.0,
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

fn organic_ground_pixels(ground: &SceneGround, seed: u64) -> (u32, u32, Vec<u8>) {
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
    (width as u32, depth as u32, pixels)
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

pub(super) fn on_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    presentations: Query<(
        &ScenePresentationOf,
        &MeshMaterial3d<TacticalTerrainMaterial>,
    )>,
    ground: Query<&SceneGround>,
    mut terrain_materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    let ground = ground.get(event.entity).ok();
    for (source, material) in &presentations {
        if source.0 == event.entity
            && let Some(mut material) = terrain_materials.get_mut(&material.0)
        {
            *material = terrain_material(environment, ground, &mut images);
        }
    }
    Ok(())
}

pub(super) fn on_ground_added(
    event: On<Add, SceneGround>,
    grounds: Query<&SceneGround>,
    environments: Query<&SceneEnvironment>,
    presentations: Query<(
        &ScenePresentationOf,
        &MeshMaterial3d<TacticalTerrainMaterial>,
    )>,
    mut terrain_materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
) -> Result {
    let ground = grounds.get(event.entity)?;
    let environment = environments.get(event.entity)?;
    for (source, material) in &presentations {
        if source.0 == event.entity
            && let Some(mut material) = terrain_materials.get_mut(&material.0)
        {
            *material = terrain_material(environment, Some(ground), &mut images);
        }
    }
    Ok(())
}

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

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

        let valid_pixels = [
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
                .all(|pixel| valid_pixels.iter().any(|valid| pixel == valid))
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
}
