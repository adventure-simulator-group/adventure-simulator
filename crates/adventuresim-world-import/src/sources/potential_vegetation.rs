//! Jung/IIASA European potential-natural-vegetation v1.1 COG sampling.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    ForestCover, PotentialVegetation, PotentialVegetationClass, PotentialVegetationPosterior,
    SourceProvenance, SuitabilityBasisPoints,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tiff::{
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};

use crate::{
    Error, Result,
    draft::{
        ForestSettlementDraft, PotentialVegetationSettlementDraft, WorldDraft, push_source_note,
    },
    spatial::SpatialProjection,
};

const SOURCE_NAME: &str = "Jung/IIASA Current and future European potential vegetation types v1.1";
const SOURCE_URL: &str = "https://doi.org/10.5281/zenodo.14627466";
const SOURCE_LICENSE: &str = "Creative Commons Attribution 4.0 International (CC BY 4.0)";
const WIDTH: u32 = 5_583;
const HEIGHT: u32 = 4_474;
const WEST: f64 = 944_000.0;
const NORTH: f64 = 5_416_000.0;
const PIXEL: f64 = 1_000.0;
const TILE: u32 = 512;
const MAX_TILE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const GEO_KEYS: [u16; 80] = [
    1, 1, 0, 19, 1024, 0, 1, 1, 1025, 0, 1, 1, 1026, 34737, 8, 0, 2048, 0, 1, 32767, 2049, 34737,
    80, 8, 2050, 0, 1, 32767, 2054, 0, 1, 9102, 2056, 0, 1, 7019, 2057, 34736, 1, 5, 2059, 34736,
    1, 4, 2061, 34736, 1, 6, 3072, 0, 1, 32767, 3074, 0, 1, 32767, 3075, 0, 1, 10, 3076, 0, 1,
    9001, 3082, 34736, 1, 2, 3083, 34736, 1, 3, 3088, 34736, 1, 1, 3089, 34736, 1, 0,
];
const GEO_DOUBLES: [f64; 7] = [
    52.0,
    10.0,
    4_321_000.0,
    3_210_000.0,
    298.257222101004,
    6_378_137.0,
    0.0,
];
const MANIFEST: &str = "jung-pnv-manifest.json";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const CATEGORICAL: &str = "pnv_mostlikely_current_laea_1km.tif";
const POSTERIORS: [(PotentialVegetationClass, &str); 6] = [
    (
        PotentialVegetationClass::WoodlandAndForest,
        "pnv_Woodland.and.forest_current_laea_1km.tif",
    ),
    (
        PotentialVegetationClass::HeathlandAndShrub,
        "pnv_Heathland.and.shrub_current_laea_1km.tif",
    ),
    (
        PotentialVegetationClass::Grassland,
        "pnv_Grassland_current_laea_1km.tif",
    ),
    (
        PotentialVegetationClass::SparselyVegetatedAreas,
        "pnv_Sparsely.vegetated.areas_current_laea_1km.tif",
    ),
    (
        PotentialVegetationClass::Wetlands,
        "pnv_Wetlands_current_laea_1km.tif",
    ),
    (
        PotentialVegetationClass::MarineInletsAndTransitionalWaters,
        "pnv_Marine.inlets.and.transitional.waters_current_laea_1km.tif",
    ),
];

