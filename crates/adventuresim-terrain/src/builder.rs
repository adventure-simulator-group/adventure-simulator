//! Offline GLO-30/CLMS pack compiler. This module is not linked into servers.

use crate::{CHUNK_SIDE, Entry, Manifest, SCHEMA, Surface, TerrainPurpose, hex_sha};
use flate2::{Compression, write::DeflateEncoder};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
};
use tiff::decoder::{Decoder, DecodingResult};

#[derive(Default)]
pub struct Features {
    pub roads: Vec<Vec<[f64; 2]>>,
    pub water: Vec<Vec<Vec<[f64; 2]>>>,
    pub wetlands: Vec<Vec<Vec<[f64; 2]>>>,
    pub wetland_source_sha256: String,
    pub cultivated: Vec<Vec<Vec<[f64; 2]>>>,
    pub cultivation_source_sha256: String,
    pub cultivation_rules_version: u16,
}

pub fn build(
    elevation_dir: &Path,
    forest_dir: &Path,
    bounds: [f64; 4],
    manifest_path: &Path,
    pack_path: &Path,
    features: &Features,
    purpose: TerrainPurpose,
) -> crate::Result<Manifest> {
    let [west, south, east, north] = bounds;
    if !bounds.into_iter().all(f64::is_finite) || west >= east || south >= north {
        return Err(crate::Error::Validation(
            "invalid terrain build bounds".into(),
        ));
    }
    let source_bounds = [
        west.floor() as i16,
        south.floor() as i16,
        east.ceil() as i16,
        north.ceil() as i16,
    ];
    let mut entries = Vec::new();
    let mut pack = Vec::new();
    let mut cultivated_native_cells = 0_u64;
    for south in source_bounds[1]..source_bounds[3] {
        for west in source_bounds[0]..source_bounds[2] {
            let path = elevation_path(elevation_dir, south, west)?;
            let (width, height, elevations, synthetic_water) = if path.is_file() {
                let mut decoder = Decoder::new(BufReader::new(File::open(&path)?))
                    .map_err(|error| crate::Error::Validation(error.to_string()))?;
                let (width, height) = decoder
                    .dimensions()
                    .map_err(|error| crate::Error::Validation(error.to_string()))?;
                if ![1_800, 2_400, 3_600].contains(&width) || height != 3_600 {
                    return Err(crate::Error::Validation(format!(
                        "{} is not a native GLO-30 tile",
                        path.display()
                    )));
                }
                let DecodingResult::F32(elevations) = decoder
                    .read_image()
                    .map_err(|error| crate::Error::Validation(error.to_string()))?
                else {
                    return Err(crate::Error::Validation(format!(
                        "{} is not Float32",
                        path.display()
                    )));
                };
                if elevations.len() != width as usize * height as usize {
                    return Err(crate::Error::Validation(format!(
                        "{} is truncated",
                        path.display()
                    )));
                }
                (width, height, Some(elevations), false)
            } else if is_known_offshore_gap(south, west) {
                // The initialized request intentionally omits five all-water
                // North Sea cells. Preserve the native latitude-band geometry
                // and mark them impassable without inventing terrain heights.
                (2_400, 3_600, None, true)
            } else {
                return Err(crate::Error::Validation(format!(
                    "missing {}",
                    path.display()
                )));
            };
            let forest = read_forest(forest_dir, south, west)?;
            let (roads, water, wetlands, cultivated) = masks(features, south, west, width, height);
            let chunks_x = width.div_ceil(u32::from(CHUNK_SIDE));
            let chunks_y = height.div_ceil(u32::from(CHUNK_SIDE));
            for chunk_y in 0..chunks_y {
                for chunk_x in 0..chunks_x {
                    let chunk_width =
                        (width - chunk_x * u32::from(CHUNK_SIDE)).min(u32::from(CHUNK_SIDE));
                    let chunk_height =
                        (height - chunk_y * u32::from(CHUNK_SIDE)).min(u32::from(CHUNK_SIDE));
                    if !chunk_intersects_bounds(
                        south,
                        west,
                        width,
                        height,
                        chunk_x,
                        chunk_y,
                        chunk_width,
                        chunk_height,
                        bounds,
                    ) {
                        continue;
                    }
                    let mut decoded =
                        Vec::with_capacity(chunk_width as usize * chunk_height as usize * 5);
                    for local_y in 0..chunk_height {
                        let y = chunk_y * u32::from(CHUNK_SIDE) + local_y;
                        for local_x in 0..chunk_width {
                            let x = chunk_x * u32::from(CHUNK_SIDE) + local_x;
                            let elevation = elevations.as_ref().map_or(0.0, |pixels| {
                                pixels[y as usize * width as usize + x as usize]
                            });
                            let metres = if elevation.is_finite() {
                                elevation.round().clamp(-500.0, 9_000.0) as i16
                            } else {
                                0
                            };
                            let forest_x = x as usize * 1_000 / width as usize;
                            let forest_y = y as usize * 1_000 / height as usize;
                            let canopy = forest
                                .as_ref()
                                .map_or(0, |pixels| pixels[forest_y * 1_000 + forest_x]);
                            let canopy = if canopy <= 100 { canopy } else { 0 };
                            let pixel_index = y as usize * width as usize + x as usize;
                            let on_road = roads.contains(&(x as u16, y as u16));
                            let on_water = water[pixel_index] != 0;
                            let on_wetland = wetlands[pixel_index] != 0;
                            let is_cultivated = cultivated[pixel_index] != 0;
                            cultivated_native_cells += u64::from(is_cultivated);
                            let surface = choose_surface(
                                on_road,
                                on_water,
                                on_wetland,
                                synthetic_water,
                                canopy,
                            );
                            decoded.extend_from_slice(&metres.to_le_bytes());
                            decoded.push(surface as u8);
                            let crossing = on_road && (on_water || synthetic_water);
                            let hilly = elevations.as_ref().is_some_and(|pixels| {
                                native_cell_is_hilly(pixels, x, y, width, height, south)
                            });
                            decoded.push(
                                u8::from(crossing)
                                    | (u8::from(hilly) << 1)
                                    | (u8::from(is_cultivated) << 2)
                                    | (u8::from(on_wetland) << 3),
                            );
                            decoded.push(canopy);
                        }
                    }
                    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
                    encoder.write_all(&decoded)?;
                    let compressed = encoder.finish()?;
                    let offset = pack.len() as u64;
                    pack.extend_from_slice(&compressed);
                    entries.push(Entry {
                        south,
                        west,
                        tile_width: width as u16,
                        tile_height: height as u16,
                        chunk_x: chunk_x as u16,
                        chunk_y: chunk_y as u16,
                        width: chunk_width as u16,
                        height: chunk_height as u16,
                        offset,
                        length: compressed.len() as u32,
                        decoded_sha256: hex_sha(&decoded),
                    });
                }
            }
        }
    }
    let content_sha256 = hex_sha(&pack);
    let road_geometry_sha256 = feature_digest(&features.roads)?;
    let wetland_cells = features.wetlands.len() as u64;
    let mut manifest = Manifest {
        schema: SCHEMA,
        purpose,
        bounds,
        source_resolution_m: 30,
        content_sha256,
        road_geometry_sha256,
        wetland_source_sha256: features.wetland_source_sha256.clone(),
        wetland_cells,
        cultivation_grid_crs: "EPSG:3035".into(),
        cultivation_grid_resolution_m: 1_000,
        cultivation_rules_version: features.cultivation_rules_version,
        cultivation_source_sha256: features.cultivation_source_sha256.clone(),
        cultivated_square_count: features.cultivated.len() as u64,
        cultivated_native_cells,
        entries,
        package_sha256: "0".repeat(64),
    };
    manifest.package_sha256 = manifest_digest(&manifest)?;
    let mut json = serde_json::to_vec(&manifest)?;
    json.push(b'\n');
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = pack_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(pack_path, pack)?;
    fs::write(manifest_path, json)?;
    Ok(manifest)
}

