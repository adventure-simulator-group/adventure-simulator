//! Select procedural landforms from mapped lithology and unmodified source
//! relief. The selected process is a plausible realization, not a mapped event.
//!
//! Rebuild world data and terrain packs to supply containment-verified EGDI
//! windows. Coverage is deliberately limited to mapped polygons around import
//! anchors; absent coverage leaves the heightfield intact. Fault traces retain
//! priority. Roads, water, cultivation, settlements and the central traversal
//! corridor exclude additional landforms before any geometry is generated.

use super::*;
use adventuresim_world_schema::{IgneousRock, SedimentaryRock, SurfaceLithology};
use proj4rs::{proj::Proj, transform::transform};

const MIN_SOURCE_GRADE: f32 = 0.20;
const MAX_SOURCE_GRADE: f32 = 0.65;
const MIN_SOURCE_RELIEF_METRES: f32 = 4.0;
const MAX_RELIEF_METRES: f32 = 8.0;
const HALF_LENGTH_CM: u16 = 1_200;
const HALF_WIDTH_CM: u16 = 1_000;
const COLLAR_CM: u16 = 250;
const CORRIDOR_HALF_WIDTH_METRES: f32 = 7.0;
const PROTECTED_COVER_BPS: u16 = 1_000;
const GEOLOGIC_PROJECTION_MARGIN_METRES: f64 = 2.0;
const CANDIDATE_STRIDE: usize = 8;

pub(super) fn select(
    features: &[TerrainFeature],
    center: Wgs84CoordinateE7,
    grid: &TerrainSampleGrid,
    seed: u64,
) -> Option<TerrainLandformRecipe> {
    let geographic =
        Proj::from_proj_string("+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs").ok()?;
    let projected = Proj::from_proj_string("+proj=lcc +lat_0=52 +lon_0=10 +lat_1=35 +lat_2=65 +x_0=4000000 +y_0=2800000 +ellps=GRS80 +units=m +no_defs").ok()?;
    let terrain = SceneTerrain::from_heightmap(
        usize::from(grid.width),
        usize::from(grid.depth),
        grid.spacing_metres,
        grid.heights_metres.clone(),
    )?;
    for z in
        (CANDIDATE_STRIDE..usize::from(grid.depth) - CANDIDATE_STRIDE).step_by(CANDIDATE_STRIDE)
    {
        for x in
            (CANDIDATE_STRIDE..usize::from(grid.width) - CANDIDATE_STRIDE).step_by(CANDIDATE_STRIDE)
        {
            let point = Vec2::new(
                x as f32 - (f32::from(grid.width) - 1.0) * 0.5,
                z as f32 - (f32::from(grid.depth) - 1.0) * 0.5,
            ) * grid.spacing_metres;
            let offset = grid.spacing_metres * CANDIDATE_STRIDE as f32;
            let gradient = Vec2::new(
                terrain.height_at(point + Vec2::X * offset)?
                    - terrain.height_at(point - Vec2::X * offset)?,
                terrain.height_at(point + Vec2::Y * offset)?
                    - terrain.height_at(point - Vec2::Y * offset)?,
            ) / (offset * 2.0);
            let grade = gradient.length();
            if !(MIN_SOURCE_GRADE..=MAX_SOURCE_GRADE).contains(&grade) {
                continue;
            }
            let downhill = -gradient.normalize();
            let tangent = Vec2::new(downhill.y, -downhill.x);
            let relief = (grade * f32::from(HALF_WIDTH_CM) * 2.0 / 100.0).min(MAX_RELIEF_METRES);
            if relief < MIN_SOURCE_RELIEF_METRES {
                continue;
            }
            let mut recipe = TerrainLandformRecipe {
                kind: TerrainLandformKind::SandstoneAlcove,
                seed,
                origin_cm: [
                    (point.x * 100.0).round() as i32,
                    (point.y * 100.0).round() as i32,
                ],
                tangent_permyriad: [
                    (tangent.x * 10000.0).round() as i16,
                    (tangent.y * 10000.0).round() as i16,
                ],
                relief_cm: (relief * 100.0).round() as u16,
                half_length_cm: HALF_LENGTH_CM,
                half_width_cm: HALF_WIDTH_CM,
                collar_cm: COLLAR_CM,
                lod: TerrainLandformLod::Detail,
            };
            if !safe_footprint(grid, &terrain, recipe) {
                continue;
            }
            let Some(kind) = mapped_kind(features, center, point, &geographic, &projected) else {
                continue;
            };
            recipe.kind = kind;
            return Some(recipe);
        }
    }
    None
}