#[derive(Clone, Copy)]
struct PinnedFile {
    filename: &'static str,
    size: u64,
    md5: &'static str,
    sha256: &'static str,
}
const PINNED_FILES: [PinnedFile; 7] = [
    PinnedFile {
        filename: CATEGORICAL,
        size: 4_195_207,
        md5: "db680904cec1b046c0c4d1479c3b8cf7",
        sha256: "b5c1e48263fe7eb3ef4a7a926605821851b64662dc03a33ea53fec24f56b72eb",
    },
    PinnedFile {
        filename: "pnv_Grassland_current_laea_1km.tif",
        size: 137_352_581,
        md5: "9f271670bdf9abbd636f02da4ac204be",
        sha256: "66425840c4993f4ed3c9d8415374c5b11838aa73f38f5d72c10cf35483a24170",
    },
    PinnedFile {
        filename: "pnv_Heathland.and.shrub_current_laea_1km.tif",
        size: 137_713_877,
        md5: "0a094c5f8d5b129845dd9eb1752465f6",
        sha256: "dccbe42b114059399b5a272b9956cad2ef37dbcd651ed6b90fa7615958a33923",
    },
    PinnedFile {
        filename: "pnv_Marine.inlets.and.transitional.waters_current_laea_1km.tif",
        size: 142_525_264,
        md5: "9f4f4c6dc435102895ad3e399bbcd8fe",
        sha256: "f26b14e2f2a18098a43f561699072f97a7ed9a91308998d0ac9e6796697fe8eb",
    },
    PinnedFile {
        filename: "pnv_Sparsely.vegetated.areas_current_laea_1km.tif",
        size: 137_536_892,
        md5: "86d62568a2befc40885ed5c9e5e5750f",
        sha256: "6fb5476ac9eb438c2fd4c36aed270ce8a4092d218281551a02b119cd32ccb92b",
    },
    PinnedFile {
        filename: "pnv_Wetlands_current_laea_1km.tif",
        size: 137_250_364,
        md5: "0cc844d28b230ee6f86ad411532afe78",
        sha256: "b4029e292af3fce6d30fda4d7bf72d51abc942665ace836533347c599362b5b2",
    },
    PinnedFile {
        filename: "pnv_Woodland.and.forest_current_laea_1km.tif",
        size: 138_346_368,
        md5: "0361cd4f289a7069d8e17bc00deb5c92",
        sha256: "d0850e59de86e5631818eb711747fcd2b1ffcf4e415eb77006fd4f48d509e877",
    },
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceManifest {
    schema: u32,
    record: String,
    doi: String,
    version: String,
    publication_date: String,
    license: String,
    files: Vec<ManifestFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    filename: String,
    size: u64,
    md5: String,
    sha256: String,
    url: String,
}

pub(crate) fn enrich(
    draft: WorldDraft<ForestSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<PotentialVegetationSettlementDraft>> {
    validate_manifest_and_files(directory)?;
    enrich_verified(draft, directory)
}

fn enrich_verified(
    draft: WorldDraft<ForestSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<PotentialVegetationSettlementDraft>> {
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), 0);
    }
    let projection = SpatialProjection::new()?;
    let cell_size = f64::from(draft.spatial_grid.cell_size_meters().get());
    let cells = draft
        .settlements
        .iter()
        .map(|settlement| {
            let base = &settlement.land.elevated.settlement;
            projection
                .project(base.latitude, base.longitude)
                .map(|point| point.cell(draft.spatial_grid))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut posterior = vec![[None; 6]; cells.len()];
    let mut files_read = 0;
    for (class_index, (_, filename)) in POSTERIORS.iter().enumerate() {
        let mut raster = JungRaster::open(&directory.join(filename), RasterKind::Posterior)?;
        files_read += 1;
        for (index, cell) in cells.iter().enumerate() {
            posterior[index][class_index] = raster.mean_over_cell(
                cell.column() as f64 * cell_size,
                cell.row() as f64 * cell_size,
                cell_size,
            )?;
        }
    }
    let mut categorical = JungRaster::open(&directory.join(CATEGORICAL), RasterKind::Categorical)?;
    files_read += 1;
    let mut samples = Vec::with_capacity(cells.len());
    for (index, (cell, settlement)) in cells.iter().zip(&draft.settlements).enumerate() {
        let values = posterior[index];
        if values.iter().all(Option::is_some) {
            let q = |i: usize| {
                SuitabilityBasisPoints::new(
                    (f64::from(values[i].unwrap()) * 10_000.0).round() as u16
                )
                .unwrap()
            };
            samples.push(PotentialVegetation::Posterior(
                PotentialVegetationPosterior {
                    woodland_and_forest: q(0),
                    heathland_and_shrub: q(1),
                    grassland: q(2),
                    sparsely_vegetated_areas: q(3),
                    wetlands: q(4),
                    marine_inlets_and_transitional_waters: q(5),
                },
            ));
            continue;
        }
        let west = cell.column() as f64 * cell_size;
        let south = cell.row() as f64 * cell_size;
        if let Some(raw) = categorical.dominant_over_cell(west, south, cell_size)? {
            samples.push(PotentialVegetation::Categorical(class_from_byte(raw)?));
        } else {
            samples.push(PotentialVegetation::Inferred(infer_class(settlement)));
        }
    }
    finish(draft, samples, files_read)
}

fn finish(
    mut draft: WorldDraft<ForestSettlementDraft>,
    samples: Vec<PotentialVegetation>,
    files_read: usize,
) -> Result<WorldDraft<PotentialVegetationSettlementDraft>> {
    if samples.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "potential-vegetation samples do not match settlements".into(),
        ));
    }
    let posterior = samples
        .iter()
        .filter(|v| matches!(v, PotentialVegetation::Posterior(_)))
        .count();
    let categorical = samples
        .iter()
        .filter(|v| matches!(v, PotentialVegetation::Categorical(_)))
        .count();
    let inferred = samples.len() - posterior - categorical;
    let settlements: Vec<_> = std::mem::take(&mut draft.settlements).into_iter().zip(samples).map(|(mut forest, potential_vegetation)| {
        let note = match potential_vegetation {
            PotentialVegetation::Posterior(_) => "**[Jung/IIASA European PNV v1.1](https://doi.org/10.5281/zenodo.14627466):** Six current-climate posterior mean rasters provide potential-vegetation suitability.",
            PotentialVegetation::Categorical(_) => "**Jung/IIASA categorical fallback:** Posterior coverage was incomplete; the pinned most-likely class raster provides the potential-vegetation class.",
            PotentialVegetation::Inferred(_) => "**Potential-vegetation coverage inference:** Neither posterior nor categorical source data covered this cell; class is deterministically inferred from forest cover, elevation, latitude, and HYDE context.",
        };
        push_source_note(&mut forest, note);
        PotentialVegetationSettlementDraft { forest, potential_vegetation }
    }).collect();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.potential_vegetation_raster_files_read = files_read;
    draft.report.potential_vegetation_samples = settlements.len();
    draft.report.potential_vegetation_posterior_samples = posterior;
    draft.report.potential_vegetation_categorical_samples = categorical;
    draft.report.potential_vegetation_inferred_samples = inferred;
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

