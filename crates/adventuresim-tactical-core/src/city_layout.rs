//! Deterministic urban frontage around one connected, gently warped street network.

use std::collections::{BTreeMap, BTreeSet};

use bevy::math::Vec2;
use fabelgeist_determinism::mix64;

use crate::scene_input::BuildingOrientation;

mod houses;
mod surfaces;

pub use houses::CityHouseClass;
pub use surfaces::{
    CityStreetPatch, CityStreetSurface, CityYardPatch, CityYardSurface, MAX_CITY_STREET_PATCHES,
    MAX_CITY_YARD_PATCHES,
};
use surfaces::{city_street_patches, city_yard_patches};

const STREET_LINE_COUNT: usize = 18;
const BLOCK_COUNT: usize = STREET_LINE_COUNT - 1;
const NOMINAL_BLOCK_METRES: f32 = 72.0;
const CITY_RADIUS_X_METRES: f32 = 610.0;
const CITY_RADIUS_Y_METRES: f32 = 570.0;
const STREET_LINE_JITTER_METRES: f32 = 8.0;
const STREET_CURVE_METRES: f32 = 6.0;
const ORDINARY_STREET_HALF_WIDTH_METRES: f32 = 3.5;
const SECONDARY_STREET_HALF_WIDTH_METRES: f32 = 4.5;
const PRIMARY_STREET_HALF_WIDTH_METRES: f32 = 6.0;
const FRONTAGE_CORNER_CLEARANCE_METRES: f32 = 7.0;
const PARTY_WALL_CLEARANCE_METRES: f32 = 0.12;
const REAR_COURT_EDGE_CLEARANCE_METRES: f32 = 27.5;
const REAR_COURT_PRIORITY_PENALTY: u32 = 7;
const MAXIMUM_HOUSE_DEPTH_METRES: f32 = 19.5;
const SPATIAL_BUCKET_METRES: f32 = 32.0;
const CENTRAL_MARKET_BLOCK: (usize, usize) = (BLOCK_COUNT / 2, BLOCK_COUNT / 2);
const STREET_GEOMETRY_DOMAIN: u64 = 0x7374_7265_6574_6765;
const HOUSE_CLASS_DOMAIN: u64 = 0x686f_7573_655f_636c;
const DEVELOPMENT_DOMAIN: u64 = 0x6465_7665_6c6f_706d;
pub const MAX_CITY_LOTS: usize = 8_192;

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedCityLayout {
    pub lots: Vec<CityBuildingLot>,
    pub streets: Vec<CityStreetPatch>,
    pub yards: Vec<CityYardPatch>,
}

/// One rectangular building lot aligned to one locally straight street frontage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CityBuildingLot {
    pub id: u64,
    pub centre_metres: Vec2,
    pub orientation: BuildingOrientation,
    pub house_class: CityHouseClass,
}

#[derive(Clone, Copy)]
struct CandidateLot {
    lot: CityBuildingLot,
    block_key: u64,
    rear_court: bool,
    selection_key: u64,
}

#[derive(Clone, Copy)]
struct CityBlock {
    row: usize,
    column: usize,
    corners: [Vec2; 4],
}

impl CityBlock {
    fn centre(self) -> Vec2 {
        self.corners.into_iter().sum::<Vec2>() * 0.25
    }

    fn key(self) -> u64 {
        ((self.row as u64) << 32) | self.column as u64
    }

    fn is_market(self) -> bool {
        (self.row, self.column) == CENTRAL_MARKET_BLOCK
    }
}

