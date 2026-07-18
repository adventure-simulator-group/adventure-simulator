//! European Soil Database v2 polygon and attribute sampling.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CompiledWorld, MappedSoilProfile, MineralSoil,
    MineralSoilTexture, OrganicSoil, OtherNonTexturedSoil, ParentMaterialCode,
    PotentialVegetationFormation, RockOutcropSoil, SettlementImport, SoilDepth, SoilMappingUnit,
    SoilProfile, SoilProperties, SoilSubstrate, SoilWaterRegime, SourceProvenance,
    StoneContentPercent, TopsoilOrganicCarbon, WORLD_SCHEMA_VERSION, WorldMetadata,
    WrbReferenceGroup,
};
use dbase::{FieldValue, Record};
use geo::{BoundingRect, Intersects, MultiPolygon, Point, Rect};
use proj4rs::{proj::Proj, transform::transform};

use crate::{
    Error, Result,
    draft::{TreeSpeciesSettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "European Soil Database v2.0 (SGDBE/PTRDB)";
const SOURCE_URL: &str =
    "https://esdac.jrc.ec.europa.eu/content/european-soil-database-v20-vector-and-attribute-data";
const SOURCE_LICENSE: &str =
    "ESDAC registration and project-specific permission required; redistribution not granted";
const SHAPEFILE_NAME: &str = "SGDBE4_0.shp";
const PROJECTION_NAME: &str = "SGDBE4_0.prj";
const SGDBE_ATTRIBUTES: &str = "STU_sgdbe.dbf";
const PTRDB_ATTRIBUTES: &str = "STU_ptrdb.dbf";
const BUCKET_METERS: f64 = 100_000.0;
const MAX_BUCKETS_PER_FEATURE: i64 = 10_000;
const EXPECTED_PROJECTION: &str = "PROJCS[\"User_Defined_Lambert_Azimuthal_Equal_Area\",GEOGCS[\"GCS_User_Defined\",DATUM[\"D_User_Defined\",SPHEROID[\"User_Defined_Spheroid\",6378388.0,0.0]],PRIMEM[\"Greenwich\",0.0],UNIT[\"Degree\",0.0174532925199433]],PROJECTION[\"Lambert_Azimuthal_Equal_Area\"],PARAMETER[\"False_Easting\",0.0],PARAMETER[\"False_Northing\",0.0],PARAMETER[\"Central_Meridian\",9.0],PARAMETER[\"Latitude_Of_Origin\",48.0],UNIT[\"Meter\",1.0]]";

pub(crate) fn enrich(
    draft: WorldDraft<TreeSpeciesSettlementDraft>,
    directory: &Path,
) -> Result<CompiledWorld> {
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), 0, 0, 0);
    }
    let map = SoilMap::read(directory)?;
    let projection = SoilProjection::new()?;
    let mut profiles = Vec::with_capacity(draft.settlements.len());
    let mut fallbacks = 0;
    for settlement in &draft.settlements {
        let base = &settlement.vegetated.forest.land.elevated.settlement;
        let point = projection.project(base.latitude, base.longitude)?;
        let profile = map.sample(point).unwrap_or_else(|| {
            fallbacks += 1;
            SoilProfile::Inferred(infer_properties(settlement))
        });
        profiles.push(profile);
    }
    finish(
        draft,
        profiles,
        map.polygons_read,
        map.attribute_rows_read,
        fallbacks,
    )
}

