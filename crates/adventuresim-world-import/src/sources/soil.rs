//! Bounded SoilGrids 2.0 0--5 cm physical-property sampling.
//!
//! The cache contract deliberately keeps a modeled sample distinct from an
//! inferred fallback.  SoilGrids provides modern, modelled physical values;
//! depth, wetness, and agricultural limits remain deterministic game rules
//! because this pipeline stage runs before hydrology.

use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, MineralSoil, MineralSoilTexture,
    ModeledSoilProfile, OrganicSoil, PotentialVegetation, PotentialVegetationFormation,
    RockOutcropSoil, SoilDepth, SoilProfile, SoilProperties, SoilSubstrate, SoilWaterRegime,
    SourceProvenance, StoneContentPercent, TopsoilOrganicCarbon, WorldBounds,
};
use serde::Deserialize;
use serde_json::Value;
use tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
};

use crate::{
    Error, Result,
    draft::{SoilSettlementDraft, TreeSpeciesSettlementDraft, WorldDraft, push_source_note},
};

const SOURCE_NAME: &str = "ISRIC SoilGrids 2.0";
const SOURCE_URL: &str = "https://maps.isric.org/";
const SOURCE_LICENSE: &str = "CC BY 4.0; attribution required";
const FORMAT: &str = "adventuresim-soilgrids-2.0-0-5cm-v1";
const MANIFEST: &str = "soilgrids-manifest.json";
const LAYERS: [&str; 6] = ["sand", "silt", "clay", "soc", "cfvo", "bdod"];
const NODATA: i16 = i16::MIN;
const MAX_RASTER_PIXELS: u64 = 16_000_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedManifest {
    format: String,
    source_url: String,
    layers: Vec<String>,
    world_bounds: Value,
}