/// Builds one nested population-scaled city without regard to presentation or collision.
pub fn generate_city(seed: u64, resident_population: u32) -> GeneratedCityLayout {
    let nodes = street_nodes(seed);
    let mut candidates = city_blocks(nodes)
        .filter(|block| block_is_inside_city(*block) && !block.is_market())
        .flat_map(|block| block_lots(seed, block))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| {
        (
            candidate.rear_court,
            candidate.selection_key,
            candidate.block_key,
        )
    });
    let mut candidates = remove_overlapping_candidates(candidates);
    candidates.sort_by_key(|candidate| {
        let radial_band = (candidate.lot.centre_metres.length() / NOMINAL_BLOCK_METRES) as u32
            + u32::from(candidate.rear_court) * REAR_COURT_PRIORITY_PENALTY;
        (radial_band, candidate.block_key, candidate.selection_key)
    });

    let target_population = resident_population.max(1);
    let mut represented_population = 0_u32;
    let mut selected = Vec::new();
    for candidate in candidates.into_iter().take(MAX_CITY_LOTS) {
        if represented_population >= target_population {
            break;
        }
        let mut lot = candidate.lot;
        lot.id = selected.len() as u64 + 1;
        represented_population =
            represented_population.saturating_add(lot.house_class.resident_capacity());
        selected.push(CandidateLot { lot, ..candidate });
    }
    let developed_blocks = selected
        .iter()
        .map(|candidate| candidate.block_key)
        .collect::<BTreeSet<_>>();
    GeneratedCityLayout {
        lots: selected
            .into_iter()
            .map(|candidate| candidate.lot)
            .collect(),
        streets: city_street_patches(nodes, &developed_blocks),
        yards: city_yard_patches(seed, nodes, &developed_blocks),
    }
}

fn street_nodes(seed: u64) -> [[Vec2; STREET_LINE_COUNT]; STREET_LINE_COUNT] {
    core::array::from_fn(|row| {
        core::array::from_fn(|column| {
            let centred_column = column as f32 - (STREET_LINE_COUNT - 1) as f32 * 0.5;
            let centred_row = row as f32 - (STREET_LINE_COUNT - 1) as f32 * 0.5;
            let column_key = mix64(seed ^ STREET_GEOMETRY_DOMAIN ^ column as u64);
            let row_key = mix64(seed ^ STREET_GEOMETRY_DOMAIN ^ (row as u64).rotate_left(31));
            let column_shift = signed_sample(column_key) * STREET_LINE_JITTER_METRES;
            let row_shift = signed_sample(row_key) * STREET_LINE_JITTER_METRES;
            let vertical_phase = signed_sample(column_key.rotate_left(17)) * core::f32::consts::PI;
            let horizontal_phase = signed_sample(row_key.rotate_left(23)) * core::f32::consts::PI;
            Vec2::new(
                centred_column * NOMINAL_BLOCK_METRES
                    + column_shift
                    + (centred_row * 0.48 + vertical_phase).sin() * STREET_CURVE_METRES,
                centred_row * NOMINAL_BLOCK_METRES
                    + row_shift
                    + (centred_column * 0.44 + horizontal_phase).sin() * STREET_CURVE_METRES,
            )
        })
    })
}

fn city_blocks(
    nodes: [[Vec2; STREET_LINE_COUNT]; STREET_LINE_COUNT],
) -> impl Iterator<Item = CityBlock> {
    (0..BLOCK_COUNT).flat_map(move |row| {
        (0..BLOCK_COUNT).map(move |column| CityBlock {
            row,
            column,
            corners: [
                nodes[row][column],
                nodes[row][column + 1],
                nodes[row + 1][column + 1],
                nodes[row + 1][column],
            ],
        })
    })
}

fn block_is_inside_city(block: CityBlock) -> bool {
    let centre = block.centre();
    (centre.x / CITY_RADIUS_X_METRES).powi(2) + (centre.y / CITY_RADIUS_Y_METRES).powi(2) <= 1.0
}

fn block_lots(seed: u64, block: CityBlock) -> Vec<CandidateLot> {
    let mut lots = Vec::new();
    let edges = [
        (block.corners[0], block.corners[1], block.row),
        (block.corners[1], block.corners[2], block.column + 1),
        (block.corners[2], block.corners[3], block.row + 1),
        (block.corners[3], block.corners[0], block.column),
    ];
    for (edge_index, (start, end, street_line)) in edges.into_iter().enumerate() {
        append_frontage(
            &mut lots,
            seed,
            block.key() ^ (edge_index as u64).rotate_left(48),
            block.key(),
            start,
            end,
            street_half_width(street_line),
        );
    }
    append_rear_court(&mut lots, seed, block);
    lots
}

