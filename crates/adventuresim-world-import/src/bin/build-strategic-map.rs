use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{CompiledWorld, PLAYABLE_BOUNDS, TravelEdgeProvenance};
use clap::Parser;
use raster::{ElevationLayer, ForestLayer, MapRasterLayers};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[path = "build-strategic-map/raster.rs"]
mod raster;
#[path = "build-strategic-map/terrain_features.rs"]
mod terrain_features;
#[path = "build-strategic-map/tiles.rs"]
mod tiles;

const PACKAGE_SCHEMA: u32 = 5;
const RENDERER_REVISION: u32 = 10;
const YEAR: i32 = 1544;
const VIABUNDUS_DOI: &str = "https://doi.org/10.5281/zenodo.16611998";
const RECORD_URL: &str = "https://zenodo.org/api/records/16611998";
const BOUNDS: [f64; 4] = PLAYABLE_BOUNDS;
const MAX_SOURCE_FILES: usize = 64;
const DATA_LICENSE_FILENAME: &str = "STRATEGIC_MAP_DATA_LICENSE.md";
const DATA_LICENSE: &str = include_str!("../../../../MAP_DATA_LICENSE.md");

#[derive(Parser)]
#[command(about = "Build the bounded AVIF strategic-map package from initialized world data")]
struct Args {
    #[arg(long, help = "build only the documented-road base terrain contract")]
    base_only: bool,
    #[arg(long, default_value = "viabundus")]
    viabundus_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/elevation")]
    elevation_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/forest-cover")]
    forest_cover_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/jung-pnv")]
    potential_vegetation_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/hyde35-land-use")]
    hyde_dir: PathBuf,
    #[arg(long, default_value = "target/world-1544.json")]
    compiled_world: PathBuf,
    #[arg(long, default_value = "target/strategic-map/strategic-map-v1.json")]
    output: PathBuf,
    #[arg(
        long,
        default_value = "target/strategic-map/strategic-map-tiles-v1.pack"
    )]
    tiles_output: PathBuf,
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v3.json")]
    terrain_output: PathBuf,
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v3.pack")]
    terrain_pack_output: PathBuf,
    #[arg(
        long,
        default_value = "target/strategic-map/terrain-routing-base-v3.json"
    )]
    base_terrain_output: PathBuf,
    #[arg(
        long,
        default_value = "target/strategic-map/terrain-routing-base-v3.pack"
    )]
    base_terrain_pack_output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceManifest {
    record_url: String,
    version: String,
    files: Vec<SourceFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    name: String,
    sha256: String,
    url: String,
    size: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Point([f64; 2]);

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Line {
    kind: String,
    importance: u8,
    points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Package {
    schema: u32,
    year: i32,
    bounds: [f64; 4],
    source: Source,
    roads: Vec<Line>,
    /// Full active Viabundus geometry used only by the offline routing pack.
    /// Presentation filtering and simplification must never affect it.
    routing_roads: Vec<Vec<Point>>,
    water: Vec<WaterPolygon>,
    wetlands: Vec<WaterPolygon>,
    cultivated: Vec<WaterPolygon>,
    elevation: ElevationLayer,
    forest: ForestLayer,
    tiles: TilePyramid,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct WaterPolygon {
    rings: Vec<Vec<Point>>,
}

struct CultivatedLand {
    polygons: Vec<Vec<Vec<[f64; 2]>>>,
    source_sha256: String,
}

#[derive(Serialize)]
struct DeploymentPackage<'a> {
    schema: u32,
    renderer_revision: u32,
    year: i32,
    bounds: [f64; 4],
    source: &'a Source,
    elevation: DeploymentLayer<'a>,
    forest: DeploymentForestLayer<'a>,
    cultivation: DeploymentCultivation,
    tiles: &'a TilePyramid,
    terrain_package_sha256: String,
    inferred_road_geometry_sha256: String,
    wetland_source_sha256: String,
    package_sha256: String,
}

#[derive(Serialize)]
struct DeploymentLayer<'a> {
    source: &'a raster::LayerSource,
}

#[derive(Serialize)]
struct DeploymentForestLayer<'a> {
    source: &'a raster::LayerSource,
    coverage_tiles: usize,
}

