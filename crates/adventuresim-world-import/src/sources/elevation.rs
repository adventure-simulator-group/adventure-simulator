//! Copernicus DEM GLO-30 settlement elevation sampling.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};

use adventuresim_world_schema::{
    CompiledWorld, ElevationMeters, SettlementImport, SourceProvenance, WORLD_SCHEMA_VERSION,
    WorldMetadata,
};
use tiff::decoder::{Decoder, DecodingResult};

use crate::{Error, Result, draft::WorldDraft};

const SOURCE_NAME: &str = "Copernicus DEM GLO-30";
const SOURCE_URL: &str = "https://doi.org/10.5270/ESA-c5d3d65";
const SOURCE_LICENSE: &str = "Copernicus DEM licence";
const SEARCH_RADIUS_PIXELS: u32 = 8;
const GLO30_TILE_SIZE: u32 = 3_600;

pub(crate) fn enrich(mut draft: WorldDraft, directory: &Path) -> Result<CompiledWorld> {
    let mut by_tile: BTreeMap<TileKey, Vec<usize>> = BTreeMap::new();
    for (index, settlement) in draft.settlements.iter().enumerate() {
        let tile = TileKey::from_coordinate(settlement.latitude, settlement.longitude)?;
        by_tile.entry(tile).or_default().push(index);
    }

    let mut samples = vec![None; draft.settlements.len()];
    let mut fallback_samples = 0;
    for (tile, settlement_indexes) in &by_tile {
        let path = directory.join(tile.filename());
        if !path.is_file() {
            return Err(Error::MissingSource(path));
        }
        let raster = Raster::read(&path)?;
        for &index in settlement_indexes {
            let settlement = &draft.settlements[index];
            let (column, row) = tile.pixel(
                settlement.latitude,
                settlement.longitude,
                raster.width,
                raster.height,
            );
            let (elevation, used_fallback) = raster.elevation_near(column, row);
            fallback_samples += usize::from(used_fallback);
            samples[index] = Some(elevation);
        }
    }

    let settlements: Vec<_> = draft
        .settlements
        .into_iter()
        .zip(samples)
        .map(|(settlement, elevation)| {
            let elevation = elevation.expect("every settlement was grouped into a raster tile");
            SettlementImport {
                id: settlement.id,
                source_node_id: settlement.source_node_id,
                name: settlement.name,
                longitude: settlement.longitude,
                latitude: settlement.latitude,
                population_level: settlement.population_level,
                population_estimate: settlement.population_estimate,
                elevation,
                scene_key: settlement.scene_key,
                religion_id: settlement.religion_id,
            }
        })
        .collect();

    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.elevation_tiles_read = by_tile.len();
    draft.report.elevation_samples = settlements.len();
    draft.report.elevation_fallback_samples = fallback_samples;
    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            world_year: draft.year,
            sources: draft.sources,
            road_types: draft.road_types,
        },
        nodes: draft.nodes,
        edges: draft.edges,
        settlements,
        report: draft.report,
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TileKey {
    south: i16,
    west: i16,
}

impl TileKey {
    fn from_coordinate(latitude: f64, longitude: f64) -> Result<Self> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "settlement coordinate ({latitude}, {longitude}) is outside the geographic grid"
            )));
        }
        // GLO-30 has no tile whose southern or western edge is the maximum
        // coordinate. Put an exact pole/dateline coordinate in the preceding tile.
        let latitude = if latitude == 90.0 {
            f64::from_bits(latitude.to_bits() - 1)
        } else {
            latitude
        };
        let longitude = if longitude == 180.0 {
            f64::from_bits(longitude.to_bits() - 1)
        } else {
            longitude
        };
        Ok(Self {
            south: latitude.floor() as i16,
            west: longitude.floor() as i16,
        })
    }

    fn filename(self) -> String {
        let latitude_hemisphere = if self.south >= 0 { 'N' } else { 'S' };
        let longitude_hemisphere = if self.west >= 0 { 'E' } else { 'W' };
        format!(
            "Copernicus_DSM_COG_10_{latitude_hemisphere}{:02}_00_{longitude_hemisphere}{:03}_00_DEM.tif",
            self.south.unsigned_abs(),
            self.west.unsigned_abs()
        )
    }

    fn pixel(self, latitude: f64, longitude: f64, width: u32, height: u32) -> (u32, u32) {
        let x = ((longitude - f64::from(self.west)) * f64::from(width)).floor();
        let north = f64::from(self.south) + 1.0;
        let y = ((north - latitude) * f64::from(height)).floor();
        (
            x.clamp(0.0, f64::from(width - 1)) as u32,
            y.clamp(0.0, f64::from(height - 1)) as u32,
        )
    }
}

struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<f32>,
}