fn append_frontage(
    lots: &mut Vec<CandidateLot>,
    seed: u64,
    run_key: u64,
    block_key: u64,
    start: Vec2,
    end: Vec2,
    street_half_width: f32,
) {
    let displacement = end - start;
    let length = displacement.length();
    let tangent = displacement / length;
    let inward = Vec2::new(-tangent.y, tangent.x);
    let mut cursor = FRONTAGE_CORNER_CLEARANCE_METRES;
    let mut index = 0_u64;
    loop {
        let lot_key = run_key ^ index.rotate_left(19);
        let house_class = house_class(seed, lot_key, false);
        let frontage = house_class.frontage_width_metres();
        if cursor + frontage > length - FRONTAGE_CORNER_CLEARANCE_METRES {
            break;
        }
        let street_point = start + tangent * (cursor + frontage * 0.5);
        lots.push(candidate(
            seed,
            lot_key,
            block_key,
            street_point + inward * (street_half_width + house_class.depth_metres() * 0.5),
            tangent,
            house_class,
            false,
        ));
        cursor += frontage + PARTY_WALL_CLEARANCE_METRES;
        index += 1;
    }
}

fn append_rear_court(lots: &mut Vec<CandidateLot>, seed: u64, block: CityBlock) {
    let horizontal =
        ((block.corners[1] - block.corners[0]) + (block.corners[2] - block.corners[3])) * 0.5;
    let vertical =
        ((block.corners[3] - block.corners[0]) + (block.corners[2] - block.corners[1])) * 0.5;
    let (axis, cross_extent) = if horizontal.length() >= vertical.length() {
        (horizontal, vertical.length())
    } else {
        (vertical, horizontal.length())
    };
    let required_cross_extent = 2.0
        * (PRIMARY_STREET_HALF_WIDTH_METRES + MAXIMUM_HOUSE_DEPTH_METRES)
        + CityHouseClass::Cottage.depth_metres();
    if cross_extent < required_cross_extent {
        return;
    }
    let length = axis.length();
    let tangent = axis / length;
    let start = block.centre() - tangent * length * 0.5;
    let mut cursor = REAR_COURT_EDGE_CLEARANCE_METRES;
    let mut index = 0_u64;
    while cursor + CityHouseClass::Cottage.frontage_width_metres()
        <= length - REAR_COURT_EDGE_CLEARANCE_METRES
    {
        let lot_key = block.key() ^ 0x7265_6172_0000_0000 ^ index.rotate_left(19);
        let house_class = house_class(seed, lot_key, true);
        let frontage = house_class.frontage_width_metres();
        if cursor + frontage > length - REAR_COURT_EDGE_CLEARANCE_METRES {
            break;
        }
        lots.push(candidate(
            seed,
            lot_key,
            block.key(),
            start + tangent * (cursor + frontage * 0.5),
            tangent,
            house_class,
            true,
        ));
        cursor += frontage + PARTY_WALL_CLEARANCE_METRES;
        index += 1;
    }
}

fn candidate(
    seed: u64,
    lot_key: u64,
    block_key: u64,
    centre_metres: Vec2,
    frontage_tangent: Vec2,
    house_class: CityHouseClass,
    rear_court: bool,
) -> CandidateLot {
    CandidateLot {
        lot: CityBuildingLot {
            id: lot_key,
            centre_metres,
            orientation: BuildingOrientation::from_frontage_tangent(frontage_tangent)
                .expect("street frontage tangent is finite and nonzero"),
            house_class,
        },
        block_key,
        rear_court,
        selection_key: mix64(seed ^ DEVELOPMENT_DOMAIN ^ lot_key),
    }
}