#[derive(Serialize)]
struct DeploymentCultivation {
    grid_crs: &'static str,
    grid_resolution_m: u16,
    rules_version: u16,
    source_sha256: String,
    square_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct TilePyramid {
    format: &'static str,
    tile_size: u32,
    gutter: u8,
    max_zoom: u8,
    content_sha256: String,
    entries: Vec<TileEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct TileEntry {
    theme: &'static str,
    zoom: u8,
    x: u16,
    y: u16,
    offset: u64,
    length: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Source {
    name: &'static str,
    version: String,
    url: &'static str,
    license: &'static str,
    files_sha256: BTreeMap<String, String>,
    verification_status: &'static str,
}

const RENDER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let render = std::thread::Builder::new()
        .name("strategic-map-renderer".into())
        .stack_size(RENDER_STACK_BYTES)
        .spawn(move || run(args).map_err(|error| error.to_string()))?;
    match render.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(message)) => Err(std::io::Error::other(message).into()),
        Err(_) => Err(std::io::Error::other("strategic map renderer panicked").into()),
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let layers = raster::load(&args.elevation_dir, &args.forest_cover_dir, BOUNDS)?;
    let mut package = build(&args.viabundus_dir, layers)?;
    let wetland =
        adventuresim_world_import::wetland_spatial_data(&args.potential_vegetation_dir, BOUNDS)?;
    package.wetlands = wetland
        .presentation_polygons
        .iter()
        .map(|rings| WaterPolygon {
            rings: rings
                .iter()
                .map(|ring| ring.iter().copied().map(Point).collect())
                .collect(),
        })
        .collect();
    let base_features = terrain_features::build(
        &package,
        wetland.polygons.clone(),
        wetland.source_sha256.clone(),
    );
    if args.base_only {
        let terrain = adventuresim_terrain::builder::build(
            &args.elevation_dir,
            &args.forest_cover_dir,
            BOUNDS,
            &args.base_terrain_output,
            &args.base_terrain_pack_output,
            &base_features,
            adventuresim_terrain::TerrainPurpose::DocumentedBase,
        )?;
        write_data_license(&[&args.base_terrain_output, &args.base_terrain_pack_output])?;
        println!(
            "Wrote documented-road base terrain {} (digest {}, {} wetland pixels)",
            args.base_terrain_output.display(),
            terrain.package_sha256,
            terrain.wetland_cells
        );
        return Ok(());
    }
    let base = adventuresim_terrain::TerrainPack::load(
        &args.base_terrain_output,
        &args.base_terrain_pack_output,
    )?;
    let current_base_road_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&base_features.roads)?)
    );
    if !base_contract_matches(
        base.purpose(),
        base.bounds(),
        base.source_resolution_m(),
        base.road_geometry_sha256(),
        base.wetland_source_sha256(),
        &current_base_road_digest,
        &base_features.wetland_source_sha256,
    ) {
        return Err(
            "base terrain does not match current documented roads, wetlands, bounds, or resolution"
                .into(),
        );
    }
    let world: CompiledWorld = serde_json::from_slice(&fs::read(&args.compiled_world)?)?;
    adventuresim_world_import::validate_world(&world)?;
    if world.report.base_terrain_package_sha256 != base.package_sha256() {
        return Err("compiled world was inferred against a different base terrain digest".into());
    }
    append_inferred_roads(&mut package, &world);
    package.roads.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.importance.cmp(&b.importance))
            .then_with(|| point_order(&a.points, &b.points))
    });
    package.routing_roads.sort_by(|a, b| point_order(a, b));
    let cultivated = cultivated_land(&args.hyde_dir, &base, &package, &world)?;
    let terrain_features = terrain_features::finalize(
        &mut package,
        wetland.polygons,
        wetland.source_sha256,
        cultivated,
        &world,
    );
    let terrain = adventuresim_terrain::builder::build(
        &args.elevation_dir,
        &args.forest_cover_dir,
        BOUNDS,
        &args.terrain_output,
        &args.terrain_pack_output,
        &terrain_features,
        adventuresim_terrain::TerrainPurpose::Final,
    )?;
    let expected_road_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&terrain_features.roads)?)
    );
    if terrain.road_geometry_sha256 != expected_road_digest {
        return Err(
            "final terrain road mask identity differs from rendered routing geometry".into(),
        );
    }
    let native_terrain =
        adventuresim_terrain::TerrainPack::load(&args.terrain_output, &args.terrain_pack_output)?;
    let (tile_manifest, tile_bytes) = tiles::build(
        &package,
        Some(&native_terrain),
        tiles::TileConfig::default(),
    )?;
    package.tiles = tile_manifest;
    let mut deployment = deployment_package(
        &package,
        &terrain.package_sha256,
        &world.report.inferred_road_geometry_sha256,
        &terrain.wetland_source_sha256,
        &terrain.cultivation_source_sha256,
    );
    deployment.package_sha256 = package_digest(&deployment)?;
    let mut bytes = serde_json::to_vec(&deployment)?;
    bytes.push(b'\n');
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, bytes)?;
    if let Some(parent) = args.tiles_output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.tiles_output, tile_bytes)?;
    write_data_license(&[
        &args.output,
        &args.tiles_output,
        &args.terrain_output,
        &args.terrain_pack_output,
    ])?;
    println!(
        "Wrote {} roads, {} water polygons, {} elevation cells, {} contours, {} forest regions, and {} AVIF tiles to {} and {}",
        package.roads.len(),
        package.water.len(),
        package.elevation.cells.len(),
        package.elevation.contours.len(),
        package.forest.regions.len(),
        package.tiles.entries.len(),
        args.output.display(),
        args.tiles_output.display()
    );
    println!(
        "Wrote {} native 30 m terrain chunks to {} and {} (digest {})",
        terrain.entries.len(),
        args.terrain_output.display(),
        args.terrain_pack_output.display(),
        terrain.package_sha256
    );
    Ok(())
}

fn base_contract_matches(
    purpose: adventuresim_terrain::TerrainPurpose,
    bounds: [f64; 4],
    resolution: u16,
    road_digest: &str,
    wetland_digest: &str,
    current_road_digest: &str,
    current_wetland_digest: &str,
) -> bool {
    purpose == adventuresim_terrain::TerrainPurpose::DocumentedBase
        && bounds == BOUNDS
        && resolution == 30
        && road_digest == current_road_digest
        && wetland_digest == current_wetland_digest
}

fn append_inferred_roads(package: &mut Package, world: &CompiledWorld) {
    let geometries = world
        .edges
        .iter()
        .filter(|edge| edge.provenance == TravelEdgeProvenance::InferredWalkingLink)
        .map(|edge| edge.geometry.as_slice())
        .collect::<Vec<_>>();
    append_inferred_geometry(package, &geometries);
}

fn append_inferred_geometry(
    package: &mut Package,
    geometries: &[&[adventuresim_world_schema::TravelGeometryPoint]],
) {
    for geometry in geometries {
        let points = geometry
            .iter()
            .map(|point| Point([point.longitude(), point.latitude()]))
            .collect::<Vec<_>>();
        package.routing_roads.push(points.clone());
        package.roads.push(Line {
            kind: "inferred".into(),
            importance: 4,
            points,
        });
    }
}