fn choose_surface(
    on_road: bool,
    on_water: bool,
    on_wetland: bool,
    synthetic_water: bool,
    canopy: u8,
) -> Surface {
    if on_road {
        Surface::Road
    } else if on_water || synthetic_water {
        Surface::Water
    } else if on_wetland {
        Surface::Wetland
    } else {
        match canopy {
            10..=44 => Surface::SparseWoods,
            45..=100 => Surface::DeepWoods,
            _ => Surface::Open,
        }
    }
}

fn feature_digest<T: serde::Serialize>(value: &T) -> crate::Result<String> {
    Ok(hex_sha(&serde_json::to_vec(value)?))
}

#[allow(clippy::too_many_arguments)]
fn chunk_intersects_bounds(
    south: i16,
    west: i16,
    tile_width: u32,
    tile_height: u32,
    chunk_x: u32,
    chunk_y: u32,
    chunk_width: u32,
    chunk_height: u32,
    bounds: [f64; 4],
) -> bool {
    let x = chunk_x * u32::from(CHUNK_SIDE);
    let y = chunk_y * u32::from(CHUNK_SIDE);
    let chunk_bounds = [
        f64::from(west) + f64::from(x) / f64::from(tile_width),
        f64::from(south + 1) - f64::from(y + chunk_height) / f64::from(tile_height),
        f64::from(west) + f64::from(x + chunk_width) / f64::from(tile_width),
        f64::from(south + 1) - f64::from(y) / f64::from(tile_height),
    ];
    chunk_bounds[2] > bounds[0]
        && chunk_bounds[0] < bounds[2]
        && chunk_bounds[3] > bounds[1]
        && chunk_bounds[1] < bounds[3]
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    #[test]
    fn only_chunks_intersecting_exact_bounds_are_emitted() {
        let bounds = [8.965, 50.877, 11.110, 52.211];
        assert!(!chunk_intersects_bounds(
            50, 8, 3_600, 3_600, 0, 0, 256, 256, bounds
        ));
        assert!(chunk_intersects_bounds(
            50, 8, 3_600, 3_600, 13, 0, 256, 256, bounds
        ));
        assert!(!chunk_intersects_bounds(
            52, 11, 3_600, 3_600, 0, 4, 256, 256, bounds
        ));
    }

    #[test]
    fn wetland_is_slow_and_has_explicit_road_water_precedence() {
        assert_eq!(
            choose_surface(false, false, true, false, 0),
            Surface::Wetland
        );
        assert_eq!(choose_surface(false, false, false, false, 0), Surface::Open);
        assert_eq!(choose_surface(true, false, true, false, 0), Surface::Road);
        assert_eq!(choose_surface(false, true, true, false, 0), Surface::Water);
        assert!(Surface::Wetland.speed_metres_per_hour() < Surface::Open.speed_metres_per_hour());
    }
}