impl Raster {
    fn read(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let raster = Self::decode(BufReader::new(file), path)?;
        if raster.width != GLO30_TILE_SIZE || raster.height != GLO30_TILE_SIZE {
            return Err(Error::Validation(format!(
                "GLO-30 tile {} is {}x{} instead of {GLO30_TILE_SIZE}x{GLO30_TILE_SIZE}",
                path.display(),
                raster.width,
                raster.height
            )));
        }
        Ok(raster)
    }

    fn decode(reader: impl Read + Seek, path: &Path) -> Result<Self> {
        let mut decoder = Decoder::new(reader).map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        let (width, height) = decoder.dimensions().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        if width == 0 || height == 0 {
            return Err(Error::Validation(format!(
                "elevation raster {} has zero dimensions",
                path.display()
            )));
        }
        let decoded = decoder.read_image().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        let DecodingResult::F32(pixels) = decoded else {
            return Err(Error::Validation(format!(
                "elevation raster {} does not contain GLO-30 Float32 samples",
                path.display()
            )));
        };
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "elevation raster {} dimensions overflow the address space",
                    path.display()
                ))
            })?;
        if pixels.len() != expected {
            return Err(Error::Validation(format!(
                "elevation raster {} is not a single-channel {width}x{height} image",
                path.display()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    fn elevation_near(&self, column: u32, row: u32) -> (ElevationMeters, bool) {
        if let Some(value) = self.elevation(column, row) {
            return (value, false);
        }
        for radius in 1..=SEARCH_RADIUS_PIXELS {
            let min_x = column.saturating_sub(radius);
            let max_x = column.saturating_add(radius).min(self.width - 1);
            let min_y = row.saturating_sub(radius);
            let max_y = row.saturating_add(radius).min(self.height - 1);
            for x in min_x..=max_x {
                for y in [min_y, max_y] {
                    if let Some(value) = self.elevation(x, y) {
                        return (value, true);
                    }
                }
            }
            for y in min_y.saturating_add(1)..max_y {
                for x in [min_x, max_x] {
                    if let Some(value) = self.elevation(x, y) {
                        return (value, true);
                    }
                }
            }
        }
        // GLO-30 voids are principally open water. Zero metres is a stable,
        // plausible fallback and keeps the canonical schema free of unknowns.
        (ElevationMeters::new(0).unwrap(), true)
    }

    fn elevation(&self, column: u32, row: u32) -> Option<ElevationMeters> {
        let index = row as usize * self.width as usize + column as usize;
        let value = f64::from(*self.pixels.get(index)?);
        if !value.is_finite() {
            return None;
        }
        let rounded = value.round();
        if rounded < f64::from(i16::MIN) || rounded > f64::from(i16::MAX) {
            return None;
        }
        ElevationMeters::new(rounded as i16)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tiff::encoder::{TiffEncoder, colortype::Gray32Float};

    use super::{Raster, TileKey};

    #[test]
    fn coordinates_select_signed_degree_tiles() {
        let northwest = TileKey::from_coordinate(52.5, -1.25).unwrap();
        assert_eq!(northwest.south, 52);
        assert_eq!(northwest.west, -2);
        assert_eq!(
            northwest.filename(),
            "Copernicus_DSM_COG_10_N52_00_W002_00_DEM.tif"
        );

        let southeast = TileKey::from_coordinate(-3.25, 7.5).unwrap();
        assert_eq!(
            southeast.filename(),
            "Copernicus_DSM_COG_10_S04_00_E007_00_DEM.tif"
        );
        assert!(TileKey::from_coordinate(f64::NAN, 0.0).is_err());
        assert!(TileKey::from_coordinate(91.0, 0.0).is_err());
    }

    #[test]
    fn pixels_are_measured_from_the_northwest_corner() {
        let tile = TileKey { south: 48, west: 0 };
        assert_eq!(tile.pixel(48.999, 0.001, 1_000, 1_000), (1, 0));
        assert_eq!(tile.pixel(48.001, 0.999, 1_000, 1_000), (999, 999));
        assert_eq!(tile.pixel(48.0, 0.0, 1_000, 1_000), (0, 999));
    }

    #[test]
    fn decoder_parses_float_pixels_and_replaces_voids() {
        let mut bytes = Cursor::new(Vec::new());
        TiffEncoder::new(&mut bytes)
            .unwrap()
            .write_image::<Gray32Float>(
                3,
                3,
                &[10.2, 11.6, 12.0, 13.0, f32::NAN, 15.0, 16.0, 17.0, 18.0],
            )
            .unwrap();
        bytes.set_position(0);
        let raster = Raster::decode(bytes, std::path::Path::new("fixture.tif")).unwrap();
        assert_eq!(raster.elevation_near(0, 0).0.get(), 10);
        let (replacement, used_fallback) = raster.elevation_near(1, 1);
        assert!(used_fallback);
        assert_eq!(replacement.get(), 10);
    }
}
