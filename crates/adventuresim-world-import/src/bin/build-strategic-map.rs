use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use raster::{ElevationLayer, ForestLayer, MapRasterLayers};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[path = "build-strategic-map/raster.rs"]
mod raster;
#[path = "build-strategic-map/tiles.rs"]
mod tiles;

const PACKAGE_SCHEMA: u32 = 3;
const RENDERER_REVISION: u32 = 6;
const YEAR: i32 = 1544;
const VIABUNDUS_DOI: &str = "https://doi.org/10.5281/zenodo.16611998";
const RECORD_URL: &str = "https://zenodo.org/api/records/16611998";
const BOUNDS: [f64; 4] = [-11.0, 43.0, 31.0, 70.0];
const MAX_SOURCE_FILES: usize = 64;

#[derive(Parser)]
#[command(about = "Build the bounded AVIF strategic-map package from initialized world data")]
struct Args {
    #[arg(long, default_value = "viabundus")]
    viabundus_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/elevation")]
    elevation_dir: PathBuf,
    #[arg(long, default_value = "target/world-data-sources/raw/forest-cover")]
    forest_cover_dir: PathBuf,
    #[arg(long, default_value = "target/strategic-map/strategic-map-v1.json")]
    output: PathBuf,
    #[arg(
        long,
        default_value = "target/strategic-map/strategic-map-tiles-v1.pack"
    )]
    tiles_output: PathBuf,
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v1.json")]
    terrain_output: PathBuf,
    #[arg(long, default_value = "target/strategic-map/terrain-routing-v1.pack")]
    terrain_pack_output: PathBuf,
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
    #[serde(default)]
    size: Option<u64>,
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
    elevation: ElevationLayer,
    forest: ForestLayer,
    tiles: TilePyramid,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct WaterPolygon {
    rings: Vec<Vec<Point>>,
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
    tiles: &'a TilePyramid,
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
    let terrain_features = adventuresim_terrain::builder::Features {
        roads: package
            .routing_roads
            .iter()
            .map(|line| line.iter().map(|point| point.0).collect())
            .collect(),
        water: package
            .water
            .iter()
            .map(|polygon| {
                polygon
                    .rings
                    .iter()
                    .map(|ring| ring.iter().map(|point| point.0).collect())
                    .collect()
            })
            .collect(),
    };
    let terrain = adventuresim_terrain::builder::build(
        &args.elevation_dir,
        &args.forest_cover_dir,
        [5, 50, 16, 56],
        &args.terrain_output,
        &args.terrain_pack_output,
        &terrain_features,
    )?;
    let native_terrain =
        adventuresim_terrain::TerrainPack::load(&args.terrain_output, &args.terrain_pack_output)?;
    let (tile_manifest, tile_bytes) = tiles::build(
        &package,
        Some(&native_terrain),
        tiles::TileConfig::default(),
    )?;
    package.tiles = tile_manifest;
    let mut deployment = deployment_package(&package);
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

fn build(root: &Path, layers: MapRasterLayers) -> Result<Package, Box<dyn std::error::Error>> {
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
        if entry.size.is_some_and(|size| size != bytes.len() as u64) {
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
        verification_status: if manifest.files.iter().all(|entry| entry.size.is_some()) {
            "verified"
        } else {
            "legacy-release-blocked-missing-sizes"
        },
    };
    let package = Package {
        schema: PACKAGE_SCHEMA,
        year: YEAR,
        bounds: BOUNDS,
        source,
        roads,
        routing_roads,
        water,
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

fn deployment_package(package: &Package) -> DeploymentPackage<'_> {
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
        tiles: &package.tiles,
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
    wkt.split(|c: char| matches!(c, '(' | ')' | ','))
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

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-map-package-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let edges = b"id,section,type,certainty,zoomlevel,fromyear,toyear,descriptionid,length,fromnode,tonode,wkt,slopemultiplier\n1,A,land,1,2,1500,,x,100,1,2,\"LINESTRING(10 53,10.5 53.2,11 53.3)\",1\n2,B,land,1,6,1500,,x,100,2,3,\"LINESTRING(11 53.3,11.123456 53.345678,11.5 53.5)\",1\n";
        let water = b"WKT\n\"MULTIPOLYGON (((10 52,11 52,11 53,10 52)))\"\n";
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
                    bounds: [10.0, 53.0, 10.25, 53.25],
                    band_m: 100,
                }],
                contours: vec![raster::ElevationContour {
                    elevation_m: 100,
                    points: vec![[10.0, 53.0], [10.25, 53.25]],
                }],
            },
            forest: ForestLayer {
                source: layer_source(),
                coverage: vec![[10.0, 53.0, 11.0, 54.0]],
                regions: vec![raster::ForestRegion {
                    bounds: [10.0, 53.0, 10.05, 53.05],
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
        assert_eq!(first.routing_roads[1][1], Point([11.123456, 53.345678]));
        assert_eq!(first.water.len(), 1);
        assert_eq!(first.water[0].rings.len(), 1);

        let mut rendered = first.clone();
        rendered.tiles = first_manifest;
        let mut deployment = deployment_package(&rendered);
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
        let crossing = clip_polyline(&[Point([-20.0, 50.0]), Point([40.0, 50.0])], BOUNDS);
        assert_eq!(
            crossing,
            vec![vec![Point([-11.0, 50.0]), Point([31.0, 50.0])]]
        );
        let polygon = clip_polygon(
            &[
                Point([-20.0, 50.0]),
                Point([0.0, 50.0]),
                Point([0.0, 60.0]),
                Point([-20.0, 50.0]),
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
