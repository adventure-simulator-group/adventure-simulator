use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    EdgeEndpoint, SpatialGridSpec, TravelEdgeKind, WorldBuildReport, WorldNodeImport,
};
use serde::{Deserialize, Deserializer, de};

use crate::{
    Error, Result,
    draft::{SettlementDraft, TravelEdgeDraft, TravelRouteDraft, WorldDraft},
};

mod descriptions;
mod names;

const SOURCE_DOI: &str = "https://doi.org/10.5281/zenodo.16611998";

#[derive(Debug, Deserialize)]
struct RawNode {
    id: String,
    #[serde(default)]
    parentid: String,
    #[serde(default)]
    name: String,
    longitude: String,
    latitude: String,
    #[serde(rename = "Is_Settlement", default)]
    is_settlement: SourceFlag,
    #[serde(rename = "Is_Town", default)]
    is_town: SourceFlag,
    #[serde(rename = "Is_Ferry", default)]
    is_ferry: SourceFlag,
    #[serde(rename = "Is_Harbour", default)]
    is_harbour: SourceFlag,
    #[serde(rename = "Is_Bridge")]
    is_bridge: SourceFlag,
    #[serde(rename = "Bridge_From")]
    bridge_from: String,
    #[serde(rename = "Bridge_To")]
    bridge_to: String,
    #[serde(rename = "Is_Toll")]
    is_toll: SourceFlag,
    #[serde(rename = "Toll_From")]
    toll_from: String,
    #[serde(rename = "Toll_To")]
    toll_to: String,
    #[serde(rename = "Settlement_From", default)]
    settlement_from: String,
    #[serde(rename = "Settlement_To", default)]
    settlement_to: String,
}

#[derive(Debug, Deserialize)]
struct RawEdge {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    fromnode: String,
    tonode: String,
    length: String,
    #[serde(default)]
    slopemultiplier: String,
    certainty: String,
    #[serde(default)]
    section: String,
    #[serde(default)]
    fromyear: String,
    #[serde(default)]
    toyear: String,
}

#[derive(Debug, Deserialize)]
struct RawPopulation {
    nodesid: String,
    year: String,
    inhabitants: String,
}

