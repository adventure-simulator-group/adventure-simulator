//! Deterministic downscaling of HYDE cropland km² to canonical EPSG:3035
//! one-kilometre squares.
//!
//! The allocator is source-agnostic. The map compiler supplies HYDE quotas and
//! candidate facts sampled from its already validated terrain, settlement,
//! road, water, wetland, and canopy inputs.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

pub const CULTIVATION_RULES_VERSION: u16 = 2;
pub const MAX_CULTIVATION_CANDIDATES: usize = 5_000_000;
const DISTANCE_BUCKET_M: i64 = 10_000;
const MAX_DISTANCE_SEGMENT_REFERENCES: usize = 20_000_000;
const MAX_INTERIOR_CAPACITY_SHORTFALL_KM2: u32 = 2;
const MAX_INTERIOR_CAPACITY_SHORTFALL_PERCENT: u32 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricSegment {
    pub from: [i64; 2],
    pub to: [i64; 2],
}

/// Bounded uniform-grid index for exact point-to-segment distance queries.
#[derive(Clone, Debug)]
pub struct SegmentDistanceIndex {
    segments: Vec<MetricSegment>,
    buckets: BTreeMap<(i64, i64), Vec<usize>>,
}

impl SegmentDistanceIndex {
    pub fn new(segments: Vec<MetricSegment>) -> Result<Self, String> {
        let mut buckets = BTreeMap::<_, Vec<_>>::new();
        let mut references = 0usize;
        for (index, segment) in segments.iter().enumerate() {
            let min_x = segment.from[0]
                .min(segment.to[0])
                .div_euclid(DISTANCE_BUCKET_M);
            let max_x = segment.from[0]
                .max(segment.to[0])
                .div_euclid(DISTANCE_BUCKET_M);
            let min_y = segment.from[1]
                .min(segment.to[1])
                .div_euclid(DISTANCE_BUCKET_M);
            let max_y = segment.from[1]
                .max(segment.to[1])
                .div_euclid(DISTANCE_BUCKET_M);
            let count = usize::try_from((max_x - min_x + 1) * (max_y - min_y + 1))
                .map_err(|_| "distance index reference count overflow")?;
            references = references
                .checked_add(count)
                .ok_or("distance index reference count overflow")?;
            if references > MAX_DISTANCE_SEGMENT_REFERENCES {
                return Err("distance index exceeds its segment-reference bound".into());
            }
            for bucket_y in min_y..=max_y {
                for bucket_x in min_x..=max_x {
                    buckets.entry((bucket_x, bucket_y)).or_default().push(index);
                }
            }
        }
        Ok(Self { segments, buckets })
    }

    pub fn nearest_distance_m(&self, point: [i64; 2], cap_m: u32) -> u32 {
        self.nearest_distance_with_checks(point, cap_m).0
    }

    pub fn nearest_distance_with_checks(&self, point: [i64; 2], cap_m: u32) -> (u32, usize) {
        let radius = (i64::from(cap_m) + DISTANCE_BUCKET_M - 1) / DISTANCE_BUCKET_M;
        let center = (
            point[0].div_euclid(DISTANCE_BUCKET_M),
            point[1].div_euclid(DISTANCE_BUCKET_M),
        );
        let mut seen = BTreeSet::new();
        let mut best = f64::from(cap_m);
        let mut checks = 0;
        for bucket_y in center.1 - radius..=center.1 + radius {
            for bucket_x in center.0 - radius..=center.0 + radius {
                for &index in self
                    .buckets
                    .get(&(bucket_x, bucket_y))
                    .into_iter()
                    .flatten()
                {
                    if seen.insert(index) {
                        checks += 1;
                        best = best.min(point_segment_distance(point, self.segments[index]));
                    }
                }
            }
        }
        (best.round().clamp(0.0, f64::from(cap_m)) as u32, checks)
    }
}

