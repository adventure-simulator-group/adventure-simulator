//! SoilGrids rolling-v2 prepared-subset prediction and final soil synthesis.
//!
//! Raw WebDAV/VRT assets are never opened at runtime. `init_soilgrids.py`
//! prepares bounded EPSG:3035 GeoTIFFs and a content-addressed manifest.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CationExchangeCapacity, MineralSoil,
    MineralSoilTexture, OrganicSoil, PotentialVegetationClass, RockOutcropSoil, SoilAcidity,
    SoilBasisPoints, SoilEvidence, SoilFertility, SoilPrediction, SoilProfile, SoilProperties,
    SoilSubstrate, SoilWaterRegime, SourceProvenance, StoneContentPercent, SurfaceGeology,
    SurfaceLithology, TopsoilOrganicCarbon, WrbReferenceGroup,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

use crate::{
    Error, Result,
    draft::{
        FinalizedSoilSettlementDraft, FinalizedSoilWorldDraft, HydrologyWorldDraft,
        SoilPredictionSettlementDraft, TreeSpeciesSettlementDraft, WorldDraft, push_source_note,
    },
    spatial::SpatialProjection,
};

const SOURCE_NAME: &str = "ISRIC SoilGrids rolling version 2";
const SOURCE_URL: &str = "https://www.isric.org/explore/soilgrids";
const SOURCE_LICENSE: &str = "CC BY 4.0";
const MANIFEST: &str = "soilgrids-manifest.json";
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RASTER_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 32_000_000;
const PREPARED_EXTENT: (i64, i64, i64, i64) = (900_000, 900_000, 7_400_000, 5_500_000);
const DEPTHS: [(&str, f64); 6] = [
    ("0-5cm", 50.0),
    ("5-15cm", 100.0),
    ("15-30cm", 150.0),
    ("30-60cm", 300.0),
    ("60-100cm", 400.0),
    ("100-200cm", 1_000.0),
];
const QUANTILES: [&str; 4] = ["Q0.05", "Q0.50", "mean", "Q0.95"];
const PROPERTIES: [&str; 11] = [
    "sand", "silt", "clay", "cfvo", "soc", "phh2o", "cec", "bdod", "wv0033", "wv1500", "wrb",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    source: String,
    source_version: String,
    source_reproducibility: String,
    retrieved_at: String,
    generation: String,
    crs: String,
    origin_easting_meters: i64,
    origin_northing_meters: i64,
    cell_size_meters: u32,
    west: i64,
    south: i64,
    east: i64,
    north: i64,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    property: String,
    depth: String,
    quantile: String,
    unit: String,
    filename: String,
    source_url: String,
    source_observation_size: u64,
    source_observation_sha256: String,
    source_observation_etag: Option<String>,
    source_observation_last_modified: Option<String>,
    prepared_size: u64,
    prepared_sha256: String,
}