pub(crate) fn compile(
    directory: &Path,
    year: i32,
    spatial_grid: SpatialGridSpec,
) -> Result<WorldDraft<SettlementDraft>> {
    let nodes_path = require(directory, "nodes.csv")?;
    let edges_path = require(directory, "edges.csv")?;
    let population_path = require(directory, "population.csv")?;
    let alternative_names_path = require(directory, "alternativenames.csv")?;
    let descriptions_path = require(directory, "descriptions.csv")?;

    let mut nodes_by_id = BTreeMap::new();
    for raw in read_csv::<RawNode>(&nodes_path)? {
        let id = required_number(&nodes_path, "id", &raw.id)?;
        let node = SourceNode {
            id,
            parent_node_id: optional_number(&nodes_path, "parentid", &raw.parentid)?,
            name: raw.name,
            longitude: required_number(&nodes_path, "longitude", &raw.longitude)?,
            latitude: required_number(&nodes_path, "latitude", &raw.latitude)?,
            is_settlement: raw.is_settlement.is_set(),
            is_town: raw.is_town.is_set(),
            is_ferry: raw.is_ferry.is_set(),
            is_harbour: raw.is_harbour.is_set(),
            bridge: DatedFeature::parse(
                &nodes_path,
                "Bridge",
                raw.is_bridge,
                &raw.bridge_from,
                &raw.bridge_to,
            )?,
            toll: DatedFeature::parse(
                &nodes_path,
                "Toll",
                raw.is_toll,
                &raw.toll_from,
                &raw.toll_to,
            )?,
            settlement_interval: ActiveInterval::parse(
                &nodes_path,
                "Settlement_From",
                &raw.settlement_from,
                "Settlement_To",
                &raw.settlement_to,
            )?,
        };
        if nodes_by_id.insert(id, node).is_some() {
            return Err(Error::Validation(format!(
                "Viabundus node ID {id} occurs more than once"
            )));
        }
    }

    let mut population_by_node: HashMap<u64, (i32, u32)> = HashMap::new();
    for raw in read_csv::<RawPopulation>(&population_path)? {
        let node_id = optional_number(&population_path, "nodesid", &raw.nodesid)?;
        let population_year = optional_number(&population_path, "year", &raw.year)?;
        let inhabitants = optional_number(&population_path, "inhabitants", &raw.inhabitants)?;
        let (Some(node_id), Some(population_year), Some(inhabitants)) =
            (node_id, population_year, inhabitants)
        else {
            continue;
        };
        if population_year > year {
            continue;
        }
        let previous = population_by_node.get(&node_id);
        if previous.is_none_or(|(previous_year, _)| population_year >= *previous_year) {
            population_by_node.insert(node_id, (population_year, inhabitants));
        }
    }

    let mut edges = Vec::new();
    let mut endpoint_ids = HashSet::new();
    let mut excluded_edges = BTreeMap::new();
    for raw in read_csv::<RawEdge>(&edges_path)? {
        let kind = match raw.kind.as_str() {
            "land" => TravelEdgeKind::Land,
            "ferry" => TravelEdgeKind::Ferry,
            other => {
                increment(&mut excluded_edges, format!("type:{other}"));
                continue;
            }
        };
        let interval = ActiveInterval::parse(
            &edges_path,
            "fromyear",
            &raw.fromyear,
            "toyear",
            &raw.toyear,
        )?;
        if !interval.contains(year) {
            increment(&mut excluded_edges, "inactive".into());
            continue;
        }
        let from_node_id = required_number(&edges_path, "fromnode", &raw.fromnode)?;
        let to_node_id = required_number(&edges_path, "tonode", &raw.tonode)?;
        if !nodes_by_id.contains_key(&from_node_id) || !nodes_by_id.contains_key(&to_node_id) {
            increment(&mut excluded_edges, "missing-node".into());
            continue;
        }
        if from_node_id == to_node_id {
            increment(&mut excluded_edges, "self-loop".into());
            continue;
        }
        endpoint_ids.extend([from_node_id, to_node_id]);
        let from_node = &nodes_by_id[&from_node_id];
        let to_node = &nodes_by_id[&to_node_id];
        let route = match kind {
            TravelEdgeKind::Ferry => TravelRouteDraft::Ferry,
            TravelEdgeKind::Land => TravelRouteDraft::Land {
                bridge: endpoints(
                    from_node.bridge.active_in(year),
                    to_node.bridge.active_in(year),
                ),
            },
        };
        let edge_id = required_number(&edges_path, "id", &raw.id)?;
        let slope_was_inferred = raw.slopemultiplier.trim().is_empty();
        edges.push(TravelEdgeDraft {
            id: edge_id,
            from_node_id,
            to_node_id,
            route,
            toll: endpoints(from_node.toll.active_in(year), to_node.toll.active_in(year)),
            length_m: required_number(&edges_path, "length", &raw.length)?,
            slope_multiplier: if raw.slopemultiplier.trim().is_empty() {
                1.0
            } else {
                required_number(&edges_path, "slopemultiplier", &raw.slopemultiplier)?
            },
            certainty: required_number(&edges_path, "certainty", &raw.certainty)?,
            section: raw.section,
            sources: format!(
                "- **[Viabundus 2]({SOURCE_DOI}):** Direct edge record `{edge_id}` supplies endpoints, route type, length, certainty, section, and active bridge/toll endpoint evidence.{}",
                if slope_was_inferred {
                    " The source slope multiplier was empty, so the importer used the documented neutral multiplier `1.0`."
                } else {
                    " The slope multiplier is source-provided."
                }
            ),
        });
    }

    let mut settlement_node_ids = HashSet::new();
    let mut settlements = Vec::new();
    for node in nodes_by_id.values() {
        if !node.is_settlement || !node.settlement_interval.contains(year) {
            continue;
        }
        settlement_node_ids.insert(node.id);
        let population = population_by_node.get(&node.id).copied();
        let estimate = population.map(|(_, value)| value);
        settlements.push(SettlementDraft {
            id: format!("viabundus-{}", node.id),
            source_node_id: node.id,
            name: node.name.clone(),
            longitude: node.longitude,
            latitude: node.latitude,
            population_level: population_level(estimate),
            population_estimate: population_estimate(&population_path, estimate)?,
            scene_key: "hills".into(),
            sources: format!(
                "- **[Viabundus 2]({SOURCE_DOI}):** Direct settlement node `{}` supplies name, coordinates, settlement/town status, and transport flags. {}",
                node.id,
                population.map_or_else(
                    || "No eligible population observation was available; the canonical estimate is `0` and the population band uses the documented fallback.".to_owned(),
                    |(population_year, value)| format!(
                        "Population `{value}` comes from the latest source observation not after the world year (`{population_year}`)."
                    ),
                )
            ),
        });
    }
    settlements.sort_by(|left, right| left.id.cmp(&right.id));
    let settlement_ids: HashMap<_, _> = settlements
        .iter()
        .map(|settlement| (settlement.source_node_id, settlement.id.clone()))
        .collect();
    let settlement_aliases = names::compile(&alternative_names_path, year, &settlement_ids)?;
    let (settlement_descriptions, deferred_settlement_descriptions) =
        descriptions::compile(&descriptions_path, &settlement_ids)?;

    let mut required_nodes: HashSet<_> =
        endpoint_ids.union(&settlement_node_ids).copied().collect();
    loop {
        let parents: Vec<_> = required_nodes
            .iter()
            .filter_map(|node_id| nodes_by_id[node_id].parent_node_id)
            .filter(|parent_id| nodes_by_id.contains_key(parent_id))
            .collect();
        let previous_len = required_nodes.len();
        required_nodes.extend(parents);
        if required_nodes.len() == previous_len {
            break;
        }
    }
    let nodes: Vec<_> = nodes_by_id
        .values()
        .filter(|node| required_nodes.contains(&node.id))
        .map(|node| WorldNodeImport {
            id: node.id,
            parent_node_id: node.parent_node_id,
            latitude: node.latitude,
            longitude: node.longitude,
            is_settlement: node.is_settlement,
            is_town: node.is_town,
            is_ferry: node.is_ferry,
            is_harbour: node.is_harbour,
            sources: format!(
                "- **[Viabundus 2]({SOURCE_DOI}):** Direct node record `{}` supplies coordinates, parent relationship, and settlement/town/ferry/harbour flags.",
                node.id
            ),
        })
        .collect();
    let connected_settlements: HashSet<_> = edges
        .iter()
        .flat_map(|edge| [edge.from_node_id, edge.to_node_id])
        .filter(|node_id| settlement_node_ids.contains(node_id))
        .collect();
    let route_crossings = edges
        .iter()
        .filter(|edge| edge.route.has_crossing())
        .count();
    let toll_edges = edges.iter().filter(|edge| edge.toll.is_some()).count();
    let contradictory_feature_dates = nodes_by_id
        .values()
        .flat_map(|node| [node.bridge, node.toll])
        .filter(|feature| feature.is_contradictory())
        .count();

    Ok(WorldDraft {
        year,
        spatial_grid,
        sources: vec![crate::manifest::viabundus(directory)?],
        road_types: vec![TravelEdgeKind::Ferry, TravelEdgeKind::Land],
        report: WorldBuildReport {
            nodes: nodes.len(),
            edges: edges.len(),
            settlements: settlements.len(),
            settlement_aliases: settlement_aliases.len(),
            settlement_descriptions: settlement_descriptions.len(),
            deferred_settlement_descriptions,
            settlements_connected_to_road_network: connected_settlements.len(),
            route_crossings,
            toll_edges,
            contradictory_feature_dates,
            elevation_tiles_read: 0,
            elevation_samples: 0,
            elevation_fallback_samples: 0,
            land_use_rasters_read: 0,
            land_use_samples: 0,
            land_use_fallback_samples: 0,
            land_use_normalized_samples: 0,
            forest_tiles_read: 0,
            forest_samples: 0,
            forest_fallback_samples: 0,
            potential_vegetation_raster_files_read: 0,
            potential_vegetation_samples: 0,
            potential_vegetation_posterior_samples: 0,
            potential_vegetation_categorical_samples: 0,
            potential_vegetation_inferred_samples: 0,
            tree_species_rasters_read: 0,
            tree_species_samples: 0,
            tree_species_fallback_samples: 0,
            tree_species_candidates: 0,
            soil_rasters_read: 0,
            soil_depth_layers_read: 0,
            soil_samples: 0,
            soil_fallback_samples: 0,
            geology_features_read: 0,
            geology_samples: 0,
            geology_fallback_samples: 0,
            religion_regions_read: 0,
            religion_samples: 0,
            religion_fallback_samples: 0,
            drought_grid_cells_read: 0,
            drought_samples: 0,
            drought_neighbor_samples: 0,
            drought_fallback_samples: 0,
            hydrology_files_read: 0,
            hydrology_features_read: 0,
            hydrology_settlement_samples: 0,
            hydrology_landlocked_settlements: 0,
            hydrology_edge_crossings: 0,
            hydrology_inferred_ferry_waterways: 0,
            historical_vegetation_direct_samples: 0,
            historical_vegetation_derived_samples: 0,
            historical_vegetation_fallback_samples: 0,
            historical_vegetation_tie_breaks: 0,
            route_terrain_edges: 0,
            route_terrain_dem_samples: 0,
            route_terrain_dem_fallbacks: 0,
            route_terrain_water_adjacencies: 0,
            route_terrain_landforms: 0,
            route_terrain_seasonal_risks: 0,
            route_terrain_encounter_tags: 0,
            industry_settlements: 0,
            industry_derived_outputs: 0,
            industry_fallback_settlements: 0,
            industry_fallback_outputs: 0,
            industry_agriculture_outputs: 0,
            industry_fishing_outputs: 0,
            industry_quarrying_outputs: 0,
            industry_mining_outputs: 0,
            industry_pottery_outputs: 0,
            industry_peat_outputs: 0,
            industry_forestry_outputs: 0,
            industry_charcoal_outputs: 0,
            industry_saltmaking_outputs: 0,
            industry_construction_outputs: 0,
            excluded_edges,
        },
        nodes,
        edges,
        settlements,
        settlement_aliases,
        settlement_descriptions,
    })
}