fn point_segment_distance(point: [i64; 2], segment: MetricSegment) -> f64 {
    let px = point[0] as f64;
    let py = point[1] as f64;
    let ax = segment.from[0] as f64;
    let ay = segment.from[1] as f64;
    let dx = (segment.to[0] - segment.from[0]) as f64;
    let dy = (segment.to[1] - segment.from[1]) as f64;
    let length_sq = dx * dx + dy * dy;
    if length_sq == 0.0 {
        return (px - ax).hypot(py - ay);
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / length_sq).clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CultivationCell {
    pub column: i64,
    pub row: i64,
}

impl CultivationCell {
    fn neighbours(self) -> [Self; 4] {
        [
            Self {
                column: self.column - 1,
                row: self.row,
            },
            Self {
                column: self.column + 1,
                row: self.row,
            },
            Self {
                column: self.column,
                row: self.row - 1,
            },
            Self {
                column: self.column,
                row: self.row + 1,
            },
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CultivationCandidate {
    pub cell: CultivationCell,
    pub hyde_cell: (i16, i16),
    pub usable_land: bool,
    pub settlement_distance_m: u32,
    pub road_distance_m: u32,
    pub water_distance_m: u32,
    pub slope_permille: u16,
    pub relief_m: u16,
    pub canopy_percent: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct HydeCropQuota {
    pub cell: (i16, i16),
    /// Raw 1544-interpolated HYDE cropland area. It is deliberately not a
    /// settlement-profile percentage.
    pub crop_km2: f64,
    /// Whether the playable bounds clip this source cell. Canonical one-
    /// kilometre squares cannot exactly tessellate an arbitrary geographic
    /// boundary, so only clipped edge cells may saturate at their usable
    /// square capacity.
    pub boundary_clipped: bool,
}

/// Require at least three quarters of the explicitly sampled square to be
/// non-water passable land. HYDE's historical cropland is authoritative over
/// Jung potential-natural wetland, but mapped water remains ineligible.
/// Center-point classification is insufficient at coasts and water boundaries.
pub fn square_is_usable(non_water_samples: u16, total_samples: u16) -> bool {
    total_samples > 0 && u32::from(non_water_samples) * 4 >= u32::from(total_samples) * 3
}

#[derive(Clone, Debug, PartialEq)]
pub struct CultivationAllocation {
    pub cells: BTreeSet<CultivationCell>,
    pub rounded_quotas: BTreeMap<(i16, i16), u32>,
    pub residual_km2: f64,
    pub capacity_limited_km2: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Priority {
    score: i64,
    cell: CultivationCell,
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .cmp(&other.score)
            .then_with(|| Reverse(self.cell).cmp(&Reverse(other.cell)))
    }
}
impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn enqueue_fallback(
    hyde_cell: (i16, i16),
    candidates: &[CultivationCandidate],
    grouped: &BTreeMap<(i16, i16), Vec<usize>>,
    cursors: &mut BTreeMap<(i16, i16), usize>,
    enqueued: &mut BTreeSet<CultivationCell>,
    frontier: &mut BinaryHeap<Priority>,
    frontier_counts: &mut BTreeMap<(i16, i16), usize>,
) {
    let Some(indices) = grouped.get(&hyde_cell) else {
        return;
    };
    let cursor = cursors.entry(hyde_cell).or_default();
    while let Some(&index) = indices.get(*cursor) {
        *cursor += 1;
        let candidate = candidates[index];
        if enqueued.insert(candidate.cell) {
            frontier.push(Priority {
                score: desirability(&candidate),
                cell: candidate.cell,
            });
            *frontier_counts.entry(hyde_cell).or_default() += 1;
            return;
        }
    }
}

/// Largest-remainder rounding preserves the rounded global HYDE area and
/// leaves an absolute residual below half a square kilometre.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundedHydeQuotas {
    pub by_cell: BTreeMap<(i16, i16), u32>,
    pub residual_km2: f64,
}

pub fn round_quotas(quotas: &[HydeCropQuota]) -> Result<RoundedHydeQuotas, String> {
    if quotas
        .iter()
        .any(|quota| !quota.crop_km2.is_finite() || quota.crop_km2 < 0.0)
    {
        return Err("HYDE crop quota is negative or non-finite".into());
    }
    let target = quotas
        .iter()
        .map(|quota| quota.crop_km2)
        .sum::<f64>()
        .round() as u64;
    let mut values = quotas
        .iter()
        .map(|quota| {
            (
                quota.cell,
                quota.crop_km2.floor() as u32,
                quota.crop_km2.fract(),
            )
        })
        .collect::<Vec<_>>();
    let floors = values.iter().map(|row| u64::from(row.1)).sum::<u64>();
    values.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    for row in values
        .iter_mut()
        .take(target.saturating_sub(floors) as usize)
    {
        row.1 += 1;
    }
    let rounded = values.into_iter().map(|row| (row.0, row.1)).collect();
    Ok(RoundedHydeQuotas {
        by_cell: rounded,
        residual_km2: quotas.iter().map(|quota| quota.crop_km2).sum::<f64>() - target as f64,
    })
}

pub fn allocate(
    candidates: &[CultivationCandidate],
    quotas: &[HydeCropQuota],
) -> Result<CultivationAllocation, String> {
    if candidates.is_empty() || candidates.len() > MAX_CULTIVATION_CANDIDATES {
        return Err("cultivation candidate grid is empty or exceeds its bound".into());
    }
    let RoundedHydeQuotas {
        by_cell: mut rounded_quotas,
        residual_km2,
    } = round_quotas(quotas)?;
    let by_cell = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.cell, index))
        .collect::<BTreeMap<_, _>>();
    if by_cell.len() != candidates.len() {
        return Err("cultivation candidate grid contains duplicate cells".into());
    }
    let usable_by_quota = candidates
        .iter()
        .filter(|candidate| candidate.usable_land)
        .fold(BTreeMap::<_, u32>::new(), |mut counts, candidate| {
            *counts.entry(candidate.hyde_cell).or_default() += 1;
            counts
        });
    let boundary_clipped = quotas
        .iter()
        .map(|quota| (quota.cell, quota.boundary_clipped))
        .collect::<BTreeMap<_, _>>();
    let mut capacity_limited_km2 = 0u32;
    for (&cell, quota) in &mut rounded_quotas {
        let usable = usable_by_quota.get(&cell).copied().unwrap_or(0);
        if *quota > usable {
            let shortfall = *quota - usable;
            let within_interior_tolerance = shortfall <= MAX_INTERIOR_CAPACITY_SHORTFALL_KM2
                && u64::from(shortfall) * 100
                    <= u64::from(*quota) * u64::from(MAX_INTERIOR_CAPACITY_SHORTFALL_PERCENT);
            if boundary_clipped.get(&cell).copied().unwrap_or(false) || within_interior_tolerance {
                capacity_limited_km2 = capacity_limited_km2
                    .checked_add(shortfall)
                    .ok_or("cultivation capacity loss overflow")?;
                *quota = usable;
                continue;
            }
            return Err(format!(
                "HYDE cell {cell:?} requests {quota} cultivated km2 but has only {usable} usable canonical squares",
            ));
        }
    }

    let mut grouped = BTreeMap::<(i16, i16), Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.usable_land {
            grouped.entry(candidate.hyde_cell).or_default().push(index);
        }
    }
    for indices in grouped.values_mut() {
        indices.sort_by_key(|&index| {
            (
                Reverse(desirability(&candidates[index])),
                candidates[index].cell,
            )
        });
    }

    let mut selected = BTreeSet::new();
    let mut used = BTreeMap::<(i16, i16), u32>::new();
    let mut frontier = BinaryHeap::new();
    let mut frontier_counts = BTreeMap::<(i16, i16), usize>::new();
    let mut enqueued = BTreeSet::new();
    let mut cursors = BTreeMap::<(i16, i16), usize>::new();

    // Seed all settlement-adjacent candidates in one linear pass.
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.usable_land && candidate.settlement_distance_m <= 1_500)
        .filter(|candidate| {
            rounded_quotas
                .get(&candidate.hyde_cell)
                .copied()
                .unwrap_or(0)
                > 0
        })
    {
        if enqueued.insert(candidate.cell) {
            frontier.push(Priority {
                score: desirability(candidate),
                cell: candidate.cell,
            });
            *frontier_counts.entry(candidate.hyde_cell).or_default() += 1;
        }
    }
    for (&hyde_cell, &quota) in &rounded_quotas {
        if quota > 0 && frontier_counts.get(&hyde_cell).copied().unwrap_or(0) == 0 {
            enqueue_fallback(
                hyde_cell,
                candidates,
                &grouped,
                &mut cursors,
                &mut enqueued,
                &mut frontier,
                &mut frontier_counts,
            );
        }
    }

    let total_target = rounded_quotas
        .values()
        .map(|value| u64::from(*value))
        .sum::<u64>();
    while selected.len() < total_target as usize {
        let Some(priority) = frontier.pop() else {
            return Err("cultivation region growth exhausted before filling HYDE quotas".into());
        };
        let candidate = candidates[by_cell[&priority.cell]];
        let remaining_frontier = frontier_counts
            .get(&candidate.hyde_cell)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        frontier_counts.insert(candidate.hyde_cell, remaining_frontier);
        if selected.contains(&candidate.cell)
            || used.get(&candidate.hyde_cell).copied().unwrap_or(0)
                >= rounded_quotas
                    .get(&candidate.hyde_cell)
                    .copied()
                    .unwrap_or(0)
        {
            if used.get(&candidate.hyde_cell).copied().unwrap_or(0)
                < rounded_quotas
                    .get(&candidate.hyde_cell)
                    .copied()
                    .unwrap_or(0)
                && frontier_counts
                    .get(&candidate.hyde_cell)
                    .copied()
                    .unwrap_or(0)
                    == 0
            {
                enqueue_fallback(
                    candidate.hyde_cell,
                    candidates,
                    &grouped,
                    &mut cursors,
                    &mut enqueued,
                    &mut frontier,
                    &mut frontier_counts,
                );
            }
            continue;
        }
        selected.insert(candidate.cell);
        *used.entry(candidate.hyde_cell).or_default() += 1;
        for neighbour in candidate.cell.neighbours() {
            let Some(&index) = by_cell.get(&neighbour) else {
                continue;
            };
            let next = candidates[index];
            if next.usable_land && enqueued.insert(next.cell) {
                // Adjacency dominates marginal geographic desirability, yielding
                // coherent four-neighbour districts instead of road ribbons.
                frontier.push(Priority {
                    score: desirability(&next) + 2_000_000,
                    cell: next.cell,
                });
                *frontier_counts.entry(next.hyde_cell).or_default() += 1;
            }
        }
        if used.get(&candidate.hyde_cell).copied().unwrap_or(0)
            < rounded_quotas
                .get(&candidate.hyde_cell)
                .copied()
                .unwrap_or(0)
            && frontier_counts
                .get(&candidate.hyde_cell)
                .copied()
                .unwrap_or(0)
                == 0
        {
            enqueue_fallback(
                candidate.hyde_cell,
                candidates,
                &grouped,
                &mut cursors,
                &mut enqueued,
                &mut frontier,
                &mut frontier_counts,
            );
        }
    }
    Ok(CultivationAllocation {
        cells: selected,
        rounded_quotas,
        residual_km2,
        capacity_limited_km2,
    })
}

