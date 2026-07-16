//! EU-Trees4F v2 current-climate habitat sampling.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CompiledWorld, HabitatSuitability, InferredTreeSpeciesProfile, ModeledTreeSpecies,
    ModeledTreeSpeciesProfile, NativeRangeEvidence, PotentialVegetation,
    PotentialVegetationFormation, SettlementImport, SourceProvenance, TreeSpeciesId,
    TreeSpeciesProfile, WORLD_SCHEMA_VERSION, WorldMetadata,
};
use proj4rs::{proj::Proj, transform::transform};
use tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};
use zip::ZipArchive;

use crate::{
    Error, Result,
    draft::{PotentialVegetationSettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "EU-Trees4F v2 current-climate ensemble";
const SOURCE_URL: &str = "https://doi.org/10.6084/m9.figshare.17032328";
const SOURCE_LICENSE: &str = "CC0 1.0";
const ARCHIVE_ROOT: &str = "ens_clim/";
const CURRENT_STEM: &str = "_ens-clim_cur2005_";
const EXPECTED_RASTERS: usize = 201;
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const NODATA: i16 = -32_768;

const EXPECTED_SPECIES: &[&str] = &[
    "Abies_alba",
    "Acer_campestre",
    "Acer_opalus",
    "Acer_platanoides",
    "Acer_pseudoplatanus",
    "Alnus_glutinosa",
    "Alnus_incana",
    "Arbutus_unedo",
    "Aria_edulis",
    "Betula_pendula",
    "Betula_pubescens",
    "Borkhausenia_intermedia",
    "Carpinus_betulus",
    "Carpinus_orientalis",
    "Castanea_sativa",
    "Celtis_australis",
    "Ceratonia_siliqua",
    "Cormus_domestica",
    "Corylus_avellana",
    "Cupressus_sempervirens",
    "Fagus_sylvatica",
    "Fraxinus_angustifolia",
    "Fraxinus_excelsior",
    "Fraxinus_ornus",
    "Juglans_regia",
    "Juniperus_thurifera",
    "Larix_decidua",
    "Laurus_nobilis",
    "Malus_sylvestris",
    "Olea_europaea",
    "Ostrya_carpinifolia",
    "Picea_abies",
    "Pinus_brutia",
    "Pinus_cembra",
    "Pinus_halepensis",
    "Pinus_nigra",
    "Pinus_pinaster",
    "Pinus_pinea",
    "Pinus_sylvestris",
    "Pistacia_lentiscus",
    "Pistacia_terebinthus",
    "Populus_alba",
    "Populus_nigra",
    "Populus_tremula",
    "Prunus_avium",
    "Prunus_padus",
    "Pyrus_communis",
    "Quercus_cerris",
    "Quercus_coccifera",
    "Quercus_faginea",
    "Quercus_frainetto",
    "Quercus_ilex",
    "Quercus_petraea",
    "Quercus_pubescens",
    "Quercus_pyrenaica",
    "Quercus_robur",
    "Quercus_suber",
    "Robinia_pseudoacacia",
    "Salix_alba",
    "Sorbus_aucuparia",
    "Taxus_baccata",
    "Tilia_cordata",
    "Tilia_platyphyllos",
    "Torminalis_glaberrima",
    "Ulmus_glabra",
    "Ulmus_laevis",
    "Ulmus_minor",
];

pub(crate) fn enrich(
    draft: WorldDraft<PotentialVegetationSettlementDraft>,
    archive_path: &Path,
) -> Result<CompiledWorld> {
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), 0, 0);
    }
    let file = File::open(archive_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::MissingSource(archive_path.into())
        } else {
            Error::Io(error)
        }
    })?;
    let mut archive = ZipArchive::new(file).map_err(|source| Error::Archive {
        path: archive_path.into(),
        source,
    })?;
    let manifest = ArchiveManifest::parse(&mut archive, archive_path)?;
    let projection = BinaryProjection::new()?;
    let projected = draft
        .settlements
        .iter()
        .map(|settlement| {
            let base = &settlement.forest.land.elevated.settlement;
            projection.project(base.latitude, base.longitude)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut candidates = vec![Vec::new(); draft.settlements.len()];
    let mut rasters_read = 0;
    for (species, paths) in manifest.species {
        let suitability = SignedRaster::read(
            read_entry(&mut archive, archive_path, &paths.probability)?,
            &paths.probability,
            RasterContract::Probability,
        )?;
        let potential = SignedRaster::read(
            read_entry(&mut archive, archive_path, &paths.potential)?,
            &paths.potential,
            RasterContract::Binary,
        )?;
        let native = SignedRaster::read(
            read_entry(&mut archive, archive_path, &paths.native)?,
            &paths.native,
            RasterContract::Binary,
        )?;
        rasters_read += 3;
        if potential.grid != native.grid {
            return Err(Error::Validation(format!(
                "EU-Trees4F potential/native grids disagree for {}",
                species.as_str()
            )));
        }
        for (index, settlement) in draft.settlements.iter().enumerate() {
            let base = &settlement.forest.land.elevated.settlement;
            let probability = suitability.sample(base.longitude, base.latitude);
            let potential_value = potential.sample(projected[index].0, projected[index].1);
            let native_value = native.sample(projected[index].0, projected[index].1);
            match (probability, potential_value, native_value) {
                (_, Some(0), Some(1)) => {
                    return Err(Error::Validation(format!(
                        "EU-Trees4F marks {} native but not potentially suitable at {}",
                        species.as_str(),
                        base.name
                    )));
                }
                (Some(score), Some(1), Some(native @ (0 | 1))) => {
                    let score = u16::try_from(score)
                        .ok()
                        .and_then(HabitatSuitability::new)
                        .ok_or_else(|| {
                            Error::Validation(format!(
                                "invalid EU-Trees4F suitability for {}",
                                species.as_str()
                            ))
                        })?;
                    candidates[index].push(ModeledTreeSpecies {
                        species: species.clone(),
                        suitability: score,
                        native_range: if native == 1 {
                            NativeRangeEvidence::WithinNativeRange
                        } else {
                            NativeRangeEvidence::OutsideNativeRange
                        },
                    });
                }
                (_, Some(0), Some(0)) | (_, None, _) | (_, _, None) => {}
                (None, Some(1), Some(_)) => {}
                (_, Some(other), _) => unreachable!("binary raster admitted {other}"),
            }
        }
    }

    let mut profiles = Vec::with_capacity(draft.settlements.len());
    let mut fallbacks = 0;
    for (settlement, mut modeled) in draft.settlements.iter().zip(candidates) {
        modeled.sort_by(|left, right| {
            right
                .suitability
                .cmp(&left.suitability)
                .then_with(|| left.species.cmp(&right.species))
        });
        modeled.truncate(adventuresim_world_schema::MAX_MODELED_TREE_SPECIES);
        let profile = if modeled.is_empty() {
            fallbacks += 1;
            TreeSpeciesProfile::Inferred(infer_species(&settlement.potential_vegetation))
        } else {
            TreeSpeciesProfile::Modeled(
                ModeledTreeSpeciesProfile::new(modeled)
                    .expect("archive contains one raster triplet per species"),
            )
        };
        profiles.push(profile);
    }
    finish(draft, profiles, rasters_read, fallbacks)
}

fn finish(
    mut draft: WorldDraft<PotentialVegetationSettlementDraft>,
    profiles: Vec<TreeSpeciesProfile>,
    rasters_read: usize,
    fallbacks: usize,
) -> Result<CompiledWorld> {
    if profiles.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "tree-species profiles do not match settlements".into(),
        ));
    }
    let candidate_count = profiles
        .iter()
        .map(|profile| match profile {
            TreeSpeciesProfile::Modeled(profile) => profile.candidates().len(),
            TreeSpeciesProfile::Inferred(profile) => profile.species().len(),
        })
        .sum();
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(profiles)
        .map(|(vegetated, tree_species)| {
            let forest = vegetated.forest;
            let land = forest.land;
            let elevated = land.elevated;
            let settlement = elevated.settlement;
            SettlementImport {
                id: settlement.id,
                source_node_id: settlement.source_node_id,
                name: settlement.name,
                longitude: settlement.longitude,
                latitude: settlement.latitude,
                population_level: settlement.population_level,
                population_estimate: settlement.population_estimate,
                elevation: elevated.elevation,
                land_use: land.land_use,
                forest_cover: forest.forest_cover,
                potential_vegetation: vegetated.potential_vegetation,
                tree_species,
                scene_key: settlement.scene_key,
                religion_id: settlement.religion_id,
            }
        })
        .collect::<Vec<_>>();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.tree_species_rasters_read = rasters_read;
    draft.report.tree_species_samples = settlements.len();
    draft.report.tree_species_fallback_samples = fallbacks;
    draft.report.tree_species_candidates = candidate_count;
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
enum Layer {
    Probability,
    Potential,
    Native,
}

