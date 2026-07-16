//! EuroVegMap 2.1 potential-natural-vegetation polygon sampling.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CompiledWorld, DominantLeafType, EuroVegMapUnitCode, ForestCover, MappedPotentialVegetation,
    PotentialVegetation, PotentialVegetationFormation, SettlementImport, SourceProvenance,
    WORLD_SCHEMA_VERSION, WorldMetadata,
};
use dbase::{FieldValue, Record};
use geo::{BoundingRect, Intersects, MultiPolygon, Point, Rect};
use proj4rs::{proj::Proj, transform::transform};

use crate::{
    Error, Result,
    draft::{ForestSettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "EuroVegMap 2.1 Map of the Natural Vegetation of Europe";
const SOURCE_URL: &str = "https://www.synbiosys.alterra.nl/eurovegmap/";
const SOURCE_LICENSE: &str = "No redistribution licence stated in the 2.1 distribution";
const SHAPEFILE_NAME: &str = "Vegetation.shp";
const PROJECTION_NAME: &str = "Vegetation.prj";
const BUCKET_METERS: f64 = 100_000.0;
const MAX_BUCKETS_PER_FEATURE: i64 = 10_000;
const EXPECTED_PROJECTION: &str = "PROJCS[\"ETRS89-LAEA5220\",GEOGCS[\"ETRS89\",DATUM[\"<custom>\",SPHEROID[\"GRS_1980\",6378137.0,298.257222101]],PRIMEM[\"Greenwich\",0.0],UNIT[\"Degree\",0.0174532925199433]],PROJECTION[\"Lambert_Azimuthal_Equal_Area\"],PARAMETER[\"False_Easting\",5071000.0],PARAMETER[\"False_Northing\",3210000.0],PARAMETER[\"Central_Meridian\",20.0],PARAMETER[\"Latitude_Of_Origin\",52.0],UNIT[\"Meter\",1.0]]";

pub(crate) fn enrich(
    draft: WorldDraft<ForestSettlementDraft>,
    directory: &Path,
) -> Result<CompiledWorld> {
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), 0, 0);
    }

    let map = VegetationMap::read(directory)?;
    let projection = EuroVegProjection::new()?;
    let mut samples = Vec::with_capacity(draft.settlements.len());
    let mut fallbacks = 0;
    for settlement in &draft.settlements {
        let base = &settlement.land.elevated.settlement;
        let point = projection.project(base.latitude, base.longitude)?;
        let vegetation = map.sample(point).unwrap_or_else(|| {
            fallbacks += 1;
            PotentialVegetation::Inferred(infer_formation(settlement))
        });
        samples.push(vegetation);
    }
    let polygons_read = map.polygons_read;
    finish(draft, samples, polygons_read, fallbacks)
}