#[derive(Debug)]
struct SourceNode {
    id: u64,
    parent_node_id: Option<u64>,
    name: String,
    longitude: f64,
    latitude: f64,
    is_settlement: bool,
    is_town: bool,
    is_ferry: bool,
    is_harbour: bool,
    bridge: DatedFeature,
    toll: DatedFeature,
    settlement_interval: ActiveInterval,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveInterval {
    from: Option<i32>,
    to: Option<i32>,
}

impl ActiveInterval {
    fn parse(
        path: &Path,
        from_field: &'static str,
        from: &str,
        to_field: &'static str,
        to: &str,
    ) -> Result<Self> {
        let interval = Self {
            from: optional_number(path, from_field, from)?,
            to: optional_number(path, to_field, to)?,
        };
        if interval
            .from
            .zip(interval.to)
            .is_some_and(|(from, to)| from > to)
        {
            return Err(Error::InvalidField {
                path: path.into(),
                field: from_field,
                value: format!("{:?}..{:?}", interval.from, interval.to),
                message: format!("{from_field} must not be later than {to_field}"),
            });
        }
        Ok(interval)
    }

    fn contains(self, year: i32) -> bool {
        self.from.is_none_or(|from| from <= year) && self.to.is_none_or(|to| year < to)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DatedFeature {
    Absent,
    ContradictoryEmptyRange { year: i32 },
    Present { from: Option<i32>, to: Option<i32> },
}

impl DatedFeature {
    fn parse(
        path: &Path,
        field: &'static str,
        present: SourceFlag,
        from: &str,
        to: &str,
    ) -> Result<Self> {
        let from_year = optional_number(path, field, from)?;
        let to_year = optional_number(path, field, to)?;
        if !present.is_set() {
            if from_year.is_some() || to_year.is_some() {
                return Err(Error::InvalidField {
                    path: path.into(),
                    field,
                    value: format!("{from:?}..{to:?}"),
                    message: "feature dates are present while its source flag is unset".into(),
                });
            }
            return Ok(Self::Absent);
        }
        if let Some((from, to)) = from_year.zip(to_year)
            && from == to
        {
            return Ok(Self::ContradictoryEmptyRange { year: from });
        }
        if from_year.zip(to_year).is_some_and(|(from, to)| from > to) {
            return Err(Error::InvalidField {
                path: path.into(),
                field,
                value: format!("{from:?}..{to:?}"),
                message: "feature end year must be later than its start year".into(),
            });
        }
        Ok(Self::Present {
            from: from_year,
            to: to_year,
        })
    }

    fn active_in(self, year: i32) -> bool {
        match self {
            Self::Absent => false,
            Self::ContradictoryEmptyRange { .. } => false,
            Self::Present { from, to } => ActiveInterval { from, to }.contains(year),
        }
    }

    const fn is_contradictory(self) -> bool {
        matches!(self, Self::ContradictoryEmptyRange { .. })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SourceFlag(bool);

impl SourceFlag {
    const fn is_set(self) -> bool {
        self.0
    }
}

impl<'de> Deserialize<'de> for SourceFlag {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.trim() {
            "y" => Ok(Self(true)),
            "" | "n" | "null" => Ok(Self(false)),
            other => Err(de::Error::unknown_variant(other, &["y", "n", "null", ""])),
        }
    }
}

fn require(directory: &Path, name: &str) -> Result<PathBuf> {
    let path = directory.join(name);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}

fn read_csv<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|source| Error::Csv {
            path: path.into(),
            source,
        })?;
    reader
        .deserialize()
        .collect::<std::result::Result<_, _>>()
        .map_err(|source| Error::Csv {
            path: path.into(),
            source,
        })
}

fn optional_number<T>(path: &Path, field: &'static str, value: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = value.trim();
    if value.is_empty() || value == "null" {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|error: T::Err| Error::InvalidField {
            path: path.into(),
            field,
            value: value.into(),
            message: error.to_string(),
        })
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != "null").then(|| value.to_owned())
}

fn required_number<T>(path: &Path, field: &'static str, value: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    optional_number(path, field, value)?.ok_or_else(|| Error::InvalidField {
        path: path.into(),
        field,
        value: value.into(),
        message: "value is required".into(),
    })
}