fn infer_class(settlement: &ForestSettlementDraft) -> PotentialVegetationClass {
    match settlement.forest_cover {
        ForestCover::Wooded(_) => PotentialVegetationClass::WoodlandAndForest,
        ForestCover::Open
            if settlement.land.elevated.elevation.get() >= 1_500
                || settlement.land.elevated.settlement.latitude >= 60.0 =>
        {
            PotentialVegetationClass::SparselyVegetatedAreas
        }
        ForestCover::Open => PotentialVegetationClass::Grassland,
    }
}

fn class_from_byte(value: u8) -> Result<PotentialVegetationClass> {
    use PotentialVegetationClass::*;
    match value {
        1 => Ok(WoodlandAndForest),
        2 => Ok(HeathlandAndShrub),
        3 => Ok(Grassland),
        4 => Ok(SparselyVegetatedAreas),
        5 => Ok(Wetlands),
        6 => Ok(MarineInletsAndTransitionalWaters),
        _ => Err(Error::Validation(format!(
            "Jung categorical class {value} is outside 1..=6"
        ))),
    }
}

fn validate_manifest_and_files(directory: &Path) -> Result<()> {
    let manifest_path = directory.join(MANIFEST);
    if !manifest_path.is_file() {
        return Err(Error::MissingSource(manifest_path));
    }
    let manifest_size = fs::metadata(&manifest_path)?.len();
    if manifest_size > MAX_MANIFEST_BYTES {
        return Err(Error::Validation(format!(
            "{} is {manifest_size} bytes; Jung manifest limit is {MAX_MANIFEST_BYTES} bytes",
            manifest_path.display()
        )));
    }
    let manifest: SourceManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?).map_err(|source| Error::JsonSource {
            path: manifest_path.clone(),
            source,
        })?;
    if manifest.schema != 1
        || manifest.record != "14627466"
        || manifest.doi != "10.5281/zenodo.14627466"
        || manifest.version != "1.1"
        || manifest.publication_date != "2025-01-10"
        || manifest.license != "CC-BY-4.0"
        || manifest.files.len() != PINNED_FILES.len()
    {
        return Err(Error::Validation(format!(
            "{} does not identify the pinned Jung/IIASA v1.1 source",
            manifest_path.display()
        )));
    }
    for (actual, expected) in manifest.files.iter().zip(PINNED_FILES) {
        let expected_url = format!(
            "https://zenodo.org/api/records/14627466/files/{}/content",
            expected.filename
        );
        if actual.filename != expected.filename
            || actual.size != expected.size
            || actual.md5 != expected.md5
            || actual.sha256 != expected.sha256
            || actual.url != expected_url
        {
            return Err(Error::Validation(format!(
                "{} has an unexpected file entry for {}",
                manifest_path.display(),
                expected.filename
            )));
        }
        validate_source_file(directory, expected)?;
    }
    Ok(())
}

