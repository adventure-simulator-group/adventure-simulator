//! Terrain-aware rules-v9 repair of sparse Viabundus topology.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet},
};

use adventuresim_terrain::{TerrainPack, TerrainPurpose};
use adventuresim_world_schema::{
    CompiledWorld, LandRoute, MAX_EDGE_GEOMETRY_POINTS, TravelEdgeImport, TravelEdgeProvenance,
    TravelGeometryPoint, TravelRoute,
};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

const MAX_ROUTE_METRES: u64 = 12_000;
const MAX_ROUTE_MINUTES: u64 = 360;
const MAX_EVALUATIONS_PER_SETTLEMENT: u8 = 8;
const MAX_INFERRED_DEGREE: u8 = 2;
const BUCKET_DEGREES: f64 = 0.20;
const MAX_BUCKET_SETTLEMENTS: usize = 256;

#[derive(Clone, Copy, Debug)]
struct Candidate {
    from: u64,
    to: u64,
    direct_m: u32,
}

pub(crate) fn enrich(mut world: CompiledWorld, terrain: &TerrainPack) -> Result<CompiledWorld> {
    if terrain.purpose() != TerrainPurpose::DocumentedBase {
        return Err(Error::Validation(
            "road inference requires a documented-base terrain pack".into(),
        ));
    }
    let nodes = world
        .nodes
        .iter()
        .map(|n| (n.id, (n.latitude, n.longitude)))
        .collect::<HashMap<_, _>>();
    let settlement_ids = world
        .settlements
        .iter()
        .map(|s| s.source_node_id)
        .collect::<HashSet<_>>();
    let mut buckets = BTreeMap::<(i32, i32), Vec<u64>>::new();
    for id in settlement_ids.iter().copied() {
        let &(lat, lon) = nodes
            .get(&id)
            .ok_or_else(|| Error::Validation("settlement node missing".into()))?;
        let key = (
            (lat / BUCKET_DEGREES).floor() as i32,
            (lon / BUCKET_DEGREES).floor() as i32,
        );
        let bucket = buckets.entry(key).or_default();
        if bucket.len() >= MAX_BUCKET_SETTLEMENTS {
            return Err(Error::Validation(
                "road inference bucket exceeds bound".into(),
            ));
        }
        bucket.push(id);
    }
    for ids in buckets.values_mut() {
        ids.sort_unstable();
    }
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut ids = settlement_ids.into_iter().collect::<Vec<_>>();
    ids.sort_unstable();
    for from in ids {
        let (lat, lon) = nodes[&from];
        let key = (
            (lat / BUCKET_DEGREES).floor() as i32,
            (lon / BUCKET_DEGREES).floor() as i32,
        );
        let mut nearest = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(bucket) = buckets.get(&(key.0 + dy, key.1 + dx)) {
                    for &to in bucket {
                        if to == from {
                            continue;
                        }
                        let distance = haversine_m((lat, lon), nodes[&to]);
                        if distance <= MAX_ROUTE_METRES as u32 {
                            nearest.push((distance, to));
                        }
                    }
                }
            }
        }
        nearest.sort_unstable();
        nearest.truncate(MAX_EVALUATIONS_PER_SETTLEMENT as usize);
        for (direct_m, to) in nearest {
            let pair = (from.min(to), from.max(to));
            if seen.insert(pair) {
                candidates.push(Candidate {
                    from: pair.0,
                    to: pair.1,
                    direct_m,
                });
            }
        }
    }
    candidates.sort_by_key(|c| (c.direct_m, c.from, c.to));
    let mut adjacency = graph(&world.edges);
    let mut evaluations = HashMap::<u64, u8>::new();
    let mut degree = HashMap::<u64, u8>::new();
    let existing_ids = world.edges.iter().map(|e| e.id).collect::<HashSet<_>>();
    let mut accepted = Vec::new();
    for candidate in candidates {
        if degree.get(&candidate.from).copied().unwrap_or(0) >= MAX_INFERRED_DEGREE
            || degree.get(&candidate.to).copied().unwrap_or(0) >= MAX_INFERRED_DEGREE
        {
            continue;
        }
        if evaluations.get(&candidate.from).copied().unwrap_or(0) >= MAX_EVALUATIONS_PER_SETTLEMENT
            || evaluations.get(&candidate.to).copied().unwrap_or(0)
                >= MAX_EVALUATIONS_PER_SETTLEMENT
        {
            continue;
        }
        if shortest_within(
            &adjacency,
            candidate.from,
            candidate.to,
            u64::from(candidate.direct_m) * 18 / 10,
        ) {
            continue;
        }
        *evaluations.entry(candidate.from).or_default() += 1;
        *evaluations.entry(candidate.to).or_default() += 1;
        let a = nodes[&candidate.from];
        let b = nodes[&candidate.to];
        let Ok(plan) = terrain.plan(a, b) else {
            continue;
        };
        if plan.distance_m == 0
            || plan.distance_m > MAX_ROUTE_METRES
            || plan.minutes > MAX_ROUTE_MINUTES
        {
            continue;
        }
        let geometry = plan
            .points
            .iter()
            .map(|p| TravelGeometryPoint::new(p.longitude, p.latitude).map_err(Error::Validation))
            .collect::<Result<Vec<_>>>()?;
        if geometry.len() < 2 || geometry.len() > MAX_EDGE_GEOMETRY_POINTS {
            continue;
        }
        let id = stable_id(candidate.from, candidate.to, &existing_ids, &accepted);
        let length_m = u32::try_from(plan.distance_m)
            .map_err(|_| Error::Validation("inferred road length overflow".into()))?;
        accepted.push(TravelEdgeImport {
            id, from_node_id: candidate.from, to_node_id: candidate.to,
            route: TravelRoute::Land(LandRoute { bridge: None, water_crossings: Vec::new() }),
            provenance: TravelEdgeProvenance::InferredWalkingLink, geometry,
            toll: None, length_m, slope_multiplier: 1.0,
            terrain: adventuresim_world_schema::RouteTerrain::stage_placeholder(), certainty: 1,
            section: "inferred-terrain-link-v9".into(),
            sources: format!("- **Terrain-aware road inference rules v9:** A* over documented-base terrain {} found a bounded {} m / {} minute passable alignment between nodes `{}` and `{}`. This is plausible gap-fill, not a Viabundus-documented road.", terrain.package_sha256(), plan.distance_m, plan.minutes, candidate.from, candidate.to),
        });
        adjacency
            .entry(candidate.from)
            .or_default()
            .push((candidate.to, u64::from(length_m)));
        adjacency
            .entry(candidate.to)
            .or_default()
            .push((candidate.from, u64::from(length_m)));
        *degree.entry(candidate.from).or_default() += 1;
        *degree.entry(candidate.to).or_default() += 1;
    }
    accepted.sort_by_key(|e| e.id);
    let geometry_bytes = serde_json::to_vec(
        &accepted
            .iter()
            .map(|e| (&e.id, &e.geometry))
            .collect::<Vec<_>>(),
    )?;
    world.report.base_terrain_package_sha256 = terrain.package_sha256().into();
    world.report.inferred_road_edges = accepted.len();
    world.report.inferred_road_geometry_sha256 = format!("{:x}", Sha256::digest(geometry_bytes));
    world.edges.extend(accepted);
    world.edges.sort_by_key(|edge| edge.id);
    world.report.edges = world.edges.len();
    world.report.settlements_connected_to_road_network = connected_settlement_count(&world);
    Ok(world)
}