fn finish(
    mut draft: WorldDraft<ForestSettlementDraft>,
    samples: Vec<PotentialVegetation>,
    polygons_read: usize,
    fallbacks: usize,
) -> Result<CompiledWorld> {
    if samples.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "potential-vegetation samples do not match settlements".into(),
        ));
    }
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(samples)
        .map(|(forest, potential_vegetation)| {
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
                potential_vegetation,
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
    draft.report.potential_vegetation_polygons_read = polygons_read;
    draft.report.potential_vegetation_samples = settlements.len();
    draft.report.potential_vegetation_fallback_samples = fallbacks;
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

fn infer_formation(settlement: &ForestSettlementDraft) -> PotentialVegetationFormation {
    match settlement.forest_cover {
        ForestCover::Wooded(woodland) => match woodland.dominant {
            DominantLeafType::Broadleaf => PotentialVegetationFormation::DeciduousAndMixedForest,
            DominantLeafType::Coniferous | DominantLeafType::Mixed => {
                PotentialVegetationFormation::ConiferousAndMixedForest
            }
        },
        ForestCover::Open => {
            let elevated = &settlement.land.elevated;
            let base = &elevated.settlement;
            if elevated.elevation.get() >= 1_500 || base.latitude >= 60.0 {
                PotentialVegetationFormation::TundraAndAlpine
            } else if base.latitude < 40.0 {
                PotentialVegetationFormation::MediterraneanSclerophyll
            } else {
                PotentialVegetationFormation::DeciduousAndMixedForest
            }
        }
    }
}

struct EuroVegProjection {
    geographic: Proj,
    projected: Proj,
}

impl EuroVegProjection {
    fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=laea +lat_0=52 +lon_0=20 +x_0=5071000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs +type=crs",
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
                "invalid coordinate ({latitude}, {longitude}) for EuroVegMap"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        if !coordinate.0.is_finite() || !coordinate.1.is_finite() {
            return Err(Error::Validation(
                "EuroVegMap projection produced a non-finite coordinate".into(),
            ));
        }
        Ok(Point::new(coordinate.0, coordinate.1))
    }
}

struct VegetationMap {
    features: Vec<VegetationFeature>,
    buckets: BTreeMap<(i32, i32), Vec<usize>>,
    polygons_read: usize,
}

impl VegetationMap {
    fn read(directory: &Path) -> Result<Self> {
        validate_projection(directory)?;
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
        let mut source_ids = BTreeSet::new();
        for item in reader.iter_shapes_and_records_as::<shapefile::Polygon, Record>() {
            let (shape, record) = item.map_err(|source| Error::Shapefile {
                path: path.clone(),
                source,
            })?;
            polygons_read += 1;
            let attributes = SourceAttributes::parse(&record, &path)?;
            if !source_ids.insert(attributes.source_id) {
                return Err(Error::Validation(format!(
                    "duplicate EuroVegMap polygon ID {} in {}",
                    attributes.source_id,
                    path.display()
                )));
            }
            let Some(vegetation) = attributes.vegetation else {
                continue;
            };
            let geometry = MultiPolygon::<f64>::try_from(shape).map_err(|message| {
                Error::Validation(format!(
                    "invalid polygon geometry in {}: {message}",
                    path.display()
                ))
            })?;
            let bounds = geometry.bounding_rect().ok_or_else(|| {
                Error::Validation(format!("empty polygon geometry in {}", path.display()))
            })?;
            features.push(VegetationFeature {
                source_id: attributes.source_id,
                vegetation,
                geometry,
                bounds,
            });
        }
        if polygons_read == 0 || features.is_empty() {
            return Err(Error::Validation(format!(
                "{} contains no mapped vegetation polygons",
                path.display()
            )));
        }
        let buckets = build_buckets(&features)?;
        Ok(Self {
            features,
            buckets,
            polygons_read,
        })
    }

    fn sample(&self, point: Point<f64>) -> Option<PotentialVegetation> {
        let key = bucket(point.x(), point.y())?;
        self.buckets
            .get(&key)?
            .iter()
            .filter_map(|&index| {
                let feature = &self.features[index];
                (bounds_intersect(feature.bounds, point) && feature.geometry.intersects(&point))
                    .then_some(feature)
            })
            .min_by_key(|feature| feature.source_id)
            .map(|feature| PotentialVegetation::Mapped(feature.vegetation.clone()))
    }
}

struct VegetationFeature {
    source_id: u32,
    vegetation: MappedPotentialVegetation,
    geometry: MultiPolygon<f64>,
    bounds: Rect<f64>,
}

struct SourceAttributes {
    source_id: u32,
    vegetation: Option<MappedPotentialVegetation>,
}

impl SourceAttributes {
    fn parse(record: &Record, path: &Path) -> Result<Self> {
        let source_id = numeric_u32(record, "ID", path)?;
        let formation = SourceFormation::parse(character(record, "FORMATION", path)?, path)?;
        let vegetation = match formation {
            SourceFormation::Mapped(formation) => {
                let raw_unit = character(record, "CODE_E", path)?.ok_or_else(|| {
                    invalid_field(path, "CODE_E", "", "mapped polygon has no unit code")
                })?;
                let unit = EuroVegMapUnitCode::new(raw_unit.to_owned()).ok_or_else(|| {
                    invalid_field(path, "CODE_E", raw_unit, "invalid mapping-unit code")
                })?;
                Some(MappedPotentialVegetation { unit, formation })
            }
            SourceFormation::NonVegetation => None,
        };
        Ok(Self {
            source_id,
            vegetation,
        })
    }
}

enum SourceFormation {
    Mapped(PotentialVegetationFormation),
    NonVegetation,
}

impl SourceFormation {
    fn parse(value: Option<&str>, path: &Path) -> Result<Self> {
        use PotentialVegetationFormation as F;
        let parsed = match value {
            Some("A") | Some("Glet") => Self::Mapped(F::PolarDesertAndNival),
            Some("B") => Self::Mapped(F::TundraAndAlpine),
            Some("C") => Self::Mapped(F::OpenWoodlandAndSubalpine),
            Some("D") => Self::Mapped(F::ConiferousAndMixedForest),
            Some("E") => Self::Mapped(F::Heath),
            Some("F") => Self::Mapped(F::DeciduousAndMixedForest),
            Some("G") => Self::Mapped(F::ThermophilousBroadleafForest),
            Some("H") => Self::Mapped(F::HygroThermophilousBroadleafForest),
            Some("J") => Self::Mapped(F::MediterraneanSclerophyll),
            Some("K") => Self::Mapped(F::XerophyticConiferAndScrub),
            Some("L") => Self::Mapped(F::ForestSteppe),
            Some("M") => Self::Mapped(F::Steppe),
            Some("N") => Self::Mapped(F::Oroxerophytic),
            Some("O") => Self::Mapped(F::Desert),
            Some("P") => Self::Mapped(F::CoastalAndHalophytic),
            Some("R") => Self::Mapped(F::AquaticAndReed),
            Some("S") => Self::Mapped(F::Mire),
            Some("T") => Self::Mapped(F::SwampAndFenForest),
            Some("U") => Self::Mapped(F::FloodplainAndWetland),
            None | Some("See") | Some("Meer") | Some("Obdo") | Some("Salz") | Some("Salt")
            | Some("Pfan") | Some("grau") | Some("X") => Self::NonVegetation,
            Some(other) => {
                return Err(invalid_field(
                    path,
                    "FORMATION",
                    other,
                    "unrecognized EuroVegMap formation",
                ));
            }
        };
        Ok(parsed)
    }
}

fn character<'a>(record: &'a Record, field: &'static str, path: &Path) -> Result<Option<&'a str>> {
    match record.get(field) {
        Some(FieldValue::Character(value)) => Ok(value.as_deref()),
        Some(value) => Err(invalid_field(
            path,
            field,
            &format!("{value:?}"),
            "expected a character field",
        )),
        None => Err(invalid_field(path, field, "", "field is missing")),
    }
}

fn numeric_u32(record: &Record, field: &'static str, path: &Path) -> Result<u32> {
    let value = match record.get(field) {
        Some(FieldValue::Numeric(Some(value))) => *value,
        Some(value) => {
            return Err(invalid_field(
                path,
                field,
                &format!("{value:?}"),
                "expected a populated numeric field",
            ));
        }
        None => return Err(invalid_field(path, field, "", "field is missing")),
    };
    if !value.is_finite() || value < 0.0 || value > f64::from(u32::MAX) || value.fract() != 0.0 {
        return Err(invalid_field(
            path,
            field,
            &value.to_string(),
            "expected an unsigned integer",
        ));
    }
    Ok(value as u32)
}

fn invalid_field(path: &Path, field: &'static str, value: &str, message: &str) -> Error {
    Error::InvalidField {
        path: path.into(),
        field,
        value: value.into(),
        message: message.into(),
    }
}

fn build_buckets(features: &[VegetationFeature]) -> Result<BTreeMap<(i32, i32), Vec<usize>>> {
    let mut buckets = BTreeMap::new();
    for (index, feature) in features.iter().enumerate() {
        let min = bucket(feature.bounds.min().x, feature.bounds.min().y)
            .ok_or_else(|| Error::Validation("non-finite vegetation bounds".into()))?;
        let max = bucket(feature.bounds.max().x, feature.bounds.max().y)
            .ok_or_else(|| Error::Validation("non-finite vegetation bounds".into()))?;
        let width = i64::from(max.0) - i64::from(min.0) + 1;
        let height = i64::from(max.1) - i64::from(min.1) + 1;
        let cells = width.checked_mul(height).ok_or_else(|| {
            Error::Validation("vegetation feature bucket coverage overflow".into())
        })?;
        if width <= 0 || height <= 0 || cells > MAX_BUCKETS_PER_FEATURE {
            return Err(Error::Validation(format!(
                "vegetation feature {} has implausible bounds",
                feature.source_id
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
    let projection = fs::read_to_string(&path)?;
    if projection.trim() != EXPECTED_PROJECTION {
        return Err(Error::Validation(format!(
            "{} does not contain the EuroVegMap 2.1 ETRS89-LAEA5220 projection",
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use adventuresim_world_schema::{PotentialVegetation, PotentialVegetationFormation};

    use super::{EuroVegProjection, SourceFormation, VegetationMap};

    #[test]
    fn source_formations_parse_exhaustively_without_unknown() {
        assert!(matches!(
            SourceFormation::parse(Some("F"), Path::new("Vegetation.dbf")).unwrap(),
            SourceFormation::Mapped(PotentialVegetationFormation::DeciduousAndMixedForest)
        ));
        assert!(matches!(
            SourceFormation::parse(Some("See"), Path::new("Vegetation.dbf")).unwrap(),
            SourceFormation::NonVegetation
        ));
        assert!(SourceFormation::parse(Some("future-code"), Path::new("Vegetation.dbf")).is_err());
    }

    #[test]
    fn custom_projection_maps_its_origin_to_false_offsets() {
        let point = EuroVegProjection::new()
            .unwrap()
            .project(52.0, 20.0)
            .unwrap();
        assert!((point.x() - 5_071_000.0).abs() < 0.001);
        assert!((point.y() - 3_210_000.0).abs() < 0.001);
    }

    #[test]
    #[ignore = "requires extracted EuroVegMap 2.1 files in EUROVEGMAP_DIR"]
    fn full_source_boundary_reads_official_distribution() {
        let directory = std::env::var_os("EUROVEGMAP_DIR").expect("set EUROVEGMAP_DIR");
        let map = VegetationMap::read(Path::new(&directory)).unwrap();
        assert_eq!(map.polygons_read, 19_059);
        assert!(!map.features.is_empty());
        let projection = EuroVegProjection::new().unwrap();
        for (latitude, longitude) in [(48.8566, 2.3522), (52.52, 13.405), (41.9028, 12.4964)] {
            assert!(matches!(
                map.sample(projection.project(latitude, longitude).unwrap()),
                Some(PotentialVegetation::Mapped(_))
            ));
        }

        let viabundus = std::env::var_os("VIABUNDUS_DIR").expect("set VIABUNDUS_DIR");
        let settlements = crate::sources::viabundus::compile(Path::new(&viabundus), 1544)
            .unwrap()
            .settlements;
        let mapped = settlements
            .iter()
            .filter(|settlement| {
                projection
                    .project(settlement.latitude, settlement.longitude)
                    .ok()
                    .and_then(|point| map.sample(point))
                    .is_some()
            })
            .count();
        eprintln!(
            "EuroVegMap mapped {mapped}/{} settlements",
            settlements.len()
        );
        assert!(mapped > settlements.len() * 9 / 10);
    }
}
