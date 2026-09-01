use adventuresim_building_generator::{
    BuildingArchetype, BuildingCollision, BuildingPlan, BuildingProgram,
    compile_building_collision, generate,
};
use bevy::{math::Vec2, prelude::Component};
use serde::{Deserialize, Serialize};

use super::{GeneratedObstacle, SceneInputError, invalid};
use crate::city_layout::MAX_CITY_LOTS;

const MAX_TACTICAL_BUILDINGS: usize = 64;
const LEVEL_MARGIN_METRES: f32 = 1.5;
const TERRACE_APRON_METRES: f32 = 4.0;
const PARTY_WALL_PROJECTION_ALLOWANCE_METRES: f32 = 0.4;

/// Rotation of one orthogonal building grid in the settlement plane.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildingOrientation(f32);

impl BuildingOrientation {
    pub const IDENTITY: Self = Self(0.0);

    pub fn from_radians(yaw_radians: f32) -> Option<Self> {
        yaw_radians.is_finite().then(|| {
            Self(
                (yaw_radians + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU)
                    - core::f32::consts::PI,
            )
        })
    }

    pub fn from_frontage_tangent(tangent: Vec2) -> Option<Self> {
        (tangent.is_finite() && tangent.length_squared() > f32::EPSILON).then(|| {
            let tangent = tangent.normalize();
            Self((-tangent.y).atan2(tangent.x))
        })
    }