fn remove_overlapping_candidates(candidates: Vec<CandidateLot>) -> Vec<CandidateLot> {
    let mut accepted = Vec::<CandidateLot>::with_capacity(candidates.len());
    let mut buckets = BTreeMap::<(i32, i32), Vec<usize>>::new();
    for candidate in candidates {
        let bucket = spatial_bucket(candidate.lot.centre_metres);
        let overlaps = (-1..=1).any(|row_offset| {
            (-1..=1).any(|column_offset| {
                buckets
                    .get(&(bucket.0 + column_offset, bucket.1 + row_offset))
                    .into_iter()
                    .flatten()
                    .any(|&index| lots_overlap(candidate.lot, accepted[index].lot))
            })
        });
        if !overlaps {
            let index = accepted.len();
            accepted.push(candidate);
            buckets.entry(bucket).or_default().push(index);
        }
    }
    accepted
}

fn lots_overlap(first: CityBuildingLot, second: CityBuildingLot) -> bool {
    let first_half = Vec2::new(
        first.house_class.frontage_width_metres(),
        first.house_class.depth_metres(),
    ) * 0.5;
    let second_half = Vec2::new(
        second.house_class.frontage_width_metres(),
        second.house_class.depth_metres(),
    ) * 0.5;
    let first_axes = [
        first.orientation.local_to_world(Vec2::X),
        first.orientation.local_to_world(Vec2::Y),
    ];
    let second_axes = [
        second.orientation.local_to_world(Vec2::X),
        second.orientation.local_to_world(Vec2::Y),
    ];
    let centre_delta = second.centre_metres - first.centre_metres;
    first_axes.into_iter().chain(second_axes).all(|axis| {
        let first_radius = first_half.x * axis.dot(first_axes[0]).abs()
            + first_half.y * axis.dot(first_axes[1]).abs();
        let second_radius = second_half.x * axis.dot(second_axes[0]).abs()
            + second_half.y * axis.dot(second_axes[1]).abs();
        centre_delta.dot(axis).abs() < first_radius + second_radius
    })
}

fn spatial_bucket(point: Vec2) -> (i32, i32) {
    (
        (point.x / SPATIAL_BUCKET_METRES).floor() as i32,
        (point.y / SPATIAL_BUCKET_METRES).floor() as i32,
    )
}

fn street_half_width(line_index: usize) -> f32 {
    if line_index == CENTRAL_MARKET_BLOCK.0 || line_index == CENTRAL_MARKET_BLOCK.0 + 1 {
        PRIMARY_STREET_HALF_WIDTH_METRES
    } else if line_index.is_multiple_of(4) {
        SECONDARY_STREET_HALF_WIDTH_METRES
    } else {
        ORDINARY_STREET_HALF_WIDTH_METRES
    }
}

fn street_surface(line_index: usize) -> CityStreetSurface {
    if line_index == CENTRAL_MARKET_BLOCK.0 || line_index == CENTRAL_MARKET_BLOCK.0 + 1 {
        CityStreetSurface::Fieldstone
    } else if line_index.is_multiple_of(4) {
        CityStreetSurface::Gravel
    } else {
        CityStreetSurface::CompactedEarth
    }
}

fn house_class(seed: u64, lot_key: u64, rear_court: bool) -> CityHouseClass {
    let sample = mix64(seed ^ HOUSE_CLASS_DOMAIN ^ lot_key);
    if rear_court {
        return if sample.is_multiple_of(5) {
            CityHouseClass::CraftTownHouse
        } else {
            CityHouseClass::Cottage
        };
    }
    match sample % 16 {
        0..=4 => CityHouseClass::Cottage,
        5..=11 => CityHouseClass::CraftTownHouse,
        12..=13 => CityHouseClass::HallHouse,
        _ => CityHouseClass::MerchantHouse,
    }
}