fn graph(edges: &[TravelEdgeImport]) -> HashMap<u64, Vec<(u64, u64)>> {
    let mut g = HashMap::new();
    for e in edges {
        g.entry(e.from_node_id)
            .or_insert_with(Vec::new)
            .push((e.to_node_id, u64::from(e.length_m)));
        g.entry(e.to_node_id)
            .or_insert_with(Vec::new)
            .push((e.from_node_id, u64::from(e.length_m)));
    }
    g
}
fn shortest_within(g: &HashMap<u64, Vec<(u64, u64)>>, start: u64, goal: u64, limit: u64) -> bool {
    let mut q = BinaryHeap::new();
    let mut best = HashMap::new();
    q.push((Reverse(0_u64), start));
    while let Some((Reverse(d), n)) = q.pop() {
        if d > limit {
            continue;
        }
        if n == goal {
            return true;
        }
        if best.get(&n).is_some_and(|v| *v <= d) {
            continue;
        }
        best.insert(n, d);
        for &(m, w) in g.get(&n).into_iter().flatten() {
            if let Some(nd) = d.checked_add(w).filter(|v| *v <= limit) {
                q.push((Reverse(nd), m));
            }
        }
    }
    false
}
fn connected_settlement_count(world: &CompiledWorld) -> usize {
    let incident = world
        .edges
        .iter()
        .flat_map(|e| [e.from_node_id, e.to_node_id])
        .collect::<HashSet<_>>();
    world
        .settlements
        .iter()
        .filter(|s| incident.contains(&s.source_node_id))
        .count()
}
fn stable_id(from: u64, to: u64, existing: &HashSet<u64>, accepted: &[TravelEdgeImport]) -> u64 {
    for salt in 0_u32.. {
        let mut h = Sha256::new();
        h.update(b"adventuresim-inferred-road-v9");
        h.update(from.to_le_bytes());
        h.update(to.to_le_bytes());
        h.update(salt.to_le_bytes());
        let raw = u64::from_le_bytes(h.finalize()[..8].try_into().unwrap()) | (1_u64 << 63);
        if !existing.contains(&raw) && accepted.iter().all(|e| e.id != raw) {
            return raw;
        }
    }
    unreachable!()
}
fn haversine_m(a: (f64, f64), b: (f64, f64)) -> u32 {
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    (6_371_000.0 * 2.0 * h.sqrt().asin()).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distance_and_ids_are_reorder_stable() {
        assert_eq!(haversine_m((52.0, 10.0), (52.05, 10.0)), 5_560);
        let ids = HashSet::new();
        assert_eq!(stable_id(1, 2, &ids, &[]), stable_id(1, 2, &ids, &[]));
        assert_ne!(stable_id(1, 2, &ids, &[]), stable_id(1, 3, &ids, &[]));
        assert_eq!(stable_id(1, 2, &ids, &[]) >> 63, 1);
    }

    #[test]
    fn existing_short_graph_route_suppresses_clique_edge() {
        let graph = HashMap::from([
            (1, vec![(2, 4_000)]),
            (2, vec![(1, 4_000), (3, 4_000)]),
            (3, vec![(2, 4_000)]),
        ]);
        assert!(shortest_within(&graph, 1, 3, 9_000));
        assert!(!shortest_within(&graph, 1, 3, 7_999));
    }
}