fn native_cell_is_hilly(
    elevations: &[f32],
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    south: i16,
) -> bool {
    const TAN_FIFTEEN_DEGREES: f64 = 0.267_949_192_431_122_7;
    const METRES_PER_DEGREE: f64 = 111_320.0;
    let center = elevations[y as usize * width as usize + x as usize];
    if !center.is_finite() || !(-500.0..=9_000.0).contains(&center) {
        return false;
    }
    let latitude = f64::from(south + 1) - (f64::from(y) + 0.5) / f64::from(height);
    let north_step = METRES_PER_DEGREE / f64::from(height);
    let east_step = METRES_PER_DEGREE * latitude.to_radians().cos() / f64::from(width);
    for offset_y in -1_i32..=1 {
        for offset_x in -1_i32..=1 {
            if offset_x == 0 && offset_y == 0 {
                continue;
            }
            let neighbour_x = x as i32 + offset_x;
            let neighbour_y = y as i32 + offset_y;
            if neighbour_x < 0
                || neighbour_y < 0
                || neighbour_x >= width as i32
                || neighbour_y >= height as i32
            {
                continue;
            }
            let neighbour =
                elevations[neighbour_y as usize * width as usize + neighbour_x as usize];
            if !neighbour.is_finite() || !(-500.0..=9_000.0).contains(&neighbour) {
                continue;
            }
            let run = (f64::from(offset_x).powi(2) * east_step.powi(2)
                + f64::from(offset_y).powi(2) * north_step.powi(2))
            .sqrt();
            if f64::from((center - neighbour).abs()) > run * TAN_FIFTEEN_DEGREES {
                return true;
            }
        }
    }
    false
}

type RoadMask = HashSet<(u16, u16)>;
type Mask = Vec<u8>;