fn validate_source_file(directory: &Path, expected: PinnedFile) -> Result<()> {
    let path = directory.join(expected.filename);
    let metadata = path.metadata().map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::MissingSource(path.clone())
        } else {
            source.into()
        }
    })?;
    if metadata.len() != expected.size {
        return Err(Error::Validation(format!(
            "{} has size {}; expected {}",
            path.display(),
            metadata.len(),
            expected.size
        )));
    }
    let mut file = BufReader::new(File::open(&path)?);
    let mut sha = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha.update(&buffer[..read]);
    }
    if format!("{:x}", sha.finalize()) != expected.sha256 {
        return Err(Error::Validation(format!(
            "{} SHA-256 mismatch",
            path.display()
        )));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum RasterKind {
    Posterior,
    Categorical,
}

struct JungRaster {
    path: PathBuf,
    decoder: Decoder<BufReader<File>>,
    kind: RasterKind,
    cache: BTreeMap<u32, Vec<f32>>,
    cache_bytes: usize,
}

impl JungRaster {
    fn open(path: &Path, kind: RasterKind) -> Result<Self> {
        let file = File::open(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::MissingSource(path.into())
            } else {
                source.into()
            }
        })?;
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 16 * 1024 * 1024;
        limits.intermediate_buffer_size = 16 * 1024 * 1024;
        limits.ifd_value_size = 256 * 1024;
        let mut decoder = Decoder::new(BufReader::new(file))
            .map_err(|source| Error::Tiff {
                path: path.into(),
                source,
            })?
            .with_limits(limits);
        validate_contract(&mut decoder, path, kind)?;
        Ok(Self {
            path: path.into(),
            decoder,
            kind,
            cache: BTreeMap::new(),
            cache_bytes: 0,
        })
    }

    fn value(&mut self, column: u32, row: u32) -> Result<Option<f32>> {
        if column >= WIDTH || row >= HEIGHT {
            return Ok(None);
        }
        let tiles_across = WIDTH.div_ceil(TILE);
        let tile_col = column / TILE;
        let tile_row = row / TILE;
        let tile_index = tile_row * tiles_across + tile_col;
        if !self.cache.contains_key(&tile_index) {
            let DecodingResult::F32(values) =
                self.decoder
                    .read_chunk(tile_index)
                    .map_err(|source| Error::Tiff {
                        path: self.path.clone(),
                        source,
                    })?
            else {
                return Err(Error::Validation(format!(
                    "{} is not Float32",
                    self.path.display()
                )));
            };
            let (chunk_width, chunk_height) = self.decoder.chunk_data_dimensions(tile_index);
            let channels = self.channels();
            let bytes = decoded_chunk_bytes(
                &self.path,
                tile_index,
                values.len(),
                chunk_width,
                chunk_height,
                channels,
            )?;
            if bytes > MAX_TILE_CACHE_BYTES {
                return Err(Error::Validation(format!(
                    "{} tile exceeds the cache byte bound",
                    self.path.display()
                )));
            }
            while self
                .cache_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_TILE_CACHE_BYTES)
            {
                let Some(key) = self.cache.keys().next().copied() else {
                    break;
                };
                if let Some(removed) = self.cache.remove(&key) {
                    self.cache_bytes -= removed.len() * std::mem::size_of::<f32>();
                }
            }
            self.cache_bytes += bytes;
            self.cache.insert(tile_index, values);
        }
        let width = self.decoder.chunk_data_dimensions(tile_index).0;
        let local = ((row % TILE) * width + column % TILE) as usize;
        let channels = self.channels();
        let value = *self
            .cache
            .get(&tile_index)
            .and_then(|values| values.get(local.checked_mul(channels)?))
            .ok_or_else(|| {
                Error::Validation(format!(
                    "{} chunk {tile_index} is truncated",
                    self.path.display()
                ))
            })?;
        if value.is_nan() {
            return Ok(None);
        }
        if !value.is_finite() {
            return Err(Error::Validation(format!(
                "{} contains non-finite non-nodata data",
                self.path.display()
            )));
        }
        match self.kind {
            RasterKind::Posterior if !(0.0..=1.0).contains(&value) => Err(Error::Validation(
                format!("{} posterior mean is outside 0..=1", self.path.display()),
            )),
            RasterKind::Categorical if value.fract() != 0.0 || !(1.0..=6.0).contains(&value) => {
                Err(Error::Validation(format!(
                    "{} categorical value is not 1..=6",
                    self.path.display()
                )))
            }
            _ => Ok(Some(value)),
        }
    }

    fn channels(&self) -> usize {
        match self.kind {
            RasterKind::Posterior => 7,
            RasterKind::Categorical => 1,
        }
    }

    fn mean_over_cell(&mut self, west: f64, south: f64, size: f64) -> Result<Option<f32>> {
        let east = west + size;
        let north = south + size;
        let c0 = ((west - WEST) / PIXEL).floor() as i64;
        let c1 = ((east - WEST) / PIXEL).ceil() as i64;
        let r0 = ((NORTH - north) / PIXEL).floor() as i64;
        let r1 = ((NORTH - south) / PIXEL).ceil() as i64;
        let mut weighted = 0.0;
        let mut area = 0.0;
        for row in r0..r1 {
            for col in c0..c1 {
                if row < 0 || col < 0 {
                    continue;
                }
                let pw = WEST + col as f64 * PIXEL;
                let pn = NORTH - row as f64 * PIXEL;
                let overlap = (east.min(pw + PIXEL) - west.max(pw)).max(0.0)
                    * (north.min(pn) - south.max(pn - PIXEL)).max(0.0);
                if overlap > 0.0 {
                    if let Some(value) = self.value(col as u32, row as u32)? {
                        weighted += f64::from(value) * overlap;
                        area += overlap;
                    }
                }
            }
        }
        Ok((area > 0.0).then_some((weighted / area) as f32))
    }

    fn dominant_over_cell(&mut self, west: f64, south: f64, size: f64) -> Result<Option<u8>> {
        let east = west + size;
        let north = south + size;
        let mut area = [0.0_f64; 6];
        let c0 = ((west - WEST) / PIXEL).floor() as i64;
        let c1 = ((east - WEST) / PIXEL).ceil() as i64;
        let r0 = ((NORTH - north) / PIXEL).floor() as i64;
        let r1 = ((NORTH - south) / PIXEL).ceil() as i64;
        for row in r0..r1 {
            for col in c0..c1 {
                if row < 0 || col < 0 {
                    continue;
                }
                let pw = WEST + col as f64 * PIXEL;
                let pn = NORTH - row as f64 * PIXEL;
                let overlap = (east.min(pw + PIXEL) - west.max(pw)).max(0.0)
                    * (north.min(pn) - south.max(pn - PIXEL)).max(0.0);
                if overlap > 0.0 {
                    if let Some(value) = self.value(col as u32, row as u32)? {
                        area[value as usize - 1] += overlap;
                    }
                }
            }
        }
        Ok(area
            .iter()
            .enumerate()
            .filter(|(_, a)| **a > 0.0)
            .max_by(|(ia, a), (ib, b)| a.total_cmp(b).then_with(|| ib.cmp(ia)))
            .map(|(i, _)| i as u8 + 1))
    }
}