fn finish(
    mut draft: WorldDraft<TreeSpeciesSettlementDraft>,
    profiles: Vec<SoilProfile>,
    polygons_read: usize,
    attribute_rows_read: usize,
    fallbacks: usize,
) -> Result<CompiledWorld> {
    if profiles.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "soil profiles do not match settlements".into(),
        ));
    }
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(profiles)
        .map(|(trees, soil)| {
            let vegetated = trees.vegetated;
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
                tree_species: trees.tree_species,
                soil,
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
    draft.report.soil_polygons_read = polygons_read;
    draft.report.soil_attribute_rows_read = attribute_rows_read;
    draft.report.soil_samples = settlements.len();
    draft.report.soil_fallback_samples = fallbacks;
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

fn infer_properties(settlement: &TreeSpeciesSettlementDraft) -> SoilProperties {
    use PotentialVegetationFormation as V;
    let formation = match &settlement.vegetated.potential_vegetation {
        adventuresim_world_schema::PotentialVegetation::Mapped(mapped) => mapped.formation(),
        adventuresim_world_schema::PotentialVegetation::Inferred(formation) => *formation,
    };
    let elevation = settlement.vegetated.forest.land.elevated.elevation.get();
    let stones =
        |percent| StoneContentPercent::new(percent).expect("fallback percentage is bounded");
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

struct SoilProjection {
    geographic: Proj,
    projected: Proj,
}

impl SoilProjection {
    fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=laea +lat_0=48 +lon_0=9 +x_0=0 +y_0=0 +R=6378388 +units=m +no_defs +type=crs",
            )?,
        })
    }

    fn project(&self, latitude: f64, longitude: f64) -> Result<Point<f64>> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "invalid coordinate ({latitude}, {longitude}) for ESDB"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        Ok(Point::new(coordinate.0, coordinate.1))
    }
}

struct SoilMap {
    features: Vec<SoilFeature>,
    buckets: BTreeMap<(i32, i32), Vec<usize>>,
    profiles: BTreeMap<u32, SoilAttributeProfile>,
    polygons_read: usize,
    attribute_rows_read: usize,
}

impl SoilMap {
    fn read(directory: &Path) -> Result<Self> {
        validate_projection(directory)?;
        let sgdbe = read_sgdbe(&require(directory, SGDBE_ATTRIBUTES)?)?;
        let ptrdb = read_ptrdb(&require(directory, PTRDB_ATTRIBUTES)?)?;
        let attribute_rows_read = sgdbe.rows_read + ptrdb.rows_read;
        let profiles = join_profiles(sgdbe.complete, ptrdb.complete)?;
        let path = require(directory, SHAPEFILE_NAME)?;
        let mut reader =
            shapefile::Reader::from_path(&path).map_err(|source| Error::Shapefile {
                path: path.clone(),
                source,
            })?;
        if reader.header().shape_type != shapefile::ShapeType::Polygon {
            return Err(Error::Validation(format!(
                "{} is not a polygon shapefile",
                path.display()
            )));
        }
        let mut features = Vec::new();
        let mut polygons_read = 0;
        for item in reader.iter_shapes_and_records_as::<shapefile::Polygon, Record>() {
            let (shape, record) = item.map_err(|source| Error::Shapefile {
                path: path.clone(),
                source,
            })?;
            polygons_read += 1;
            let mapping = parse_mapping_unit(&record, &path)?;
            let geometry = MultiPolygon::<f64>::try_from(shape).map_err(|message| {
                Error::Validation(format!(
                    "invalid ESDB polygon geometry in {}: {message}",
                    path.display()
                ))
            })?;
            let bounds = geometry.bounding_rect().ok_or_else(|| {
                Error::Validation(format!("empty ESDB polygon in {}", path.display()))
            })?;
            features.push(SoilFeature {
                mapping,
                geometry,
                bounds,
            });
        }
        if features.is_empty() {
            return Err(Error::Validation(format!(
                "{} contains no soil polygons",
                path.display()
            )));
        }
        let buckets = build_buckets(&features)?;
        Ok(Self {
            features,
            buckets,
            profiles,
            polygons_read,
            attribute_rows_read,
        })
    }

    fn sample(&self, point: Point<f64>) -> Option<SoilProfile> {
        let key = bucket(point.x(), point.y())?;
        self.buckets
            .get(&key)?
            .iter()
            .filter_map(|&index| {
                let feature = &self.features[index];
                (bounds_intersect(feature.bounds, point) && feature.geometry.intersects(&point))
                    .then_some(feature)
            })
            .min_by_key(|feature| feature.mapping.smu())
            .and_then(|feature| {
                self.profiles
                    .get(&feature.mapping.dominant_stu())
                    .cloned()
                    .map(|profile| {
                        SoilProfile::Mapped(MappedSoilProfile {
                            mapping_unit: feature.mapping,
                            wrb_group: profile.wrb_group,
                            parent_material: profile.parent_material,
                            properties: profile.properties,
                        })
                    })
            })
    }
}