fn masks(
    features: &Features,
    south: i16,
    west: i16,
    width: u32,
    height: u32,
) -> (RoadMask, Mask, Mask, Mask) {
    let mut roads = HashSet::new();
    let to_pixel = |point: [f64; 2]| -> (i32, i32) {
        (
            ((point[0] - f64::from(west)) * f64::from(width)).round() as i32,
            ((f64::from(south + 1) - point[1]) * f64::from(height)).round() as i32,
        )
    };
    for line in &features.roads {
        for pair in line.windows(2) {
            let Some((clipped_a, clipped_b)) = clip_segment(
                pair[0],
                pair[1],
                f64::from(west),
                f64::from(south),
                f64::from(west + 1),
                f64::from(south + 1),
            ) else {
                continue;
            };
            let a = to_pixel(clipped_a);
            let b = to_pixel(clipped_b);
            let steps = (a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))).max(1);
            for step in 0..=steps {
                let t = f64::from(step) / f64::from(steps);
                let x = (f64::from(a.0) + (f64::from(b.0 - a.0)) * t).round() as i32;
                let y = (f64::from(a.1) + (f64::from(b.1 - a.1)) * t).round() as i32;
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        let px = x + ox;
                        let py = y + oy;
                        if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                            roads.insert((px as u16, py as u16));
                        }
                    }
                }
            }
        }
    }
    let water = polygon_mask(&features.water, south, west, width, height);
    let wetlands = polygon_mask(&features.wetlands, south, west, width, height);
    let cultivated = polygon_mask(&features.cultivated, south, west, width, height);
    (roads, water, wetlands, cultivated)
}

fn polygon_mask(
    polygons: &[Vec<Vec<[f64; 2]>>],
    south: i16,
    west: i16,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut mask = vec![0_u8; width as usize * height as usize];
    for polygon in polygons {
        let Some([min_x, min_y, max_x, max_y]) = polygon_bounds(polygon) else {
            continue;
        };
        if max_x < f64::from(west)
            || min_x > f64::from(west + 1)
            || max_y < f64::from(south)
            || min_y > f64::from(south + 1)
        {
            continue;
        }
        let first_y = ((f64::from(south + 1) - max_y) * f64::from(height))
            .floor()
            .clamp(0.0, f64::from(height)) as u32;
        let last_y = ((f64::from(south + 1) - min_y) * f64::from(height))
            .ceil()
            .clamp(0.0, f64::from(height)) as u32;
        for y in first_y..last_y {
            let latitude = f64::from(south + 1) - (f64::from(y) + 0.5) / f64::from(height);
            let mut intersections = Vec::new();
            for ring in polygon {
                for pair in ring.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    if (a[1] > latitude) != (b[1] > latitude) {
                        intersections
                            .push(a[0] + (latitude - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                    }
                }
            }
            intersections.sort_by(f64::total_cmp);
            for pair in intersections.as_chunks::<2>().0 {
                let start = ((pair[0] - f64::from(west)) * f64::from(width))
                    .floor()
                    .max(0.0) as usize;
                let end = ((pair[1] - f64::from(west)) * f64::from(width))
                    .ceil()
                    .min(f64::from(width)) as usize;
                for x in start.min(width as usize)..end.min(width as usize) {
                    mask[y as usize * width as usize + x] ^= 1;
                }
            }
        }
    }
    mask
}

fn polygon_bounds(polygon: &[Vec<[f64; 2]>]) -> Option<[f64; 4]> {
    let mut points = polygon.iter().flatten();
    let first = *points.next()?;
    let mut bounds = [first[0], first[1], first[0], first[1]];
    for point in points {
        bounds[0] = bounds[0].min(point[0]);
        bounds[1] = bounds[1].min(point[1]);
        bounds[2] = bounds[2].max(point[0]);
        bounds[3] = bounds[3].max(point[1]);
    }
    Some(bounds)
}

fn clip_segment(
    a: [f64; 2],
    b: [f64; 2],
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Option<([f64; 2], [f64; 2])> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let mut enter = 0.0_f64;
    let mut leave = 1.0_f64;
    for (p, q) in [
        (-dx, a[0] - min_x),
        (dx, max_x - a[0]),
        (-dy, a[1] - min_y),
        (dy, max_y - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            enter = enter.max(ratio);
        } else {
            leave = leave.min(ratio);
        }
        if enter > leave {
            return None;
        }
    }
    Some((
        [a[0] + enter * dx, a[1] + enter * dy],
        [a[0] + leave * dx, a[1] + leave * dy],
    ))
}

fn manifest_digest(manifest: &Manifest) -> crate::Result<String> {
    let mut unsigned = manifest.clone();
    unsigned.package_sha256 = "0".repeat(64);
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&unsigned)?)
    ))
}

