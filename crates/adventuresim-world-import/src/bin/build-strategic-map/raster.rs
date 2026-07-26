use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use adventuresim_world_import::{read_prepared_forest_raster, validate_prepared_forest_manifest};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tiff::decoder::{Decoder, DecodingResult};

const ELEVATION_SAMPLES_PER_DEGREE: usize = 4;
const FOREST_CELLS_PER_DEGREE: usize = 20;
const FOREST_PIXELS_PER_DEGREE: usize = 1_000;
const ELEVATION_THRESHOLDS: [i16; 7] = [50, 100, 250, 500, 1_000, 1_500, 2_000];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct LayerSource {
    pub name: String,
    pub version: String,
    pub url: String,
    pub license: String,
    pub file_count: usize,
    pub files_sha256: BTreeMap<String, String>,
    pub verification_status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ElevationLayer {
    pub source: LayerSource,
    pub cells: Vec<ElevationCell>,
    pub contours: Vec<ElevationContour>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ElevationCell {
    pub bounds: [f64; 4],
    pub band_m: i16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ElevationContour {
    pub elevation_m: i16,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ForestLayer {
    pub source: LayerSource,
    pub coverage: Vec<[f64; 4]>,
    pub regions: Vec<ForestRegion>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct ForestRegion {
    pub bounds: [f64; 4],
    pub density: u8,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct MapRasterLayers {
    pub elevation: ElevationLayer,
    pub forest: ForestLayer,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DegreeTile {
    south: i16,
    west: i16,
}

impl DegreeTile {
    fn bounds(self) -> [f64; 4] {
        [
            f64::from(self.west),
            f64::from(self.south),
            f64::from(self.west + 1),
            f64::from(self.south + 1),
        ]
    }

    fn intersects(self, [west, south, east, north]: [f64; 4]) -> bool {
        let bounds = self.bounds();
        bounds[2] > west && bounds[0] < east && bounds[3] > south && bounds[1] < north
    }
}

pub(super) fn load(
    elevation_directory: &Path,
    forest_directory: &Path,
    bounds: [f64; 4],
) -> Result<MapRasterLayers, Box<dyn std::error::Error>> {
    Ok(MapRasterLayers {
        elevation: load_elevation(elevation_directory, bounds)?,
        forest: load_forest(forest_directory, bounds)?,
    })
}

fn load_elevation(
    directory: &Path,
    bounds: [f64; 4],
) -> Result<ElevationLayer, Box<dyn std::error::Error>> {
    let mut files = source_files(directory, "Copernicus_DSM_COG_10_", "_DEM.tif")?;
    files.sort();
    let mut cells = Vec::new();
    let mut contours = Vec::new();
    let mut used = 0;
    for path in files {
        let name = file_name(&path)?;
        let Some(tile) = elevation_tile(name) else {
            continue;
        };
        if !tile.intersects(bounds) {
            continue;
        }
        let samples = sample_elevation(&path)?;
        append_elevation_features(tile, &samples, &mut cells, &mut contours);
        used += 1;
    }
    if used == 0 {
        return Err("strategic map elevation has no source tiles within its bounds".into());
    }
    cells.sort_by(|a, b| {
        a.band_m
            .cmp(&b.band_m)
            .then_with(|| bounds_order(a.bounds, b.bounds))
    });
    contours.sort_by(|a, b| {
        a.elevation_m
            .cmp(&b.elevation_m)
            .then_with(|| point_order(&a.points, &b.points))
    });
    Ok(ElevationLayer {
        source: LayerSource {
            name: "Copernicus DEM GLO-30".into(),
            version: "GLO-30".into(),
            url: "https://doi.org/10.5270/ESA-c5d3d65".into(),
            license: "Copernicus DEM licence".into(),
            file_count: used,
            files_sha256: BTreeMap::new(),
            verification_status: "release-blocked-unpinned-tile-inventory".into(),
        },
        cells,
        contours,
    })
}

fn sample_elevation(path: &Path) -> Result<Vec<Option<f64>>, Box<dyn std::error::Error>> {
    let mut decoder = Decoder::new(BufReader::new(File::open(path)?))?;
    let (width, height) = decoder.dimensions()?;
    if ![1_800, 2_400, 3_600].contains(&width) || height != 3_600 {
        return Err(format!(
            "elevation tile {} has invalid GLO-30 dimensions {width}x{height}",
            path.display()
        )
        .into());
    }
    let (chunk_width, chunk_height) = decoder.chunk_dimensions();
    if chunk_width == 0 || chunk_height == 0 {
        return Err(format!("elevation tile {} has invalid chunks", path.display()).into());
    }
    let chunks_across = width.div_ceil(chunk_width);
    let side = ELEVATION_SAMPLES_PER_DEGREE + 1;
    let mut requests: BTreeMap<u32, Vec<(usize, u32, u32)>> = BTreeMap::new();
    for row in 0..side {
        let y = (u64::try_from(row)? * u64::from(height - 1)
            / u64::try_from(ELEVATION_SAMPLES_PER_DEGREE)?) as u32;
        for column in 0..side {
            let x = (u64::try_from(column)? * u64::from(width - 1)
                / u64::try_from(ELEVATION_SAMPLES_PER_DEGREE)?) as u32;
            let chunk = (y / chunk_height) * chunks_across + x / chunk_width;
            requests
                .entry(chunk)
                .or_default()
                .push((row * side + column, x, y));
        }
    }
    // Decode one source tile at a time (at most 49.5 MiB) and immediately
    // reduce it. This bounds memory independently of total source coverage.
    let DecodingResult::F32(values) = decoder.read_image()? else {
        return Err(format!("elevation tile {} is not Float32", path.display()).into());
    };
    if values.len() != width as usize * height as usize {
        return Err(format!("elevation tile {} is truncated", path.display()).into());
    }
    let mut samples = vec![None; side * side];
    for targets in requests.into_values() {
        for (index, x, y) in targets {
            let value = f64::from(values[y as usize * width as usize + x as usize]);
            samples[index] = value.is_finite().then_some(value.clamp(-500.0, 9_000.0));
        }
    }
    Ok(samples)
}

fn append_elevation_features(
    tile: DegreeTile,
    samples: &[Option<f64>],
    cells: &mut Vec<ElevationCell>,
    contours: &mut Vec<ElevationContour>,
) {
    let side = ELEVATION_SAMPLES_PER_DEGREE + 1;
    debug_assert_eq!(samples.len(), side * side);
    let step = 1.0 / ELEVATION_SAMPLES_PER_DEGREE as f64;
    for row in 0..ELEVATION_SAMPLES_PER_DEGREE {
        for column in 0..ELEVATION_SAMPLES_PER_DEGREE {
            let corners = [
                samples[row * side + column],
                samples[row * side + column + 1],
                samples[(row + 1) * side + column + 1],
                samples[(row + 1) * side + column],
            ];
            let valid = corners.into_iter().flatten().collect::<Vec<_>>();
            if valid.len() >= 2 {
                let mean = valid.iter().copied().sum::<f64>() / valid.len() as f64;
                if let Some(band_m) = elevation_band(mean) {
                    let west = f64::from(tile.west) + column as f64 * step;
                    let east = west + step;
                    let north = f64::from(tile.south + 1) - row as f64 * step;
                    cells.push(ElevationCell {
                        bounds: [west, north - step, east, north],
                        band_m,
                    });
                }
            }
            if corners.iter().all(Option::is_some) {
                let values = corners.map(|value| value.expect("checked"));
                let west = f64::from(tile.west) + column as f64 * step;
                let north = f64::from(tile.south + 1) - row as f64 * step;
                for threshold in ELEVATION_THRESHOLDS {
                    for points in contour_segments(values, f64::from(threshold)) {
                        contours.push(ElevationContour {
                            elevation_m: threshold,
                            points: points
                                .into_iter()
                                .map(|[x, y]| [west + x * step, north - y * step])
                                .collect(),
                        });
                    }
                }
            }
        }
    }
}

fn elevation_band(value: f64) -> Option<i16> {
    ELEVATION_THRESHOLDS
        .iter()
        .rev()
        .copied()
        .find(|threshold| value >= f64::from(*threshold))
}

fn contour_segments(values: [f64; 4], threshold: f64) -> Vec<Vec<[f64; 2]>> {
    let coordinates = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let edges = [(0, 1), (1, 2), (2, 3), (3, 0)];
    let mut intersections = Vec::new();
    for (start, end) in edges {
        let a = values[start];
        let b = values[end];
        if (a < threshold) == (b < threshold) || a == b {
            continue;
        }
        let t = ((threshold - a) / (b - a)).clamp(0.0, 1.0);
        intersections.push([
            coordinates[start][0] + t * (coordinates[end][0] - coordinates[start][0]),
            coordinates[start][1] + t * (coordinates[end][1] - coordinates[start][1]),
        ]);
    }
    match intersections.as_slice() {
        [a, b] => vec![vec![*a, *b]],
        [a, b, c, d] => vec![vec![*a, *b], vec![*c, *d]],
        _ => Vec::new(),
    }
}

fn load_forest(
    directory: &Path,
    bounds: [f64; 4],
) -> Result<ForestLayer, Box<dyn std::error::Error>> {
    let manifest_path = directory.join("forest-cover-manifest.json");
    let manifest_bytes = fs::read(&manifest_path)?;
    let prepared_format = validate_prepared_forest_manifest(&manifest_bytes, &manifest_path)?;
    let mut densities = source_files(directory, "TCD_", ".tif")?;
    densities.sort();
    let mut coverage = Vec::new();
    let mut regions = Vec::new();
    let mut identities = BTreeMap::new();
    identities.insert(
        "forest-cover-manifest.json".into(),
        format!("{:x}", Sha256::digest(&manifest_bytes)),
    );
    for density_path in densities {
        let name = file_name(&density_path)?;
        let Some(tile) = forest_tile(name) else {
            continue;
        };
        if !tile.intersects(bounds) {
            continue;
        }
        let leaf_name = name.replacen("TCD_", "DLT_", 1);
        let leaf_path = directory.join(&leaf_name);
        if !leaf_path.is_file() {
            return Err(format!("forest tile {name} has no matching {leaf_name}").into());
        }
        let density_bytes = fs::read(&density_path)?;
        let leaf_bytes = fs::read(&leaf_path)?;
        identities.insert(name.into(), format!("{:x}", Sha256::digest(&density_bytes)));
        identities.insert(leaf_name, format!("{:x}", Sha256::digest(&leaf_bytes)));
        let density = read_prepared_forest_raster(&density_path, tile.south, tile.west)?;
        let leaves = read_prepared_forest_raster(&leaf_path, tile.south, tile.west)?;
        if !density.has_same_grid(&leaves) {
            return Err(format!("forest TCD and DLT transforms disagree for {name}").into());
        }
        append_forest_regions(tile, density.pixels(), leaves.pixels(), &mut regions);
        coverage.push(tile.bounds());
    }
    coverage.sort_by(|a, b| bounds_order(*a, *b));
    regions.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.density.cmp(&b.density))
            .then_with(|| bounds_order(a.bounds, b.bounds))
    });
    Ok(ForestLayer {
        source: LayerSource {
            name: "Copernicus HRL Forests 2018".into(),
            version: prepared_format.into(),
            url: "https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a".into(),
            license: "Copernicus full, free, and open data policy".into(),
            file_count: identities.len(),
            files_sha256: identities,
            verification_status: forest_verification_status(prepared_format).into(),
        },
        coverage,
        regions,
    })
}

fn forest_verification_status(format: &str) -> &'static str {
    if format == adventuresim_world_import::PREPARED_FOREST_FORMAT_V2 {
        "exact-pinned-bundle-inventory;upstream-reacquisition-not-byte-reproducible"
    } else {
        "release-blocked-local-v1;upstream-reacquisition-not-byte-reproducible"
    }
}

fn append_forest_regions(
    tile: DegreeTile,
    density: &[u8],
    leaves: &[u8],
    output: &mut Vec<ForestRegion>,
) {
    let block = FOREST_PIXELS_PER_DEGREE / FOREST_CELLS_PER_DEGREE;
    let geographic_step = 1.0 / FOREST_CELLS_PER_DEGREE as f64;
    for row in 0..FOREST_CELLS_PER_DEGREE {
        for column in 0..FOREST_CELLS_PER_DEGREE {
            let mut density_sum = 0_u64;
            let mut density_count = 0_u64;
            let mut leaf_counts = [0_u64; 3];
            for y in row * block..(row + 1) * block {
                for x in column * block..(column + 1) * block {
                    let index = y * FOREST_PIXELS_PER_DEGREE + x;
                    let canopy = density[index];
                    if canopy <= 100 {
                        density_sum += u64::from(canopy);
                        density_count += 1;
                        if canopy > 0 && (1..=3).contains(&leaves[index]) {
                            leaf_counts[usize::from(leaves[index] - 1)] += 1;
                        }
                    }
                }
            }
            if density_count == 0 {
                continue;
            }
            let average = (density_sum / density_count) as u8;
            if average == 0 {
                continue;
            }
            let kind = if leaf_counts[2] >= leaf_counts[0] && leaf_counts[2] >= leaf_counts[1] {
                "mixed"
            } else if leaf_counts[1] > leaf_counts[0] {
                "conifer"
            } else {
                "broadleaf"
            };
            let west = f64::from(tile.west) + column as f64 * geographic_step;
            let north = f64::from(tile.south + 1) - row as f64 * geographic_step;
            output.push(ForestRegion {
                bounds: [west, north - geographic_step, west + geographic_step, north],
                density: average,
                kind: kind.into(),
            });
        }
    }
}

fn source_files(
    directory: &Path,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && name.ends_with(suffix) {
            paths.push(entry.path());
        }
    }
    Ok(paths)
}

fn elevation_tile(name: &str) -> Option<DegreeTile> {
    let parts = name.split('_').collect::<Vec<_>>();
    if parts.len() != 9
        || parts[..4] != ["Copernicus", "DSM", "COG", "10"]
        || parts[5] != "00"
        || parts[7] != "00"
        || parts[8] != "DEM.tif"
    {
        return None;
    }
    Some(DegreeTile {
        south: signed_degrees(parts[4], 'N', 'S')?,
        west: signed_degrees(parts[6], 'E', 'W')?,
    })
}

fn forest_tile(name: &str) -> Option<DegreeTile> {
    let stem = name.strip_prefix("TCD_")?.strip_suffix(".tif")?;
    let (latitude, longitude) = stem.split_once('_')?;
    Some(DegreeTile {
        south: signed_degrees(latitude, 'N', 'S')?,
        west: signed_degrees(longitude, 'E', 'W')?,
    })
}

fn signed_degrees(value: &str, positive: char, negative: char) -> Option<i16> {
    let mut chars = value.chars();
    let sign = match chars.next()? {
        direction if direction == positive => 1,
        direction if direction == negative => -1,
        _ => return None,
    };
    chars.as_str().parse::<i16>().ok().map(|value| sign * value)
}

fn file_name(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("source path {} has no UTF-8 filename", path.display()).into())
}

fn bounds_order(left: [f64; 4], right: [f64; 4]) -> std::cmp::Ordering {
    left.into_iter()
        .zip(right)
        .map(|(a, b)| a.total_cmp(&b))
        .find(|ordering| !ordering.is_eq())
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn point_order(left: &[[f64; 2]], right: &[[f64; 2]]) -> std::cmp::Ordering {
    left.first()
        .zip(right.first())
        .map(|(a, b)| a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1])))
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_names_are_strict_and_signed() {
        assert_eq!(
            elevation_tile("Copernicus_DSM_COG_10_N48_00_W001_00_DEM.tif"),
            Some(DegreeTile {
                south: 48,
                west: -1
            })
        );
        assert_eq!(
            forest_tile("TCD_N53_E009.tif"),
            Some(DegreeTile { south: 53, west: 9 })
        );
        assert!(elevation_tile("almost-a-dem.tif").is_none());
        assert!(forest_tile("TCD_53_9.tif").is_none());
    }

    #[test]
    fn forest_verification_metadata_distinguishes_v1_and_v2() {
        assert_eq!(
            forest_verification_status(adventuresim_world_import::PREPARED_FOREST_FORMAT_V2),
            "exact-pinned-bundle-inventory;upstream-reacquisition-not-byte-reproducible"
        );
        assert_eq!(
            forest_verification_status(adventuresim_world_import::PREPARED_FOREST_FORMAT_V1),
            "release-blocked-local-v1;upstream-reacquisition-not-byte-reproducible"
        );
    }

    #[test]
    fn elevation_cells_and_contours_are_derived_from_samples() {
        let side = ELEVATION_SAMPLES_PER_DEGREE + 1;
        let samples = (0..side * side)
            .map(|index| Some((index % side) as f64 * 300.0))
            .collect::<Vec<_>>();
        let mut cells = Vec::new();
        let mut contours = Vec::new();
        append_elevation_features(
            DegreeTile { south: 53, west: 9 },
            &samples,
            &mut cells,
            &mut contours,
        );
        assert!(!cells.is_empty());
        assert!(contours.iter().any(|line| line.elevation_m == 500));
        assert!(
            cells
                .iter()
                .all(|cell| cell.bounds[0] >= 9.0 && cell.bounds[2] <= 10.0)
        );
    }

    #[test]
    fn forest_regions_keep_partial_tile_coverage() {
        let mut density = vec![0; FOREST_PIXELS_PER_DEGREE * FOREST_PIXELS_PER_DEGREE];
        let mut leaves = vec![1; density.len()];
        for y in 0..50 {
            for x in 0..50 {
                density[y * FOREST_PIXELS_PER_DEGREE + x] = 80;
                leaves[y * FOREST_PIXELS_PER_DEGREE + x] = 2;
            }
        }
        let mut regions = Vec::new();
        append_forest_regions(
            DegreeTile { south: 53, west: 9 },
            &density,
            &leaves,
            &mut regions,
        );
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind, "conifer");
        assert_eq!(regions[0].density, 80);
        assert_eq!(regions[0].bounds, [9.0, 53.95, 9.05, 54.0]);
    }
}