    pub fn yaw_radians(self) -> f32 {
        self.0
    }

    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 >= -core::f32::consts::PI && self.0 < core::f32::consts::PI
    }

    pub fn local_to_world(self, vector: Vec2) -> Vec2 {
        let (sine, cosine) = self.0.sin_cos();
        Vec2::new(
            cosine * vector.x + sine * vector.y,
            -sine * vector.x + cosine * vector.y,
        )
    }

    pub fn world_to_local(self, vector: Vec2) -> Vec2 {
        let (sine, cosine) = self.0.sin_cos();
        Vec2::new(
            cosine * vector.x - sine * vector.y,
            sine * vector.x + cosine * vector.y,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalBuildingPlacement {
    pub id: u64,
    pub program: BuildingProgram,
    pub centre_metres: Vec2,
    pub orientation: BuildingOrientation,
}

/// Compact presentation-only building outside the authoritative tactical area.
///
/// Distant buildings deliberately carry a curated recipe key instead of a
/// complete [`BuildingProgram`]. Clients reconstruct and batch their shell LODs;
/// the tactical server never gives them collision or simulation state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistantBuildingPlacement {
    pub id: u64,
    pub archetype: BuildingArchetype,
    pub seed: u64,
    pub centre_metres: Vec2,
    pub base_elevation_metres: f32,
    pub orientation: BuildingOrientation,
}

impl DistantBuildingPlacement {
    pub fn program(self) -> BuildingProgram {
        BuildingProgram::fixture(self.archetype, self.seed)
    }
}

#[derive(Clone, Debug, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
#[serde(deny_unknown_fields)]
pub struct SceneBuilding {
    pub id: u64,
    pub program: BuildingProgram,
    pub orientation: BuildingOrientation,
}

/// Compact identity and dimensions for one server-authoritative operable leaf.
#[derive(Clone, Copy, Debug, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
pub struct SceneDoor {
    pub building_id: u64,
    pub opening_id: u64,
    pub size_metres: bevy::math::Vec3,
    pub doorway_centre_metres: bevy::math::Vec3,
    pub tangent: bevy::math::Vec3,
    pub outward: bevy::math::Vec3,
}

/// Compact identity and dimensions for one server-authoritative window casement.
#[derive(Clone, Copy, Debug, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
pub struct SceneWindow {
    pub building_id: u64,
    pub opening_id: u64,
    pub size_metres: bevy::math::Vec3,
    pub opening_centre_metres: bevy::math::Vec3,
    pub tangent: bevy::math::Vec3,
    pub outward: bevy::math::Vec3,
    pub barred: bool,
}

#[derive(Clone, Debug)]
pub struct GeneratedBuilding {
    pub placement: TacticalBuildingPlacement,
    pub plan: BuildingPlan,
    pub collision: BuildingCollision,
    pub pad_elevation_metres: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BuildingPad {
    pub centre: Vec2,
    pub half_extents: Vec2,
    pub orientation: BuildingOrientation,
    pub elevation_metres: f32,
}

impl BuildingPad {
    fn local_offset(self, point: Vec2) -> Vec2 {
        let offset = point - self.centre;
        self.orientation.world_to_local(offset)
    }

    pub(crate) fn contains_level_ground(self, point: Vec2) -> bool {
        let offset = self.local_offset(point).abs();
        offset
            .cmple(self.half_extents + Vec2::splat(LEVEL_MARGIN_METRES))
            .all()
    }

    pub(super) fn contains_apron(self, point: Vec2) -> bool {
        let offset = self.local_offset(point).abs();
        offset
            .cmple(self.half_extents + Vec2::splat(LEVEL_MARGIN_METRES + TERRACE_APRON_METRES))
            .all()
    }

    fn blend_weight(self, point: Vec2) -> f32 {
        let core = self.half_extents + Vec2::splat(LEVEL_MARGIN_METRES);
        let outside = (self.local_offset(point).abs() - core).max(Vec2::ZERO);
        let distance = outside.length();
        let linear = (1.0 - distance / TERRACE_APRON_METRES).clamp(0.0, 1.0);
        linear * linear * (3.0 - 2.0 * linear)
    }
}

pub(super) fn validate_building_placements(
    placements: &[TacticalBuildingPlacement],
) -> Result<(), SceneInputError> {
    if placements.len() > MAX_TACTICAL_BUILDINGS {
        return invalid("scene has too many tactical buildings");
    }
    let mut ids = std::collections::BTreeSet::new();
    for placement in placements {
        if placement.id == 0 || !ids.insert(placement.id) {
            return invalid("building identity is zero or duplicated");
        }
        if !placement.centre_metres.is_finite() || !placement.orientation.is_valid() {
            return invalid("building placement is invalid");
        }
    }
    Ok(())
}

pub(super) fn validate_distant_building_placements(
    placements: &[DistantBuildingPlacement],
) -> Result<(), SceneInputError> {
    if placements.len() > MAX_CITY_LOTS {
        return invalid("scene has too many distant buildings");
    }
    let mut ids = std::collections::BTreeSet::new();
    for placement in placements {
        if placement.id == 0 || !ids.insert(placement.id) {
            return invalid("distant building identity is zero or duplicated");
        }
        if !placement.centre_metres.is_finite()
            || !placement.base_elevation_metres.is_finite()
            || !placement.orientation.is_valid()
        {
            return invalid("distant building placement is invalid");
        }
    }
    Ok(())
}

pub(super) fn prepare_buildings(
    placements: &[TacticalBuildingPlacement],
) -> Result<Vec<GeneratedBuilding>, SceneInputError> {
    placements
        .iter()
        .cloned()
        .map(|placement| {
            let plan = generate(&placement.program).map_err(|error| {
                SceneInputError::Validation(format!(
                    "building {} program is invalid: {error}",
                    placement.id
                ))
            })?;
            let collision = compile_building_collision(&plan);
            Ok(GeneratedBuilding {
                placement,
                plan,
                collision,
                pad_elevation_metres: 0.0,
            })
        })
        .collect()
}

pub(super) fn validate_building_pads(
    buildings: &[GeneratedBuilding],
) -> Result<(), SceneInputError> {
    let footprints = buildings
        .iter()
        .map(|building| {
            let half_extents = (building.collision.bounds.plan_half_extents()
                - Vec2::splat(PARTY_WALL_PROJECTION_ALLOWANCE_METRES))
            .max(Vec2::splat(0.1));
            (
                building.placement.id,
                building.placement.centre_metres,
                half_extents,
                building.placement.orientation,
            )
        })
        .collect::<Vec<_>>();
    for (index, &(id, centre, half_extents, orientation)) in footprints.iter().enumerate() {
        for &(other_id, other_centre, other_half_extents, other_orientation) in
            &footprints[index + 1..]
        {
            if oriented_rectangles_overlap(
                centre,
                half_extents,
                orientation,
                other_centre,
                other_half_extents,
                other_orientation,
            ) {
                return invalid(format!(
                    "building {id} footprint overlaps building {other_id}"
                ));
            }
        }
    }
    Ok(())
}

fn oriented_rectangles_overlap(
    first_centre: Vec2,
    first_half_extents: Vec2,
    first_orientation: BuildingOrientation,
    second_centre: Vec2,
    second_half_extents: Vec2,
    second_orientation: BuildingOrientation,
) -> bool {
    let first_axes = [
        first_orientation.local_to_world(Vec2::X),
        first_orientation.local_to_world(Vec2::Y),
    ];
    let second_axes = [
        second_orientation.local_to_world(Vec2::X),
        second_orientation.local_to_world(Vec2::Y),
    ];
    let centre_delta = second_centre - first_centre;
    first_axes.into_iter().chain(second_axes).all(|axis| {
        let first_radius = first_half_extents.x * axis.dot(first_axes[0]).abs()
            + first_half_extents.y * axis.dot(first_axes[1]).abs();
        let second_radius = second_half_extents.x * axis.dot(second_axes[0]).abs()
            + second_half_extents.y * axis.dot(second_axes[1]).abs();
        centre_delta.dot(axis).abs() < first_radius + second_radius
    })
}

pub(super) fn level_building_pads(
    grid_width: usize,
    grid_depth: usize,
    spacing: f32,
    heights: &mut [f32],
    buildings: &mut [GeneratedBuilding],
) -> (Vec<BuildingPad>, u32) {
    let half_extent = Vec2::new(
        (grid_width - 1) as f32 * spacing,
        (grid_depth - 1) as f32 * spacing,
    ) * 0.5;
    let mut pads = Vec::with_capacity(buildings.len());
    let mut adjusted = 0u32;
    for building in buildings {
        let mut pad = BuildingPad {
            centre: building.placement.centre_metres,
            half_extents: building.collision.bounds.plan_half_extents(),
            orientation: building.placement.orientation,
            elevation_metres: 0.0,
        };
        let mut covered = sample_indices(grid_width, grid_depth, spacing, half_extent)
            .filter(|(_, point)| {
                let offset = pad.local_offset(*point).abs();
                offset.cmple(pad.half_extents).all()
            })
            .map(|(index, _)| heights[index])
            .collect::<Vec<_>>();
        covered.sort_by(f32::total_cmp);
        pad.elevation_metres = covered.get(covered.len() / 2).copied().unwrap_or_else(|| {
            nearest_height(grid_width, grid_depth, spacing, heights, pad.centre)
        });
        for (index, point) in sample_indices(grid_width, grid_depth, spacing, half_extent) {
            let weight = pad.blend_weight(point);
            if weight <= 0.0 {
                continue;
            }
            let previous = heights[index];
            heights[index] = previous + (pad.elevation_metres - previous) * weight;
            adjusted += u32::from((heights[index] - previous).abs() > f32::EPSILON);
        }
        building.pad_elevation_metres = pad.elevation_metres;
        pads.push(pad);
    }
    (pads, adjusted)
}

fn sample_indices(
    width: usize,
    depth: usize,
    spacing: f32,
    half_extent: Vec2,
) -> impl Iterator<Item = (usize, Vec2)> {
    (0..width * depth).map(move |index| {
        let point =
            Vec2::new((index % width) as f32, (index / width) as f32) * spacing - half_extent;
        (index, point)
    })
}

fn nearest_height(width: usize, depth: usize, spacing: f32, heights: &[f32], point: Vec2) -> f32 {
    let half_extent = Vec2::new((width - 1) as f32 * spacing, (depth - 1) as f32 * spacing) * 0.5;
    let grid = ((point + half_extent) / spacing).round();
    let x = (grid.x as isize).clamp(0, width as isize - 1) as usize;
    let z = (grid.y as isize).clamp(0, depth as isize - 1) as usize;
    heights[z * width + x]
}

pub(super) fn obstacle_intersects_building(
    obstacle: GeneratedObstacle,
    obstacle_spacing: f32,
    terrain_extent: Vec2,
    pads: &[BuildingPad],
) -> bool {
    let (x, z) = match obstacle {
        GeneratedObstacle::Tree { x, z } | GeneratedObstacle::Rock { x, z, .. } => (x, z),
    };
    let point = Vec2::new(f32::from(x), f32::from(z)) * obstacle_spacing - terrain_extent * 0.5;
    pads.iter().any(|pad| pad.contains_apron(point))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_orientation_is_finite_canonical_and_rejects_a_zero_frontage() {
        let wrapped = BuildingOrientation::from_radians(core::f32::consts::TAU + 0.25).unwrap();
        assert!((wrapped.yaw_radians() - 0.25).abs() < 0.000_001);
        assert!(BuildingOrientation::from_radians(f32::NAN).is_none());
        assert!(BuildingOrientation::from_frontage_tangent(Vec2::ZERO).is_none());
    }

    #[test]
    fn oriented_pad_overlap_uses_both_local_frames() {
        let identity = BuildingOrientation::IDENTITY;
        let diagonal = BuildingOrientation::from_radians(core::f32::consts::FRAC_PI_4).unwrap();
        let half_extents = Vec2::new(4.0, 1.0);

        assert!(oriented_rectangles_overlap(
            Vec2::ZERO,
            half_extents,
            identity,
            Vec2::new(2.0, 0.0),
            half_extents,
            diagonal,
        ));
        assert!(!oriented_rectangles_overlap(
            Vec2::ZERO,
            half_extents,
            identity,
            Vec2::new(0.0, 5.0),
            half_extents,
            diagonal,
        ));
    }
}