struct SoilFeature {
    mapping: SoilMappingUnit,
    geometry: MultiPolygon<f64>,
    bounds: Rect<f64>,
}

fn parse_mapping_unit(record: &Record, path: &Path) -> Result<SoilMappingUnit> {
    let smu = required_u32(record, "SMU", path)?;
    let stu = required_u32(record, "STU_DOM", path)?;
    let percent = required_u8(record, "PCAREA", path)?;
    SoilMappingUnit::new(smu, stu, percent).ok_or_else(|| {
        invalid_field(
            path,
            "PCAREA",
            &percent.to_string(),
            "mapping unit IDs and dominance must be positive",
        )
    })
}

#[derive(Clone)]
struct SgdbeRow {
    wrb: WrbReferenceGroup,
    parent: ParentMaterialCode,
    water_regime: SoilWaterRegime,
}

fn read_sgdbe(path: &Path) -> Result<TableRows<SgdbeRow>> {
    read_dbf(path, |record| {
        let stu = required_u32(record, "STU", path)?;
        let Some(wrb_value) = source_token(record, "WRBLV1", path)? else {
            return Ok((stu, None));
        };
        let Some(wrb) = parse_wrb_group(&wrb_value, path)? else {
            return Ok((stu, None));
        };
        let Some(parent_value) = source_token(record, "PARMADO", path)? else {
            return Ok((stu, None));
        };
        let Some(water_value) = source_token(record, "WR", path)? else {
            return Ok((stu, None));
        };
        let parent = ParentMaterialCode::new(parent_value.clone()).ok_or_else(|| {
            invalid_field(
                path,
                "PARMADO",
                &parent_value,
                "invalid parent-material code",
            )
        })?;
        let water_regime = parse_water_regime(&water_value, path)?;
        Ok((
            stu,
            Some(SgdbeRow {
                wrb,
                parent,
                water_regime,
            }),
        ))
    })
}

#[derive(Clone, Copy)]
struct PtrdbRow {
    substrate: SoilSubstrate,
    agricultural_limitation: AgriculturalLimitation,
}

fn read_ptrdb(path: &Path) -> Result<TableRows<PtrdbRow>> {
    read_dbf(path, |record| {
        let stu = required_u32(record, "STU", path)?;
        let fields = ["TEXT", "PEAT", "DR", "AWC_TOP", "OC_TOP", "VS", "AGLI1NNI"]
            .into_iter()
            .map(|field| source_token(record, field, path))
            .collect::<Result<Vec<_>>>()?;
        let [
            texture_value,
            peat_value,
            depth_value,
            water_value,
            carbon_value,
            stones_value,
            limitation_value,
        ]: [Option<String>; 7] = fields
            .try_into()
            .expect("seven PTRDB fields were requested");
        let (
            Some(texture_value),
            Some(peat_value),
            Some(depth_value),
            Some(water_value),
            Some(carbon_value),
            Some(stones_value),
            Some(limitation_value),
        ) = (
            texture_value,
            peat_value,
            depth_value,
            water_value,
            carbon_value,
            stones_value,
            limitation_value,
        )
        else {
            return Ok((stu, None));
        };
        let texture = parse_texture(&texture_value, path)?;
        let peat = parse_peat(&peat_value, path)?;
        let depth = parse_depth(&depth_value, path)?;
        let available_water = parse_water(&water_value, path)?;
        let organic_carbon = parse_carbon(&carbon_value, path)?;
        let stones = parse_stones(&stones_value, path)?;
        let agricultural_limitation = parse_limitation(&limitation_value, path)?;
        let substrate = match (peat, texture) {
            (true, _) | (false, SourceTexture::Organic) => SoilSubstrate::Organic(OrganicSoil {
                depth,
                available_water,
                stones,
            }),
            (false, SourceTexture::Mineral(texture)) => SoilSubstrate::Mineral(MineralSoil {
                texture,
                depth,
                available_water,
                organic_carbon,
                stones,
            }),
            (false, SourceTexture::RockOutcrop) => {
                SoilSubstrate::RockOutcrop(RockOutcropSoil { stones })
            }
            (false, SourceTexture::OtherNonTextured) => {
                SoilSubstrate::OtherNonTextured(OtherNonTexturedSoil {
                    depth,
                    available_water,
                    organic_carbon,
                    stones,
                })
            }
        };
        Ok((
            stu,
            Some(PtrdbRow {
                substrate,
                agricultural_limitation,
            }),
        ))
    })
}