fn cultivated_land(
    hyde_dir: &Path,
    terrain: &adventuresim_terrain::TerrainPack,
    package: &Package,
    world: &CompiledWorld,
) -> Result<CultivatedLand, Box<dyn std::error::Error>> {
    use adventuresim_world_import::{
        cultivation::{
            CultivationCandidate, CultivationCell, HydeCropQuota, MetricSegment,
            SegmentDistanceIndex, allocate, square_is_usable,
        },
        spatial::{ProjectedCoordinate, SpatialProjection},
    };
    let projection = SpatialProjection::new()?;
    let projected_corners = [
        projection.project(BOUNDS[1], BOUNDS[0])?,
        projection.project(BOUNDS[1], BOUNDS[2])?,
        projection.project(BOUNDS[3], BOUNDS[0])?,
        projection.project(BOUNDS[3], BOUNDS[2])?,
    ];
    let min_column = projected_corners
        .iter()
        .map(|point| point.easting_millimeters().div_euclid(1_000_000))
        .min()
        .ok_or("projected map has no corners")?
        - 1;
    let max_column = projected_corners
        .iter()
        .map(|point| point.easting_millimeters().div_euclid(1_000_000))
        .max()
        .ok_or("projected map has no corners")?
        + 1;
    let min_row = projected_corners
        .iter()
        .map(|point| point.northing_millimeters().div_euclid(1_000_000))
        .min()
        .ok_or("projected map has no corners")?
        - 1;
    let max_row = projected_corners
        .iter()
        .map(|point| point.northing_millimeters().div_euclid(1_000_000))
        .max()
        .ok_or("projected map has no corners")?
        + 1;
    let metric_point = |point: ProjectedCoordinate| {
        [
            point.easting_millimeters().div_euclid(1_000),
            point.northing_millimeters().div_euclid(1_000),
        ]
    };
    let settlement_segments = world
        .settlements
        .iter()
        .map(|settlement| projection.project(settlement.latitude, settlement.longitude))
        .map(|point| {
            point.map(|point| {
                let point = metric_point(point);
                MetricSegment {
                    from: point,
                    to: point,
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let project_segments =
        |lines: &[Vec<Point>]| -> Result<Vec<MetricSegment>, Box<dyn std::error::Error>> {
            lines
                .iter()
                .flat_map(|line| line.windows(2))
                .map(|pair| {
                    Ok(MetricSegment {
                        from: metric_point(projection.project(pair[0].0[1], pair[0].0[0])?),
                        to: metric_point(projection.project(pair[1].0[1], pair[1].0[0])?),
                    })
                })
                .collect()
        };
    let settlement_index = SegmentDistanceIndex::new(settlement_segments)?;
    let road_index = SegmentDistanceIndex::new(project_segments(&package.routing_roads)?)?;
    let water_lines = package
        .water
        .iter()
        .flat_map(|polygon| polygon.rings.iter().cloned())
        .collect::<Vec<_>>();
    let water_index = SegmentDistanceIndex::new(project_segments(&water_lines)?)?;
    let mut candidates = Vec::new();
    for row in min_row..=max_row {
        for column in min_column..=max_column {
            let center = ProjectedCoordinate::from_meters(
                column as f64 * 1_000.0 + 500.0,
                row as f64 * 1_000.0 + 500.0,
            )?;
            let (latitude, longitude) = projection.unproject(center)?;
            if longitude < BOUNDS[0]
                || longitude >= BOUNDS[2]
                || latitude < BOUNDS[1]
                || latitude >= BOUNDS[3]
            {
                continue;
            }
            let native = terrain.cell(latitude, longitude)?;
            let mut non_water_samples = 0u16;
            for sample_row in 0..4 {
                for sample_column in 0..4 {
                    let sample = ProjectedCoordinate::from_meters(
                        column as f64 * 1_000.0 + (f64::from(sample_column) + 0.5) * 250.0,
                        row as f64 * 1_000.0 + (f64::from(sample_row) + 0.5) * 250.0,
                    )?;
                    let (sample_latitude, sample_longitude) = projection.unproject(sample)?;
                    if terrain
                        .cell(sample_latitude, sample_longitude)?
                        .is_some_and(|cell| {
                            !matches!(cell.surface, adventuresim_terrain::Surface::Water)
                        })
                    {
                        non_water_samples += 1;
                    }
                }
            }
            let usable_land = square_is_usable(non_water_samples, 16);
            let elevation_samples = [
                (latitude, longitude),
                (latitude + 0.0045, longitude),
                (latitude - 0.0045, longitude),
                (latitude, longitude + 0.0075),
                (latitude, longitude - 0.0075),
            ]
            .into_iter()
            .filter_map(|(latitude, longitude)| terrain.cell(latitude, longitude).ok().flatten())
            .map(|cell| cell.elevation_m)
            .collect::<Vec<_>>();
            let relief = elevation_samples
                .iter()
                .max()
                .zip(elevation_samples.iter().min())
                .map_or(0, |(high, low)| high.saturating_sub(*low).max(0) as u16);
            let hyde_row = ((90.0 - latitude) * 12.0).floor().clamp(0.0, 2_159.0) as i16;
            let hyde_column = ((longitude + 180.0) * 12.0).floor().clamp(0.0, 4_319.0) as i16;
            candidates.push(CultivationCandidate {
                cell: CultivationCell { column, row },
                hyde_cell: (hyde_row, hyde_column),
                usable_land,
                settlement_distance_m: settlement_index
                    .nearest_distance_m(metric_point(center), 100_000),
                road_distance_m: road_index.nearest_distance_m(metric_point(center), 10_000),
                water_distance_m: water_index.nearest_distance_m(metric_point(center), 10_000),
                slope_permille: native.map_or(0, |cell| {
                    if cell.hilly_fraction_percent >= 50 {
                        268
                    } else {
                        0
                    }
                }),
                relief_m: relief,
                canopy_percent: native.map_or(0, |cell| cell.canopy_percent),
            });
        }
    }
    let (hyde, source_sha256) = adventuresim_world_import::hyde_crop_cells(hyde_dir, YEAR, BOUNDS)?;
    let quotas = hyde
        .into_iter()
        .map(|cell| {
            let longitude_fraction = ((cell.bounds[2].min(BOUNDS[2])
                - cell.bounds[0].max(BOUNDS[0]))
                / (cell.bounds[2] - cell.bounds[0]))
                .clamp(0.0, 1.0);
            let latitude_fraction = ((cell.bounds[3].min(BOUNDS[3])
                - cell.bounds[1].max(BOUNDS[1]))
                / (cell.bounds[3] - cell.bounds[1]))
                .clamp(0.0, 1.0);
            HydeCropQuota {
                cell: (cell.row, cell.column),
                crop_km2: cell.crop_km2 * longitude_fraction * latitude_fraction,
                boundary_clipped: longitude_fraction < 1.0 || latitude_fraction < 1.0,
            }
        })
        .collect::<Vec<_>>();
    let allocation = allocate(&candidates, &quotas)?;
    if allocation.residual_km2.abs() >= 0.500_001 {
        return Err("cultivation quota rounding residual exceeded 0.5 km2".into());
    }
    if allocation.capacity_limited_km2 > 0 {
        eprintln!(
            "Cultivation capacity omitted {} km2 that cannot be represented by usable canonical squares under the bounded grid rules",
            allocation.capacity_limited_km2
        );
    }
    let polygons = allocation
        .cells
        .into_iter()
        .map(|cell| {
            let ring = [
                (cell.column as f64 * 1_000.0, cell.row as f64 * 1_000.0),
                (
                    (cell.column + 1) as f64 * 1_000.0,
                    cell.row as f64 * 1_000.0,
                ),
                (
                    (cell.column + 1) as f64 * 1_000.0,
                    (cell.row + 1) as f64 * 1_000.0,
                ),
                (
                    cell.column as f64 * 1_000.0,
                    (cell.row + 1) as f64 * 1_000.0,
                ),
                (cell.column as f64 * 1_000.0, cell.row as f64 * 1_000.0),
            ]
            .into_iter()
            .map(|(easting, northing)| {
                let (latitude, longitude) =
                    projection.unproject(ProjectedCoordinate::from_meters(easting, northing)?)?;
                Ok([longitude, latitude])
            })
            .collect::<Result<Vec<_>, adventuresim_world_import::Error>>()?;
            Ok(vec![ring])
        })
        .collect::<Result<Vec<_>, adventuresim_world_import::Error>>()?;
    Ok(CultivatedLand {
        polygons,
        source_sha256,
    })
}

fn write_data_license(outputs: &[&Path]) -> std::io::Result<()> {
    let directories = outputs
        .iter()
        .map(|output| {
            output
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        })
        .collect::<BTreeSet<_>>();
    for directory in directories {
        fs::create_dir_all(&directory)?;
        fs::write(directory.join(DATA_LICENSE_FILENAME), DATA_LICENSE)?;
    }
    Ok(())
}

fn build(root: &Path, layers: MapRasterLayers) -> Result<Package, Box<dyn std::error::Error>> {
    let layers = clip_raster_layers(layers, BOUNDS);
    let manifest: SourceManifest =
        serde_json::from_slice(&fs::read(root.join(".viabundus-source.json"))?)?;
    if manifest.version != "2" || manifest.record_url != RECORD_URL {
        return Err("strategic map requires Viabundus v2".into());
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_SOURCE_FILES {
        return Err("Viabundus sidecar file inventory is outside its bound".into());
    }
    let mut names = BTreeSet::new();
    for entry in &manifest.files {
        let safe = !entry.name.is_empty()
            && entry.name.len() <= 128
            && entry
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            && entry.name.ends_with(".csv");
        let hash_ok = entry.sha256.len() == 64
            && entry
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let expected_url = format!("{RECORD_URL}/files/{}/content", entry.name);
        if !safe || !hash_ok || entry.url != expected_url || !names.insert(entry.name.as_str()) {
            return Err(format!("invalid Viabundus sidecar entry {}", entry.name).into());
        }
    }
    let required = ["edges.csv", "water-1500.csv"];
    let mut identities = BTreeMap::new();
    for name in required {
        let entry = manifest
            .files
            .iter()
            .find(|entry| entry.name == name)
            .ok_or("source manifest is incomplete")?;
        let bytes = fs::read(root.join(name))?;
        if entry.size != bytes.len() as u64 {
            return Err(format!("{name} does not match its initialized size").into());
        }
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != entry.sha256 {
            return Err(format!("{name} does not match its initialized SHA-256").into());
        }
        identities.insert(name.to_string(), actual);
    }

    let mut roads = Vec::new();
    let mut routing_roads = Vec::new();
    let mut reader = csv::Reader::from_path(root.join("edges.csv"))?;
    for row in reader.deserialize::<BTreeMap<String, String>>() {
        let row = row?;
        if !active(&row, YEAR) {
            continue;
        }
        let zoom = row
            .get("zoomlevel")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(99);
        let Some(wkt) = row.get("wkt") else { continue };
        for points in clip_polyline(&coordinates(wkt), BOUNDS) {
            if points.len() >= 2 {
                routing_roads.push(points.clone());
            }
            if zoom > 4 {
                continue;
            }
            let points = simplify(&points, 0.001);
            if points.len() < 2 {
                continue;
            }
            roads.push(Line {
                kind: if row.get("type").is_some_and(|v| v == "ferry") {
                    "ferry"
                } else {
                    "land"
                }
                .into(),
                importance: zoom,
                points,
            });
        }
    }
    roads.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.importance.cmp(&b.importance))
            .then_with(|| point_order(&a.points, &b.points))
    });
    routing_roads.sort_by(|a, b| point_order(a, b));

    let mut water = Vec::new();
    let mut reader = csv::Reader::from_path(root.join("water-1500.csv"))?;
    for row in reader.records() {
        let row = row?;
        let Some(wkt) = row.get(0) else { continue };
        for polygon in wkt_polygons(wkt) {
            let rings: Vec<_> = polygon
                .into_iter()
                .map(|ring| simplify(&clip_polygon(&ring, BOUNDS), 0.002))
                .filter(|ring| ring.len() >= 4)
                .collect();
            if !rings.is_empty() {
                water.push(WaterPolygon { rings });
            }
        }
    }
    water.sort_by(|a, b| point_order(&a.rings[0], &b.rings[0]));

    let source = Source {
        name: "Viabundus Pre-modern Street Map 2",
        version: manifest.version,
        url: VIABUNDUS_DOI,
        license: "CC-BY-SA-4.0",
        files_sha256: identities,
        verification_status: "verified",
    };
    let package = Package {
        schema: PACKAGE_SCHEMA,
        year: YEAR,
        bounds: BOUNDS,
        source,
        roads,
        routing_roads,
        water,
        wetlands: Vec::new(),
        cultivated: Vec::new(),
        elevation: layers.elevation,
        forest: layers.forest,
        tiles: TilePyramid {
            format: "avif",
            tile_size: 0,
            gutter: 0,
            max_zoom: 0,
            content_sha256: String::new(),
            entries: Vec::new(),
        },
    };
    validate_geometry(&package)?;
    Ok(package)
}

fn clip_raster_layers(mut layers: MapRasterLayers, bounds: [f64; 4]) -> MapRasterLayers {
    layers.elevation.cells = layers
        .elevation
        .cells
        .into_iter()
        .filter_map(|mut cell| {
            cell.bounds = bounds_intersection(cell.bounds, bounds)?;
            Some(cell)
        })
        .collect();
    layers.elevation.contours = layers
        .elevation
        .contours
        .into_iter()
        .flat_map(|contour| {
            let points = contour.points.into_iter().map(Point).collect::<Vec<_>>();
            clip_polyline(&points, bounds)
                .into_iter()
                .map(move |points| raster::ElevationContour {
                    elevation_m: contour.elevation_m,
                    points: points.into_iter().map(|point| point.0).collect(),
                })
        })
        .collect();
    layers.forest.coverage = layers
        .forest
        .coverage
        .into_iter()
        .filter_map(|coverage| bounds_intersection(coverage, bounds))
        .collect();
    layers.forest.regions = layers
        .forest
        .regions
        .into_iter()
        .filter_map(|mut region| {
            region.bounds = bounds_intersection(region.bounds, bounds)?;
            Some(region)
        })
        .collect();
    layers
}

fn bounds_intersection(value: [f64; 4], bounds: [f64; 4]) -> Option<[f64; 4]> {
    let clipped = [
        value[0].max(bounds[0]),
        value[1].max(bounds[1]),
        value[2].min(bounds[2]),
        value[3].min(bounds[3]),
    ];
    (clipped[0] < clipped[2] && clipped[1] < clipped[3]).then_some(clipped)
}

fn deployment_package<'a>(
    package: &'a Package,
    terrain_package_sha256: &str,
    inferred_geometry_sha256: &str,
    wetland_source_sha256: &str,
    cultivation_source_sha256: &str,
) -> DeploymentPackage<'a> {
    DeploymentPackage {
        schema: PACKAGE_SCHEMA,
        renderer_revision: RENDERER_REVISION,
        year: package.year,
        bounds: package.bounds,
        source: &package.source,
        elevation: DeploymentLayer {
            source: &package.elevation.source,
        },
        forest: DeploymentForestLayer {
            source: &package.forest.source,
            coverage_tiles: package.forest.coverage.len(),
        },
        cultivation: DeploymentCultivation {
            grid_crs: "EPSG:3035",
            grid_resolution_m: 1_000,
            rules_version: adventuresim_world_import::cultivation::CULTIVATION_RULES_VERSION,
            source_sha256: cultivation_source_sha256.into(),
            square_count: package.cultivated.len(),
        },
        tiles: &package.tiles,
        terrain_package_sha256: terrain_package_sha256.into(),
        inferred_road_geometry_sha256: inferred_geometry_sha256.into(),
        wetland_source_sha256: wetland_source_sha256.into(),
        package_sha256: "0".repeat(64),
    }
}

fn package_digest(package: &DeploymentPackage<'_>) -> Result<String, serde_json::Error> {
    debug_assert_eq!(package.package_sha256, "0".repeat(64));
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(package)?)
    ))
}