pub(crate) fn predict(
    draft: WorldDraft<TreeSpeciesSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<SoilPredictionSettlementDraft>> {
    let manifest = validate_manifest(directory, draft.spatial_grid.cell_size_meters().get())?;
    if draft.settlements.is_empty() {
        return finish_predictions(draft, vec![], manifest.files.len(), 0);
    }
    let projection = SpatialProjection::new()?;
    let points = draft
        .settlements
        .iter()
        .map(|s| {
            let base = &s.vegetated.forest.land.elevated.settlement;
            projection.project(base.latitude, base.longitude)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut values: Vec<BTreeMap<(String, String, String), Option<f64>>> =
        vec![BTreeMap::new(); points.len()];
    for entry in &manifest.files {
        let raster = PreparedRaster::open(
            &directory
                .join("generations")
                .join(&manifest.generation)
                .join(&entry.filename),
            &manifest,
        )?;
        for (sample, point) in values.iter_mut().zip(&points) {
            sample.insert(
                (
                    entry.property.clone(),
                    entry.depth.clone(),
                    entry.quantile.clone(),
                ),
                raster.sample(point.easting_meters(), point.northing_meters())?,
            );
        }
    }
    let mut inferred = 0;
    let predictions = values
        .iter()
        .zip(&draft.settlements)
        .map(
            |(sample, settlement)| match prediction_from_sample(sample) {
                Ok(Some(value)) => Ok(value),
                Ok(None) => {
                    inferred += 1;
                    Ok(infer_prediction(settlement))
                }
                Err(error) => Err(error),
            },
        )
        .collect::<Result<Vec<_>>>()?;
    finish_predictions(draft, predictions, manifest.files.len(), inferred)
}

fn finish_predictions(
    mut draft: WorldDraft<TreeSpeciesSettlementDraft>,
    predictions: Vec<SoilPrediction>,
    rasters: usize,
    inferred: usize,
) -> Result<WorldDraft<SoilPredictionSettlementDraft>> {
    if predictions.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "soil predictions do not match settlements".into(),
        ));
    }
    let settlements: Vec<_> = std::mem::take(&mut draft.settlements).into_iter().zip(predictions).map(|(mut trees, prediction)| {
        push_source_note(&mut trees, if prediction.evidence == SoilEvidence::SoilGridsPrediction {
            "**[ISRIC SoilGrids](https://www.isric.org/explore/soilgrids):** Prepared rolling-v2 depth and uncertainty layers provide the soil prediction; geology and hydrology finalize it later."
        } else {
            "**Soil prediction fallback:** Prepared SoilGrids coverage is incomplete; a deterministic vegetation/elevation prediction is carried to finalization."
        });
        SoilPredictionSettlementDraft { trees, prediction }
    }).collect();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.soil_rasters_read = rasters;
    draft.report.soil_depth_layers_read = rasters.saturating_sub(3);
    draft.report.soil_samples = settlements.len();
    draft.report.soil_fallback_samples = inferred;
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

/// Resolve the prediction against geology, hydrology and Jung wetland evidence.
pub(crate) fn finalize(draft: HydrologyWorldDraft) -> Result<FinalizedSoilWorldDraft> {
    let settlements = draft.settlements.into_iter().map(|mut wet| {
        push_source_note(&mut wet, "**Soil finalizer v3:** SoilGrids prediction is deterministically resolved with geology, Jung wetland evidence, elevation, and EU-Hydro context; slope/roughness refinement is deferred.");
        let drought = wet.drought;
        let religious = drought.religious;
        let geologic = religious.geologic;
        let predicted = geologic.predicted;
        let trees = predicted.trees;
        let vegetated = trees.vegetated;
        let forest = vegetated.forest;
        let land = forest.land;
        let elevated = land.elevated;
        let settlement = elevated.settlement;
        let soil = finalize_prediction(predicted.prediction, &geologic.geology, wet.hydrology, vegetated.potential_vegetation.class(), elevated.elevation.get());
        // Keep the complete nested evidence chain for the post-soil synthesis stage.
        let wet = crate::draft::HydrologySettlementDraft {
            drought: crate::draft::DroughtSettlementDraft {
                religious: crate::draft::ReligionSettlementDraft {
                    geologic: crate::draft::GeologySettlementDraft {
                        predicted: crate::draft::SoilPredictionSettlementDraft {
                            trees: crate::draft::TreeSpeciesSettlementDraft {
                                vegetated: crate::draft::PotentialVegetationSettlementDraft {
                                    forest: crate::draft::ForestSettlementDraft {
                                        land: crate::draft::LandUseSettlementDraft {
                                            elevated: crate::draft::ElevatedSettlementDraft {
                                                settlement,
                                                elevation: elevated.elevation,
                                            },
                                            land_use: land.land_use,
                                            evidence: land.evidence,
                                        },
                                        forest_cover: forest.forest_cover,
                                    },
                                    potential_vegetation: vegetated.potential_vegetation,
                                },
                                tree_species: trees.tree_species,
                            },
                            prediction: predicted.prediction,
                        },
                        geology: geologic.geology,
                    },
                    religious_status: religious.religious_status,
                },
                drought: drought.drought,
            },
            hydrology: wet.hydrology,
        };
        Ok(FinalizedSoilSettlementDraft { hydrologic: wet, soil })
    }).collect::<Result<Vec<_>>>()?;
    Ok(FinalizedSoilWorldDraft {
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

fn finalize_prediction(
    p: SoilPrediction,
    geology: &SurfaceGeology,
    hydrology: adventuresim_world_schema::SettlementHydrology,
    vegetation: PotentialVegetationClass,
    elevation: i16,
) -> SoilProfile {
    let parent_material = match geology {
        SurfaceGeology::Mapped(v) => match v.setting.lithology {
            adventuresim_world_schema::GeologicLithologyEvidence::Mapped(x)
            | adventuresim_world_schema::GeologicLithologyEvidence::Inferred(x) => x,
        },
        SurfaceGeology::Inferred(v) => v.lithology,
    };
    let freshwater = hydrology.has_freshwater();
    let tidal = matches!(
        hydrology.marine,
        Some(adventuresim_world_schema::MarineWaterAccess::Tidal(_))
    );
    let source_wet = freshwater || tidal;
    let jung_wet = vegetation == PotentialVegetationClass::Wetlands;
    let wrb_wet = matches!(
        p.wrb_group,
        WrbReferenceGroup::Histosol
            | WrbReferenceGroup::Gleysol
            | WrbReferenceGroup::Stagnosol
            | WrbReferenceGroup::Fluvisol
    );
    let peat = p.histosol_probability.get() >= 5_000
        && vegetation == PotentialVegetationClass::Wetlands
        && source_wet;
    let shallow = p.leptosol_probability.get() >= 5_000
        || matches!(
            parent_material,
            SurfaceLithology::Igneous(_) | SurfaceLithology::Metamorphic(_)
        ) && elevation >= 900;
    let stones = p.stones;
    let substrate = if peat {
        SoilSubstrate::Organic(OrganicSoil {
            depth: adventuresim_world_schema::SoilDepth::Deep,
            available_water: p.available_water,
            stones,
        })
    } else if shallow {
        SoilSubstrate::RockOutcrop(RockOutcropSoil { stones })
    } else {
        SoilSubstrate::Mineral(MineralSoil {
            texture: p.texture,
            depth: adventuresim_world_schema::SoilDepth::Deep,
            available_water: p.available_water,
            organic_carbon: p.organic_carbon,
            stones,
        })
    };
    let drained = wrb_wet && !source_wet && !jung_wet;
    let usually_dry = drained
        || matches!(
            p.available_water,
            AvailableWaterCapacity::VeryLow | AvailableWaterCapacity::Low
        );
    let flooded = source_wet && (wrb_wet || jung_wet);
    SoilProfile {
        wrb_group: p.wrb_group,
        parent_material,
        properties: SoilProperties {
            substrate,
            water_regime: if peat {
                SoilWaterRegime::PermanentlyWet
            } else if flooded {
                SoilWaterRegime::LongSeasonWet
            } else if usually_dry {
                SoilWaterRegime::UsuallyDry
            } else {
                SoilWaterRegime::SeasonallyWet
            },
            agricultural_limitation: if peat {
                AgriculturalLimitation::ShallowWaterTable
            } else if shallow {
                AgriculturalLimitation::ShallowRock
            } else if flooded {
                AgriculturalLimitation::Flooded
            } else if drained {
                AgriculturalLimitation::Drained
            } else if p.stones.percent() >= 35 {
                AgriculturalLimitation::Stony
            } else {
                AgriculturalLimitation::None
            },
        },
        acidity: p.acidity,
        cation_exchange_capacity: p.cation_exchange_capacity,
        fertility: p.fertility,
        confidence: p.confidence,
        evidence: p.evidence,
    }
}

fn prediction_from_sample(
    sample: &BTreeMap<(String, String, String), Option<f64>>,
) -> Result<Option<SoilPrediction>> {
    let q = |prop: &str, depth: &str, quantile: &str| {
        sample
            .get(&(prop.into(), depth.into(), quantile.into()))
            .copied()
            .flatten()
    };
    let Some(wrb) = q("wrb", "surface", "most-probable") else {
        return Ok(None);
    };
    let wrb_group = wrb_from_code(wrb)?;
    let hist = probability(q("wrb", "surface", "Histosols-probability").unwrap_or(0.0))?;
    let lept = probability(q("wrb", "surface", "Leptosols-probability").unwrap_or(0.0))?;
    let mut summary = BTreeMap::new();
    for property in PROPERTIES.iter().take(10) {
        for quantile in QUANTILES {
            let mut total = 0.0;
            let mut weight = 0.0;
            for (depth, thickness) in
                DEPTHS
                    .iter()
                    .take(if matches!(*property, "wv0033" | "wv1500") {
                        5
                    } else {
                        3
                    })
            {
                if let Some(value) = q(property, depth, quantile) {
                    total += value * thickness;
                    weight += thickness;
                }
            }
            if weight > 0.0 {
                summary.insert((*property, quantile), total / weight);
            }
        }
    }
    for property in PROPERTIES.iter().take(10) {
        if let (Some(a), Some(b), Some(c), Some(d)) = (
            summary.get(&(*property, "Q0.05")),
            summary.get(&(*property, "Q0.50")),
            summary.get(&(*property, "mean")),
            summary.get(&(*property, "Q0.95")),
        ) {
            if !(a <= b && b <= d && a <= c && c <= d) {
                return Err(Error::Validation(format!(
                    "SoilGrids quantiles are unordered for {property}"
                )));
            }
        }
    }
    let mean = |p| summary.get(&(p, "mean")).copied();
    let (
        Some(sand),
        Some(silt),
        Some(clay),
        Some(stones),
        Some(soc),
        Some(ph),
        Some(cec),
        Some(w33),
        Some(w1500),
    ) = (
        mean("sand"),
        mean("silt"),
        mean("clay"),
        mean("cfvo"),
        mean("soc"),
        mean("phh2o"),
        mean("cec"),
        mean("wv0033"),
        mean("wv1500"),
    )
    else {
        return Ok(None);
    };
    let sum = sand + silt + clay;
    if !(900.0..=1100.0).contains(&sum) || w33 < w1500 {
        return Err(Error::Validation(
            "invalid SoilGrids texture sum or water quantiles".into(),
        ));
    }
    let texture = texture(sand / sum, silt / sum, clay / sum);
    let aw = (w33 - w1500) * 1000.0 / 1000.0;
    Ok(Some(SoilPrediction {
        wrb_group,
        histosol_probability: hist,
        leptosol_probability: lept,
        texture,
        available_water: water_class(aw),
        organic_carbon: carbon_class(soc / 10.0),
        stones: StoneContentPercent::new((stones / 10.0).round().clamp(0.0, 100.0) as u8).unwrap(),
        acidity: acidity(ph / 10.0),
        cation_exchange_capacity: cec_class(cec / 10.0),
        fertility: fertility(cec / 10.0, soc / 10.0, ph / 10.0),
        confidence: uncertainty(
            summary.get(&("clay", "Q0.05")).copied(),
            summary.get(&("clay", "Q0.95")).copied(),
        ),
        evidence: SoilEvidence::SoilGridsPrediction,
    }))
}

fn infer_prediction(s: &TreeSpeciesSettlementDraft) -> SoilPrediction {
    let wet = s.vegetated.potential_vegetation.class() == PotentialVegetationClass::Wetlands;
    let high = s.vegetated.forest.land.elevated.elevation.get() >= 900;
    SoilPrediction {
        wrb_group: if wet {
            WrbReferenceGroup::Histosol
        } else if high {
            WrbReferenceGroup::Leptosol
        } else {
            WrbReferenceGroup::Cambisol
        },
        histosol_probability: bp(if wet { 7000 } else { 500 }),
        leptosol_probability: bp(if high { 7000 } else { 1000 }),
        texture: MineralSoilTexture::Medium,
        available_water: if wet {
            AvailableWaterCapacity::VeryHigh
        } else {
            AvailableWaterCapacity::Medium
        },
        organic_carbon: if wet {
            TopsoilOrganicCarbon::High
        } else {
            TopsoilOrganicCarbon::Medium
        },
        stones: StoneContentPercent::new(if high { 45 } else { 10 }).unwrap(),
        acidity: SoilAcidity::Acid,
        cation_exchange_capacity: CationExchangeCapacity::Medium,
        fertility: SoilFertility::Medium,
        confidence: bp(2500),
        evidence: SoilEvidence::DeterministicInference,
    }
}

fn validate_manifest(directory: &Path, cell_size: u32) -> Result<Manifest> {
    let path = directory.join(MANIFEST);
    let metadata = fs::metadata(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::MissingSource(path.clone())
        } else {
            e.into()
        }
    })?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Validation(
            "SoilGrids manifest exceeds size limit".into(),
        ));
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(&path)?).map_err(|source| Error::JsonSource {
            path: path.clone(),
            source,
        })?;
    if manifest.schema != 1
        || manifest.source != "ISRIC SoilGrids rolling-v2"
        || manifest.source_reproducibility != "unpinned-rolling-latest"
        || manifest.crs != "EPSG:3035"
        || manifest.origin_easting_meters != 0
        || manifest.origin_northing_meters != 0
        || manifest.cell_size_meters != cell_size
        || manifest.west >= manifest.east
        || manifest.south >= manifest.north
        || manifest.retrieved_at.trim().is_empty()
        || manifest.source_version.trim().is_empty()
        || !valid_hash(&manifest.generation)
        || manifest.files.len() != 207
    {
        return Err(Error::Validation(
            "SoilGrids manifest source/grid mismatch".into(),
        ));
    }
    validate_prepared_extent(
        manifest.west,
        manifest.south,
        manifest.east,
        manifest.north,
        cell_size,
    )?;
    let width = manifest
        .east
        .checked_sub(manifest.west)
        .ok_or_else(|| Error::Validation("SoilGrids extent overflow".into()))?;
    let height = manifest
        .north
        .checked_sub(manifest.south)
        .ok_or_else(|| Error::Validation("SoilGrids extent overflow".into()))?;
    let cell = i64::from(cell_size);
    if [manifest.west, manifest.south, manifest.east, manifest.north]
        .iter()
        .any(|value| value.rem_euclid(cell) != 0)
    {
        return Err(Error::Validation(
            "SoilGrids extent is shifted off the zero-origin grid".into(),
        ));
    }
    if width <= 0 || height <= 0 || width % cell != 0 || height % cell != 0 {
        return Err(Error::Validation(
            "SoilGrids extent is not exactly divisible by cell size".into(),
        ));
    }
    let pixels = u64::try_from(width / cell)
        .ok()
        .and_then(|w| {
            u64::try_from(height / cell)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or_else(|| Error::Validation("SoilGrids raster dimensions overflow".into()))?;
    if pixels > MAX_PIXELS {
        return Err(Error::Validation(
            "SoilGrids preparation exceeds importer pixel bound".into(),
        ));
    }
    let generation_dir = directory.join("generations").join(&manifest.generation);
    let canonical_root = fs::canonicalize(&generation_dir)?;
    let canonical_generations = fs::canonicalize(directory.join("generations"))?;
    if canonical_root.parent() != Some(canonical_generations.as_path()) {
        return Err(Error::Validation(
            "SoilGrids generation escapes source directory".into(),
        ));
    }
    let mut keys: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut filenames = BTreeSet::new();
    for file in &manifest.files {
        if !safe_filename(&file.filename)
            || !safe_source_url(&file.source_url)
            || file.source_observation_size == 0
            || file.prepared_size == 0
            || file
                .source_observation_etag
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 512 || v.contains('\0'))
            || file
                .source_observation_last_modified
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 128 || v.contains('\0'))
            || !valid_hash(&file.source_observation_sha256)
            || !valid_hash(&file.prepared_sha256)
        {
            return Err(Error::Validation(
                "invalid SoilGrids file manifest entry".into(),
            ));
        }
        let expected_filename = if file.property == "wrb" {
            format!("wrb_{}.tif", file.quantile)
        } else {
            format!("{}_{}_{}.tif", file.property, file.depth, file.quantile)
        };
        if file.filename != expected_filename
            || canonical_source_url(&file.property, &file.depth, &file.quantile).as_deref()
                != Some(file.source_url.as_str())
            || !filenames.insert(file.filename.clone())
        {
            return Err(Error::Validation(
                "duplicate or noncanonical SoilGrids filename".into(),
            ));
        }
        if !keys.insert((
            file.property.clone(),
            file.depth.clone(),
            file.quantile.clone(),
        )) {
            return Err(Error::Validation("duplicate SoilGrids layer".into()));
        }
        let raster = generation_dir.join(&file.filename);
        if fs::canonicalize(raster.parent().unwrap())? != canonical_root {
            return Err(Error::Validation(
                "SoilGrids file escapes generation directory".into(),
            ));
        }
        let actual = fs::metadata(&raster).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::MissingSource(raster.clone())
            } else {
                e.into()
            }
        })?;
        if actual.len() != file.prepared_size
            || actual.len() > MAX_RASTER_BYTES
            || hash(&raster)? != file.prepared_sha256
        {
            return Err(Error::Validation(format!(
                "SoilGrids prepared file mismatch: {}",
                file.filename
            )));
        }
        validate_unit(file)?;
    }
    for property in PROPERTIES.iter().take(10) {
        for (depth, _) in DEPTHS {
            let quantiles: &[&str] = if matches!(*property, "wv0033" | "wv1500") {
                &["mean"]
            } else {
                &QUANTILES
            };
            for quantile in quantiles {
                if !keys.contains(&(
                    property.to_string(),
                    depth.to_string(),
                    (*quantile).to_string(),
                )) {
                    return Err(Error::Validation(format!(
                        "missing SoilGrids layer {property}/{depth}/{quantile}"
                    )));
                }
            }
        }
    }
    for quantile in [
        "most-probable",
        "Histosols-probability",
        "Leptosols-probability",
    ] {
        if !keys.contains(&(
            "wrb".to_string(),
            "surface".to_string(),
            quantile.to_string(),
        )) {
            return Err(Error::Validation(format!(
                "missing SoilGrids WRB layer {quantile}"
            )));
        }
    }
    Ok(manifest)
}