struct ArchiveManifest {
    species: BTreeMap<TreeSpeciesId, SpeciesPaths>,
}

struct SpeciesPaths {
    probability: String,
    potential: String,
    native: String,
}

impl ArchiveManifest {
    fn parse<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &Path) -> Result<Self> {
        let mut found: BTreeMap<TreeSpeciesId, BTreeMap<Layer, String>> = BTreeMap::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|source| Error::Archive {
                path: path.into(),
                source,
            })?;
            let name = entry.name().to_owned();
            drop(entry);
            let Some((species, layer)) = parse_current_name(&name)? else {
                continue;
            };
            if found
                .entry(species)
                .or_default()
                .insert(layer, name.clone())
                .is_some()
            {
                return Err(Error::Validation(format!(
                    "duplicate EU-Trees4F current layer {name}"
                )));
            }
        }
        let expected = EXPECTED_SPECIES.iter().copied().collect::<BTreeSet<_>>();
        let actual = found
            .keys()
            .map(TreeSpeciesId::as_str)
            .collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(Error::Validation(
                "EU-Trees4F current species set does not match v2".into(),
            ));
        }
        let species = found
            .into_iter()
            .map(|(species, mut layers)| {
                let probability = layers.remove(&Layer::Probability);
                let potential = layers.remove(&Layer::Potential);
                let native = layers.remove(&Layer::Native);
                match (probability, potential, native, layers.is_empty()) {
                    (Some(probability), Some(potential), Some(native), true) => Ok((
                        species,
                        SpeciesPaths {
                            probability,
                            potential,
                            native,
                        },
                    )),
                    _ => Err(Error::Validation(format!(
                        "incomplete EU-Trees4F current triplet for {}",
                        species.as_str()
                    ))),
                }
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        if species.len() * 3 != EXPECTED_RASTERS {
            return Err(Error::Validation(
                "EU-Trees4F v2 must contain 201 current rasters".into(),
            ));
        }
        Ok(Self { species })
    }
}