fn elevation_path(directory: &Path, south: i16, west: i16) -> crate::Result<PathBuf> {
    let latitude = if south >= 0 {
        format!("N{south:02}")
    } else {
        format!("S{:02}", -south)
    };
    let longitude = if west >= 0 {
        format!("E{west:03}")
    } else {
        format!("W{:03}", -west)
    };
    let path = directory.join(format!(
        "Copernicus_DSM_COG_10_{latitude}_00_{longitude}_00_DEM.tif"
    ));
    Ok(path)
}

fn is_known_offshore_gap(south: i16, west: i16) -> bool {
    matches!(
        (south, west),
        (54, 5) | (54, 6) | (55, 5) | (55, 6) | (55, 7)
    )
}

fn read_forest(directory: &Path, south: i16, west: i16) -> crate::Result<Option<Vec<u8>>> {
    let latitude = if south >= 0 {
        format!("N{south:02}")
    } else {
        format!("S{:02}", -south)
    };
    let longitude = if west >= 0 {
        format!("E{west:03}")
    } else {
        format!("W{:03}", -west)
    };
    let path = directory.join(format!("TCD_{latitude}_{longitude}.tif"));
    if !path.is_file() {
        return Ok(None);
    }
    let mut decoder = Decoder::new(BufReader::new(File::open(&path)?))
        .map_err(|error| crate::Error::Validation(error.to_string()))?;
    if decoder
        .dimensions()
        .map_err(|error| crate::Error::Validation(error.to_string()))?
        != (1_000, 1_000)
    {
        return Err(crate::Error::Validation(format!(
            "{} has invalid forest grid",
            path.display()
        )));
    }
    let DecodingResult::U8(pixels) = decoder
        .read_image()
        .map_err(|error| crate::Error::Validation(error.to_string()))?
    else {
        return Err(crate::Error::Validation(format!(
            "{} is not UInt8",
            path.display()
        )));
    };
    Ok(Some(pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_roads_that_cross_a_tile_with_both_vertices_outside() {
        let clipped = clip_segment([-1.0, 0.5], [2.0, 0.5], 0.0, 0.0, 1.0, 1.0).unwrap();
        assert_eq!(clipped, ([0.0, 0.5], [1.0, 0.5]));
        assert!(clip_segment([-1.0, -1.0], [-0.5, -0.5], 0.0, 0.0, 1.0, 1.0).is_none());
    }

    #[test]
    fn polygon_bounds_include_every_ring() {
        let polygon = vec![vec![[1.0, 2.0], [4.0, 3.0]], vec![[-1.0, 8.0], [2.0, -2.0]]];
        assert_eq!(polygon_bounds(&polygon), Some([-1.0, -2.0, 4.0, 8.0]));
        assert_eq!(polygon_bounds(&[]), None);
    }

    #[test]
    fn cultivated_square_polygon_sets_only_covered_native_cells() {
        let polygon = vec![vec![vec![
            [0.2, 0.4],
            [0.6, 0.4],
            [0.6, 0.8],
            [0.2, 0.8],
            [0.2, 0.4],
        ]]];
        let mask = polygon_mask(&polygon, 0, 0, 5, 5);
        assert_eq!(mask.iter().filter(|&&value| value != 0).count(), 4);
        assert_eq!(mask[0], 0);
        assert_eq!(mask[1 + 5], 1);
        assert_eq!(mask[2 + 2 * 5], 1);
        assert_eq!(mask[4 + 4 * 5], 0);
    }

    #[test]
    fn only_reviewed_north_sea_source_gaps_are_synthetic() {
        assert!(is_known_offshore_gap(54, 5));
        assert!(is_known_offshore_gap(55, 7));
        assert!(!is_known_offshore_gap(53, 5));
        assert!(!is_known_offshore_gap(55, 8));
    }

    #[test]
    fn native_hill_threshold_is_fifteen_degrees() {
        let width = 2_400;
        let height = 3;
        let center = width + 1;
        let mut gentle = vec![0.0_f32; width * height];
        gentle[center + 1] = 7.0;
        assert!(!native_cell_is_hilly(
            &gentle,
            1,
            1,
            width as u32,
            height as u32,
            53,
        ));

        gentle[center + 1] = 9.0;
        assert!(native_cell_is_hilly(
            &gentle,
            1,
            1,
            width as u32,
            height as u32,
            53,
        ));
    }
}
