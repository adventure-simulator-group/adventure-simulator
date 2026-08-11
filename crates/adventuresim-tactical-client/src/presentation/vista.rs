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
    let mut inner_half_extent = 55.0;
    for lod in bundle.lods.iter().take(settings.max_vista_lods) {
        let meshes_for_lod = vista_lod_meshes(lod, inner_half_extent);
        if meshes_for_lod.is_empty() {
            warn!(level = lod.level, "Rejected malformed tactical vista LOD");
            continue;
        }
        let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
        let color = vista_lod_color(lod);
        let material = materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 1.0,
            ..default()
        });
        for (chunk, mesh) in meshes_for_lod.into_iter().enumerate() {
            commands.spawn((
                Name::new(format!("Tactical vista LOD {} chunk {chunk}", lod.level)),
                VistaTerrain(lod.level),
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(
                    lod.origin_east_metres as f32,
                    -0.06 * (f32::from(lod.level) + 1.0),
                    lod.origin_north_metres as f32,
                ),
            ));
        }
        inner_half_extent = half_extent;
    }
}

pub(super) fn vista_lod_meshes(lod: &VistaLod, inner_half_extent: f32) -> Vec<Mesh> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    if width < 2
        || depth < 2
        || width
            .checked_mul(depth)
            .is_none_or(|samples| lod.heights_metres.len() != samples)
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
                        [
                            (vx as f32 - center_x) * lod.spacing_metres,
                            lod.heights_metres[vz * width + vx],
                            (vz as f32 - center_z) * lod.spacing_metres,
                        ]
                    };
                    let base = positions.len() as u32;
                    positions.extend_from_slice(&[
                        vertex(x, z),
                        vertex(x + 1, z),
                        vertex(x + 1, z + 1),
                        vertex(x, z + 1),
                    ]);
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
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh.with_computed_area_weighted_normals());
        }
    }
    meshes
}

pub(super) fn vista_lod_color(lod: &VistaLod) -> Color {
    let count = lod.environment.len().max(1) as f32;
    let (canopy, wetland, cultivation, water) =
        lod.environment
            .iter()
            .fold((0.0, 0.0, 0.0, 0.0), |sum, sample| {
                (
                    sum.0 + f32::from(sample.canopy_bps),
                    sum.1 + f32::from(sample.wetland_bps),
                    sum.2 + f32::from(sample.cultivation_bps),
                    sum.3 + f32::from(sample.water_bps),
                )
            });
    let environment = SceneEnvironment {
        scene_digest: String::new(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        weather: WeatherSnapshot {
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
        },
        canopy_bps: (canopy / count) as u16,
        wetland_bps: (wetland / count) as u16,
        cultivation_bps: (cultivation / count) as u16,
        water_bps: (water / count) as u16,
        hilly_bps: 0,
    };
    scene_ground_color(&environment)
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
}
