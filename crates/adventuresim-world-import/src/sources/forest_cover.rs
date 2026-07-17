//! Copernicus Tree Cover Density and Dominant Leaf Type sampling.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read, Seek},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CanopyDensity, DominantLeafType, ForestCover, SourceProvenance, Woodland,
};
use serde::Deserialize;
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};

use crate::{
    Error, Result,
    draft::{ForestSettlementDraft, LandUseSettlementDraft, WorldDraft, push_source_note},
};

const SOURCE_NAME: &str = "Copernicus Land Monitoring Service Forest 2018";
const SOURCE_URL: &str =
    "https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover";
const SOURCE_LICENSE: &str = "Copernicus data licence";
const NODATA: u8 = 255;
const MANIFEST_FILENAME: &str = "forest-cover-manifest.json";
const PIXELS_PER_DEGREE: u32 = 1_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedManifest {
    format: PreparedFormat,
}

#[derive(Debug, Deserialize)]
enum PreparedFormat {
    #[serde(rename = "adventuresim-copernicus-forest-2018-v1")]
    CopernicusForest2018V1,
}

pub(crate) fn enrich(
    mut draft: WorldDraft<LandUseSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<ForestSettlementDraft>> {
    let mut by_tile: BTreeMap<DegreeTile, Vec<usize>> = BTreeMap::new();
    for (index, settlement) in draft.settlements.iter().enumerate() {
        let base = &settlement.elevated.settlement;
        by_tile
            .entry(DegreeTile::containing(base.latitude, base.longitude)?)
            .or_default()
            .push(index);
    }
    if !by_tile.is_empty() {
        read_manifest(directory)?;
    }
    let mut covers = vec![None; draft.settlements.len()];
    let mut fallbacks = 0;
    for (tile, indexes) in &by_tile {
        let density = ByteRaster::read(&require(directory, &tile.filename("TCD"))?, *tile)?;
        let leaves = ByteRaster::read(&require(directory, &tile.filename("DLT"))?, *tile)?;
        if density.grid != leaves.grid {
            return Err(Error::Validation(format!(
                "forest TCD and DLT transforms disagree for {}",
                tile.label()
            )));
        }
        for &index in indexes {
            let settlement = &draft.settlements[index];
            let base = &settlement.elevated.settlement;
            let (column, row) =
                density
                    .grid
                    .pixel(base.latitude, base.longitude, density.width, density.height)?;
            let density_value = density.value(column, row);
            let leaf_value = leaves.value(column, row);
            let (cover, fallback) = forest_cover(
                density_value,
                leaf_value,
                settlement.land_use.natural().basis_points(),
                settlement.elevated.elevation.get(),
            );
            fallbacks += usize::from(fallback);
            covers[index] = Some((cover, fallback));
        }
    }
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(covers)
        .map(|(mut land, cover)| {
            let (forest_cover, fallback) =
                cover.expect("every settlement was grouped into a forest tile");
            push_source_note(
                &mut land,
                if fallback {
                    "**[Copernicus HRL Forests](https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover):** At least one TCD/DLT source value was missing or reserved; forest density/type uses the documented deterministic HYDE/elevation fallback."
                } else {
                    "**[Copernicus HRL Forests](https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover):** Forest density and dominant leaf type are sampled from the prepared 2018 TCD/DLT rasters."
                },
            );
            ForestSettlementDraft {
                land,
                forest_cover,
            }
        })
        .collect::<Vec<_>>();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.forest_tiles_read = by_tile.len();
    draft.report.forest_samples = settlements.len();
    draft.report.forest_fallback_samples = fallbacks;
    Ok(WorldDraft {
        year: draft.year,
        spatial_grid: draft.spatial_grid,
        sources: draft.sources,
        road_types: draft.road_types,
        nodes: draft.nodes,
        edges: draft.edges,
        settlement_aliases: draft.settlement_aliases,
        settlement_descriptions: draft.settlement_descriptions,
        settlements,
        report: draft.report,
    })
}

fn read_manifest(directory: &Path) -> Result<()> {
    let path = require(directory, MANIFEST_FILENAME)?;
    let file = File::open(&path)?;
    let manifest: PreparedManifest =
        serde_json::from_reader(BufReader::new(file)).map_err(|source| Error::JsonSource {
            path: path.clone(),
            source,
        })?;
    match manifest.format {
        PreparedFormat::CopernicusForest2018V1 => Ok(()),
    }
}

fn forest_cover(
    density: Option<u8>,
    leaf: Option<u8>,
    natural_basis_points: u16,
    elevation_meters: i16,
) -> (ForestCover, bool) {
    let Some(density) = density.filter(|value| *value <= 100) else {
        if natural_basis_points < 500 {
            return (ForestCover::Open, true);
        }
        let inferred = ((u32::from(natural_basis_points) * 60) / 10_000).clamp(5, 60) as u8;
        let dominant = leaf
            .and_then(source_leaf)
            .unwrap_or_else(|| fallback_leaf(elevation_meters));
        return (
            ForestCover::Wooded(Woodland {
                density: CanopyDensity::new(inferred).unwrap(),
                dominant,
            }),
            true,
        );
    };
    if density == 0 {
        return (ForestCover::Open, false);
    }
    let dominant = match leaf.and_then(source_leaf) {
        Some(dominant) => dominant,
        None => {
            return (
                ForestCover::Wooded(Woodland {
                    density: CanopyDensity::new(density).unwrap(),
                    dominant: fallback_leaf(elevation_meters),
                }),
                true,
            );
        }
    };
    (
        ForestCover::Wooded(Woodland {
            density: CanopyDensity::new(density).unwrap(),
            dominant,
        }),
        false,
    )
}

fn source_leaf(value: u8) -> Option<DominantLeafType> {
    match value {
        1 => Some(DominantLeafType::Broadleaf),
        2 => Some(DominantLeafType::Coniferous),
        3 => Some(DominantLeafType::Mixed),
        _ => None,
    }
}

fn fallback_leaf(elevation_meters: i16) -> DominantLeafType {
    match elevation_meters {
        ..=399 => DominantLeafType::Broadleaf,
        400..=799 => DominantLeafType::Mixed,
        _ => DominantLeafType::Coniferous,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DegreeTile {
    south: i16,
    west: i16,
}

impl DegreeTile {
    fn containing(latitude: f64, longitude: f64) -> Result<Self> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || latitude <= -90.0
            || latitude >= 90.0
            || !(-180.0..180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "forest coordinate ({latitude}, {longitude}) is outside the tiled grid"
            )));
        }
        // A point on a horizontal degree boundary is sampled from the tile
        // immediately south. That tile owns the boundary as its north edge;
        // choosing the northern tile would produce row == height.
        let south = if latitude.fract() == 0.0 {
            latitude as i16 - 1
        } else {
            latitude.floor() as i16
        };
        Ok(Self {
            south,
            west: longitude.floor() as i16,
        })
    }
    fn label(self) -> String {
        format!(
            "{}{:02}_{}{:03}",
            if self.south >= 0 { 'N' } else { 'S' },
            self.south.unsigned_abs(),
            if self.west >= 0 { 'E' } else { 'W' },
            self.west.unsigned_abs()
        )
    }
    fn filename(self, layer: &str) -> String {
        format!("{layer}_{}.tif", self.label())
    }
}