#[derive(Clone)]
struct SoilAttributeProfile {
    wrb_group: WrbReferenceGroup,
    parent_material: ParentMaterialCode,
    properties: SoilProperties,
}

fn join_profiles(
    sgdbe: BTreeMap<u32, SgdbeRow>,
    ptrdb: BTreeMap<u32, PtrdbRow>,
) -> Result<BTreeMap<u32, SoilAttributeProfile>> {
    let mut profiles = BTreeMap::new();
    for (stu, geography) in sgdbe {
        if let Some(properties) = ptrdb.get(&stu) {
            profiles.insert(
                stu,
                SoilAttributeProfile {
                    wrb_group: geography.wrb,
                    parent_material: geography.parent,
                    properties: SoilProperties {
                        substrate: properties.substrate,
                        water_regime: geography.water_regime,
                        agricultural_limitation: properties.agricultural_limitation,
                    },
                },
            );
        }
    }
    if profiles.is_empty() {
        return Err(Error::Validation(
            "ESDB SGDBE and PTRDB tables have no complete matching STU rows".into(),
        ));
    }
    Ok(profiles)
}

struct TableRows<T> {
    complete: BTreeMap<u32, T>,
    rows_read: usize,
}

fn read_dbf<T>(
    path: &Path,
    mut parse: impl FnMut(&Record) -> Result<(u32, Option<T>)>,
) -> Result<TableRows<T>> {
    let mut reader = dbase::Reader::from_path(path).map_err(|source| Error::Dbase {
        path: path.into(),
        source,
    })?;
    let mut rows = BTreeMap::new();
    let mut seen = BTreeMap::new();
    let mut rows_read = 0;
    for record in reader.iter_records() {
        rows_read += 1;
        let record = record.map_err(|source| Error::Dbase {
            path: path.into(),
            source,
        })?;
        let (id, row) = parse(&record)?;
        if seen.insert(id, ()).is_some() {
            return Err(Error::Validation(format!(
                "duplicate STU {id} in {}",
                path.display()
            )));
        }
        if let Some(row) = row {
            rows.insert(id, row);
        }
    }
    if rows_read == 0 {
        return Err(Error::Validation(format!(
            "{} contains no attribute rows",
            path.display()
        )));
    }
    Ok(TableRows {
        complete: rows,
        rows_read,
    })
}

#[derive(Clone, Copy)]
enum SourceTexture {
    Mineral(MineralSoilTexture),
    Organic,
    RockOutcrop,
    OtherNonTextured,
}

fn parse_texture(value: &str, path: &Path) -> Result<SourceTexture> {
    Ok(match value {
        "1" => SourceTexture::Mineral(MineralSoilTexture::Coarse),
        "2" => SourceTexture::Mineral(MineralSoilTexture::Medium),
        "3" => SourceTexture::Mineral(MineralSoilTexture::MediumFine),
        "4" => SourceTexture::Mineral(MineralSoilTexture::Fine),
        "5" => SourceTexture::Mineral(MineralSoilTexture::VeryFine),
        "6" => SourceTexture::OtherNonTextured,
        "7" => SourceTexture::RockOutcrop,
        "8" => SourceTexture::Organic,
        _ => {
            return Err(invalid_field(
                path,
                "TEXT",
                value,
                "unrecognized PTRDB texture",
            ));
        }
    })
}