fn kind_for_lithology(lithology: SurfaceLithology) -> Option<TerrainLandformKind> {
    match lithology {
        SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone) => {
            Some(TerrainLandformKind::SandstoneAlcove)
        }
        SurfaceLithology::Sedimentary(SedimentaryRock::Limestone | SedimentaryRock::Dolostone) => {
            Some(TerrainLandformKind::CarbonateDissolution)
        }
        SurfaceLithology::Igneous(IgneousRock::Granite | IgneousRock::Granitoid) => {
            Some(TerrainLandformKind::GraniteJointRockfall)
        }
        _ => None,
    }
}

fn safe_footprint(
    grid: &TerrainSampleGrid,
    terrain: &SceneTerrain,
    recipe: TerrainLandformRecipe,
) -> bool {
    let origin = Vec2::new(recipe.origin_cm[0] as f32, recipe.origin_cm[1] as f32) / 100.0;
    let radius = f32::from(recipe.half_length_cm.max(recipe.half_width_cm)) / 100.0 + 2.0;
    let half = Vec2::new(terrain.width(), terrain.depth()) * 0.5;
    if origin.x.abs() + radius >= half.x
        || origin.y.abs() + radius >= half.y
        || origin.y.abs() - radius <= CORRIDOR_HALF_WIDTH_METRES
    {
        return false;
    }
    for (index, sample) in grid.environment.iter().enumerate() {
        let point = (Vec2::new(
            (index % usize::from(grid.width)) as f32,
            (index / usize::from(grid.width)) as f32,
        ) - Vec2::new(
            (f32::from(grid.width) - 1.0) * 0.5,
            (f32::from(grid.depth) - 1.0) * 0.5,
        )) * grid.spacing_metres;
        if (point - origin).abs().max_element() <= radius
            && (sample.water_bps > 0
                || sample.crossing_bps > 0
                || sample.wetland_bps >= PROTECTED_COVER_BPS
                || sample.cultivation_bps > 0
                || matches!(
                    sample.surface,
                    TacticalSurface::Road | TacticalSurface::Water | TacticalSurface::Wetland
                ))
        {
            return false;
        }
    }
    true
}