fn signed_sample(sample: u64) -> f32 {
    (sample as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_lots_are_deterministic_nested_and_follow_many_connected_street_segments() {
        let small = generate_city(42, 8_000);
        let large = generate_city(42, 40_000);
        assert_eq!(small, generate_city(42, 8_000));
        assert_eq!(small.lots, large.lots[..small.lots.len()]);
        let mut headings = large
            .lots
            .iter()
            .map(|lot| (lot.orientation.yaw_radians().to_degrees() / 2.0).round() as i16)
            .collect::<Vec<_>>();
        headings.sort_unstable();
        headings.dedup();
        assert!(headings.len() >= 12, "headings={headings:?}");
    }

    #[test]
    fn population_is_represented_by_physical_house_capacity() {
        for population in [900, 6_500, 40_000] {
            let lots = generate_city(42, population).lots;
            let capacity = lots
                .iter()
                .map(|lot| lot.house_class.resident_capacity())
                .sum::<u32>();
            assert!(
                capacity >= population,
                "population={population} capacity={capacity} lots={}",
                lots.len()
            );
            assert!(capacity < population + 30);
        }
    }

    #[test]
    fn the_street_graph_shares_intersections_and_reserves_only_the_market_block() {
        let nodes = street_nodes(42);
        let blocks = city_blocks(nodes).collect::<Vec<_>>();
        for row in 0..BLOCK_COUNT {
            for column in 0..BLOCK_COUNT - 1 {
                let left = blocks[row * BLOCK_COUNT + column];
                let right = blocks[row * BLOCK_COUNT + column + 1];
                assert_eq!(left.corners[1], right.corners[0]);
                assert_eq!(left.corners[2], right.corners[3]);
            }
        }
        assert_eq!(blocks.iter().filter(|block| block.is_market()).count(), 1);
    }

    #[test]
    fn accepted_building_footprints_do_not_overlap() {
        let lots = generate_city(42, 40_000).lots;
        for (index, lot) in lots.iter().enumerate() {
            assert!(
                lots[index + 1..]
                    .iter()
                    .all(|other| !lots_overlap(*lot, *other))
            );
        }
    }

    #[test]
    fn building_south_side_faces_out_of_its_block() {
        let block = city_blocks(street_nodes(42)).next().unwrap();
        let tangent = (block.corners[1] - block.corners[0]).normalize();
        let inward = Vec2::new(-tangent.y, tangent.x);
        let orientation = BuildingOrientation::from_frontage_tangent(tangent).unwrap();
        assert!(orientation.local_to_world(-Vec2::Y).dot(-inward) > 0.999);
    }

    #[test]
    fn street_surfaces_are_mixed_grass_free_patches_from_the_same_graph() {
        let city = generate_city(42, 40_000);
        assert!(city.streets.len() > 100);
        for surface in [
            CityStreetSurface::CompactedEarth,
            CityStreetSurface::Gravel,
            CityStreetSurface::Fieldstone,
        ] {
            assert!(city.streets.iter().any(|patch| patch.surface() == surface));
        }
        assert_eq!(
            city.streets
                .iter()
                .filter(|patch| matches!(patch, CityStreetPatch::Market { .. }))
                .count(),
            1
        );
        for patch in &city.streets {
            let centre = match *patch {
                CityStreetPatch::Corridor {
                    start_metres,
                    end_metres,
                    ..
                } => (start_metres + end_metres) * 0.5,
                CityStreetPatch::Market { corners_metres, .. } => {
                    corners_metres.into_iter().sum::<Vec2>() * 0.25
                }
            };
            assert!(patch.contains(centre));
        }
    }

    #[test]
    fn every_selected_lot_belongs_to_a_deterministic_developed_yard() {
        let city = generate_city(42, 40_000);
        assert!(!city.yards.is_empty());
        assert!(city.yards.iter().all(|yard| yard.is_valid()));
        assert!(
            city.yards
                .iter()
                .any(|yard| yard.surface == CityYardSurface::PackedEarth)
        );
        assert!(
            city.yards
                .iter()
                .any(|yard| yard.surface == CityYardSurface::KitchenGarden)
        );
        assert!(city.lots.iter().all(|lot| {
            city.yards
                .iter()
                .any(|yard| yard.contains(lot.centre_metres))
        }));
    }
}