fn parse_wrb_group(value: &str, path: &Path) -> Result<Option<WrbReferenceGroup>> {
    use WrbReferenceGroup as W;
    Ok(Some(match value {
        "AB" => W::Albeluvisol,
        "AC" => W::Acrisol,
        "AL" => W::Alisol,
        "AN" => W::Andosol,
        "AR" => W::Arenosol,
        "AT" => W::Anthrosol,
        "CH" => W::Chernozem,
        "CL" => W::Calcisol,
        "CM" => W::Cambisol,
        "CR" => W::Cryosol,
        "DU" => W::Durisol,
        "FL" => W::Fluvisol,
        "FR" => W::Ferralsol,
        "GL" => W::Gleysol,
        "GY" => W::Gypsisol,
        "HS" => W::Histosol,
        "KS" => W::Kastanozem,
        "LP" => W::Leptosol,
        "LV" => W::Luvisol,
        "LX" => W::Lixisol,
        "NT" => W::Nitisol,
        "PH" => W::Phaeozem,
        "PL" => W::Planosol,
        "PT" => W::Plinthosol,
        "PZ" => W::Podzol,
        "RG" => W::Regosol,
        "SC" => W::Solonchak,
        "SN" => W::Solonetz,
        "UM" => W::Umbrisol,
        "VR" => W::Vertisol,
        "1" | "2" | "3" | "4" | "5" | "6" => return Ok(None),
        _ => {
            return Err(invalid_field(
                path,
                "WRBLV1",
                value,
                "unrecognized WRB reference group",
            ));
        }
    }))
}

fn parse_peat(value: &str, path: &Path) -> Result<bool> {
    match value {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(invalid_field(path, "PEAT", value, "expected Y or N")),
    }
}
fn parse_depth(value: &str, path: &Path) -> Result<SoilDepth> {
    match value {
        "S" => Ok(SoilDepth::Shallow),
        "M" => Ok(SoilDepth::Moderate),
        "D" => Ok(SoilDepth::Deep),
        "V" => Ok(SoilDepth::VeryDeep),
        _ => Err(invalid_field(
            path,
            "DR",
            value,
            "unrecognized depth-to-rock class",
        )),
    }
}
fn parse_water(value: &str, path: &Path) -> Result<AvailableWaterCapacity> {
    match value {
        "L" => Ok(AvailableWaterCapacity::Low),
        "M" => Ok(AvailableWaterCapacity::Medium),
        "H" => Ok(AvailableWaterCapacity::High),
        "VH" => Ok(AvailableWaterCapacity::VeryHigh),
        _ => Err(invalid_field(
            path,
            "AWC_TOP",
            value,
            "unrecognized available-water class",
        )),
    }
}
fn parse_carbon(value: &str, path: &Path) -> Result<TopsoilOrganicCarbon> {
    match value {
        "V" => Ok(TopsoilOrganicCarbon::VeryLow),
        "L" => Ok(TopsoilOrganicCarbon::Low),
        "M" => Ok(TopsoilOrganicCarbon::Medium),
        "H" => Ok(TopsoilOrganicCarbon::High),
        _ => Err(invalid_field(
            path,
            "OC_TOP",
            value,
            "unrecognized organic-carbon class",
        )),
    }
}
fn parse_water_regime(value: &str, path: &Path) -> Result<SoilWaterRegime> {
    match value {
        "1" => Ok(SoilWaterRegime::UsuallyDry),
        "2" => Ok(SoilWaterRegime::SeasonallyWet),
        "3" => Ok(SoilWaterRegime::LongSeasonWet),
        "4" => Ok(SoilWaterRegime::PermanentlyWet),
        _ => Err(invalid_field(
            path,
            "WR",
            value,
            "unrecognized SGDBE water-regime class",
        )),
    }
}
fn parse_stones(value: &str, path: &Path) -> Result<StoneContentPercent> {
    let percent = match value {
        "00" | "0" => 0,
        "10" => 10,
        "15" => 15,
        "20" => 20,
        _ => {
            return Err(invalid_field(
                path,
                "VS",
                value,
                "unrecognized stone-volume class",
            ));
        }
    };
    Ok(StoneContentPercent::new(percent).expect("documented percentage is bounded"))
}
fn parse_limitation(value: &str, path: &Path) -> Result<AgriculturalLimitation> {
    Ok(match value {
        "1" => AgriculturalLimitation::None,
        "2" => AgriculturalLimitation::Gravelly,
        "3" => AgriculturalLimitation::Stony,
        "4" => AgriculturalLimitation::ShallowRock,
        "5" => AgriculturalLimitation::Concretionary,
        "6" => AgriculturalLimitation::CementedCalcic,
        "7" => AgriculturalLimitation::Saline,
        "8" => AgriculturalLimitation::Sodic,
        "9" => AgriculturalLimitation::GlacierOrSnow,
        "10" => AgriculturalLimitation::Disturbed,
        "20" => AgriculturalLimitation::Fragic,
        "21" => AgriculturalLimitation::Drained,
        "22" => AgriculturalLimitation::Flooded,
        "30" => AgriculturalLimitation::Eroded,
        "31" => AgriculturalLimitation::ShallowWaterTable,
        _ => {
            return Err(invalid_field(
                path,
                "AGLI1NNI",
                value,
                "unrecognized agricultural limitation",
            ));
        }
    })
}