fn validate_contract(
    decoder: &mut Decoder<BufReader<File>>,
    path: &Path,
    kind: RasterKind,
) -> Result<()> {
    let dimensions = decoder.dimensions().map_err(|source| Error::Tiff {
        path: path.into(),
        source,
    })?;
    let scale = decoder
        .get_tag_f64_vec(Tag::ModelPixelScaleTag)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let tie = decoder
        .get_tag_f64_vec(Tag::ModelTiepointTag)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let keys = decoder
        .get_tag_u16_vec(Tag::GeoKeyDirectoryTag)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let doubles = decoder
        .get_tag_f64_vec(Tag::GeoDoubleParamsTag)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let compression = decoder
        .get_tag_u16_vec(Tag::Compression)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let predictor = decoder
        .get_tag_u16_vec(Tag::Predictor)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let planar = decoder
        .get_tag_u16_vec(Tag::PlanarConfiguration)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let photometric = decoder
        .get_tag_u16_vec(Tag::PhotometricInterpretation)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let gdal_metadata = decoder
        .get_tag_ascii_string(Tag::Unknown(42_112))
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let nodata = decoder
        .get_tag_ascii_string(Tag::GdalNodata)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let bits = decoder
        .get_tag_u16_vec(Tag::BitsPerSample)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let formats = decoder
        .get_tag_u16_vec(Tag::SampleFormat)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
    let samples = decoder
        .get_tag_u16_vec(Tag::SamplesPerPixel)
        .map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?
        .first()
        .copied()
        .ok_or_else(|| {
            Error::Validation(format!(
                "{} has an empty SamplesPerPixel tag",
                path.display()
            ))
        })?;
    let tile_count = decoder.tile_count().map_err(|source| Error::Tiff {
        path: path.into(),
        source,
    })?;
    let expected = match kind {
        RasterKind::Posterior => 7,
        RasterKind::Categorical => 1,
    };
    if dimensions != (WIDTH, HEIGHT)
        || scale != [PIXEL, PIXEL, 0.0]
        || tie != [0.0, 0.0, 0.0, WEST, NORTH, 0.0]
        || samples != expected
        || bits != vec![32; expected as usize]
        || formats != vec![3; expected as usize]
        || compression != [8]
        || predictor != [1]
        || planar != [1]
        || photometric != [1]
        || decoder.chunk_dimensions() != (TILE, TILE)
        || tile_count != WIDTH.div_ceil(TILE) * HEIGHT.div_ceil(TILE)
        || nodata.trim_matches('\0').trim() != "nan"
        || !crs_contract_matches(&keys, &doubles)
        || !band_descriptions_match(&gdal_metadata, kind)
    {
        return Err(Error::Validation(format!(
            "{} does not match the pinned Jung/IIASA v1.1 GeoTIFF contract",
            path.display()
        )));
    }
    Ok(())
}