fn validate_prepared_extent(
    west: i64,
    south: i64,
    east: i64,
    north: i64,
    cell_size: u32,
) -> Result<()> {
    if (west, south, east, north) != PREPARED_EXTENT {
        return Err(Error::Validation(
            "SoilGrids manifest does not use the fixed Europe extent".into(),
        ));
    }
    let cell = i64::from(cell_size);
    if [west, south, east, north]
        .iter()
        .any(|value| value.rem_euclid(cell) != 0)
    {
        return Err(Error::Validation(
            "SoilGrids extent is shifted off the zero-origin grid".into(),
        ));
    }
    Ok(())
}

fn validate_unit(f: &ManifestFile) -> Result<()> {
    let expected = match f.property.as_str() {
        "sand" | "silt" | "clay" => "g/kg",
        "cfvo" => "cm3/dm3",
        "soc" => "dg/kg",
        "phh2o" => "pH*10",
        "cec" => "mmol(c)/kg",
        "bdod" => "cg/cm3",
        "wv0033" | "wv1500" => "10^-3 cm3/cm3",
        "wrb" => "class-or-percent",
        _ => return Err(Error::Validation("unsupported SoilGrids property".into())),
    };
    if f.unit != expected {
        return Err(Error::Validation(format!(
            "unexpected SoilGrids unit for {}",
            f.property
        )));
    }
    Ok(())
}
fn canonical_source_url(property: &str, depth: &str, quantile: &str) -> Option<String> {
    if property == "wrb" && depth == "surface" {
        let remote = match quantile {
            "most-probable" => "MostProbable",
            "Histosols-probability" => "Histosols",
            "Leptosols-probability" => "Leptosols",
            _ => return None,
        };
        return Some(format!(
            "https://files.isric.org/soilgrids/latest/data/wrb/{remote}.vrt"
        ));
    }
    if !DEPTHS.iter().any(|(value, _)| *value == depth) {
        return None;
    }
    if matches!(property, "wv0033" | "wv1500") {
        return (quantile == "mean").then(|| format!("https://files.isric.org/soilgrids/latest/data_aggregated/1000m/{property}/{property}_{depth}_mean_1000.tif"));
    }
    if !PROPERTIES.iter().take(8).any(|value| *value == property) || !QUANTILES.contains(&quantile)
    {
        return None;
    }
    let remote = if quantile == "Q0.50" {
        "Q0.5"
    } else {
        quantile
    };
    Some(format!(
        "https://files.isric.org/soilgrids/latest/data/{property}/{property}_{depth}_{remote}.vrt"
    ))
}
fn safe_filename(v: &str) -> bool {
    !v.is_empty() && v.len() <= 128 && !v.contains(['/', '\\']) && v.ends_with(".tif")
}
fn safe_source_url(v: &str) -> bool {
    (v.starts_with("https://files.isric.org/soilgrids/latest/data/")
        || v.starts_with("https://files.isric.org/soilgrids/latest/data_aggregated/1000m/"))
        && !v.contains(['?', '#', '\\'])
        && !v.contains("..")
}
fn valid_hash(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_hexdigit() && (!b.is_ascii_alphabetic() || b.is_ascii_lowercase()))
}
fn hash(path: &Path) -> Result<String> {
    let mut h = Sha256::new();
    let mut f = BufReader::new(File::open(path)?);
    let mut b = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

struct PreparedRaster {
    width: u32,
    height: u32,
    west: f64,
    north: f64,
    pixel: f64,
    values: Vec<f64>,
}
impl PreparedRaster {
    fn open(path: &Path, m: &Manifest) -> Result<Self> {
        let tiff = |source| Error::Tiff {
            path: path.to_path_buf(),
            source,
        };
        let mut limits = Limits::default();
        limits.decoding_buffer_size = MAX_RASTER_BYTES as usize;
        let mut d = Decoder::new(BufReader::new(File::open(path)?))
            .map_err(tiff)?
            .with_limits(limits);
        let (width, height) = d.dimensions().map_err(tiff)?;
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(Error::Validation(
                "SoilGrids raster pixel limit exceeded".into(),
            ));
        }
        let scale = d.get_tag_f64_vec(Tag::ModelPixelScaleTag).map_err(tiff)?;
        let tie = d.get_tag_f64_vec(Tag::ModelTiepointTag).map_err(tiff)?;
        let keys = d.get_tag_u16_vec(Tag::GeoKeyDirectoryTag).map_err(tiff)?;
        let nodata = d.get_tag_ascii_string(Tag::GdalNodata).map_err(tiff)?;
        let bits = d.get_tag_u16_vec(Tag::BitsPerSample).map_err(tiff)?;
        let formats = d.get_tag_u16_vec(Tag::SampleFormat).map_err(tiff)?;
        let samples = d.get_tag_u16_vec(Tag::SamplesPerPixel).map_err(tiff)?;
        let compression = d.get_tag_u16_vec(Tag::Compression).map_err(tiff)?;
        let geokey = |id, value| {
            keys.len() >= 4
                && keys[3] as usize * 4 + 4 <= keys.len()
                && keys[4..]
                    .chunks_exact(4)
                    .any(|key| key == [id, 0, 1, value])
        };
        let has_epsg3035 = geokey(1024, 1) && geokey(1025, 1) && geokey(3072, 3035);
        if scale
            != [
                f64::from(m.cell_size_meters),
                f64::from(m.cell_size_meters),
                0.0,
            ]
            || tie != [0.0, 0.0, 0.0, m.west as f64, m.north as f64, 0.0]
            || !has_epsg3035
            || !nodata.trim_end_matches('\0').eq_ignore_ascii_case("nan")
            || bits != [32]
            || formats != [3]
            || samples != [1]
            || compression != [8]
        {
            return Err(Error::Validation(format!(
                "{} has an invalid SoilGrids GeoTIFF contract",
                path.display()
            )));
        }
        let values: Vec<f64> = match d.read_image().map_err(tiff)? {
            DecodingResult::U8(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::U16(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::I16(v) => v.into_iter().map(f64::from).collect(),
            DecodingResult::F32(v) => v.into_iter().map(f64::from).collect(),
            _ => {
                return Err(Error::Validation(
                    "unsupported SoilGrids TIFF sample type".into(),
                ));
            }
        };
        let extent_w = m
            .east
            .checked_sub(m.west)
            .ok_or_else(|| Error::Validation("SoilGrids extent overflow".into()))?;
        let extent_h = m
            .north
            .checked_sub(m.south)
            .ok_or_else(|| Error::Validation("SoilGrids extent overflow".into()))?;
        let cell = i64::from(m.cell_size_meters);
        if extent_w <= 0 || extent_h <= 0 || extent_w % cell != 0 || extent_h % cell != 0 {
            return Err(Error::Validation("SoilGrids extent/cell mismatch".into()));
        }
        let expected_w = u32::try_from(extent_w / cell)
            .map_err(|_| Error::Validation("SoilGrids width overflow".into()))?;
        let expected_h = u32::try_from(extent_h / cell)
            .map_err(|_| Error::Validation("SoilGrids height overflow".into()))?;
        if (width, height) != (expected_w, expected_h)
            || values.len() != width as usize * height as usize
        {
            return Err(Error::Validation(
                "SoilGrids raster dimensions/grid mismatch".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            west: m.west as f64,
            north: m.north as f64,
            pixel: f64::from(m.cell_size_meters),
            values,
        })
    }
    fn sample(&self, x: f64, y: f64) -> Result<Option<f64>> {
        let col = ((x - self.west) / self.pixel).floor() as i64;
        let row = ((self.north - y) / self.pixel).floor() as i64;
        if col < 0 || row < 0 || col >= i64::from(self.width) || row >= i64::from(self.height) {
            return Ok(None);
        }
        let i = (row as usize)
            .checked_mul(self.width as usize)
            .and_then(|v| v.checked_add(col as usize))
            .ok_or_else(|| Error::Validation("SoilGrids raster index overflow".into()))?;
        let v = self.values[i];
        if v == -32768.0 || v.is_nan() {
            Ok(None)
        } else if !v.is_finite() {
            Err(Error::Validation(
                "SoilGrids raster contains non-finite data".into(),
            ))
        } else {
            Ok(Some(v))
        }
    }
}

fn bp(v: u16) -> SoilBasisPoints {
    SoilBasisPoints::new(v).unwrap()
}
fn probability(v: f64) -> Result<SoilBasisPoints> {
    if !v.is_finite() || !(0.0..=100.0).contains(&v) {
        return Err(Error::Validation("WRB probability outside 0..=100".into()));
    }
    Ok(bp((v * 100.0).round() as u16))
}
fn texture(s: f64, _si: f64, c: f64) -> MineralSoilTexture {
    if c >= 0.6 {
        MineralSoilTexture::VeryFine
    } else if c >= 0.35 {
        MineralSoilTexture::Fine
    } else if s >= 0.65 {
        MineralSoilTexture::Coarse
    } else if c >= 0.2 {
        MineralSoilTexture::MediumFine
    } else {
        MineralSoilTexture::Medium
    }
}
fn water_class(v: f64) -> AvailableWaterCapacity {
    if v < 50.0 {
        AvailableWaterCapacity::VeryLow
    } else if v < 100.0 {
        AvailableWaterCapacity::Low
    } else if v < 150.0 {
        AvailableWaterCapacity::Medium
    } else if v < 200.0 {
        AvailableWaterCapacity::High
    } else {
        AvailableWaterCapacity::VeryHigh
    }
}
fn carbon_class(v: f64) -> TopsoilOrganicCarbon {
    if v < 6.0 {
        TopsoilOrganicCarbon::VeryLow
    } else if v < 12.0 {
        TopsoilOrganicCarbon::Low
    } else if v < 30.0 {
        TopsoilOrganicCarbon::Medium
    } else {
        TopsoilOrganicCarbon::High
    }
}
fn acidity(v: f64) -> SoilAcidity {
    if v < 5.0 {
        SoilAcidity::StronglyAcid
    } else if v < 6.5 {
        SoilAcidity::Acid
    } else if v < 7.5 {
        SoilAcidity::Neutral
    } else {
        SoilAcidity::Alkaline
    }
}
fn cec_class(v: f64) -> CationExchangeCapacity {
    if v < 5.0 {
        CationExchangeCapacity::VeryLow
    } else if v < 10.0 {
        CationExchangeCapacity::Low
    } else if v < 20.0 {
        CationExchangeCapacity::Medium
    } else if v < 40.0 {
        CationExchangeCapacity::High
    } else {
        CationExchangeCapacity::VeryHigh
    }
}
fn fertility(c: f64, soc: f64, ph: f64) -> SoilFertility {
    let score = (c / 10.0) + (soc / 20.0) - ((ph - 6.5).abs() / 2.0);
    if score < 1.0 {
        SoilFertility::VeryLow
    } else if score < 2.0 {
        SoilFertility::Low
    } else if score < 3.0 {
        SoilFertility::Medium
    } else if score < 5.0 {
        SoilFertility::High
    } else {
        SoilFertility::VeryHigh
    }
}
fn uncertainty(q05: Option<f64>, q95: Option<f64>) -> SoilBasisPoints {
    match (q05, q95) {
        (Some(a), Some(b)) if b >= a => {
            bp((10_000.0 / (1.0 + (b - a).abs() / 100.0)).round() as u16)
        }
        _ => bp(2500),
    }
}
fn wrb_from_code(v: f64) -> Result<WrbReferenceGroup> {
    use WrbReferenceGroup::*;
    let all = [
        Acrisol,
        Albeluvisol,
        Alisol,
        Andosol,
        Arenosol,
        Calcisol,
        Cambisol,
        Chernozem,
        Cryosol,
        Durisol,
        Ferralsol,
        Fluvisol,
        Gleysol,
        Gypsisol,
        Histosol,
        Kastanozem,
        Leptosol,
        Lixisol,
        Luvisol,
        Nitisol,
        Phaeozem,
        Planosol,
        Plinthosol,
        Podzol,
        Regosol,
        Solonchak,
        Solonetz,
        Stagnosol,
        Umbrisol,
        Vertisol,
    ];
    let i = v.round() as isize;
    if v.is_finite() && (v - v.round()).abs() < f64::EPSILON && i >= 0 && (i as usize) < all.len() {
        Ok(all[i as usize])
    } else {
        Err(Error::Validation(format!("invalid SoilGrids WRB code {v}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tiff::encoder::{Compression, TiffEncoder, colortype::Gray32Float};
    #[test]
    fn quantile_and_texture_validation() {
        let mut s = BTreeMap::new();
        for p in PROPERTIES.iter().take(10) {
            for (d, _) in DEPTHS {
                for (q, v) in [
                    ("Q0.05", 100.0),
                    ("Q0.50", 200.0),
                    ("mean", 250.0),
                    ("Q0.95", 400.0),
                ] {
                    s.insert((p.to_string(), d.to_string(), q.to_string()), Some(v));
                }
            }
        }
        for p in ["silt", "clay"] {
            for (d, _) in DEPTHS {
                for q in QUANTILES {
                    s.insert(
                        (p.into(), d.into(), q.into()),
                        Some(if p == "silt" { 350.0 } else { 400.0 }),
                    );
                }
            }
        }
        s.insert(
            ("wrb".into(), "surface".into(), "most-probable".into()),
            Some(9.0),
        );
        s.insert(
            (
                "wrb".into(),
                "surface".into(),
                "Histosols-probability".into(),
            ),
            Some(1.0),
        );
        s.insert(
            (
                "wrb".into(),
                "surface".into(),
                "Leptosols-probability".into(),
            ),
            Some(2.0),
        );
        assert!(prediction_from_sample(&s).unwrap().is_some());
        s.insert(("sand".into(), "0-5cm".into(), "Q0.05".into()), Some(900.0));
        assert!(prediction_from_sample(&s).is_err());
    }
    #[test]
    fn probability_is_bounded() {
        assert!(probability(-1.0).is_err());
        assert_eq!(probability(100.0).unwrap().get(), 10_000);
    }
    #[test]
    fn filename_rejects_traversal() {
        assert!(safe_filename("sand.tif"));
        assert!(!safe_filename("../sand.tif"));
    }
    #[test]
    fn prepared_extent_rejects_shifted_and_arbitrary_bounds() {
        assert!(validate_prepared_extent(900_000, 900_000, 7_400_000, 5_500_000, 1000).is_ok());
        assert!(validate_prepared_extent(900_001, 900_000, 7_400_001, 5_500_000, 1000).is_err());
        assert!(validate_prepared_extent(0, 0, 1000, 1000, 1000).is_err());
    }
    #[test]
    fn prepared_tiff_sampling_resolves_nodata_and_bounds() {
        let path = std::env::temp_dir().join(format!(
            "adventuresim-soilgrids-{}.tif",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_fixture(&path, 1000.0, 3035, &[42.0, f32::NAN]);
        let manifest = Manifest {
            schema: 1,
            source: "ISRIC SoilGrids rolling-v2".into(),
            source_version: "test".into(),
            source_reproducibility: "unpinned-rolling-latest".into(),
            retrieved_at: "test".into(),
            generation: "a".repeat(64),
            crs: "EPSG:3035".into(),
            origin_easting_meters: 0,
            origin_northing_meters: 0,
            cell_size_meters: 1000,
            west: 0,
            south: 0,
            east: 2000,
            north: 1000,
            files: vec![],
        };
        let raster = PreparedRaster::open(&path, &manifest).unwrap();
        assert_eq!(raster.sample(500.0, 500.0).unwrap(), Some(42.0));
        assert_eq!(raster.sample(1500.0, 500.0).unwrap(), None);
        assert_eq!(raster.sample(2500.0, 500.0).unwrap(), None);
        fs::remove_file(path).unwrap();
    }
    fn write_fixture(path: &Path, scale: f64, epsg: u16, values: &[f32]) {
        let file = File::create(path).unwrap();
        let mut encoder = TiffEncoder::new(file)
            .unwrap()
            .with_compression(Compression::Deflate(Default::default()));
        let mut image = encoder.new_image::<Gray32Float>(2, 1).unwrap();
        image
            .encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[scale, scale, 0.0][..])
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::ModelTiepointTag,
                &[0.0, 0.0, 0.0, 0.0, 1000.0, 0.0][..],
            )
            .unwrap();
        image
            .encoder()
            .write_tag(
                Tag::GeoKeyDirectoryTag,
                &[
                    1_u16, 1, 0, 3, 1024, 0, 1, 1, 1025, 0, 1, 1, 3072, 0, 1, epsg,
                ][..],
            )
            .unwrap();
        image.encoder().write_tag(Tag::GdalNodata, "nan").unwrap();
        image.write_data(values).unwrap();
    }
    #[test]
    fn prepared_tiff_rejects_wrong_transform_and_crs() {
        let base = std::env::temp_dir().join(format!(
            "adventuresim-soilgrids-contract-{}",
            std::process::id()
        ));
        let manifest = Manifest {
            schema: 1,
            source: "ISRIC SoilGrids rolling-v2".into(),
            source_version: "test".into(),
            source_reproducibility: "unpinned-rolling-latest".into(),
            retrieved_at: "test".into(),
            generation: "a".repeat(64),
            crs: "EPSG:3035".into(),
            origin_easting_meters: 0,
            origin_northing_meters: 0,
            cell_size_meters: 1000,
            west: 0,
            south: 0,
            east: 2000,
            north: 1000,
            files: vec![],
        };
        write_fixture(&base, 500.0, 3035, &[1.0, 2.0]);
        assert!(PreparedRaster::open(&base, &manifest).is_err());
        write_fixture(&base, 1000.0, 4326, &[1.0, 2.0]);
        assert!(PreparedRaster::open(&base, &manifest).is_err());
        fs::remove_file(base).unwrap();
    }
    #[test]
    fn manifest_helpers_reject_hash_and_unit_mismatches() {
        assert!(!valid_hash("abc"));
        assert!(valid_hash(&"a".repeat(64)));
        let mut file = ManifestFile {
            property: "sand".into(),
            depth: "0-5cm".into(),
            quantile: "mean".into(),
            unit: "g/kg".into(),
            filename: "sand.tif".into(),
            source_url: "https://files.isric.org/soilgrids/latest/data/sand/x.vrt".into(),
            source_observation_size: 1,
            source_observation_sha256: "a".repeat(64),
            source_observation_etag: None,
            source_observation_last_modified: None,
            prepared_size: 1,
            prepared_sha256: "b".repeat(64),
        };
        assert!(validate_unit(&file).is_ok());
        file.unit = "percent".into();
        assert!(validate_unit(&file).is_err());
    }
    fn rule_prediction(
        wrb_group: WrbReferenceGroup,
        histosol: u16,
        water: AvailableWaterCapacity,
    ) -> SoilPrediction {
        SoilPrediction {
            wrb_group,
            histosol_probability: bp(histosol),
            leptosol_probability: bp(0),
            texture: MineralSoilTexture::Medium,
            available_water: water,
            organic_carbon: TopsoilOrganicCarbon::High,
            stones: StoneContentPercent::new(5).unwrap(),
            acidity: SoilAcidity::Acid,
            cation_exchange_capacity: CationExchangeCapacity::Medium,
            fertility: SoilFertility::Medium,
            confidence: bp(5000),
            evidence: SoilEvidence::SoilGridsPrediction,
        }
    }
    fn alluvium() -> SurfaceGeology {
        SurfaceGeology::Inferred(adventuresim_world_schema::InferredGeologicSetting {
            lithology: SurfaceLithology::Unconsolidated(
                adventuresim_world_schema::UnconsolidatedDeposit::Alluvium,
            ),
            age: adventuresim_world_schema::GeologicEra::Quaternary,
        })
    }
    fn distance() -> adventuresim_world_schema::WaterDistanceMeters {
        adventuresim_world_schema::WaterDistanceMeters::new(100).unwrap()
    }
    #[test]
    fn final_wetness_distinguishes_coast_drainage_and_flooding() {
        let coast = adventuresim_world_schema::SettlementHydrology {
            marine: Some(adventuresim_world_schema::MarineWaterAccess::OpenCoast(
                distance(),
            )),
            ..Default::default()
        };
        let coastal = finalize_prediction(
            rule_prediction(
                WrbReferenceGroup::Cambisol,
                9000,
                AvailableWaterCapacity::Medium,
            ),
            &alluvium(),
            coast,
            PotentialVegetationClass::Wetlands,
            10,
        );
        assert!(!matches!(
            coastal.properties.substrate,
            SoilSubstrate::Organic(_)
        ));
        assert_ne!(
            coastal.properties.agricultural_limitation,
            AgriculturalLimitation::Flooded
        );
        let drained = finalize_prediction(
            rule_prediction(WrbReferenceGroup::Stagnosol, 0, AvailableWaterCapacity::Low),
            &alluvium(),
            Default::default(),
            PotentialVegetationClass::Grassland,
            10,
        );
        assert_eq!(drained.properties.water_regime, SoilWaterRegime::UsuallyDry);
        assert_eq!(
            drained.properties.agricultural_limitation,
            AgriculturalLimitation::Drained
        );
        let fresh = adventuresim_world_schema::SettlementHydrology {
            inland: Some(adventuresim_world_schema::InlandWaterAccess {
                distance: distance(),
                size: adventuresim_world_schema::InlandWaterSize::Pond,
            }),
            ..Default::default()
        };
        let peat = finalize_prediction(
            rule_prediction(
                WrbReferenceGroup::Histosol,
                9000,
                AvailableWaterCapacity::VeryHigh,
            ),
            &alluvium(),
            fresh,
            PotentialVegetationClass::Wetlands,
            10,
        );
        assert!(matches!(
            peat.properties.substrate,
            SoilSubstrate::Organic(_)
        ));
        assert_eq!(
            peat.properties.water_regime,
            SoilWaterRegime::PermanentlyWet
        );
    }
    #[test]
    fn high_soc_alone_never_creates_peat() {
        let soil = finalize_prediction(
            rule_prediction(
                WrbReferenceGroup::Cambisol,
                0,
                AvailableWaterCapacity::Medium,
            ),
            &alluvium(),
            Default::default(),
            PotentialVegetationClass::Wetlands,
            10,
        );
        assert!(!matches!(
            soil.properties.substrate,
            SoilSubstrate::Organic(_)
        ));
    }
}