fn source_token(record: &Record, field: &'static str, path: &Path) -> Result<Option<String>> {
    let token = match record.get(field) {
        Some(FieldValue::Character(Some(value))) => value.trim().to_owned(),
        Some(FieldValue::Numeric(Some(value))) if value.is_finite() && value.fract() == 0.0 => {
            format!("{value:.0}")
        }
        Some(value) => {
            return Err(invalid_field(
                path,
                field,
                &format!("{value:?}"),
                "expected a populated character or integral numeric field",
            ));
        }
        None => return Err(invalid_field(path, field, "", "field is missing")),
    };
    Ok((!token.is_empty() && token != "#" && token != "0").then_some(token))
}

fn required_u32(record: &Record, field: &'static str, path: &Path) -> Result<u32> {
    let value = source_token(record, field, path)?
        .ok_or_else(|| invalid_field(path, field, "", "identifier has no information"))?;
    value
        .parse()
        .map_err(|_| invalid_field(path, field, &value, "expected a positive integer"))
}
fn required_u8(record: &Record, field: &'static str, path: &Path) -> Result<u8> {
    let value = source_token(record, field, path)?
        .ok_or_else(|| invalid_field(path, field, "", "percentage has no information"))?;
    value
        .parse()
        .map_err(|_| invalid_field(path, field, &value, "expected an integer percentage"))
}

fn build_buckets(features: &[SoilFeature]) -> Result<BTreeMap<(i32, i32), Vec<usize>>> {
    let mut buckets = BTreeMap::new();
    for (index, feature) in features.iter().enumerate() {
        let min = bucket(feature.bounds.min().x, feature.bounds.min().y)
            .ok_or_else(|| Error::Validation("non-finite ESDB bounds".into()))?;
        let max = bucket(feature.bounds.max().x, feature.bounds.max().y)
            .ok_or_else(|| Error::Validation("non-finite ESDB bounds".into()))?;
        let width = i64::from(max.0) - i64::from(min.0) + 1;
        let height = i64::from(max.1) - i64::from(min.1) + 1;
        let cells = width
            .checked_mul(height)
            .ok_or_else(|| Error::Validation("ESDB bucket coverage overflow".into()))?;
        if width <= 0 || height <= 0 || cells > MAX_BUCKETS_PER_FEATURE {
            return Err(Error::Validation(format!(
                "ESDB mapping unit {} has implausible bounds",
                feature.mapping.smu()
            )));
        }
        for x in min.0..=max.0 {
            for y in min.1..=max.1 {
                buckets.entry((x, y)).or_insert_with(Vec::new).push(index);
            }
        }
    }
    Ok(buckets)
}