fn require(directory: &Path, filename: &str) -> Result<PathBuf> {
    let path = directory.join(filename);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}

struct ByteRaster {
    width: u32,
    height: u32,
    grid: AreaGrid,
    pixels: Vec<u8>,
}

impl ByteRaster {
    fn read(path: &Path, tile: DegreeTile) -> Result<Self> {
        let file = File::open(path)?;
        Self::decode(BufReader::new(file), path, tile)
    }

    fn decode(reader: impl Read + Seek, path: &Path, tile: DegreeTile) -> Result<Self> {
        let mut decoder = Decoder::new(reader).map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        let (width, height) = decoder.dimensions().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        if width != PIXELS_PER_DEGREE || height != PIXELS_PER_DEGREE {
            return Err(Error::Validation(format!(
                "{} is {width}x{height}; prepared forest tiles must be {PIXELS_PER_DEGREE}x{PIXELS_PER_DEGREE}",
                path.display(),
            )));
        }
        let grid = AreaGrid::parse(&mut decoder, path, tile, width, height)?;
        let DecodingResult::U8(pixels) = decoder.read_image().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?
        else {
            return Err(Error::Validation(format!(
                "{} is not a UInt8 forest raster",
                path.display()
            )));
        };
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| Error::Validation("forest raster dimensions overflow".into()))?;
        if pixels.len() != expected {
            return Err(Error::Validation(format!(
                "{} is not single-channel",
                path.display()
            )));
        }
        Ok(Self {
            width,
            height,
            grid,
            pixels,
        })
    }
    fn value(&self, column: u32, row: u32) -> Option<u8> {
        let value = self.pixels[row as usize * self.width as usize + column as usize];
        (value != NODATA).then_some(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AreaGrid {
    west: f64,
    north: f64,
    x_scale: f64,
    y_scale: f64,
}

impl AreaGrid {
    fn parse(
        reader: &mut Decoder<impl Read + Seek>,
        path: &Path,
        tile: DegreeTile,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let scale = tag(reader.get_tag_f64_vec(Tag::ModelPixelScaleTag), path)?;
        let tie = tag(reader.get_tag_f64_vec(Tag::ModelTiepointTag), path)?;
        let keys = tag(reader.get_tag_u16_vec(Tag::GeoKeyDirectoryTag), path)?;
        if scale.len() != 3
            || tie.len() != 6
            || geo_key(&keys, 1024) != Some(2)
            || geo_key(&keys, 1025) != Some(1)
            || geo_key(&keys, 2048) != Some(4326)
        {
            return Err(Error::Validation(format!(
                "{} is not an EPSG:4326 RasterPixelIsArea GeoTIFF",
                path.display()
            )));
        }
        let values = [scale[0], scale[1], tie[0], tie[1], tie[3], tie[4]];
        if !values.iter().all(|value| value.is_finite()) || scale[0] <= 0.0 || scale[1] <= 0.0 {
            return Err(Error::Validation(format!(
                "{} has invalid forest georeferencing",
                path.display()
            )));
        }
        let west = tie[3] - tie[0] * scale[0];
        let north = tie[4] + tie[1] * scale[1];
        let epsilon = 1e-9;
        if (west - f64::from(tile.west)).abs() > epsilon
            || (north - (f64::from(tile.south) + 1.0)).abs() > epsilon
            || (scale[0] * f64::from(width) - 1.0).abs() > epsilon
            || (scale[1] * f64::from(height) - 1.0).abs() > epsilon
        {
            return Err(Error::Validation(format!(
                "{} transform does not match its one-degree tile",
                path.display()
            )));
        }
        Ok(Self {
            west,
            north,
            x_scale: scale[0],
            y_scale: scale[1],
        })
    }
    fn pixel(self, latitude: f64, longitude: f64, width: u32, height: u32) -> Result<(u32, u32)> {
        let column = ((longitude - self.west) / self.x_scale).floor();
        let row = ((self.north - latitude) / self.y_scale).floor();
        if column < 0.0 || row < 0.0 || column >= f64::from(width) || row >= f64::from(height) {
            return Err(Error::Validation(
                "coordinate lies outside prepared forest tile".into(),
            ));
        }
        Ok((column as u32, row as u32))
    }
}

fn tag<T>(value: tiff::TiffResult<T>, path: &Path) -> Result<T> {
    value.map_err(|source| Error::Tiff {
        path: path.into(),
        source,
    })
}
fn geo_key(keys: &[u16], requested: u16) -> Option<u16> {
    let [1, 1, _, count, entries @ ..] = keys else {
        return None;
    };
    if entries.len() != usize::from(*count) * 4 {
        return None;
    }
    entries.as_chunks::<4>().0.iter().find_map(|entry| {
        (entry[0] == requested && entry[1] == 0 && entry[2] == 1).then_some(entry[3])
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use adventuresim_world_schema::{DominantLeafType, ForestCover, Woodland};
    use tiff::{
        encoder::{TiffEncoder, colortype::Gray8},
        tags::Tag,
    };

    use super::{ByteRaster, DegreeTile, PreparedManifest, forest_cover};

    #[test]
    fn prepared_area_geotiff_is_parsed_and_sampled() {
        let mut bytes = Cursor::new(Vec::new());
        let mut encoder = TiffEncoder::new(&mut bytes).unwrap();
        let mut image = encoder.new_image::<Gray8>(1_000, 1_000).unwrap();
        image
            .encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[0.001_f64, 0.001, 0.0][..])
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::ModelTiepointTag,
                &[0.0_f64, 0.0, 0.0, 0.0, 49.0, 0.0][..],
            )
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::GeoKeyDirectoryTag,
                &[
                    1_u16, 1, 0, 3, 1024, 0, 1, 2, 1025, 0, 1, 1, 2048, 0, 1, 4326,
                ][..],
            )
            .unwrap();
        let mut pixels = vec![0; 1_000_000];
        pixels[250 * 1_000 + 250] = 42;
        image.write_data(&pixels).unwrap();
        bytes.set_position(0);
        let raster = ByteRaster::decode(
            bytes,
            std::path::Path::new("TCD_N48_E000.tif"),
            DegreeTile { south: 48, west: 0 },
        )
        .unwrap();
        let (column, row) = raster
            .grid
            .pixel(48.75, 0.25, raster.width, raster.height)
            .unwrap();
        assert_eq!(raster.value(column, row), Some(42));
    }

    #[test]
    fn manifest_identifies_the_exact_preparation_contract() {
        assert!(
            serde_json::from_str::<PreparedManifest>(
                r#"{"format":"adventuresim-copernicus-forest-2018-v1"}"#
            )
            .is_ok()
        );
        assert!(
            serde_json::from_str::<PreparedManifest>(
                r#"{"format":"adventuresim-copernicus-forest-2018-v2"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn source_values_parse_into_valid_forest_variants() {
        assert_eq!(
            forest_cover(Some(0), Some(1), 8_000, 100),
            (ForestCover::Open, false)
        );
        assert_eq!(
            forest_cover(Some(40), Some(2), 8_000, 100),
            (
                ForestCover::Wooded(Woodland {
                    density: adventuresim_world_schema::CanopyDensity::new(40).unwrap(),
                    dominant: DominantLeafType::Coniferous
                }),
                false
            )
        );
        assert_eq!(
            forest_cover(None, None, 100, 100),
            (ForestCover::Open, true)
        );
        assert!(matches!(
            forest_cover(None, None, 8_000, 900).0,
            ForestCover::Wooded(Woodland {
                dominant: DominantLeafType::Coniferous,
                ..
            })
        ));
        assert!(matches!(
            forest_cover(None, Some(2), 8_000, 100).0,
            ForestCover::Wooded(Woodland {
                dominant: DominantLeafType::Coniferous,
                ..
            })
        ));
    }

    #[test]
    fn tile_names_preserve_signed_degree_identity() {
        assert_eq!(
            DegreeTile {
                south: 52,
                west: -2
            }
            .filename("TCD"),
            "TCD_N52_W002.tif"
        );
        assert!(DegreeTile::containing(f64::NAN, 0.0).is_err());
        assert!(DegreeTile::containing(-90.0, 0.0).is_err());
        assert!(DegreeTile::containing(90.0, 0.0).is_err());
    }

    #[test]
    fn horizontal_boundaries_select_the_tile_to_the_south() {
        assert_eq!(
            DegreeTile::containing(48.0, 2.0).unwrap(),
            DegreeTile { south: 47, west: 2 }
        );
        assert_eq!(
            DegreeTile::containing(48.000_001, 2.0).unwrap(),
            DegreeTile { south: 48, west: 2 }
        );
    }
}