fn parse_current_name(name: &str) -> Result<Option<(TreeSpeciesId, Layer)>> {
    let Some(relative) = name.strip_prefix(ARCHIVE_ROOT) else {
        return Ok(None);
    };
    let Some((species, suffix)) = relative.split_once(CURRENT_STEM) else {
        return Ok(None);
    };
    let layer = match suffix {
        "prob_pot.tif" => Layer::Probability,
        "bin_pot.tif" => Layer::Potential,
        "bin_nat.tif" => Layer::Native,
        _ => return Ok(None),
    };
    let species = TreeSpeciesId::new(species.to_owned()).ok_or_else(|| Error::InvalidField {
        path: PathBuf::from(name),
        field: "archive filename",
        value: species.into(),
        message: "invalid scientific name in current raster filename".into(),
    })?;
    Ok(Some((species, layer)))
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    name: &str,
) -> Result<Vec<u8>> {
    let entry = archive.by_name(name).map_err(|source| Error::Archive {
        path: archive_path.into(),
        source,
    })?;
    if entry.size() > MAX_ENTRY_BYTES {
        return Err(Error::Validation(format!(
            "EU-Trees4F archive entry {name} is implausibly large"
        )));
    }
    let declared_size = entry.size();
    let mut bytes = Vec::with_capacity(declared_size as usize);
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != declared_size || bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(Error::Validation(format!(
            "EU-Trees4F archive entry {name} has an invalid decoded size"
        )));
    }
    Ok(bytes)
}