fn bucket(x: f64, y: f64) -> Option<(i32, i32)> {
    let x = (x / BUCKET_METERS).floor();
    let y = (y / BUCKET_METERS).floor();
    (x.is_finite()
        && y.is_finite()
        && x >= f64::from(i32::MIN)
        && x <= f64::from(i32::MAX)
        && y >= f64::from(i32::MIN)
        && y <= f64::from(i32::MAX))
    .then_some((x as i32, y as i32))
}
fn bounds_intersect(bounds: Rect<f64>, point: Point<f64>) -> bool {
    point.x() >= bounds.min().x
        && point.x() <= bounds.max().x
        && point.y() >= bounds.min().y
        && point.y() <= bounds.max().y
}

fn validate_projection(directory: &Path) -> Result<()> {
    let path = require(directory, PROJECTION_NAME)?;
    let actual = fs::read_to_string(&path)?;
    let compact = |value: &str| {
        value
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect::<String>()
    };
    if compact(&actual) != compact(EXPECTED_PROJECTION) {
        return Err(Error::Validation(format!(
            "{} is not the ESDB legacy GISCO LAEA projection",
            path.display()
        )));
    }
    Ok(())
}
fn require(directory: &Path, filename: &str) -> Result<PathBuf> {
    let path = directory.join(filename);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}
