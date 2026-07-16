//! Copernicus DEM GLO-30 settlement elevation sampling.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};

use adventuresim_world_schema::{ElevationMeters, SourceProvenance};
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};

use crate::{
    Error, Result,
    draft::{ElevatedSettlementDraft, SettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "Copernicus DEM GLO-30";
const SOURCE_URL: &str = "https://doi.org/10.5270/ESA-c5d3d65";
const SOURCE_LICENSE: &str = "Copernicus DEM licence";
const SEARCH_RADIUS_PIXELS: u32 = 8;
const GLO30_TILE_HEIGHT: u32 = 3_600;
const GLO30_TILE_WIDTHS: [u32; 3] = [1_800, 2_400, 3_600];
const GEOGRAPHIC_MODEL_TYPE: u16 = 2;
const RASTER_PIXEL_IS_POINT: u16 = 2;
const WGS84_EPSG: u16 = 4_326;

pub(crate) fn enrich(
    mut draft: WorldDraft<SettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<ElevatedSettlementDraft>> {
    let mut nominal_tiles: BTreeMap<TileKey, Vec<usize>> = BTreeMap::new();
    for (index, settlement) in draft.settlements.iter().enumerate() {
        let tile = TileKey::containing(settlement.latitude, settlement.longitude)?;
        nominal_tiles.entry(tile).or_default().push(index);
    }
    let mut by_tile: BTreeMap<TileKey, Vec<usize>> = BTreeMap::new();
    for (tile, settlement_indexes) in nominal_tiles {
        let path = directory.join(tile.filename());
        if !path.is_file() {
            return Err(Error::MissingSource(path));
        }
        let metadata = RasterMetadata::read(&path, tile)?;
        for index in settlement_indexes {
            let settlement = &draft.settlements[index];
            let sample_tile =
                metadata.nearest_sample_tile(tile, settlement.latitude, settlement.longitude)?;
            by_tile.entry(sample_tile).or_default().push(index);
        }
    }

    let mut samples = vec![None; draft.settlements.len()];
    let mut fallback_samples = 0;
    for (tile, settlement_indexes) in &by_tile {
        let path = directory.join(tile.filename());
        if !path.is_file() {
            return Err(Error::MissingSource(path));
        }
        let raster = Raster::read(&path, *tile)?;
        for &index in settlement_indexes {
            let settlement = &draft.settlements[index];
            let (column, row) = raster.pixel(settlement.latitude, settlement.longitude)?;
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
            ElevatedSettlementDraft {
                settlement,
                elevation,
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
    Ok(WorldDraft {
        year: draft.year,
        sources: draft.sources,
        road_types: draft.road_types,
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
    fn containing(latitude: f64, longitude: f64) -> Result<Self> {
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
}

struct Raster {
    width: u32,
    height: u32,
    georeference: GeoReference,
    pixels: Vec<f32>,
}

impl Raster {
    fn read(path: &Path, expected_tile: TileKey) -> Result<Self> {
        let file = File::open(path)?;
        let raster = Self::decode(BufReader::new(file), path)?;
        raster
            .georeference
            .matches_tile(expected_tile, raster.width, raster.height)?;
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
        let georeference = GeoReference::parse(&mut decoder, path)?;
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
            georeference,
            pixels,
        })
    }

    fn pixel(&self, latitude: f64, longitude: f64) -> Result<(u32, u32)> {
        let (mut column, mut row) = self.georeference.nearest_pixel(latitude, longitude);
        if longitude == 180.0 && column == i64::from(self.width) {
            column -= 1;
        }
        if latitude == -90.0 && row == i64::from(self.height) {
            row -= 1;
        }
        if column < 0 || row < 0 || column >= i64::from(self.width) || row >= i64::from(self.height)
        {
            return Err(Error::Validation(format!(
                "coordinate ({latitude}, {longitude}) is outside its selected GLO-30 tile"
            )));
        }
        Ok((column as u32, row as u32))
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
            let mut candidates = Vec::new();
            for x in min_x..=max_x {
                candidates.extend([(x, min_y), (x, max_y)]);
            }
            for y in min_y.saturating_add(1)..max_y {
                candidates.extend([(min_x, y), (max_x, y)]);
            }
            candidates.sort_unstable_by_key(|&(x, y)| {
                let dx = i64::from(x) - i64::from(column);
                let dy = i64::from(y) - i64::from(row);
                (dx * dx + dy * dy, y, x)
            });
            for (x, y) in candidates {
                if let Some(value) = self.elevation(x, y) {
                    return (value, true);
                }
            }
        }
        // The nearest search deliberately stays within this source tile; the
        // current real dataset needs no fallback, including at tile edges.
        // GLO-30 voids are principally open water. Zero metres is a stable,
        // plausible final fallback and keeps the canonical schema free of unknowns.
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

#[derive(Clone, Copy, Debug)]
struct GeoReference {
    longitude_scale: f64,
    latitude_scale: f64,
    tie_column: f64,
    tie_row: f64,
    tie_longitude: f64,
    tie_latitude: f64,
}

impl GeoReference {
    fn parse(reader: &mut Decoder<impl Read + Seek>, path: &Path) -> Result<Self> {
        let scales = tiff_tag(reader.get_tag_f64_vec(Tag::ModelPixelScaleTag), path)?;
        let tiepoints = tiff_tag(reader.get_tag_f64_vec(Tag::ModelTiepointTag), path)?;
        let keys = tiff_tag(reader.get_tag_u16_vec(Tag::GeoKeyDirectoryTag), path)?;
        if scales.len() != 3 || tiepoints.len() != 6 {
            return Err(Error::Validation(format!(
                "GLO-30 tile {} has unsupported GeoTIFF scale or tiepoint tags",
                path.display()
            )));
        }
        if geo_key(&keys, 1_024) != Some(GEOGRAPHIC_MODEL_TYPE)
            || geo_key(&keys, 1_025) != Some(RASTER_PIXEL_IS_POINT)
            || geo_key(&keys, 2_048) != Some(WGS84_EPSG)
        {
            return Err(Error::Validation(format!(
                "GLO-30 tile {} is not an EPSG:4326 RasterPixelIsPoint grid",
                path.display()
            )));
        }
        if scales[0] <= 0.0
            || scales[1] <= 0.0
            || !scales[..2].iter().all(|v| v.is_finite())
            || ![tiepoints[0], tiepoints[1], tiepoints[3], tiepoints[4]]
                .iter()
                .all(|value| value.is_finite())
        {
            return Err(Error::Validation(format!(
                "GLO-30 tile {} has invalid pixel scales or tiepoint coordinates",
                path.display()
            )));
        }
        Ok(Self {
            longitude_scale: scales[0],
            latitude_scale: scales[1],
            tie_column: tiepoints[0],
            tie_row: tiepoints[1],
            tie_longitude: tiepoints[3],
            tie_latitude: tiepoints[4],
        })
    }

    fn nearest_pixel(self, latitude: f64, longitude: f64) -> (i64, i64) {
        let column = ((longitude - self.tie_longitude) / self.longitude_scale + self.tie_column)
            .round() as i64;
        let row =
            ((self.tie_latitude - latitude) / self.latitude_scale + self.tie_row).round() as i64;
        (column, row)
    }

    fn matches_tile(self, tile: TileKey, width: u32, height: u32) -> Result<()> {
        let epsilon = 1e-10;
        let origin_longitude = self.tie_longitude - self.tie_column * self.longitude_scale;
        let origin_latitude = self.tie_latitude + self.tie_row * self.latitude_scale;
        if height != GLO30_TILE_HEIGHT
            || !GLO30_TILE_WIDTHS.contains(&width)
            || (self.longitude_scale - 1.0 / f64::from(width)).abs() > epsilon
            || (self.latitude_scale - 1.0 / f64::from(height)).abs() > epsilon
            || (origin_longitude - f64::from(tile.west)).abs() > epsilon
            || (origin_latitude - (f64::from(tile.south) + 1.0)).abs() > epsilon
        {
            return Err(Error::Validation(
                "GLO-30 resolution, filename, and parsed GeoTIFF transform disagree".into(),
            ));
        }
        Ok(())
    }
}

struct RasterMetadata {
    width: u32,
    height: u32,
    georeference: GeoReference,
}

impl RasterMetadata {
    fn read(path: &Path, expected_tile: TileKey) -> Result<Self> {
        let file = File::open(path)?;
        let mut decoder = Decoder::new(BufReader::new(file)).map_err(|source| Error::Tiff {
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
        let georeference = GeoReference::parse(&mut decoder, path)?;
        georeference.matches_tile(expected_tile, width, height)?;
        Ok(Self {
            width,
            height,
            georeference,
        })
    }

    fn nearest_sample_tile(
        &self,
        mut tile: TileKey,
        latitude: f64,
        longitude: f64,
    ) -> Result<TileKey> {
        let (column, row) = self.georeference.nearest_pixel(latitude, longitude);
        if column == i64::from(self.width) {
            if tile.west < 179 {
                tile.west += 1;
            }
        } else if !(0..i64::from(self.width)).contains(&column) {
            return Err(Error::Validation(format!(
                "longitude {longitude} is outside its parsed GLO-30 point grid"
            )));
        }
        if row == i64::from(self.height) {
            if tile.south > -90 {
                tile.south -= 1;
            }
        } else if !(0..i64::from(self.height)).contains(&row) {
            return Err(Error::Validation(format!(
                "latitude {latitude} is outside its parsed GLO-30 point grid"
            )));
        }
        Ok(tile)
    }
}

fn tiff_tag<T>(value: tiff::TiffResult<T>, path: &Path) -> Result<T> {
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

    use tiff::encoder::{TiffEncoder, colortype::Gray32Float};

    use super::{GeoReference, Raster, RasterMetadata, TileKey};

    #[test]
    fn coordinates_select_signed_degree_tiles() {
        let northwest = TileKey::containing(52.5, -1.25).unwrap();
        assert_eq!(northwest.south, 52);
        assert_eq!(northwest.west, -2);
        assert_eq!(
            northwest.filename(),
            "Copernicus_DSM_COG_10_N52_00_W002_00_DEM.tif"
        );

        let southeast = TileKey::containing(-3.25, 7.5).unwrap();
        assert_eq!(
            southeast.filename(),
            "Copernicus_DSM_COG_10_S04_00_E007_00_DEM.tif"
        );
        assert!(TileKey::containing(f64::NAN, 0.0).is_err());
        assert!(TileKey::containing(91.0, 0.0).is_err());
        assert_eq!(
            TileKey::containing(90.0, 180.0).unwrap(),
            TileKey {
                south: 89,
                west: 179
            }
        );
    }

    #[test]
    fn pixels_are_rounded_from_the_parsed_point_grid_transform() {
        let raster = fixture_raster(&[0.0; 9], 0.25, 0.0, 49.0);
        assert_eq!(raster.pixel(48.7, 0.3).unwrap(), (1, 1));
        assert_eq!(raster.pixel(49.0, 0.0).unwrap(), (0, 0));
        assert!(raster.pixel(48.0, 0.0).is_err());

        let metadata = RasterMetadata {
            width: raster.width,
            height: raster.height,
            georeference: raster.georeference,
        };
        let tile = TileKey { south: 48, west: 0 };
        assert_eq!(
            metadata.nearest_sample_tile(tile, 48.5, 0.7).unwrap().west,
            1
        );
        assert_eq!(
            metadata.nearest_sample_tile(tile, 48.3, 0.5).unwrap().south,
            47
        );
    }

    #[test]
    fn decoder_parses_float_pixels_and_replaces_voids() {
        let raster = fixture_raster(
            &[10.2, 11.6, 12.0, 13.0, f32::NAN, 15.0, 16.0, 17.0, 18.0],
            0.25,
            0.0,
            49.0,
        );
        assert_eq!(raster.elevation_near(0, 0).0.get(), 10);
        let (replacement, used_fallback) = raster.elevation_near(1, 1);
        assert!(used_fallback);
        assert_eq!(replacement.get(), 12);
    }

    #[test]
    fn source_boundary_accepts_variable_width_but_rejects_wrong_resolution() {
        let tile = TileKey { south: 48, west: 0 };
        let variable_width = GeoReference {
            longitude_scale: 1.0 / 2_400.0,
            latitude_scale: 1.0 / 3_600.0,
            tie_column: 0.0,
            tie_row: 0.0,
            tie_longitude: 0.0,
            tie_latitude: 49.0,
        };
        assert!(variable_width.matches_tile(tile, 2_400, 3_600).is_ok());
        assert!(variable_width.matches_tile(tile, 10, 10).is_err());
    }

    #[test]
    fn decoder_rejects_non_finite_tiepoints() {
        let mut bytes = fixture_bytes(&[0.0; 9], 0.25, 0.0, f64::NAN);
        assert!(Raster::decode(&mut bytes, std::path::Path::new("fixture.tif")).is_err());
    }

    fn fixture_raster(values: &[f32], scale: f64, west: f64, north: f64) -> Raster {
        let bytes = fixture_bytes(values, scale, west, north);
        Raster::decode(bytes, std::path::Path::new("fixture.tif")).unwrap()
    }

    fn fixture_bytes(values: &[f32], scale: f64, west: f64, north: f64) -> Cursor<Vec<u8>> {
        use tiff::tags::Tag;

        let mut bytes = Cursor::new(Vec::new());
        let mut encoder = TiffEncoder::new(&mut bytes).unwrap();
        let mut image = encoder.new_image::<Gray32Float>(3, 3).unwrap();
        image
            .encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[scale, scale, 0.0][..])
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::ModelTiepointTag,
                &[0.0, 0.0, 0.0, west, north, 0.0][..],
            )
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::GeoKeyDirectoryTag,
                &[
                    1_u16, 1, 0, 3, 1_024, 0, 1, 2, 1_025, 0, 1, 2, 2_048, 0, 1, 4_326,
                ][..],
            )
            .unwrap();
        image.write_data(values).unwrap();
        bytes.set_position(0);
        bytes
    }
}