fn desirability(candidate: &CultivationCandidate) -> i64 {
    let settlement = 900_000_i64.saturating_sub(i64::from(candidate.settlement_distance_m) * 90);
    // Roads help only in a short settlement catchment; this cannot create a
    // high-scoring ribbon between distant towns.
    let road = if candidate.settlement_distance_m <= 8_000 {
        180_000_i64.saturating_sub(i64::from(candidate.road_distance_m) * 90)
    } else {
        0
    };
    // Water access peaks around 500 m. Banks and wet ground are excluded by
    // usable_land before scoring.
    let water = 120_000_i64
        .saturating_sub((i64::from(candidate.water_distance_m).saturating_sub(500)).abs() * 120);
    settlement + road + water
        - i64::from(candidate.slope_permille) * 2_500
        - i64::from(candidate.relief_m) * 600
        - i64::from(candidate.canopy_percent) * 250
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_growth_crosses_hyde_boundary_and_fills_shared_quotas() {
        let candidates = (0..6)
            .map(|column| CultivationCandidate {
                cell: CultivationCell { column, row: 0 },
                hyde_cell: if column < 3 { (0, 0) } else { (0, 1) },
                usable_land: true,
                settlement_distance_m: if column == 1 { 0 } else { 2_000 },
                road_distance_m: 100,
                water_distance_m: 500,
                slope_permille: 0,
                relief_m: 0,
                canopy_percent: 0,
            })
            .collect::<Vec<_>>();
        let result = allocate(
            &candidates,
            &[
                HydeCropQuota {
                    cell: (0, 0),
                    crop_km2: 2.4,
                    boundary_clipped: false,
                },
                HydeCropQuota {
                    cell: (0, 1),
                    crop_km2: 1.6,
                    boundary_clipped: false,
                },
            ],
        )
        .unwrap();
        assert_eq!(result.cells.len(), 4);
        assert!(result.cells.iter().all(|cell| {
            cell.neighbours()
                .iter()
                .any(|neighbour| result.cells.contains(neighbour))
        }));
    }

    #[test]
    fn impossible_coastal_quota_is_reported() {
        let candidates = [CultivationCandidate {
            cell: CultivationCell { column: 0, row: 0 },
            hyde_cell: (0, 0),
            usable_land: false,
            settlement_distance_m: 0,
            road_distance_m: 0,
            water_distance_m: 0,
            slope_permille: 0,
            relief_m: 0,
            canopy_percent: 0,
        }];
        assert!(
            allocate(
                &candidates,
                &[HydeCropQuota {
                    cell: (0, 0),
                    crop_km2: 1.0,
                    boundary_clipped: false,
                }]
            )
            .unwrap_err()
            .contains("usable")
        );
    }

    #[test]
    fn clipped_boundary_quota_saturates_at_usable_square_capacity() {
        let candidates = [
            CultivationCandidate {
                cell: CultivationCell { column: 0, row: 0 },
                hyde_cell: (0, 0),
                usable_land: true,
                settlement_distance_m: 0,
                road_distance_m: 0,
                water_distance_m: 500,
                slope_permille: 0,
                relief_m: 0,
                canopy_percent: 0,
            },
            CultivationCandidate {
                cell: CultivationCell { column: 1, row: 0 },
                hyde_cell: (0, 0),
                usable_land: false,
                settlement_distance_m: 0,
                road_distance_m: 0,
                water_distance_m: 500,
                slope_permille: 0,
                relief_m: 0,
                canopy_percent: 0,
            },
        ];
        let result = allocate(
            &candidates,
            &[HydeCropQuota {
                cell: (0, 0),
                crop_km2: 4.0,
                boundary_clipped: true,
            }],
        )
        .unwrap();
        assert_eq!(result.cells.len(), 1);
        assert_eq!(result.rounded_quotas[&(0, 0)], 1);
        assert_eq!(result.capacity_limited_km2, 3);
    }

    #[test]
    fn clipped_boundary_quota_may_saturate_to_zero() {
        let candidates = [CultivationCandidate {
            cell: CultivationCell { column: 0, row: 0 },
            hyde_cell: (0, 0),
            usable_land: false,
            settlement_distance_m: 0,
            road_distance_m: 0,
            water_distance_m: 500,
            slope_permille: 0,
            relief_m: 0,
            canopy_percent: 0,
        }];
        let result = allocate(
            &candidates,
            &[HydeCropQuota {
                cell: (0, 0),
                crop_km2: 2.0,
                boundary_clipped: true,
            }],
        )
        .unwrap();
        assert!(result.cells.is_empty());
        assert_eq!(result.rounded_quotas[&(0, 0)], 0);
        assert_eq!(result.capacity_limited_km2, 2);
    }

    #[test]
    fn small_interior_grid_capacity_shortfall_is_bounded() {
        let candidates = (0..47)
            .map(|column| CultivationCandidate {
                cell: CultivationCell { column, row: 0 },
                hyde_cell: (0, 0),
                usable_land: true,
                settlement_distance_m: 0,
                road_distance_m: 0,
                water_distance_m: 500,
                slope_permille: 0,
                relief_m: 0,
                canopy_percent: 0,
            })
            .collect::<Vec<_>>();
        let result = allocate(
            &candidates,
            &[HydeCropQuota {
                cell: (0, 0),
                crop_km2: 49.0,
                boundary_clipped: false,
            }],
        )
        .unwrap();
        assert_eq!(result.cells.len(), 47);
        assert_eq!(result.capacity_limited_km2, 2);
    }

    #[test]
    fn larger_interior_capacity_shortfall_remains_an_error() {
        let candidates = (0..46)
            .map(|column| CultivationCandidate {
                cell: CultivationCell { column, row: 0 },
                hyde_cell: (0, 0),
                usable_land: true,
                settlement_distance_m: 0,
                road_distance_m: 0,
                water_distance_m: 500,
                slope_permille: 0,
                relief_m: 0,
                canopy_percent: 0,
            })
            .collect::<Vec<_>>();
        assert!(
            allocate(
                &candidates,
                &[HydeCropQuota {
                    cell: (0, 0),
                    crop_km2: 49.0,
                    boundary_clipped: false,
                }],
            )
            .unwrap_err()
            .contains("usable canonical squares")
        );
    }

    #[test]
    fn segment_distance_uses_lines_not_only_vertices() {
        let index = SegmentDistanceIndex::new(vec![MetricSegment {
            from: [0, 0],
            to: [10_000, 0],
        }])
        .unwrap();
        assert_eq!(index.nearest_distance_m([5_000, 300], 10_000), 300);
    }

    #[test]
    fn distance_index_query_work_is_local_at_representative_scale() {
        let segments = (0..20_000)
            .map(|index| MetricSegment {
                from: [i64::from(index) * 20_000, 0],
                to: [i64::from(index) * 20_000 + 1_000, 0],
            })
            .collect();
        let index = SegmentDistanceIndex::new(segments).unwrap();
        let (_, checks) = index.nearest_distance_with_checks([200_000_000, 200], 10_000);
        assert!(checks <= 3, "query checked {checks} distant segments");
    }

    #[test]
    fn coastal_boundaries_require_three_quarters_non_water_coverage() {
        assert!(square_is_usable(12, 16));
        assert!(!square_is_usable(11, 16));
        assert!(!square_is_usable(0, 16));
    }
}