fn endpoints(from: bool, to: bool) -> Option<EdgeEndpoint> {
    match (from, to) {
        (false, false) => None,
        (true, false) => Some(EdgeEndpoint::From),
        (false, true) => Some(EdgeEndpoint::To),
        (true, true) => Some(EdgeEndpoint::Both),
    }
}

fn population_level(thousands: Option<u32>) -> i32 {
    match thousands.unwrap_or(0) {
        0..=1 => 1,
        2..=3 => 2,
        4..=10 => 3,
        11..=50 => 4,
        _ => 5,
    }
}

fn population_estimate(path: &Path, thousands: Option<u32>) -> Result<u32> {
    let Some(thousands) = thousands else {
        return Ok(0);
    };
    thousands
        .checked_mul(1_000)
        .ok_or_else(|| Error::InvalidField {
            path: path.into(),
            field: "inhabitants",
            value: thousands.to_string(),
            message: "population in thousands exceeds the supported u32 inhabitant count".into(),
        })
}

fn increment(counts: &mut BTreeMap<String, usize>, key: String) {
    *counts.entry(key).or_default() += 1;
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{ActiveInterval, DatedFeature, SourceFlag, population_estimate, population_level};

    #[test]
    fn end_year_is_exclusive() {
        let path = std::path::Path::new("source.csv");
        let interval = ActiveInterval::parse(path, "from", "1500", "to", "1545").unwrap();
        assert!(interval.contains(1544));
        assert!(!interval.contains(1545));
        assert!(ActiveInterval::parse(path, "from", "1600", "to", "1500").is_err());
        assert!(
            ActiveInterval::parse(path, "from", "", "to", "")
                .unwrap()
                .contains(1544)
        );
    }

    #[test]
    fn population_bands_match_existing_importer() {
        assert_eq!(population_level(None), 1);
        assert_eq!(population_level(Some(3)), 2);
        assert_eq!(population_level(Some(10)), 3);
        assert_eq!(population_level(Some(50)), 4);
        assert_eq!(population_level(Some(51)), 5);
    }

    #[test]
    fn source_flags_reject_unknown_values() {
        #[derive(Deserialize)]
        struct Row {
            flag: SourceFlag,
        }

        let yes = csv::Reader::from_reader("flag\ny\n".as_bytes())
            .deserialize::<Row>()
            .next()
            .unwrap()
            .unwrap();
        assert!(yes.flag.is_set());

        let unset = csv::Reader::from_reader("flag\nnull\n".as_bytes())
            .deserialize::<Row>()
            .next()
            .unwrap()
            .unwrap();
        assert!(!unset.flag.is_set());

        let invalid = csv::Reader::from_reader("flag\nyes\n".as_bytes())
            .deserialize::<Row>()
            .next()
            .unwrap();
        assert!(invalid.is_err());
    }

    #[test]
    fn population_overflow_is_an_error() {
        assert!(population_estimate(std::path::Path::new("population.csv"), None).is_ok());
        assert!(
            population_estimate(std::path::Path::new("population.csv"), Some(u32::MAX)).is_err()
        );
    }

    #[test]
    fn dated_features_parse_into_valid_states() {
        let path = std::path::Path::new("nodes.csv");
        assert_eq!(
            DatedFeature::parse(path, "Bridge", SourceFlag(false), "", "").unwrap(),
            DatedFeature::Absent
        );
        let bridge = DatedFeature::parse(path, "Bridge", SourceFlag(true), "1500", "1600").unwrap();
        assert!(bridge.active_in(1544));
        assert!(!bridge.active_in(1600));
        assert!(DatedFeature::parse(path, "Bridge", SourceFlag(false), "1500", "").is_err());
        assert!(DatedFeature::parse(path, "Bridge", SourceFlag(true), "1600", "1500").is_err());
        let contradiction =
            DatedFeature::parse(path, "Bridge", SourceFlag(true), "1544", "1544").unwrap();
        assert_eq!(
            contradiction,
            DatedFeature::ContradictoryEmptyRange { year: 1544 }
        );
        assert!(!contradiction.active_in(1544));
        assert!(contradiction.is_contradictory());
    }

    #[test]
    fn infrastructure_keeps_endpoint_identity() {
        use adventuresim_world_schema::EdgeEndpoint;

        assert_eq!(super::endpoints(false, false), None);
        assert_eq!(super::endpoints(true, false), Some(EdgeEndpoint::From));
        assert_eq!(super::endpoints(false, true), Some(EdgeEndpoint::To));
        assert_eq!(super::endpoints(true, true), Some(EdgeEndpoint::Both));
    }
}