fn active(row: &BTreeMap<String, String>, year: i32) -> bool {
    let from = row
        .get("fromyear")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(i32::MIN);
    let to = row
        .get("toyear")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(i32::MAX);
    from <= year && year < to
}

fn coordinates(wkt: &str) -> Vec<Point> {
    wkt.split(['(', ')', ','])
        .filter_map(|part| {
            let mut values = part
                .split_whitespace()
                .filter_map(|v| v.parse::<f64>().ok());
            Some(Point([values.next()?, values.next()?]))
        })
        .collect()
}

fn wkt_polygons(wkt: &str) -> Vec<Vec<Vec<Point>>> {
    let polygon_depth = if wkt.trim_start().starts_with("MULTIPOLYGON") {
        2
    } else if wkt.trim_start().starts_with("POLYGON") {
        1
    } else {
        return Vec::new();
    };
    let ring_depth = polygon_depth + 1;
    let mut depth = 0_usize;
    let mut polygons = Vec::new();
    let mut polygon = Vec::new();
    let mut ring = String::new();
    for character in wkt.chars() {
        match character {
            '(' => {
                depth += 1;
                if depth == polygon_depth {
                    polygon.clear();
                } else if depth == ring_depth {
                    ring.clear();
                }
            }
            ')' => {
                if depth == ring_depth {
                    let points = coordinates(&ring);
                    if points.len() >= 4 {
                        polygon.push(points);
                    }
                } else if depth == polygon_depth && !polygon.is_empty() {
                    polygons.push(std::mem::take(&mut polygon));
                }
                depth = depth.saturating_sub(1);
            }
            _ if depth == ring_depth => ring.push(character),
            _ => {}
        }
    }
    polygons
}