fn band_descriptions_match(metadata: &str, kind: RasterKind) -> bool {
    let expected: &[&str] = match kind {
        RasterKind::Posterior => &["mean", "sd", "q05", "q50", "q95", "mode", "cv"],
        RasterKind::Categorical => &["MostLikelyPNV"],
    };
    let descriptions = (0..expected.len()).map(|sample| {
        format!(
            "<Item name=\"DESCRIPTION\" sample=\"{sample}\" role=\"description\">{}</Item>",
            expected[sample]
        )
    });
    descriptions.clone().all(|entry| metadata.contains(&entry))
        && metadata.matches("name=\"DESCRIPTION\"").count() == expected.len()
}

fn crs_contract_matches(keys: &[u16], doubles: &[f64]) -> bool {
    keys == GEO_KEYS && doubles == GEO_DOUBLES
}

fn decoded_chunk_bytes(
    path: &Path,
    tile: u32,
    len: usize,
    width: u32,
    height: u32,
    channels: usize,
) -> Result<usize> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(channels))
        .ok_or_else(|| {
            Error::Validation(format!("{} chunk dimensions overflow", path.display()))
        })?;
    if len != expected {
        return Err(Error::Validation(format!(
            "{} chunk {tile} decoded {len} values; expected {expected}",
            path.display()
        )));
    }
    len.checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| Error::Validation("Jung tile byte size overflow".into()))
}

