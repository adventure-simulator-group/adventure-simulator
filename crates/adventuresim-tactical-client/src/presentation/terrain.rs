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
    query: Query<(&SceneId, &SceneTerrain, Option<&SceneEnvironment>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
) -> Result {
    let (id, terrain, environment) = query.get(event.entity)?;
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
        MeshMaterial3d(materials.add(terrain_material(environment))),
    ));
    Ok(())
}

pub(in crate::presentation) fn terrain_material(
    environment: &SceneEnvironment,
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
        },
    }
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
    mut terrain_materials: ResMut<Assets<TacticalTerrainMaterial>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    for (source, material) in &presentations {
        if source.0 == event.entity
            && let Some(mut material) = terrain_materials.get_mut(&material.0)
        {
            *material = terrain_material(environment);
        }
    }
    Ok(())
}

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";