#[derive(Clone, Copy)]
enum RasterContract {
    Probability,
    Binary,
}

impl RasterContract {
    fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Probability => (828, 540),
            Self::Binary => (400, 410),
        }
    }

    fn grid(self) -> AreaGrid {
        match self {
            Self::Probability => AreaGrid {
                west: -19.0,
                north: 72.0,
                x_scale: 1.0 / 12.0,
                y_scale: 1.0 / 12.0,
            },
            Self::Binary => AreaGrid {
                west: 2_600_000.0,
                north: 5_500_000.0,
                x_scale: 10_000.0,
                y_scale: 10_000.0,
            },
        }
    }

    fn epsg(self) -> (u16, u16) {
        match self {
            Self::Probability => (2, 4_326),
            Self::Binary => (1, 3_035),
        }
    }
}

struct SignedRaster {
    width: u32,
    height: u32,
    grid: AreaGrid,
    pixels: Vec<i16>,
}

impl SignedRaster {
    fn read(bytes: Vec<u8>, name: &str, contract: RasterContract) -> Result<Self> {
        let path = Path::new(name);
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 2 * 1024 * 1024;
        limits.intermediate_buffer_size = 2 * 1024 * 1024;
        limits.ifd_value_size = 64 * 1024;
        let mut decoder = Decoder::new(Cursor::new(bytes))
            .map_err(|source| Error::Tiff {
                path: path.into(),
                source,
            })?
            .with_limits(limits);
        let (width, height) = decoder.dimensions().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        if (width, height) != contract.dimensions() {
            return Err(Error::Validation(format!(
                "{name} is {width}x{height}; unexpected EU-Trees4F grid"
            )));
        }
        if tag(decoder.colortype(), path)? != ColorType::Gray(16) {
            return Err(Error::Validation(format!(
                "{name} is not a single-band 16-bit grayscale raster"
            )));
        }
        let grid = AreaGrid::parse(&mut decoder, path, contract)?;
        let DecodingResult::I16(pixels) = decoder.read_image().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?
        else {
            return Err(Error::Validation(format!(
                "{name} is not a signed Int16 raster"
            )));
        };
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| Error::Validation("EU-Trees4F dimensions overflow".into()))?;
        if pixels.len() != expected {
            return Err(Error::Validation(format!("{name} is not single-channel")));
        }
        if decoder.more_images() {
            return Err(Error::Validation(format!(
                "{name} contains more than one TIFF image"
            )));
        }
        for &value in &pixels {
            if value == NODATA {
                continue;
            }
            let valid = match contract {
                RasterContract::Probability => (0..=1_000).contains(&value),
                RasterContract::Binary => matches!(value, 0 | 1),
            };
            if !valid {
                return Err(Error::InvalidField {
                    path: path.into(),
                    field: "raster cell",
                    value: value.to_string(),
                    message: "value is outside the EU-Trees4F layer domain".into(),
                });
            }
        }
        Ok(Self {
            width,
            height,
            grid,
            pixels,
        })
    }

    fn sample(&self, x: f64, y: f64) -> Option<i16> {
        let (column, row) = self.grid.pixel(x, y, self.width, self.height)?;
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
        decoder: &mut Decoder<impl Read + Seek>,
        path: &Path,
        contract: RasterContract,
    ) -> Result<Self> {
        let scale = tag(decoder.get_tag_f64_vec(Tag::ModelPixelScaleTag), path)?;
        let tie = tag(decoder.get_tag_f64_vec(Tag::ModelTiepointTag), path)?;
        let keys = tag(decoder.get_tag_u16_vec(Tag::GeoKeyDirectoryTag), path)?;
        let nodata = tag(decoder.get_tag_ascii_string(Tag::Unknown(42_113)), path)?;
        let (model_type, epsg) = contract.epsg();
        let crs_key = if model_type == 2 { 2_048 } else { 3_072 };
        let unit_key = if model_type == 2 { 2_054 } else { 3_076 };
        let unit = if model_type == 2 { 9_102 } else { 9_001 };
        if scale.len() != 3
            || tie.len() != 6
            || geo_key(&keys, 1_024) != Some(model_type)
            || geo_key(&keys, 1_025) != Some(1)
            || geo_key(&keys, crs_key) != Some(epsg)
            || geo_key(&keys, unit_key) != Some(unit)
            || nodata.trim_matches('\0').trim() != "-32768"
        {
            return Err(Error::Validation(format!(
                "{} does not match the EU-Trees4F GeoTIFF contract",
                path.display()
            )));
        }
        let values = [scale[0], scale[1], tie[0], tie[1], tie[3], tie[4]];
        if !values.iter().all(|value| value.is_finite())
            || scale[0] <= 0.0
            || scale[1] <= 0.0
            || scale[2] != 0.0
            || tie[2] != 0.0
            || tie[5] != 0.0
        {
            return Err(Error::Validation(format!(
                "{} has invalid georeferencing",
                path.display()
            )));
        }
        let parsed = Self {
            west: tie[3] - tie[0] * scale[0],
            north: tie[4] + tie[1] * scale[1],
            x_scale: scale[0],
            y_scale: scale[1],
        };
        let expected = contract.grid();
        let epsilon = 1e-8;
        if (parsed.west - expected.west).abs() > epsilon
            || (parsed.north - expected.north).abs() > epsilon
            || (parsed.x_scale - expected.x_scale).abs() > epsilon
            || (parsed.y_scale - expected.y_scale).abs() > epsilon
        {
            return Err(Error::Validation(format!(
                "{} has an unexpected EU-Trees4F transform",
                path.display()
            )));
        }
        Ok(parsed)
    }

    fn pixel(self, x: f64, y: f64, width: u32, height: u32) -> Option<(u32, u32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let column = ((x - self.west) / self.x_scale).floor();
        let row = ((self.north - y) / self.y_scale).floor();
        (column >= 0.0 && row >= 0.0 && column < f64::from(width) && row < f64::from(height))
            .then_some((column as u32, row as u32))
    }
}