fn clip_polyline(points: &[Point], bounds: [f64; 4]) -> Vec<Vec<Point>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    for pair in points.windows(2) {
        if let Some((start, end)) = clip_segment(&pair[0], &pair[1], bounds) {
            if current.last() != Some(&start) {
                if current.len() >= 2 {
                    output.push(std::mem::take(&mut current));
                }
                current.push(start);
            }
            current.push(end);
        } else if current.len() >= 2 {
            output.push(std::mem::take(&mut current));
        }
    }
    if current.len() >= 2 {
        output.push(current);
    }
    output
}

fn clip_segment(
    start: &Point,
    end: &Point,
    [west, south, east, north]: [f64; 4],
) -> Option<(Point, Point)> {
    let [x0, y0] = start.0;
    let [x1, y1] = end.0;
    if ![x0, y0, x1, y1].into_iter().all(f64::is_finite) {
        return None;
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for (p, q) in [
        (-dx, x0 - west),
        (dx, east - x0),
        (-dy, y0 - south),
        (dy, north - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                low = low.max(t);
            } else {
                high = high.min(t);
            }
        }
        if low > high {
            return None;
        }
    }
    Some((
        Point([x0 + low * dx, y0 + low * dy]),
        Point([x0 + high * dx, y0 + high * dy]),
    ))
}

