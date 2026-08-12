use super::*;

pub(super) fn on_scene_vista_bundle(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    existing: Query<Entity, With<VistaTerrain>>,
    settings: Res<TacticalGraphicsSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let visible_lods = bundle
        .lods
        .iter()
        .take(settings.max_vista_lods)
        .collect::<Vec<_>>();
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        ..default()
    });
    let mut inner_half_extent = 55.0;
    for (index, lod) in visible_lods.iter().copied().enumerate() {
        let meshes_for_lod = vista_lod_meshes_with_morph(
            lod,
            inner_half_extent,
            visible_lods.get(index + 1).copied(),
        );
        if meshes_for_lod.is_empty() {
            warn!(level = lod.level, "Rejected malformed tactical vista LOD");
            continue;
        }
        let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
        for (chunk, mesh) in meshes_for_lod.into_iter().enumerate() {
            commands.spawn((
                Name::new(format!("Tactical vista LOD {} chunk {chunk}", lod.level)),
                VistaTerrain(lod.level),
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
        inner_half_extent = half_extent;
    }
}

pub(super) fn vista_lod_meshes(lod: &VistaLod, inner_half_extent: f32) -> Vec<Mesh> {
    vista_lod_meshes_with_morph(lod, inner_half_extent, None)
}

fn vista_lod_meshes_with_morph(
    lod: &VistaLod,
    inner_half_extent: f32,
    coarser_lod: Option<&VistaLod>,
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
    let mut meshes = Vec::new();
    for chunk_z in (0..depth - 1).step_by(VISTA_CHUNK_CELLS) {
        for chunk_x in (0..width - 1).step_by(VISTA_CHUNK_CELLS) {
            let mut positions = Vec::new();
            let mut colors = Vec::new();
            let mut indices = Vec::new();
            for z in chunk_z..(chunk_z + VISTA_CHUNK_CELLS).min(depth - 1) {
                for x in chunk_x..(chunk_x + VISTA_CHUNK_CELLS).min(width - 1) {
                    let cell_x = (x as f32 + 0.5 - center_x) * lod.spacing_metres;
                    let cell_z = (z as f32 + 0.5 - center_z) * lod.spacing_metres;
                    // Exclude cells fully covered by the playable mesh or the
                    // preceding finer ring. Testing the outer cell edge keeps
                    // one boundary cell without filling the inner hole.
                    if cell_x.abs().max(cell_z.abs()) + lod.spacing_metres * 0.5
                        <= inner_half_extent
                    {
                        continue;
                    }
                    let vertex = |vx: usize, vz: usize| {
                        let world = Vec2::new(
                            (vx as f32 - center_x) * lod.spacing_metres
                                + lod.origin_east_metres as f32,
                            (vz as f32 - center_z) * lod.spacing_metres
                                + lod.origin_north_metres as f32,
                        );
                        let height = presented_height(lod, vx, vz, world, coarser_lod);
                        (
                            [
                                (vx as f32 - center_x) * lod.spacing_metres,
                                height,
                                (vz as f32 - center_z) * lod.spacing_metres,
                            ],
                            presented_color(lod, vx, vz, world, coarser_lod),
                        )
                    };
                    let base = positions.len() as u32;
                    let vertices = [
                        vertex(x, z),
                        vertex(x + 1, z),
                        vertex(x + 1, z + 1),
                        vertex(x, z + 1),
                    ];
                    positions.extend(vertices.map(|vertex| vertex.0));
                    colors.extend(vertices.map(|vertex| vertex.1));
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
            if positions.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh.with_computed_area_weighted_normals());
        }
    }
    meshes
}

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
    let center = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
    let radius = (world - center).abs().max_element();
    let weight =
        ((radius - (half_extent - lod.spacing_metres)) / lod.spacing_metres).clamp(0.0, 1.0);
    if weight <= 0.0 {
        return own;
    }
    sample_vista_height(coarser, world)
        .map(|height| own.lerp(height, weight))
        .unwrap_or(own)
}

fn presented_color(
    lod: &VistaLod,
    x: usize,
    z: usize,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
) -> [f32; 4] {
    let own = vista_sample_color(lod.environment[z * usize::from(lod.width) + x]);
    let Some(coarser) = coarser_lod else {
        return own.to_array();
    };
    let center = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
    let radius = (world - center).abs().max_element();
    let weight =
        ((radius - (half_extent - lod.spacing_metres)) / lod.spacing_metres).clamp(0.0, 1.0);
    sample_vista_color(coarser, world)
        .map(|color| own.lerp(color, weight))
        .unwrap_or(own)
        .to_array()
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

fn vista_sample_color(sample: EnvironmentalSample) -> Vec4 {
    let environment = SceneEnvironment {
        scene_digest: String::new(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: 12 * 60,
        absolute_elevation_metres: 20,
        weather: clear_vista_weather(),
        canopy_bps: sample.canopy_bps,
        wetland_bps: sample.wetland_bps,
        cultivation_bps: sample.cultivation_bps,
        water_bps: sample.water_bps,
        hilly_bps: sample.hilly_bps,
    };
    Vec4::from_array(scene_ground_color(&environment).to_linear().to_f32_array())
}

fn sample_vista_color(lod: &VistaLod, world: Vec2) -> Option<Vec4> {
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
    let at = |x: u32, z: u32| vista_sample_color(lod.environment[z as usize * width + x as usize]);
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
    }
}

#[derive(Component)]
#[allow(dead_code)]
pub(crate) struct VistaTerrain(pub(crate) u8);

const VISTA_CHUNK_CELLS: usize = 8;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn vista_lods_build_independent_overlapping_rings() {
        let input = TacticalSceneInput::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/tactical-scenes/valley-distant-ridge.json"),
        )
        .unwrap();
        let mut inner = 55.0;
        for (index, lod) in input.vista.lods.iter().enumerate() {
            let meshes = vista_lod_meshes(lod, inner);
            assert!(!meshes.is_empty());
            assert!(meshes.iter().all(|mesh| mesh.count_vertices() > 0));
            assert!(meshes.iter().all(|mesh| {
                mesh.count_vertices() <= VISTA_CHUNK_CELLS * VISTA_CHUNK_CELLS * 4
            }));
            if index > 0 {
                assert!(
                    meshes.len() > 1,
                    "regional LODs must be independently culled"
                );
            }
            inner = f32::from(lod.width - 1) * lod.spacing_metres * 0.5;
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
            25.0
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
        assert_eq!(vista_sample_color(open).to_array(), expected);

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
            vista_lod_meshes(&lod, 0.0)
                .iter()
                .all(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some())
        );
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
            presented_color(&finer, 4, 2, Vec2::new(20.0, 0.0), Some(&coarse)),
            vista_sample_color(cultivated).to_array()
        );
        assert_eq!(
            presented_color(&finer, 2, 2, Vec2::ZERO, Some(&coarse)),
            vista_sample_color(forest).to_array()
        );
    }
}