struct BinaryProjection {
    geographic: Proj,
    projected: Proj,
}

impl BinaryProjection {
    fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=laea +lat_0=52 +lon_0=10 +x_0=4321000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs +type=crs",
            )?,
        })
    }

    fn project(&self, latitude: f64, longitude: f64) -> Result<(f64, f64)> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "invalid coordinate ({latitude}, {longitude}) for EU-Trees4F"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        if !coordinate.0.is_finite() || !coordinate.1.is_finite() {
            return Err(Error::Validation(
                "EU-Trees4F projection produced a non-finite coordinate".into(),
            ));
        }
        Ok((coordinate.0, coordinate.1))
    }
}

fn infer_species(vegetation: &PotentialVegetation) -> InferredTreeSpeciesProfile {
    use PotentialVegetationFormation as F;
    let formation = match vegetation {
        PotentialVegetation::Mapped(mapped) => mapped.formation(),
        PotentialVegetation::Inferred(formation) => *formation,
    };
    let names: &[&str] = match formation {
        F::PolarDesertAndNival | F::TundraAndAlpine => &["Betula_pubescens", "Pinus_sylvestris"],
        F::OpenWoodlandAndSubalpine | F::ConiferousAndMixedForest => {
            &["Abies_alba", "Picea_abies", "Pinus_sylvestris"]
        }
        F::Heath => &["Betula_pendula", "Pinus_sylvestris"],
        F::DeciduousAndMixedForest => &["Fagus_sylvatica", "Quercus_robur", "Tilia_cordata"],
        F::ThermophilousBroadleafForest => &["Carpinus_orientalis", "Quercus_pubescens"],
        F::HygroThermophilousBroadleafForest => &["Alnus_glutinosa", "Fraxinus_angustifolia"],
        F::MediterraneanSclerophyll => &["Olea_europaea", "Pinus_halepensis", "Quercus_ilex"],
        F::XerophyticConiferAndScrub | F::Oroxerophytic => &["Juniperus_thurifera", "Pinus_nigra"],
        F::ForestSteppe | F::Steppe => &["Pinus_sylvestris", "Quercus_pubescens"],
        F::Desert => &["Juniperus_thurifera"],
        F::CoastalAndHalophytic => &["Pinus_pinea", "Quercus_ilex"],
        F::AquaticAndReed | F::Mire => &["Alnus_glutinosa", "Salix_alba"],
        F::SwampAndFenForest | F::FloodplainAndWetland => {
            &["Alnus_glutinosa", "Populus_nigra", "Salix_alba"]
        }
    };
    InferredTreeSpeciesProfile::new(
        names
            .iter()
            .map(|name| TreeSpeciesId::new(*name).expect("fallback species names are valid"))
            .collect(),
    )
    .expect("each formation has a non-empty unique fallback profile")
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
    use std::{fs::File, path::Path};

    use adventuresim_world_schema::{
        ElevationMeters, ForestCover, LandUseFraction, LandUseProfile, PotentialVegetation,
        PotentialVegetationFormation, TreeSpeciesProfile,
    };
    use zip::ZipArchive;

    use crate::draft::{
        ElevatedSettlementDraft, ForestSettlementDraft, LandUseSettlementDraft,
        PotentialVegetationSettlementDraft, WorldDraft,
    };

    use super::{
        ArchiveManifest, AreaGrid, BinaryProjection, EXPECTED_RASTERS, EXPECTED_SPECIES, Layer,
        RasterContract, SignedRaster, parse_current_name, read_entry,
    };

    #[test]
    fn filenames_parse_as_complete_source_identifiers() {
        let (species, layer) =
            parse_current_name("ens_clim/Borkhausenia_intermedia_ens-clim_cur2005_prob_pot.tif")
                .unwrap()
                .unwrap();
        assert_eq!(species.as_str(), "Borkhausenia_intermedia");
        assert_eq!(layer, Layer::Probability);
        assert!(
            parse_current_name("ens_clim/Abies_alba_ens-clim_rcp45_2035_prob_pot.tif")
                .unwrap()
                .is_none()
        );
        assert_eq!(EXPECTED_SPECIES.len(), 67);
    }

    #[test]
    fn pixel_is_area_boundaries_are_right_and_bottom_exclusive() {
        let grid = AreaGrid {
            west: -19.0,
            north: 72.0,
            x_scale: 1.0,
            y_scale: 1.0,
        };
        assert_eq!(grid.pixel(-19.0, 72.0, 2, 2), Some((0, 0)));
        assert_eq!(grid.pixel(-17.000_001, 70.000_001, 2, 2), Some((1, 1)));
        assert_eq!(grid.pixel(-17.0, 71.0, 2, 2), None);
        assert_eq!(grid.pixel(-18.0, 70.0, 2, 2), None);
    }

    #[test]
    fn binary_projection_matches_epsg_3035_oracles() {
        let projection = BinaryProjection::new().unwrap();
        let origin = projection.project(52.0, 10.0).unwrap();
        assert!((origin.0 - 4_321_000.0).abs() < 0.001);
        assert!((origin.1 - 3_210_000.0).abs() < 0.001);
        let berlin = projection.project(52.52, 13.405).unwrap();
        assert!((berlin.0 - 4_552_036.45).abs() < 1.0);
        assert!((berlin.1 - 3_273_269.25).abs() < 1.0);
    }

    #[test]
    #[ignore = "requires the official archive in EU_TREES4F_ARCHIVE"]
    fn full_source_boundary_reads_all_current_rasters() {
        let path = std::env::var_os("EU_TREES4F_ARCHIVE").expect("set EU_TREES4F_ARCHIVE");
        let path = Path::new(&path);
        let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
        let manifest = ArchiveManifest::parse(&mut archive, path).unwrap();
        let projection = BinaryProjection::new().unwrap();
        let graz = projection.project(47.076_671_6, 15.421_369_6).unwrap();
        let mut rasters = 0;
        let mut plausible_at_graz = 0;
        for (_, paths) in manifest.species {
            let probability = SignedRaster::read(
                read_entry(&mut archive, path, &paths.probability).unwrap(),
                &paths.probability,
                RasterContract::Probability,
            )
            .unwrap();
            let potential = SignedRaster::read(
                read_entry(&mut archive, path, &paths.potential).unwrap(),
                &paths.potential,
                RasterContract::Binary,
            )
            .unwrap();
            let native = SignedRaster::read(
                read_entry(&mut archive, path, &paths.native).unwrap(),
                &paths.native,
                RasterContract::Binary,
            )
            .unwrap();
            rasters += 3;
            let score = probability.sample(15.421_369_6, 47.076_671_6);
            let potential = potential.sample(graz.0, graz.1);
            let native = native.sample(graz.0, graz.1);
            assert!(!matches!((potential, native), (Some(0), Some(1))));
            plausible_at_graz += usize::from(score.is_some() && potential == Some(1));
        }
        assert_eq!(rasters, EXPECTED_RASTERS);
        assert!(plausible_at_graz > 0);
    }

    #[test]
    #[ignore = "requires EU_TREES4F_ARCHIVE and VIABUNDUS_DIR"]
    fn full_stage_enriches_all_viabundus_settlements() {
        let viabundus = std::env::var_os("VIABUNDUS_DIR").expect("set VIABUNDUS_DIR");
        let archive = std::env::var_os("EU_TREES4F_ARCHIVE").expect("set EU_TREES4F_ARCHIVE");
        let mut raw = crate::sources::viabundus::compile(Path::new(&viabundus), 1544).unwrap();
        let settlements = std::mem::take(&mut raw.settlements)
            .into_iter()
            .map(|settlement| PotentialVegetationSettlementDraft {
                forest: ForestSettlementDraft {
                    land: LandUseSettlementDraft {
                        elevated: ElevatedSettlementDraft {
                            settlement,
                            elevation: ElevationMeters::new(100).unwrap(),
                        },
                        land_use: LandUseProfile::new(
                            LandUseFraction::new(2_000).unwrap(),
                            LandUseFraction::new(2_000).unwrap(),
                            LandUseFraction::new(100).unwrap(),
                            LandUseFraction::new(5_900).unwrap(),
                        )
                        .unwrap(),
                    },
                    forest_cover: ForestCover::Open,
                },
                potential_vegetation: PotentialVegetation::Inferred(
                    PotentialVegetationFormation::DeciduousAndMixedForest,
                ),
            })
            .collect();
        let draft = WorldDraft {
            year: raw.year,
            sources: raw.sources,
            road_types: raw.road_types,
            nodes: raw.nodes,
            edges: raw.edges,
            settlements,
            report: raw.report,
        };
        let world = super::enrich(draft, Path::new(&archive)).unwrap();
        let modeled = world
            .settlements
            .iter()
            .filter(|settlement| matches!(settlement.tree_species, TreeSpeciesProfile::Modeled(_)))
            .count();
        eprintln!(
            "EU-Trees4F modeled {modeled}/{} settlements with {} candidates",
            world.settlements.len(),
            world.report.tree_species_candidates
        );
        assert_eq!(world.report.tree_species_rasters_read, EXPECTED_RASTERS);
        assert_eq!(world.report.tree_species_samples, world.settlements.len());
        assert!(modeled > world.settlements.len() * 9 / 10);
    }
}