fn invalid_field(path: &Path, field: &'static str, value: &str, message: &str) -> Error {
    Error::InvalidField {
        path: path.into(),
        field,
        value: value.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbase::{FieldName, FieldValue, Record, TableWriterBuilder};
    use shapefile::{Point as ShapePoint, Polygon, PolygonRing, Writer};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn documented_ptrdb_code_domains_parse_without_unknown_variants() {
        let path = Path::new("STU_ptrdb.dbf");
        for code in ["1", "2", "3", "4", "5", "6", "7", "8"] {
            assert!(parse_texture(code, path).is_ok());
        }
        for code in [
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "20", "21", "22", "30", "31",
        ] {
            assert!(parse_limitation(code, path).is_ok());
        }
        assert!(parse_texture("0", path).is_err());
        assert!(parse_limitation("future", path).is_err());
        assert!(parse_water("VL", path).is_err());
        assert_eq!(
            parse_wrb_group("CM", path).unwrap(),
            Some(WrbReferenceGroup::Cambisol)
        );
        assert_eq!(parse_wrb_group("3", path).unwrap(), None);
        assert!(parse_wrb_group("FUTURE", path).is_err());
    }

    #[test]
    fn legacy_projection_maps_its_origin_to_zero() {
        let point = SoilProjection::new().unwrap().project(48.0, 9.0).unwrap();
        assert!(point.x().abs() < 0.001);
        assert!(point.y().abs() < 0.001);
    }

    #[test]
    fn synthetic_distribution_joins_tables_samples_polygons_and_skips_no_information() {
        let directory = fixture_directory("join");
        write_shape(&directory);
        write_sgdbe(&directory);
        write_ptrdb(&directory);

        let map = SoilMap::read(&directory).unwrap();
        assert_eq!(map.polygons_read, 2);
        assert_eq!(map.attribute_rows_read, 4);
        assert_eq!(map.profiles.len(), 1);
        let SoilProfile::Mapped(profile) = map.sample(Point::new(50_000.0, 50_000.0)).unwrap()
        else {
            panic!("expected mapped soil")
        };
        assert_eq!(profile.mapping_unit.smu(), 10);
        assert_eq!(profile.mapping_unit.dominant_stu(), 100);
        assert_eq!(profile.mapping_unit.dominance_percent(), 75);
        assert_eq!(profile.wrb_group, WrbReferenceGroup::Cambisol);
        assert_eq!(profile.parent_material.as_str(), "110");
        assert!(matches!(
            profile.properties.substrate,
            SoilSubstrate::Mineral(MineralSoil {
                texture: MineralSoilTexture::Medium,
                ..
            })
        ));
        assert_eq!(
            profile.properties.water_regime,
            SoilWaterRegime::LongSeasonWet
        );
        assert_eq!(map.sample(Point::new(250_000.0, 50_000.0)), None);
        fs::remove_dir_all(directory).unwrap();
    }

    fn fixture_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "adventuresim-esdb-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(PROJECTION_NAME), EXPECTED_PROJECTION).unwrap();
        directory
    }

    fn write_shape(directory: &Path) {
        let table = TableWriterBuilder::new()
            .add_numeric_field(FieldName::try_from("SMU").unwrap(), 10, 0)
            .add_numeric_field(FieldName::try_from("STU_DOM").unwrap(), 10, 0)
            .add_numeric_field(FieldName::try_from("PCAREA").unwrap(), 3, 0);
        let mut writer = Writer::from_path(directory.join(SHAPEFILE_NAME), table).unwrap();
        for (smu, stu, west) in [(10, 100, 0.0), (20, 200, 200_000.0)] {
            let mut record = Record::default();
            record.insert("SMU".into(), FieldValue::Numeric(Some(f64::from(smu))));
            record.insert("STU_DOM".into(), FieldValue::Numeric(Some(f64::from(stu))));
            record.insert("PCAREA".into(), FieldValue::Numeric(Some(75.0)));
            writer
                .write_shape_and_record(&rectangle(west, 0.0, west + 100_000.0, 100_000.0), &record)
                .unwrap();
        }
    }

    fn write_sgdbe(directory: &Path) {
        let mut writer = TableWriterBuilder::new()
            .add_numeric_field(FieldName::try_from("STU").unwrap(), 10, 0)
            .add_character_field(FieldName::try_from("WRBLV1").unwrap(), 8)
            .add_character_field(FieldName::try_from("PARMADO").unwrap(), 12)
            .add_character_field(FieldName::try_from("WR").unwrap(), 1)
            .build_with_file_dest(directory.join(SGDBE_ATTRIBUTES))
            .unwrap();
        for (stu, wrb, parent, water) in [(100, "CM", "110", "3"), (200, "#", "0", "0")] {
            let mut record = Record::default();
            record.insert("STU".into(), FieldValue::Numeric(Some(f64::from(stu))));
            record.insert("WRBLV1".into(), FieldValue::Character(Some(wrb.into())));
            record.insert("PARMADO".into(), FieldValue::Character(Some(parent.into())));
            record.insert("WR".into(), FieldValue::Character(Some(water.into())));
            writer.write_record(&record).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_ptrdb(directory: &Path) {
        let mut table =
            TableWriterBuilder::new().add_numeric_field(FieldName::try_from("STU").unwrap(), 10, 0);
        for field in ["TEXT", "PEAT", "DR", "AWC_TOP", "OC_TOP", "VS", "AGLI1NNI"] {
            table = table.add_character_field(FieldName::try_from(field).unwrap(), 4);
        }
        let mut writer = table
            .build_with_file_dest(directory.join(PTRDB_ATTRIBUTES))
            .unwrap();
        for (stu, values) in [
            (100, ["2", "N", "D", "H", "M", "10", "1"]),
            (200, ["0", "N", "#", "#", "#", "00", "0"]),
        ] {
            let mut record = Record::default();
            record.insert("STU".into(), FieldValue::Numeric(Some(f64::from(stu))));
            for (field, value) in ["TEXT", "PEAT", "DR", "AWC_TOP", "OC_TOP", "VS", "AGLI1NNI"]
                .into_iter()
                .zip(values)
            {
                record.insert(field.into(), FieldValue::Character(Some(value.into())));
            }
            writer.write_record(&record).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn rectangle(west: f64, south: f64, east: f64, north: f64) -> Polygon {
        Polygon::new(PolygonRing::Outer(vec![
            ShapePoint::new(west, south),
            ShapePoint::new(west, north),
            ShapePoint::new(east, north),
            ShapePoint::new(east, south),
        ]))
    }

    #[test]
    #[ignore = "requires registered, extracted ESDB v2 vector archive in ESDB_DIR"]
    fn full_source_boundary_reads_registered_distribution() {
        let directory = std::env::var_os("ESDB_DIR").expect("set ESDB_DIR");
        let map = SoilMap::read(Path::new(&directory)).unwrap();
        assert!(map.polygons_read > 0);
        assert!(map.attribute_rows_read > 0);
        assert!(!map.profiles.is_empty());
    }
}
