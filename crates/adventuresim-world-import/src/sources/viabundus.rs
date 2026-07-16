use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CompiledWorld, SettlementImport, SourceProvenance, TravelEdgeImport, TravelEdgeKind,
    WORLD_SCHEMA_VERSION, WorldBuildReport, WorldMetadata, WorldNodeImport,
};
use serde::Deserialize;

use crate::{Error, Result};

const SOURCE_NAME: &str = "Viabundus Pre-modern Street Map 2";
const SOURCE_DOI: &str = "https://doi.org/10.5281/zenodo.16611998";
const SOURCE_LICENSE: &str = "CC-BY-SA-4.0";

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
    is_settlement: String,
    #[serde(rename = "Is_Town", default)]
    is_town: String,
    #[serde(rename = "Is_Ferry", default)]
    is_ferry: String,
    #[serde(rename = "Is_Harbour", default)]
    is_harbour: String,
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

pub fn compile(directory: &Path, year: i32) -> Result<CompiledWorld> {
    let nodes_path = require(directory, "nodes.csv")?;
    let edges_path = require(directory, "edges.csv")?;
    let population_path = require(directory, "population.csv")?;

    let mut nodes_by_id = BTreeMap::new();
    for raw in read_csv::<RawNode>(&nodes_path)? {
        let id = required_number(&nodes_path, "id", &raw.id)?;
        let node = SourceNode {
            id,
            parent_node_id: optional_number(&nodes_path, "parentid", &raw.parentid)?,
            name: raw.name,
            longitude: required_number(&nodes_path, "longitude", &raw.longitude)?,
            latitude: required_number(&nodes_path, "latitude", &raw.latitude)?,
            is_settlement: raw.is_settlement == "y",
            is_town: raw.is_town == "y",
            is_ferry: raw.is_ferry == "y",
            is_harbour: raw.is_harbour == "y",
            settlement_from: optional_number(&nodes_path, "Settlement_From", &raw.settlement_from)?,
            settlement_to: optional_number(&nodes_path, "Settlement_To", &raw.settlement_to)?,
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
        let from_year = optional_number(&edges_path, "fromyear", &raw.fromyear)?;
        let to_year = optional_number(&edges_path, "toyear", &raw.toyear)?;
        if !active_in_year(from_year, to_year, year) {
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
        edges.push(TravelEdgeImport {
            id: required_number(&edges_path, "id", &raw.id)?,
            from_node_id,
            to_node_id,
            kind,
            length_m: required_number(&edges_path, "length", &raw.length)?,
            slope_multiplier: if raw.slopemultiplier.trim().is_empty() {
                1.0
            } else {
                required_number(&edges_path, "slopemultiplier", &raw.slopemultiplier)?
            },
            certainty: required_number(&edges_path, "certainty", &raw.certainty)?,
            section: raw.section,
        });
    }

    let mut settlement_node_ids = HashSet::new();
    let mut settlements = Vec::new();
    for node in nodes_by_id.values() {
        if !node.is_settlement || !active_in_year(node.settlement_from, node.settlement_to, year) {
            continue;
        }
        settlement_node_ids.insert(node.id);
        let estimate = population_by_node.get(&node.id).map(|(_, value)| *value);
        settlements.push(SettlementImport {
            id: format!("viabundus-{}", node.id),
            source_node_id: node.id,
            name: node.name.clone(),
            longitude: node.longitude,
            latitude: node.latitude,
            population_level: population_level(estimate),
            population_estimate: estimate
                .and_then(|value| value.checked_mul(1_000))
                .unwrap_or(0),
            scene_key: "hills".into(),
            religion_id: ["western_church", "reformed", "old_faith"][(node.id % 3) as usize].into(),
        });
    }
    settlements.sort_by(|left, right| left.id.cmp(&right.id));

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
        })
        .collect();
    let connected_settlements: HashSet<_> = edges
        .iter()
        .flat_map(|edge| [edge.from_node_id, edge.to_node_id])
        .filter(|node_id| settlement_node_ids.contains(node_id))
        .collect();

    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            world_year: year,
            sources: vec![SourceProvenance {
                name: SOURCE_NAME.into(),
                url: SOURCE_DOI.into(),
                license: SOURCE_LICENSE.into(),
            }],
            road_types: vec![TravelEdgeKind::Ferry, TravelEdgeKind::Land],
        },
        report: WorldBuildReport {
            nodes: nodes.len(),
            edges: edges.len(),
            settlements: settlements.len(),
            settlements_connected_to_road_network: connected_settlements.len(),
            excluded_edges,
        },
        nodes,
        edges,
        settlements,
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
    settlement_from: Option<i32>,
    settlement_to: Option<i32>,
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

fn active_in_year(from: Option<i32>, to: Option<i32>, year: i32) -> bool {
    from.is_none_or(|from| from <= year) && to.is_none_or(|to| year < to)
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

fn increment(counts: &mut BTreeMap<String, usize>, key: String) {
    *counts.entry(key).or_default() += 1;
}

#[cfg(test)]
mod tests {
    use super::{active_in_year, population_level};

    #[test]
    fn end_year_is_exclusive() {
        assert!(active_in_year(Some(1500), Some(1545), 1544));
        assert!(!active_in_year(Some(1500), Some(1544), 1544));
        assert!(active_in_year(None, None, 1544));
    }

    #[test]
    fn population_bands_match_existing_importer() {
        assert_eq!(population_level(None), 1);
        assert_eq!(population_level(Some(3)), 2);
        assert_eq!(population_level(Some(10)), 3);
        assert_eq!(population_level(Some(50)), 4);
        assert_eq!(population_level(Some(51)), 5);
    }
}