fn mapped_kind(
    features: &[TerrainFeature],
    center: Wgs84CoordinateE7,
    point: Vec2,
    geographic: &Proj,
    projected: &Proj,
) -> Option<TerrainLandformKind> {
    // The whole bounding square, not only the mission coordinate, must
    // lie inside one containment-verified source window. Margin absorbs
    // rounding and local east/north versus map-projection curvature.
    let radius =
        f64::from(HALF_LENGTH_CM.max(HALF_WIDTH_CM)) / 100.0 + GEOLOGIC_PROJECTION_MARGIN_METRES;
    let corners = [-radius, 0.0, radius]
        .into_iter()
        .flat_map(|dx| [-radius, 0.0, radius].map(|dz| (dx, dz)))
        .map(|(dx, dz)| {
            let longitude_scale =
                METRES_PER_LATITUDE_DEGREE * center.latitude().degrees().to_radians().cos();
            let mut position = (
                (center.longitude().degrees() + (f64::from(point.x) + dx) / longitude_scale)
                    .to_radians(),
                (center.latitude().degrees()
                    + (f64::from(point.y) + dz) / METRES_PER_LATITUDE_DEGREE)
                    .to_radians(),
                0.0,
            );
            transform(geographic, projected, &mut position).ok()?;
            Some([position.0, position.1])
        })
        .collect::<Option<Vec<_>>>()?;
    let window = features
        .iter()
        .filter_map(|feature| match feature {
            TerrainFeature::MappedGeology(window) => Some(window),
            _ => None,
        })
        .find(|window| corners.iter().all(|&corner| window.contains(corner)))?;
    kind_for_lithology(window.lithology)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::{GeologicUnitId, MappedGeologicWindow};

    fn context() -> (Wgs84CoordinateE7, TerrainSampleGrid, Vec<TerrainFeature>) {
        let center = Wgs84CoordinateE7::new(520000000, 100000000).unwrap();
        let grid = TerrainSampleGrid {
            width: 101,
            depth: 101,
            spacing_metres: 1.0,
            heights_metres: (0..101)
                .flat_map(|z| (0..101).map(move |_| -(z as f32 - 50.0) * 0.3))
                .collect(),
            environment: vec![
                EnvironmentalSample {
                    hilly_bps: 10000,
                    ..Default::default()
                };
                101 * 101
            ],
        };
        let features = vec![TerrainFeature::MappedGeology(MappedGeologicWindow {
            id: "egdi-window:test".into(),
            unit: GeologicUnitId::new("test").unwrap(),
            lithology: SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
            bounds_metres: [3999500, 2799500, 4000500, 2800500],
        })];
        (center, grid, features)
    }

    #[test]
    fn imported_lithology_and_relief_generate_a_safe_downhill_landform() {
        let (center, grid, features) = context();
        let recipe = select(&features, center, &grid, 42).unwrap();
        assert_eq!(recipe.kind, TerrainLandformKind::SandstoneAlcove);
        assert_eq!(Some(recipe), select(&features, center, &grid, 42));
        assert!(recipe.origin_cm[1].abs() > 2100);
        assert!(recipe.tangent_permyriad[0] > 9900);
    }

    #[test]
    fn mapped_competent_carbonates_select_dissolution_but_marl_does_not() {
        let (center, grid, mut features) = context();
        for rock in [
            SedimentaryRock::Limestone,
            SedimentaryRock::Dolostone,
            SedimentaryRock::Marl,
        ] {
            let TerrainFeature::MappedGeology(window) = &mut features[0] else {
                unreachable!()
            };
            window.lithology = SurfaceLithology::Sedimentary(rock);
            let selected = select(&features, center, &grid, 42);
            if rock == SedimentaryRock::Marl {
                assert!(selected.is_none());
            } else {
                assert_eq!(
                    selected.unwrap().kind,
                    TerrainLandformKind::CarbonateDissolution
                );
            }
        }
    }

    #[test]
    fn mapped_granitoids_select_joint_faces_but_unclassified_plutonic_rock_does_not() {
        let (center, grid, mut features) = context();
        for rock in [
            IgneousRock::Granite,
            IgneousRock::Granitoid,
            IgneousRock::OtherPlutonic,
        ] {
            let TerrainFeature::MappedGeology(window) = &mut features[0] else {
                unreachable!()
            };
            window.lithology = SurfaceLithology::Igneous(rock);
            let selected = select(&features, center, &grid, 42);
            if rock == IgneousRock::OtherPlutonic {
                assert!(selected.is_none());
            } else {
                assert_eq!(
                    selected.unwrap().kind,
                    TerrainLandformKind::GraniteJointRockfall
                );
            }
        }
    }

    #[test]
    fn coarse_source_steps_retain_an_overhang_after_production_repair() {
        for lithology in [
            SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
            SurfaceLithology::Sedimentary(SedimentaryRock::Limestone),
            SurfaceLithology::Igneous(IgneousRock::Granite),
        ] {
            assert_coarse_source_landform(lithology);
        }
    }

    fn assert_coarse_source_landform(lithology: SurfaceLithology) {
        let (center, mut grid, mut features) = context();
        let TerrainFeature::MappedGeology(window) = &mut features[0] else {
            unreachable!()
        };
        window.lithology = lithology;
        for (index, height) in grid.heights_metres.iter_mut().enumerate() {
            let north = (index / 101) as f32 - 50.0;
            *height = -(north / 30.0).floor() * 9.0;
        }
        let recipe = select(&features, center, &grid, 42).unwrap();
        let mut input = TacticalSceneInput::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/tactical-scenes/sandstone-alcove.json"
        )))
        .unwrap();
        input.playable = grid;
        input.landform = Some(recipe);
        let generated = input.generate().unwrap();
        let mesh = generated.terrain_patch.unwrap();
        assert!(mesh.normals.iter().any(|normal| normal[1] < -0.2));
        let half_width =
            (f32::from(input.playable.width) - 1.0) * input.playable.spacing_metres * 0.5;
        for index in 0..101 {
            let point = Vec2::new(index as f32 - half_width, 0.0);
            assert!(!recipe.transition_collar().contains(point));
        }
        if recipe.kind == TerrainLandformKind::SandstoneAlcove
            && let Some(output) = std::env::var_os("GEOLOGY_REVIEW_STEPPED_INPUT")
        {
            fs::write(output, serde_json::to_vec_pretty(&input).unwrap()).unwrap();
        }
    }

    #[test]
    fn missing_wrong_lithology_flat_water_road_and_cultivation_cannot_generate_cliffs() {
        let (center, grid, mut features) = context();
        assert!(select(&[], center, &grid, 42).is_none());
        let TerrainFeature::MappedGeology(window) = &mut features[0] else {
            unreachable!()
        };
        window.lithology = SurfaceLithology::Sedimentary(SedimentaryRock::Coal);
        assert!(select(&features, center, &grid, 42).is_none());
        let (_, grid, features) = context();
        let mut flat = grid.clone();
        flat.heights_metres.fill(0.0);
        assert!(select(&features, center, &flat, 42).is_none());
        for surface in [
            TacticalSurface::Road,
            TacticalSurface::Water,
            TacticalSurface::Wetland,
        ] {
            let mut protected = grid.clone();
            for sample in &mut protected.environment {
                sample.surface = surface;
            }
            assert!(select(&features, center, &protected, 42).is_none());
        }
        let mut cultivated = grid.clone();
        for sample in &mut cultivated.environment {
            sample.cultivation_bps = 10000;
        }
        assert!(select(&features, center, &cultivated, 42).is_none());
    }
}
