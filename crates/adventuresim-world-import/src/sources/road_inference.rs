//! Conservative rules-v8 recovery of missing local land links.
//!
//! Viabundus lacks road geometry and the runtime raster pack is a separate
//! artifact. Candidates therefore use source-independent walking semantics on
//! canonical endpoint cover/elevation, then receive the ordinary GLO-30 route
//! profile in the following compiler stage. Any endpoint water evidence is a
//! hard rejection: this system never invents a bridge, ford, or ferry.

use std::collections::{BTreeSet, HashMap};

use adventuresim_world_schema::{
    CompiledWorld, DerivedHistoricalVegetationCover, FallbackHistoricalVegetationCover,
    HistoricalVegetation, LandRoute, TravelEdgeImport, TravelEdgeProvenance, TravelRoute,
};

use crate::{Error, Result};

const MAX_CANDIDATES_PER_SETTLEMENT: usize = 8;
const MAX_INFERRED_DEGREE: u8 = 2;
const MAX_DISTANCE_M: u32 = 12_000;
const MAX_WALK_MINUTES: u32 = 360;

pub(crate) fn enrich(mut world: CompiledWorld) -> Result<CompiledWorld> {
    let occupied = world
        .edges
        .iter()
        .map(|e| ordered(e.from_node_id, e.to_node_id))
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    for (index, a) in world.settlements.iter().enumerate() {
        let mut nearest = world
            .settlements
            .iter()
            .skip(index + 1)
            .filter_map(|b| {
                let pair = ordered(a.source_node_id, b.source_node_id);
                if occupied.contains(&pair) {
                    return None;
                }
                let distance_m = haversine_m(a.longitude, a.latitude, b.longitude, b.latitude);
                let water = a.hydrology != Default::default() || b.hydrology != Default::default();
                let speed = endpoint_speed(a.historical_vegetation)
                    .min(endpoint_speed(b.historical_vegetation));
                let elevation_delta = a.elevation.get().abs_diff(b.elevation.get());
                assess(pair.0, pair.1, distance_m, speed, elevation_delta, water)
            })
            .collect::<Vec<_>>();
        nearest.sort_by_key(|c| (c.walk_minutes, c.distance_m, c.from, c.to));
        candidates.extend(nearest.into_iter().take(MAX_CANDIDATES_PER_SETTLEMENT));
    }
    candidates.sort_by_key(|c| (c.walk_minutes, c.distance_m, c.from, c.to));
    candidates.dedup_by_key(|c| (c.from, c.to));
    let mut degree = HashMap::<u64, u8>::new();
    let mut ids = world.edges.iter().map(|e| e.id).collect::<BTreeSet<_>>();
    for candidate in candidates {
        if degree.get(&candidate.from).copied().unwrap_or(0) >= MAX_INFERRED_DEGREE
            || degree.get(&candidate.to).copied().unwrap_or(0) >= MAX_INFERRED_DEGREE
        {
            continue;
        }
        let id = stable_id(candidate.from, candidate.to);
        if !ids.insert(id) {
            return Err(Error::Validation(
                "inferred road ID collides with canonical topology".into(),
            ));
        }
        *degree.entry(candidate.from).or_default() += 1;
        *degree.entry(candidate.to).or_default() += 1;
        world.edges.push(TravelEdgeImport {
            id, from_node_id: candidate.from, to_node_id: candidate.to,
            route: TravelRoute::Land(LandRoute { bridge: None, water_crossings: vec![] }),
            provenance: TravelEdgeProvenance::InferredWalkingLink,
            toll: None, length_m: candidate.distance_m, slope_multiplier: 1.0,
            terrain: adventuresim_world_schema::RouteTerrain::stage_placeholder(), certainty: 1,
            section: "inferred-local-link-v8".into(),
            sources: format!("- **Road gap-fill rules v8:** Deterministic local walking link between canonical settlement nodes `{}` and `{}` ({} m; estimated {} minutes before GLO-30 route profiling). Both endpoints lacked EU-Hydro adjacency, no crossing/bridge/ferry was invented, candidates were limited to eight neighbors, and inferred degree was capped at two.", candidate.from, candidate.to, candidate.distance_m, candidate.walk_minutes),
        });
    }
    world.edges.sort_by_key(|e| e.id);
    world.report.edges = world.edges.len();
    Ok(world)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Candidate {
    from: u64,
    to: u64,
    distance_m: u32,
    walk_minutes: u32,
}

fn assess(
    from: u64,
    to: u64,
    distance_m: u32,
    speed_mph: u32,
    elevation_delta_m: u16,
    crosses_water: bool,
) -> Option<Candidate> {
    if from == to
        || crosses_water
        || distance_m == 0
        || distance_m > MAX_DISTANCE_M
        || speed_mph == 0
    {
        return None;
    }
    let slope_penalty =
        1_000u64 + u64::from(elevation_delta_m).saturating_mul(2_000) / u64::from(distance_m);
    let minutes = u64::from(distance_m)
        .saturating_mul(60)
        .saturating_mul(slope_penalty)
        .div_ceil(u64::from(speed_mph) * 1_000);
    (minutes <= u64::from(MAX_WALK_MINUTES)).then_some(Candidate {
        from,
        to,
        distance_m,
        walk_minutes: minutes as u32,
    })
}

fn endpoint_speed(cover: HistoricalVegetation) -> u32 {
    match cover {
        HistoricalVegetation::Derived(v) => match v.cover {
            DerivedHistoricalVegetationCover::Wetland(_)
            | DerivedHistoricalVegetationCover::TransitionalWater => 500,
            DerivedHistoricalVegetationCover::Woodland(_) => 750,
            _ => 1_250,
        },
        HistoricalVegetation::Fallback(v) => match v.cover {
            FallbackHistoricalVegetationCover::Woodland(_) => 750,
            _ => 1_250,
        },
        HistoricalVegetation::Direct(_) => 1_000,
    }
}

fn ordered(a: u64, b: u64) -> (u64, u64) {
    if a <= b { (a, b) } else { (b, a) }
}
fn stable_id(a: u64, b: u64) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for byte in a.to_le_bytes().into_iter().chain(b.to_le_bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100000001b3);
    }
    h | (1u64 << 63)
}
fn haversine_m(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> u32 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    (6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt()))
        .round()
        .clamp(0.0, u32::MAX as f64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn easy_pair_is_accepted_and_bounds_are_hard() {
        assert!(assess(1, 2, 5_000, 1_250, 20, false).is_some());
        assert!(assess(1, 2, 20_000, 1_250, 0, false).is_none());
        assert!(assess(1, 2, 5_000, 500, 500, false).is_none());
    }
    #[test]
    fn water_never_invents_a_bridge() {
        assert!(assess(1, 2, 1_000, 1_250, 0, true).is_none());
    }
    #[test]
    fn ids_and_order_are_stable() {
        assert_eq!(stable_id(3, 9), stable_id(3, 9));
        assert_ne!(stable_id(3, 9), stable_id(9, 3));
    }
}