fn clip_polygon(points: &[Point], bounds: [f64; 4]) -> Vec<Point> {
    let mut output = points.to_vec();
    for edge in 0..4 {
        let input = std::mem::take(&mut output);
        if input.is_empty() {
            break;
        }
        let mut previous = input.last().expect("nonempty").clone();
        for current in input {
            let previous_inside = polygon_inside(&previous, edge, bounds);
            let current_inside = polygon_inside(&current, edge, bounds);
            if current_inside {
                if !previous_inside {
                    output.push(polygon_intersection(&previous, &current, edge, bounds));
                }
                output.push(current.clone());
            } else if previous_inside {
                output.push(polygon_intersection(&previous, &current, edge, bounds));
            }
            previous = current;
        }
    }
    if output.len() >= 3 && output.first() != output.last() {
        output.push(output[0].clone());
    }
    output
}

fn polygon_inside(point: &Point, edge: usize, [west, south, east, north]: [f64; 4]) -> bool {
    point.0.into_iter().all(f64::is_finite)
        && match edge {
            0 => point.0[0] >= west,
            1 => point.0[0] <= east,
            2 => point.0[1] >= south,
            _ => point.0[1] <= north,
        }
}

fn polygon_intersection(start: &Point, end: &Point, edge: usize, bounds: [f64; 4]) -> Point {
    let [x0, y0] = start.0;
    let [x1, y1] = end.0;
    if edge < 2 {
        let x = if edge == 0 { bounds[0] } else { bounds[2] };
        let t = if x1 == x0 { 0.0 } else { (x - x0) / (x1 - x0) };
        Point([x, y0 + t * (y1 - y0)])
    } else {
        let y = if edge == 2 { bounds[1] } else { bounds[3] };
        let t = if y1 == y0 { 0.0 } else { (y - y0) / (y1 - y0) };
        Point([x0 + t * (x1 - x0), y])
    }
}

fn validate_geometry(package: &Package) -> Result<(), Box<dyn std::error::Error>> {
    let valid = |point: &Point| {
        point.0[0].is_finite()
            && point.0[1].is_finite()
            && point.0[0] >= package.bounds[0]
            && point.0[0] <= package.bounds[2]
            && point.0[1] >= package.bounds[1]
            && point.0[1] <= package.bounds[3]
    };
    if package
        .roads
        .iter()
        .any(|line| line.points.len() < 2 || line.points.iter().any(|point| !valid(point)))
        || package.water.iter().any(|polygon| {
            polygon.rings.is_empty()
                || polygon
                    .rings
                    .iter()
                    .any(|ring| ring.len() < 4 || ring.iter().any(|point| !valid(point)))
        })
        || package.elevation.cells.iter().any(|cell| {
            !valid_bounds(cell.bounds, package.bounds)
                || ![50, 100, 250, 500, 1_000, 1_500, 2_000].contains(&cell.band_m)
        })
        || package.elevation.contours.iter().any(|line| {
            line.points.len() != 2
                || ![50, 100, 250, 500, 1_000, 1_500, 2_000].contains(&line.elevation_m)
                || line.points.iter().any(|point| !valid(&Point(*point)))
        })
        || package
            .forest
            .coverage
            .iter()
            .any(|bounds| !valid_bounds(*bounds, package.bounds))
        || package.forest.regions.iter().any(|region| {
            !valid_bounds(region.bounds, package.bounds)
                || !(1..=100).contains(&region.density)
                || !matches!(region.kind.as_str(), "broadleaf" | "conifer" | "mixed")
        })
    {
        return Err("strategic map geometry is non-finite or outside package bounds".into());
    }
    Ok(())
}

fn valid_bounds(
    [west, south, east, north]: [f64; 4],
    [map_west, map_south, map_east, map_north]: [f64; 4],
) -> bool {
    [west, south, east, north].into_iter().all(f64::is_finite)
        && west >= map_west
        && east <= map_east
        && south >= map_south
        && north <= map_north
        && west < east
        && south < north
}

