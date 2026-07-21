//! Offline GLO-30/CLMS pack compiler. This module is not linked into servers.

use crate::{CHUNK_SIDE, Entry, Manifest, SCHEMA, Surface, hex_sha};
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
}

pub fn build(
    elevation_dir: &Path,
    forest_dir: &Path,
    bounds: [i16; 4],
    manifest_path: &Path,
    pack_path: &Path,
    features: &Features,
) -> crate::Result<Manifest> {
    let mut entries = Vec::new();
    let mut pack = Vec::new();
    for south in bounds[1]..bounds[3] {
        for west in bounds[0]..bounds[2] {
            let path = elevation_path(elevation_dir, south, west)?;
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
            let forest = read_forest(forest_dir, south, west)?;
            let (roads, water) = masks(features, south, west, width, height);
            let chunks_x = width.div_ceil(u32::from(CHUNK_SIDE));
            let chunks_y = height.div_ceil(u32::from(CHUNK_SIDE));
            for chunk_y in 0..chunks_y {
                for chunk_x in 0..chunks_x {
                    let chunk_width =
                        (width - chunk_x * u32::from(CHUNK_SIDE)).min(u32::from(CHUNK_SIDE));
                    let chunk_height =
                        (height - chunk_y * u32::from(CHUNK_SIDE)).min(u32::from(CHUNK_SIDE));
                    let mut decoded =
                        Vec::with_capacity(chunk_width as usize * chunk_height as usize * 4);
                    for local_y in 0..chunk_height {
                        let y = chunk_y * u32::from(CHUNK_SIDE) + local_y;
                        for local_x in 0..chunk_width {
                            let x = chunk_x * u32::from(CHUNK_SIDE) + local_x;
                            let elevation = elevations[y as usize * width as usize + x as usize];
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
                            let pixel_index = y as usize * width as usize + x as usize;
                            let on_road = roads.contains(&(x as u16, y as u16));
                            let on_water = water[pixel_index] != 0;
                            let surface = if on_road {
                                Surface::Road
                            } else if on_water {
                                Surface::Water
                            } else {
                                match canopy {
                                    10..=44 => Surface::SparseWoods,
                                    45..=100 => Surface::DeepWoods,
                                    _ => Surface::Open,
                                }
                            };
                            decoded.extend_from_slice(&metres.to_le_bytes());
                            decoded.push(surface as u8);
                            decoded.push(u8::from(on_road && on_water));
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
    let mut manifest = Manifest {
        schema: SCHEMA,
        bounds: bounds.map(f64::from),
        source_resolution_m: 30,
        content_sha256,
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

fn masks(
    features: &Features,
    south: i16,
    west: i16,
    width: u32,
    height: u32,
) -> (HashSet<(u16, u16)>, Vec<u8>) {
    let mut roads = HashSet::new();
    let to_pixel = |point: [f64; 2]| -> Option<(i32, i32)> {
        if point[0] < f64::from(west) - 0.01
            || point[0] > f64::from(west + 1) + 0.01
            || point[1] < f64::from(south) - 0.01
            || point[1] > f64::from(south + 1) + 0.01
        {
            return None;
        }
        Some((
            ((point[0] - f64::from(west)) * f64::from(width)).round() as i32,
            ((f64::from(south + 1) - point[1]) * f64::from(height)).round() as i32,
        ))
    };
    for line in &features.roads {
        for pair in line.windows(2) {
            let Some(a) = to_pixel(pair[0]) else { continue };
            let Some(b) = to_pixel(pair[1]) else { continue };
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
    let mut water = vec![0_u8; width as usize * height as usize];
    for polygon in &features.water {
        for y in 0..height {
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
            for pair in intersections.chunks_exact(2) {
                let start = ((pair[0] - f64::from(west)) * f64::from(width))
                    .floor()
                    .max(0.0) as usize;
                let end = ((pair[1] - f64::from(west)) * f64::from(width))
                    .ceil()
                    .min(f64::from(width)) as usize;
                for x in start.min(width as usize)..end.min(width as usize) {
                    water[y as usize * width as usize + x] ^= 1;
                }
            }
        }
    }
    (roads, water)
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
    if !path.is_file() {
        return Err(crate::Error::Validation(format!(
            "missing {}",
            path.display()
        )));
    }
    Ok(path)
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
