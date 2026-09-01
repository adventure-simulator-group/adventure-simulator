//! Semantic ground patches derived from the developed city blocks.

use std::collections::BTreeSet;

use bevy::{math::Vec2, prelude::Reflect};
use fabelgeist_determinism::mix64;
use serde::{Deserialize, Serialize};

use super::*;

const YARD_SURFACE_DOMAIN: u64 = 0x7961_7264_5f73_7572;
pub const MAX_CITY_STREET_PATCHES: usize = 2_048;
pub const MAX_CITY_YARD_PATCHES: usize = BLOCK_COUNT * BLOCK_COUNT;

/// Historically plausible surface treatment for one part of the urban street network.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityStreetSurface {
    CompactedEarth,
    Gravel,
    Fieldstone,
}

impl CityStreetSurface {
    pub const fn priority(self) -> u8 {
        match self {
            Self::CompactedEarth => 0,
            Self::Gravel => 1,
            Self::Fieldstone => 2,
        }
    }
}

/// One bounded surface patch in the connected street network.
#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "shape")]
pub enum CityStreetPatch {
    Corridor {
        start_metres: Vec2,
        end_metres: Vec2,
        half_width_metres: f32,
        surface: CityStreetSurface,
    },
    Market {
        corners_metres: [Vec2; 4],
        surface: CityStreetSurface,
    },
}

impl CityStreetPatch {
    pub fn surface(self) -> CityStreetSurface {
        match self {
            Self::Corridor { surface, .. } | Self::Market { surface, .. } => surface,
        }
    }

    pub fn contains(self, point: Vec2) -> bool {
        match self {
            Self::Corridor {
                start_metres,
                end_metres,
                half_width_metres,
                ..
            } => {
                let displacement = end_metres - start_metres;
                let fraction = ((point - start_metres).dot(displacement)
                    / displacement.length_squared())
                .clamp(0.0, 1.0);
                point.distance_squared(start_metres + displacement * fraction)
                    <= half_width_metres * half_width_metres
            }
            Self::Market { corners_metres, .. } => convex_quad_contains(corners_metres, point),
        }
    }

    pub fn is_valid(self) -> bool {
        match self {
            Self::Corridor {
                start_metres,
                end_metres,
                half_width_metres,
                ..
            } => {
                start_metres.is_finite()
                    && end_metres.is_finite()
                    && start_metres.distance_squared(end_metres) > 1.0
                    && half_width_metres.is_finite()
                    && (1.0..=20.0).contains(&half_width_metres)
            }
            Self::Market { corners_metres, .. } => corners_metres.into_iter().all(Vec2::is_finite),
        }
    }
}

/// Surface treatment inside one developed urban block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityYardSurface {
    PackedEarth,
    KitchenGarden,
}

/// One developed block interior beneath its buildings and rear courts.
#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityYardPatch {
    pub corners_metres: [Vec2; 4],
    pub surface: CityYardSurface,
}

impl CityYardPatch {
    pub fn contains(self, point: Vec2) -> bool {
        convex_quad_contains(self.corners_metres, point)
    }

    pub fn is_valid(self) -> bool {
        self.corners_metres.into_iter().all(Vec2::is_finite)
            && polygon_area(self.corners_metres).abs() > 1.0
    }
}

pub(super) fn city_yard_patches(
    seed: u64,
    nodes: [[Vec2; STREET_LINE_COUNT]; STREET_LINE_COUNT],
    developed_blocks: &BTreeSet<u64>,
) -> Vec<CityYardPatch> {
    let patches = city_blocks(nodes)
        .filter(|block| developed_blocks.contains(&block.key()))
        .map(|block| {
            let surface_key = mix64(seed ^ YARD_SURFACE_DOMAIN ^ block.key());
            let radial_band = (block.centre().length() / NOMINAL_BLOCK_METRES) as u32;
            let surface = if radial_band >= 3 && surface_key % 7 == 0 {
                CityYardSurface::KitchenGarden
            } else {
                CityYardSurface::PackedEarth
            };
            CityYardPatch {
                corners_metres: block.corners,
                surface,
            }
        })
        .collect::<Vec<_>>();
    debug_assert!(patches.len() <= MAX_CITY_YARD_PATCHES);
    patches
}

pub(super) fn city_street_patches(
    nodes: [[Vec2; STREET_LINE_COUNT]; STREET_LINE_COUNT],
    developed_blocks: &BTreeSet<u64>,
) -> Vec<CityStreetPatch> {
    let mut edges = BTreeSet::<(usize, usize, usize, usize)>::new();
    for key in developed_blocks {
        let row = (key >> 32) as usize;
        let column = (*key as u32) as usize;
        edges.extend([
            (row, column, row, column + 1),
            (row, column + 1, row + 1, column + 1),
            (row + 1, column, row + 1, column + 1),
            (row, column, row + 1, column),
        ]);
    }
    let mut patches = edges
        .into_iter()
        .map(|(start_row, start_column, end_row, end_column)| {
            let line_index = if start_row == end_row {
                start_row
            } else {
                start_column
            };
            CityStreetPatch::Corridor {
                start_metres: nodes[start_row][start_column],
                end_metres: nodes[end_row][end_column],
                half_width_metres: street_half_width(line_index),
                surface: street_surface(line_index),
            }
        })
        .collect::<Vec<_>>();
    if !developed_blocks.is_empty() {
        let (row, column) = CENTRAL_MARKET_BLOCK;
        patches.push(CityStreetPatch::Market {
            corners_metres: [
                nodes[row][column],
                nodes[row][column + 1],
                nodes[row + 1][column + 1],
                nodes[row + 1][column],
            ],
            surface: CityStreetSurface::Fieldstone,
        });
    }
    debug_assert!(patches.len() <= MAX_CITY_STREET_PATCHES);
    patches
}

fn convex_quad_contains(corners: [Vec2; 4], point: Vec2) -> bool {
    let mut sign = 0.0_f32;
    for index in 0..4 {
        let start = corners[index];
        let end = corners[(index + 1) % 4];
        let cross = (end - start).perp_dot(point - start);
        if cross.abs() <= f32::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

fn polygon_area(corners: [Vec2; 4]) -> f32 {
    (0..4)
        .map(|index| corners[index].perp_dot(corners[(index + 1) % 4]))
        .sum::<f32>()
        * 0.5
}