fn simplify(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut kept = vec![false; points.len()];
    kept[0] = true;
    kept[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        let mut best = (0.0, 0);
        for index in start + 1..end {
            let distance = segment_distance(&points[index], &points[start], &points[end]);
            if distance > best.0 {
                best = (distance, index);
            }
        }
        if best.0 > tolerance {
            kept[best.1] = true;
            stack.push((start, best.1));
            stack.push((best.1, end));
        }
    }
    points
        .iter()
        .zip(kept)
        .filter_map(|(point, keep)| keep.then_some(point.clone()))
        .collect()
}

fn segment_distance(point: &Point, start: &Point, end: &Point) -> f64 {
    let [x, y] = point.0;
    let [x1, y1] = start.0;
    let [x2, y2] = end.0;
    let length = (x2 - x1).powi(2) + (y2 - y1).powi(2);
    if length == 0.0 {
        return ((x - x1).powi(2) + (y - y1).powi(2)).sqrt();
    }
    let t = (((x - x1) * (x2 - x1) + (y - y1) * (y2 - y1)) / length).clamp(0.0, 1.0);
    ((x - (x1 + t * (x2 - x1))).powi(2) + (y - (y1 + t * (y2 - y1))).powi(2)).sqrt()
}

fn point_order(left: &[Point], right: &[Point]) -> std::cmp::Ordering {
    left.first()
        .and_then(|p| {
            right.first().map(|q| {
                p.0[0]
                    .total_cmp(&q.0[0])
                    .then_with(|| p.0[1].total_cmp(&q.0[1]))
            })
        })
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn cli_defaults_to_the_initialized_hyde_directory() {
        let args = Args::try_parse_from(["build-strategic-map"]).unwrap();

        assert_eq!(
            args.hyde_dir,
            PathBuf::from("target/world-data-sources/raw/hyde35-land-use")
        );
    }

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-map-package-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let edges = b"id,section,type,certainty,zoomlevel,fromyear,toyear,descriptionid,length,fromnode,tonode,wkt,slopemultiplier\n1,A,land,1,2,1500,,x,100,1,2,\"LINESTRING(9 51,10 51.2,11 51.3)\",1\n2,B,land,1,6,1500,,x,100,2,3,\"LINESTRING(10 51.3,10.123456 51.345678,10.5 51.5)\",1\n";
        let water = b"WKT\n\"MULTIPOLYGON (((9 51,10 51,10 52,9 51)))\"\n";
        fs::write(root.join("edges.csv"), edges).unwrap();
        fs::write(root.join("water-1500.csv"), water).unwrap();
        let manifest = serde_json::json!({"record_url":RECORD_URL,"version":"2","files":[
            {"name":"edges.csv","sha256":format!("{:x}", Sha256::digest(edges)),"url":format!("{RECORD_URL}/files/edges.csv/content"),"size":edges.len()},
            {"name":"water-1500.csv","sha256":format!("{:x}", Sha256::digest(water)),"url":format!("{RECORD_URL}/files/water-1500.csv/content"),"size":water.len()}
        ]});
        fs::write(
            root.join(".viabundus-source.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        root
    }

    fn layers() -> MapRasterLayers {
        let layer_source = || raster::LayerSource {
            name: "Fixture".into(),
            version: "1".into(),
            url: "https://example.invalid/source".into(),
            license: "test-only".into(),
            file_count: 1,
            files_sha256: BTreeMap::new(),
            verification_status: "fixture".into(),
        };
        MapRasterLayers {
            elevation: ElevationLayer {
                source: layer_source(),
                cells: vec![raster::ElevationCell {
                    bounds: [9.0, 51.0, 9.25, 51.25],
                    band_m: 100,
                }],
                contours: vec![raster::ElevationContour {
                    elevation_m: 100,
                    points: vec![[9.0, 51.0], [9.25, 51.25]],
                }],
            },
            forest: ForestLayer {
                source: layer_source(),
                coverage: vec![[9.0, 51.0, 10.0, 52.0]],
                regions: vec![raster::ForestRegion {
                    bounds: [9.0, 51.0, 9.05, 51.05],
                    density: 2,
                    kind: "mixed".into(),
                }],
            },
        }
    }
    #[test]
    fn simplification_is_deterministic_and_keeps_endpoints() {
        let points = vec![Point([0.0, 0.0]), Point([0.5, 0.01]), Point([1.0, 0.0])];
        assert_eq!(
            simplify(&points, 0.02),
            vec![Point([0.0, 0.0]), Point([1.0, 0.0])]
        );
        assert_eq!(simplify(&points, 0.001), points);
    }
    #[test]
    fn year_filter_uses_half_open_intervals() {
        let row = BTreeMap::from([
            ("fromyear".into(), "1544".into()),
            ("toyear".into(), "1545".into()),
        ]);
        assert!(active(&row, 1544));
        assert!(!active(&row, 1545));
    }

    #[test]
    fn wkt_water_parser_preserves_polygon_ring_groups() {
        let polygons = wkt_polygons(
            "MULTIPOLYGON (((0 0,10 0,10 10,0 0),(2 2,3 2,3 3,2 2)),((20 20,21 20,21 21,20 20)))",
        );
        assert_eq!(polygons.len(), 2);
        assert_eq!(polygons[0].len(), 2);
        assert_eq!(polygons[1].len(), 1);
    }

    #[test]
    fn fixture_build_is_deterministic_and_rejects_changed_source_bytes() {
        let root = fixture();
        let first = build(&root, layers()).unwrap();
        let second = build(&root, layers()).unwrap();
        assert_eq!(first, second);
        let config = tiles::TileConfig {
            tile_size: 64,
            max_zoom: 0,
        };
        let (first_manifest, first_tiles) = tiles::build(&first, None, config).unwrap();
        let (second_manifest, second_tiles) = tiles::build(&second, None, config).unwrap();
        assert_eq!(first_manifest, second_manifest);
        assert_eq!(first_tiles, second_tiles);
        assert!(first_tiles.windows(8).any(|bytes| bytes == b"ftypavif"));
        assert_eq!(first_manifest.entries.len(), 247);
        assert_eq!(first_manifest.gutter, 4);
        assert_eq!(first.roads.len(), 1);
        assert_eq!(first.routing_roads.len(), 2);
        assert_eq!(first.routing_roads[1][1], Point([10.123456, 51.345678]));
        assert_eq!(first.water.len(), 1);
        assert_eq!(first.water[0].rings.len(), 1);

        let mut rendered = first.clone();
        rendered.tiles = first_manifest;
        let mut deployment = deployment_package(
            &rendered,
            &"0".repeat(64),
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
        );
        deployment.package_sha256 = package_digest(&deployment).unwrap();
        let value = serde_json::to_value(&deployment).unwrap();
        assert_eq!(value["schema"], PACKAGE_SCHEMA);
        assert_eq!(value["renderer_revision"], RENDERER_REVISION);
        for geometry in ["roads", "water", "cells", "contours", "regions"] {
            assert!(value.get(geometry).is_none());
        }
        assert!(value["elevation"].get("cells").is_none());
        assert!(value["forest"].get("regions").is_none());
        fs::write(root.join("edges.csv"), b"changed").unwrap();
        assert!(build(&root, layers()).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generated_bundle_directories_receive_the_canonical_data_license() {
        let root = fixture();
        let map_dir = root.join("map");
        let terrain_dir = root.join("terrain");
        write_data_license(&[
            &map_dir.join("strategic-map-v1.json"),
            &map_dir.join("strategic-map-tiles-v1.pack"),
            &terrain_dir.join("terrain-routing-v1.json"),
            &terrain_dir.join("terrain-routing-v1.pack"),
        ])
        .unwrap();
        assert_eq!(
            fs::read_to_string(map_dir.join(DATA_LICENSE_FILENAME)).unwrap(),
            DATA_LICENSE
        );
        assert_eq!(
            fs::read_to_string(terrain_dir.join(DATA_LICENSE_FILENAME)).unwrap(),
            DATA_LICENSE
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inferred_geometry_is_identical_in_visible_and_routing_inputs() {
        let root = fixture();
        let mut package = build(&root, layers()).unwrap();
        let geometry = [
            adventuresim_world_schema::TravelGeometryPoint::new(9.2, 51.2).unwrap(),
            adventuresim_world_schema::TravelGeometryPoint::new(9.3, 51.25).unwrap(),
        ];
        append_inferred_geometry(&mut package, &[&geometry]);
        let visible = &package.roads.last().unwrap().points;
        let routing = package.routing_roads.last().unwrap();
        assert_eq!(visible, routing);
        assert_eq!(package.roads.last().unwrap().kind, "inferred");
        let features = terrain_features(&package, Vec::new(), "0".repeat(64), Vec::new());
        assert_eq!(
            features.roads.last().unwrap(),
            &routing.iter().map(|point| point.0).collect::<Vec<_>>()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_base_inputs_must_match_loaded_contract() {
        let digest = "a".repeat(64);
        let wet = "b".repeat(64);
        assert!(base_contract_matches(
            adventuresim_terrain::TerrainPurpose::DocumentedBase,
            BOUNDS,
            30,
            &digest,
            &wet,
            &digest,
            &wet
        ));
        assert!(!base_contract_matches(
            adventuresim_terrain::TerrainPurpose::Final,
            BOUNDS,
            30,
            &digest,
            &wet,
            &digest,
            &wet
        ));
        assert!(!base_contract_matches(
            adventuresim_terrain::TerrainPurpose::DocumentedBase,
            BOUNDS,
            30,
            &digest,
            &wet,
            &"c".repeat(64),
            &wet
        ));
        assert!(!base_contract_matches(
            adventuresim_terrain::TerrainPurpose::DocumentedBase,
            BOUNDS,
            30,
            &digest,
            &wet,
            &digest,
            &"d".repeat(64)
        ));
    }

    #[test]
    fn sidecar_rejects_unknown_fields_duplicates_and_fabricated_urls() {
        for mutation in 0..3 {
            let root = fixture();
            let path = root.join(".viabundus-source.json");
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            match mutation {
                0 => value["fabricated"] = serde_json::json!(true),
                1 => {
                    let duplicate = value["files"][0].clone();
                    value["files"].as_array_mut().unwrap().push(duplicate);
                }
                _ => {
                    value["files"][0]["url"] =
                        serde_json::json!("https://example.invalid/edges.csv")
                }
            }
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(build(&root, layers()).is_err());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn clipping_drops_outside_geometry_and_bounds_crossings() {
        assert!(clip_polyline(&[Point([-20.0, 40.0]), Point([-15.0, 41.0])], BOUNDS).is_empty());
        let crossing = clip_polyline(&[Point([0.0, 51.5]), Point([20.0, 51.5])], BOUNDS);
        assert_eq!(
            crossing,
            vec![vec![Point([BOUNDS[0], 51.5]), Point([BOUNDS[2], 51.5])]]
        );
        let polygon = clip_polygon(
            &[
                Point([8.0, 51.0]),
                Point([10.0, 51.0]),
                Point([10.0, 53.0]),
                Point([8.0, 51.0]),
            ],
            BOUNDS,
        );
        assert!(
            polygon
                .iter()
                .all(|point| point.0[0] >= BOUNDS[0] && point.0[0] <= BOUNDS[2])
        );
    }
}