#[cfg(test)]
mod tests {
    use super::{JungRaster, RasterKind, class_from_byte};
    use crate::draft::{
        ElevatedSettlementDraft, ForestSettlementDraft, LandUseSettlementDraft, WorldDraft,
    };
    use adventuresim_world_schema::{
        ElevationMeters, ForestCover, LandUseFraction, LandUseProfile, PotentialVegetation,
        PotentialVegetationClass, SpatialGridSpec,
    };
    use std::path::Path;
    #[test]
    fn normal_posterior_tile_cache_is_bounded_below_64_mib() {
        let tile_bytes =
            super::TILE as usize * super::TILE as usize * 7 * std::mem::size_of::<f32>();
        assert_eq!(super::MAX_TILE_CACHE_BYTES / tile_bytes, 9);
        assert!(10 * tile_bytes > super::MAX_TILE_CACHE_BYTES);
    }
    #[test]
    fn malformed_chunk_lengths_are_errors() {
        let path = Path::new("fixture.tif");
        assert!(super::decoded_chunk_bytes(path, 0, 512 * 512 * 7, 512, 512, 7).is_ok());
        assert!(super::decoded_chunk_bytes(path, 0, 512 * 512 * 7 - 1, 512, 512, 7).is_err());
    }
    #[test]
    fn band_order_and_complete_crs_contract_are_fail_closed() {
        let correct = (0..7)
            .map(|sample| {
                format!(
                    "<Item name=\"DESCRIPTION\" sample=\"{sample}\" role=\"description\">{}</Item>",
                    ["mean", "sd", "q05", "q50", "q95", "mode", "cv"][sample]
                )
            })
            .collect::<String>();
        assert!(super::band_descriptions_match(
            &correct,
            RasterKind::Posterior
        ));
        assert!(!super::band_descriptions_match(
            &correct.replace("sample=\"0\"", "sample=\"1\""),
            RasterKind::Posterior
        ));
        assert!(super::crs_contract_matches(
            &super::GEO_KEYS,
            &super::GEO_DOUBLES
        ));
        let mut wrong = super::GEO_DOUBLES;
        wrong[2] += 1.0;
        assert!(!super::crs_contract_matches(&super::GEO_KEYS, &wrong));
    }
    #[test]
    fn manifest_is_required_and_unknown_fields_are_rejected() {
        let missing = std::env::temp_dir().join("adventuresim-missing-jung-manifest");
        assert!(super::validate_manifest_and_files(&missing).is_err());
        let json = r#"{"schema":1,"record":"14627466","doi":"10.5281/zenodo.14627466","version":"1.1","publication_date":"2025-01-10","license":"CC-BY-4.0","files":[],"unexpected":true}"#;
        assert!(serde_json::from_str::<super::SourceManifest>(json).is_err());
    }
    #[test]
    fn oversized_manifest_is_rejected_before_parsing() {
        let directory = std::env::temp_dir().join(format!(
            "adventuresim-jung-large-manifest-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(super::MANIFEST),
            vec![b' '; super::MAX_MANIFEST_BYTES as usize + 1],
        )
        .unwrap();
        let error = super::validate_manifest_and_files(&directory).unwrap_err();
        assert!(error.to_string().contains("manifest limit"));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn empty_world_reads_zero_rasters_after_source_verification() {
        let draft: WorldDraft<ForestSettlementDraft> = WorldDraft {
            year: 1544,
            spatial_grid: SpatialGridSpec::default(),
            sources: Vec::new(),
            road_types: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            settlement_aliases: Vec::new(),
            settlement_descriptions: Vec::new(),
            settlements: Vec::new(),
            report: Default::default(),
        };
        let output = super::enrich_verified(draft, Path::new("unused")).unwrap();
        assert_eq!(output.report.potential_vegetation_raster_files_read, 0);
        assert_eq!(output.report.potential_vegetation_samples, 0);
    }
    #[test]
    fn source_file_size_and_sha_are_enforced() {
        let directory =
            std::env::temp_dir().join(format!("adventuresim-jung-hash-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("fixture.tif"), b"abc").unwrap();
        let wrong_hash = super::PinnedFile {
            filename: "fixture.tif",
            size: 3,
            md5: "unused",
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };
        assert!(
            super::validate_source_file(&directory, wrong_hash)
                .unwrap_err()
                .to_string()
                .contains("SHA-256 mismatch")
        );
        let wrong_size = super::PinnedFile {
            size: 4,
            ..wrong_hash
        };
        assert!(
            super::validate_source_file(&directory, wrong_size)
                .unwrap_err()
                .to_string()
                .contains("has size 3")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn categorical_domain_is_closed() {
        for value in 1..=6 {
            assert!(class_from_byte(value).is_ok());
        }
        assert!(class_from_byte(0).is_err());
        assert!(class_from_byte(7).is_err());
    }
    #[test]
    #[ignore = "requires JUNG_PNV_DIR initialized with official v1.1 rasters"]
    fn official_raster_contract_and_sample_are_readable() {
        let root = std::env::var_os("JUNG_PNV_DIR").expect("set JUNG_PNV_DIR");
        let root = Path::new(&root);
        let mut posterior = None;
        for (_, filename) in super::POSTERIORS {
            let mut raster = JungRaster::open(&root.join(filename), RasterKind::Posterior).unwrap();
            assert!(raster.value(3_377, 2_126).unwrap().is_some());
            posterior = Some(raster);
        }
        let mut categorical = JungRaster::open(
            &root.join("pnv_mostlikely_current_laea_1km.tif"),
            RasterKind::Categorical,
        )
        .unwrap();
        assert!(posterior.unwrap().value(3_377, 2_126).unwrap().is_some());
        if let Some(value) = categorical.value(3_377, 2_126).unwrap() {
            assert!(class_from_byte(value as u8).is_ok());
        }
    }

    #[test]
    #[ignore = "requires JUNG_PNV_DIR and VIABUNDUS_DIR official sources"]
    fn official_sources_cover_all_viabundus_settlements_with_stable_digest() {
        let root = std::env::var_os("JUNG_PNV_DIR").expect("set JUNG_PNV_DIR");
        let viabundus = std::env::var_os("VIABUNDUS_DIR").expect("set VIABUNDUS_DIR");
        let raw = crate::sources::viabundus::compile(
            Path::new(&viabundus),
            1544,
            SpatialGridSpec::default(),
        )
        .unwrap();
        assert_eq!(raw.settlements.len(), 6_041);
        let natural = LandUseFraction::new(10_000).unwrap();
        let zero = LandUseFraction::new(0).unwrap();
        let land_use = LandUseProfile::new(zero, zero, zero, natural).unwrap();
        let settlements = raw
            .settlements
            .into_iter()
            .map(|settlement| ForestSettlementDraft {
                land: LandUseSettlementDraft {
                    elevated: ElevatedSettlementDraft {
                        settlement,
                        elevation: ElevationMeters::new(0).unwrap(),
                    },
                    land_use,
                },
                forest_cover: ForestCover::Open,
            })
            .collect();
        let forest = WorldDraft {
            year: raw.year,
            spatial_grid: raw.spatial_grid,
            sources: raw.sources,
            road_types: raw.road_types,
            nodes: raw.nodes,
            edges: raw.edges,
            settlement_aliases: raw.settlement_aliases,
            settlement_descriptions: raw.settlement_descriptions,
            settlements,
            report: raw.report,
        };
        let output = super::enrich(forest, Path::new(&root)).unwrap();
        assert_eq!(output.report.potential_vegetation_raster_files_read, 7);
        assert_eq!(output.report.potential_vegetation_samples, 6_041);
        assert_eq!(output.report.potential_vegetation_posterior_samples, 3_598);
        assert_eq!(output.report.potential_vegetation_categorical_samples, 0);
        assert_eq!(output.report.potential_vegetation_inferred_samples, 2_443);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&serde_json::to_vec(&output.sources).unwrap());
        for settlement in &output.settlements {
            hasher.update(settlement.forest.land.elevated.settlement.id.as_bytes());
            hasher.update(&[0]);
            match &settlement.potential_vegetation {
                PotentialVegetation::Posterior(p) => {
                    hasher.update(&[0]);
                    for value in [
                        p.woodland_and_forest,
                        p.heathland_and_shrub,
                        p.grassland,
                        p.sparsely_vegetated_areas,
                        p.wetlands,
                        p.marine_inlets_and_transitional_waters,
                    ] {
                        hasher.update(&value.get().to_le_bytes());
                    }
                }
                PotentialVegetation::Categorical(class) => {
                    hasher.update(&[1, class_code(*class)]);
                }
                PotentialVegetation::Inferred(class) => {
                    hasher.update(&[2, class_code(*class)]);
                }
            }
            hasher.update(
                settlement
                    .forest
                    .land
                    .elevated
                    .settlement
                    .sources
                    .as_bytes(),
            );
            hasher.update(&[0]);
        }
        let digest = hasher.finalize().to_hex().to_string();
        eprintln!("Jung PNV production digest {digest}");
        assert_eq!(
            digest,
            "021847d1766a15d22a794c32e4cf114a5d49c67da0730d143167b9ec1f31c79b"
        );
    }

    fn class_code(class: PotentialVegetationClass) -> u8 {
        match class {
            PotentialVegetationClass::WoodlandAndForest => 1,
            PotentialVegetationClass::HeathlandAndShrub => 2,
            PotentialVegetationClass::Grassland => 3,
            PotentialVegetationClass::SparselyVegetatedAreas => 4,
            PotentialVegetationClass::Wetlands => 5,
            PotentialVegetationClass::MarineInletsAndTransitionalWaters => 6,
        }
    }
}