pub(crate) fn enrich(
    mut draft: WorldDraft<TreeSpeciesSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<SoilSettlementDraft>> {
    let bounds = draft.world_bounds.clone().ok_or_else(|| {
        Error::Validation(
            "SoilGrids requires canonical --world-bounds so the prepared cache can be verified"
                .into(),
        )
    })?;
    read_manifest(directory, &bounds)?;
    let rasters = SoilRasters::read(directory, &bounds)?;
    let mut fallbacks = 0;
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .map(|mut trees| {
            let base = &trees.vegetated.forest.land.elevated.settlement;
            let profile = rasters.sample(base.latitude, base.longitude).map_or_else(
                || {
                    fallbacks += 1;
                    SoilProfile::Inferred(infer_properties(&trees))
                },
                |sample| SoilProfile::Modeled(ModeledSoilProfile {
                    properties: modeled_properties(&trees, sample),
                }),
            );
            push_source_note(&mut trees, match profile {
                SoilProfile::Modeled(_) => "**[ISRIC SoilGrids 2.0](https://maps.isric.org/):** Modern modelled 0--5 cm sand, silt, clay, organic carbon, coarse fragments, and bulk density were sampled from the verified bounded cache. Soil depth, wetness, and agricultural limitation are deterministic inferences; this stage has no hydrology input.",
                SoilProfile::Inferred(_) => "**SoilGrids fallback:** At least one required local raster value was nodata or outside coverage, so the complete soil profile is deterministically inferred from potential vegetation and elevation rather than partially fabricated.",
            });
            SoilSettlementDraft { trees, soil: profile }
        })
        .collect::<Vec<_>>();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.soilgrids_rasters_read = LAYERS.len();
    draft.report.soil_samples = settlements.len();
    draft.report.soil_fallback_samples = fallbacks;
    Ok(WorldDraft {
        year: draft.year,
        world_bounds: draft.world_bounds,
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

fn read_manifest(directory: &Path, bounds: &WorldBounds) -> Result<()> {
    let path = require(directory, MANIFEST)?;
    let manifest: PreparedManifest = serde_json::from_reader(BufReader::new(File::open(&path)?))
        .map_err(|source| Error::JsonSource {
            path: path.clone(),
            source,
        })?;
    let expected_bounds = serde_json::to_value(bounds)?;
    if manifest.format != FORMAT
        || manifest.source_url != SOURCE_URL
        || manifest.layers != LAYERS
        || manifest.world_bounds != expected_bounds
    {
        return Err(Error::Validation(format!(
            "{} does not bind this cache to the SoilGrids 2.0 six-layer world-bounds contract",
            path.display()
        )));
    }
    Ok(())
}

fn require(directory: &Path, name: &str) -> Result<PathBuf> {
    let path = directory.join(name);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}

struct SoilRasters {
    grid: Grid,
    values: [Vec<i16>; 6],
    width: u32,
    height: u32,
}

impl SoilRasters {
    fn read(directory: &Path, bounds: &WorldBounds) -> Result<Self> {
        let mut decoded = LAYERS
            .into_iter()
            .map(|layer| Raster::read(&require(directory, &format!("{layer}_0-5cm_mean.tif"))?))
            .collect::<Result<Vec<_>>>()?;
        let first = decoded.remove(0);
        if decoded.iter().any(|other| {
            other.width != first.width || other.height != first.height || other.grid != first.grid
        }) {
            return Err(Error::Validation(
                "SoilGrids layers do not share one exact GeoTIFF grid".into(),
            ));
        }
        first
            .grid
            .validate_bounds(bounds, first.width, first.height)?;
        let values: [Vec<i16>; 6] = std::array::from_fn(|index| {
            if index == 0 {
                first.values.clone()
            } else {
                decoded[index - 1].values.clone()
            }
        });
        Ok(Self {
            grid: first.grid,
            values,
            width: first.width,
            height: first.height,
        })
    }

    fn sample(&self, latitude: f64, longitude: f64) -> Option<PhysicalSample> {
        let (column, row) = self
            .grid
            .pixel(latitude, longitude, self.width, self.height)?;
        let index = row as usize * self.width as usize + column as usize;
        let values: [i16; 6] = std::array::from_fn(|layer| self.values[layer][index]);
        (!values.contains(&NODATA) && values.iter().all(|value| *value >= 0)).then_some(
            PhysicalSample {
                sand: values[0] as u16,
                silt: values[1] as u16,
                clay: values[2] as u16,
                soc: values[3] as u16,
                cfvo: values[4] as u16,
                bdod: values[5] as u16,
            },
        )
    }
}

struct Raster {
    grid: Grid,
    values: Vec<i16>,
    width: u32,
    height: u32,
}
impl Raster {
    fn read(path: &Path) -> Result<Self> {
        let mut decoder =
            Decoder::new(BufReader::new(File::open(path)?)).map_err(|source| Error::Tiff {
                path: path.into(),
                source,
            })?;
        let (width, height) = decoder.dimensions().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })?;
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_RASTER_PIXELS {
            return Err(Error::Validation(format!(
                "{} has an invalid or oversized SoilGrids raster dimension",
                path.display()
            )));
        }
        let values = match decoder.read_image().map_err(|source| Error::Tiff {
            path: path.into(),
            source,
        })? {
            DecodingResult::I16(values) if values.len() == width as usize * height as usize => {
                values
            }
            _ => {
                return Err(Error::Validation(format!(
                    "{} is not a single-band Int16 SoilGrids GeoTIFF",
                    path.display()
                )));
            }
        };
        Ok(Self {
            grid: Grid::parse(&mut decoder, path, width, height)?,
            values,
            width,
            height,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Grid {
    west: f64,
    north: f64,
    x_scale: f64,
    y_scale: f64,
}
impl Grid {
    fn parse(
        reader: &mut Decoder<impl Read + Seek>,
        path: &Path,
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
                "{} has invalid SoilGrids georeferencing",
                path.display()
            )));
        }
        let west = tie[3] - tie[0] * scale[0];
        let north = tie[4] + tie[1] * scale[1];
        if !(west.is_finite()
            && north.is_finite()
            && west >= -180.0
            && west + scale[0] * f64::from(width) <= 180.0
            && north <= 90.0
            && north - scale[1] * f64::from(height) >= -90.0)
        {
            return Err(Error::Validation(format!(
                "{} has impossible WGS84 coverage",
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

    fn validate_bounds(self, bounds: &WorldBounds, width: u32, height: u32) -> Result<()> {
        let value = serde_json::to_value(bounds)?;
        let south = value["south_west"]["latitude"]
            .as_f64()
            .ok_or_else(|| Error::Validation("invalid SoilGrids south bound".into()))?;
        let west = value["south_west"]["longitude"]
            .as_f64()
            .ok_or_else(|| Error::Validation("invalid SoilGrids west bound".into()))?;
        let north = value["north_east"]["latitude"]
            .as_f64()
            .ok_or_else(|| Error::Validation("invalid SoilGrids north bound".into()))?;
        let east = value["north_east"]["longitude"]
            .as_f64()
            .ok_or_else(|| Error::Validation("invalid SoilGrids east bound".into()))?;
        let raster_east = self.west + self.x_scale * f64::from(width);
        let raster_south = self.north - self.y_scale * f64::from(height);
        let epsilon = 1e-9;
        // WCS may snap a request to source pixels. Permit at most one source
        // pixel on each side, while rejecting a global or wrong-envelope TIFF.
        if self.west < west - self.x_scale - epsilon
            || self.west > west + self.x_scale + epsilon
            || self.north < north - self.y_scale - epsilon
            || self.north > north + self.y_scale + epsilon
            || raster_east < east - self.x_scale - epsilon
            || raster_east > east + self.x_scale + epsilon
            || raster_south < south - self.y_scale - epsilon
            || raster_south > south + self.y_scale + epsilon
        {
            return Err(Error::Validation(
                "SoilGrids GeoTIFF envelope does not match its manifest world bounds".into(),
            ));
        }
        Ok(())
    }
    fn pixel(self, latitude: f64, longitude: f64, width: u32, height: u32) -> Option<(u32, u32)> {
        if !latitude.is_finite() || !longitude.is_finite() {
            return None;
        }
        let col_coordinate = (longitude - self.west) / self.x_scale;
        let row_coordinate = (self.north - latitude) / self.y_scale;
        let mut col = col_coordinate.floor();
        let mut row = row_coordinate.floor();
        // A canonical bounds edge is inclusive.  The final source pixel owns its east/south edge.
        if (col_coordinate - f64::from(width)).abs() <= f64::EPSILON {
            col -= 1.0;
        }
        if (row_coordinate - f64::from(height)).abs() <= f64::EPSILON {
            row -= 1.0;
        }
        (col >= 0.0 && row >= 0.0 && col < f64::from(width) && row < f64::from(height))
            .then_some((col as u32, row as u32))
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

#[derive(Clone, Copy)]
struct PhysicalSample {
    sand: u16,
    silt: u16,
    clay: u16,
    soc: u16,
    cfvo: u16,
    bdod: u16,
}

fn modeled_properties(
    settlement: &TreeSpeciesSettlementDraft,
    sample: PhysicalSample,
) -> SoilProperties {
    // SoilGrids stores texture as g/kg with a conversion factor of ten to
    // g/100g (percent); cfvo is likewise tenths of a volumetric percent.
    let clay_percent = sample.clay / 10;
    let sand_percent = sample.sand / 10;
    let stone_percent = (sample.cfvo / 10).min(100) as u8;
    let texture = texture_from_source(sample.sand, sample.silt, sample.clay);
    let carbon = carbon_from_soc(sample.soc);
    let water = modeled_water_capacity(clay_percent, sand_percent, sample.bdod, stone_percent);
    let (depth, water_regime, agricultural_limitation) =
        contextual_inference(settlement, stone_percent);
    SoilProperties {
        substrate: SoilSubstrate::Mineral(MineralSoil {
            texture,
            depth,
            available_water: water,
            organic_carbon: carbon,
            stones: StoneContentPercent::new(stone_percent).unwrap(),
        }),
        water_regime,
        agricultural_limitation,
    }
}
fn texture_from_source(sand_raw: u16, silt_raw: u16, clay_raw: u16) -> MineralSoilTexture {
    match (clay_raw / 10, sand_raw / 10, silt_raw / 10) {
        (60.., _, _) => MineralSoilTexture::VeryFine,
        (35..=59, _, _) => MineralSoilTexture::Fine,
        (18..=34, _, _) | (_, _, 50..) => MineralSoilTexture::MediumFine,
        (_, 85.., _) => MineralSoilTexture::Coarse,
        _ => MineralSoilTexture::Medium,
    }
}
fn carbon_from_soc(soc_raw: u16) -> TopsoilOrganicCarbon {
    match soc_raw / 10 {
        0..=9 => TopsoilOrganicCarbon::VeryLow,
        10..=19 => TopsoilOrganicCarbon::Low,
        20..=59 => TopsoilOrganicCarbon::Medium,
        _ => TopsoilOrganicCarbon::High,
    }
}
fn modeled_water_capacity(
    clay: u16,
    sand: u16,
    bdod_raw: u16,
    stones: u8,
) -> AvailableWaterCapacity {
    let mut score = if sand >= 85 || clay < 10 {
        1_i8
    } else if clay <= 30 {
        2
    } else if clay <= 55 {
        3
    } else {
        2
    };
    if bdod_raw >= 160 {
        score -= 1;
    }
    if stones >= 15 {
        score -= 1;
    }
    if stones >= 35 {
        score -= 1;
    }
    match score.clamp(0, 4) {
        0 => AvailableWaterCapacity::VeryLow,
        1 => AvailableWaterCapacity::Low,
        2 => AvailableWaterCapacity::Medium,
        3 => AvailableWaterCapacity::High,
        _ => AvailableWaterCapacity::VeryHigh,
    }
}
fn contextual_inference(
    settlement: &TreeSpeciesSettlementDraft,
    stones: u8,
) -> (SoilDepth, SoilWaterRegime, AgriculturalLimitation) {
    use PotentialVegetationFormation as V;
    let formation = match &settlement.vegetated.potential_vegetation {
        PotentialVegetation::Mapped(mapped) => mapped.formation(),
        PotentialVegetation::Inferred(formation) => *formation,
    };
    let elevation = settlement.vegetated.forest.land.elevated.elevation.get();
    let wet = matches!(
        formation,
        V::Mire | V::SwampAndFenForest | V::AquaticAndReed | V::FloodplainAndWetland
    );
    let depth = if elevation >= 900 || stones >= 50 {
        SoilDepth::Shallow
    } else if stones >= 25 {
        SoilDepth::Moderate
    } else {
        SoilDepth::Deep
    };
    let regime = if wet {
        SoilWaterRegime::LongSeasonWet
    } else if matches!(
        formation,
        V::MediterraneanSclerophyll | V::XerophyticConiferAndScrub | V::Steppe | V::Desert
    ) {
        SoilWaterRegime::UsuallyDry
    } else {
        SoilWaterRegime::SeasonallyWet
    };
    let limit = if wet {
        AgriculturalLimitation::ShallowWaterTable
    } else if elevation >= 900 || stones >= 50 {
        AgriculturalLimitation::ShallowRock
    } else if stones >= 25 {
        AgriculturalLimitation::Gravelly
    } else {
        AgriculturalLimitation::None
    };
    (depth, regime, limit)
}
fn infer_properties(settlement: &TreeSpeciesSettlementDraft) -> SoilProperties {
    use PotentialVegetationFormation as V;
    let formation = match &settlement.vegetated.potential_vegetation {
        PotentialVegetation::Mapped(mapped) => mapped.formation(),
        PotentialVegetation::Inferred(formation) => *formation,
    };
    let elevation = settlement.vegetated.forest.land.elevated.elevation.get();
    let stones = |value| StoneContentPercent::new(value).unwrap();
    match formation {
        V::Mire | V::SwampAndFenForest => SoilProperties {
            substrate: SoilSubstrate::Organic(OrganicSoil {
                depth: SoilDepth::Deep,
                available_water: AvailableWaterCapacity::VeryHigh,
                stones: stones(0),
            }),
            water_regime: SoilWaterRegime::PermanentlyWet,
            agricultural_limitation: AgriculturalLimitation::ShallowWaterTable,
        },
        V::AquaticAndReed | V::FloodplainAndWetland => SoilProperties {
            substrate: SoilSubstrate::Mineral(MineralSoil {
                texture: MineralSoilTexture::MediumFine,
                depth: SoilDepth::VeryDeep,
                available_water: AvailableWaterCapacity::High,
                organic_carbon: TopsoilOrganicCarbon::Medium,
                stones: stones(0),
            }),
            water_regime: SoilWaterRegime::LongSeasonWet,
            agricultural_limitation: AgriculturalLimitation::Flooded,
        },
        V::TundraAndAlpine | V::Oroxerophytic | V::PolarDesertAndNival if elevation >= 900 => {
            SoilProperties {
                substrate: SoilSubstrate::RockOutcrop(RockOutcropSoil { stones: stones(20) }),
                water_regime: SoilWaterRegime::UsuallyDry,
                agricultural_limitation: AgriculturalLimitation::ShallowRock,
            }
        }
        V::MediterraneanSclerophyll | V::XerophyticConiferAndScrub | V::Steppe | V::Desert => {
            SoilProperties {
                substrate: SoilSubstrate::Mineral(MineralSoil {
                    texture: MineralSoilTexture::Coarse,
                    depth: SoilDepth::Moderate,
                    available_water: AvailableWaterCapacity::Low,
                    organic_carbon: TopsoilOrganicCarbon::Low,
                    stones: stones(10),
                }),
                water_regime: SoilWaterRegime::UsuallyDry,
                agricultural_limitation: AgriculturalLimitation::None,
            }
        }
        _ => SoilProperties {
            substrate: SoilSubstrate::Mineral(MineralSoil {
                texture: MineralSoilTexture::Medium,
                depth: SoilDepth::Deep,
                available_water: AvailableWaterCapacity::Medium,
                organic_carbon: TopsoilOrganicCarbon::Medium,
                stones: stones(10),
            }),
            water_regime: SoilWaterRegime::SeasonallyWet,
            agricultural_limitation: AgriculturalLimitation::None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Grid, NODATA, PhysicalSample, carbon_from_soc, modeled_water_capacity, texture_from_source,
    };
    use adventuresim_world_schema::{
        AvailableWaterCapacity, MineralSoilTexture, TopsoilOrganicCarbon,
    };
    #[test]
    fn grid_keeps_inclusive_outer_edge_in_final_pixel() {
        let grid = Grid {
            west: 9.0,
            north: 54.0,
            x_scale: 0.5,
            y_scale: 0.5,
        };
        assert_eq!(grid.pixel(53.0, 10.0, 2, 2), Some((1, 1)));
        assert_eq!(grid.pixel(52.99, 10.0, 2, 2), None);
    }
    #[test]
    fn physical_nodata_is_reserved_for_complete_fallback() {
        assert_eq!(NODATA, i16::MIN);
        let source = PhysicalSample {
            sand: 1,
            silt: 1,
            clay: 1,
            soc: 1,
            cfvo: 0,
            bdod: 1,
        };
        assert_eq!(source.cfvo, 0);
    }
    #[test]
    fn capacity_thresholds_apply_density_and_stones() {
        assert_eq!(
            modeled_water_capacity(30, 40, 150, 0),
            AvailableWaterCapacity::Medium
        );
        assert_eq!(
            modeled_water_capacity(30, 40, 160, 35),
            AvailableWaterCapacity::VeryLow
        );
    }
    #[test]
    fn physical_unit_conversion_boundaries_are_direct_and_exhaustive() {
        assert_eq!(texture_from_source(850, 0, 170), MineralSoilTexture::Coarse);
        assert_eq!(
            texture_from_source(849, 499, 170),
            MineralSoilTexture::Medium
        );
        assert_eq!(
            texture_from_source(400, 499, 180),
            MineralSoilTexture::MediumFine
        );
        assert_eq!(texture_from_source(400, 499, 350), MineralSoilTexture::Fine);
        assert_eq!(
            texture_from_source(400, 499, 600),
            MineralSoilTexture::VeryFine
        );
        assert_eq!(carbon_from_soc(99), TopsoilOrganicCarbon::VeryLow);
        assert_eq!(carbon_from_soc(100), TopsoilOrganicCarbon::Low);
        assert_eq!(carbon_from_soc(200), TopsoilOrganicCarbon::Medium);
        assert_eq!(carbon_from_soc(600), TopsoilOrganicCarbon::High);
    }
}
