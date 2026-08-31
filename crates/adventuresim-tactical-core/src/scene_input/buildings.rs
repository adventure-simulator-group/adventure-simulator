use adventuresim_building_generator::{
    BuildingArchetype, BuildingCollision, BuildingPlan, BuildingProgram,
    compile_building_collision, generate,
};
use bevy::{math::Vec2, prelude::Component};
use serde::{Deserialize, Serialize};

use super::{GeneratedObstacle, SceneInputError, invalid};

const MAX_TACTICAL_BUILDINGS: usize = 64;
const MAX_DISTANT_BUILDINGS: usize = 512;
const LEVEL_MARGIN_METRES: f32 = 1.5;
const TERRACE_APRON_METRES: f32 = 4.0;
const PAD_CLEARANCE_METRES: f32 = LEVEL_MARGIN_METRES + TERRACE_APRON_METRES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalBuildingPlacement {
    pub id: u64,
    pub program: BuildingProgram,
    pub centre_metres: Vec2,
    pub quarter_turns: u8,
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
    pub quarter_turns: u8,
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
    pub quarter_turns: u8,
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
    pub quarter_turns: u8,
    pub elevation_metres: f32,
}

impl BuildingPad {
    fn local_offset(self, point: Vec2) -> Vec2 {
        let offset = point - self.centre;
        match self.quarter_turns % 4 {
            0 => offset,
            1 => Vec2::new(offset.y, -offset.x),
            2 => -offset,
            3 => Vec2::new(-offset.y, offset.x),
            _ => unreachable!(),
        }
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
        if !placement.centre_metres.is_finite() || placement.quarter_turns >= 4 {
            return invalid("building placement is invalid");
        }
    }
    Ok(())
}

pub(super) fn validate_distant_building_placements(
    placements: &[DistantBuildingPlacement],
) -> Result<(), SceneInputError> {
    if placements.len() > MAX_DISTANT_BUILDINGS {
        return invalid("scene has too many distant buildings");
    }
    let mut ids = std::collections::BTreeSet::new();
    for placement in placements {
        if placement.id == 0 || !ids.insert(placement.id) {
            return invalid("distant building identity is zero or duplicated");
        }
        if !placement.centre_metres.is_finite()
            || !placement.base_elevation_metres.is_finite()
            || placement.quarter_turns >= 4
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
    terrain_half_extent: Vec2,
) -> Result<(), SceneInputError> {
    let footprints = buildings
        .iter()
        .map(|building| {
            let mut half_extents = building.collision.bounds.plan_half_extents();
            if building.placement.quarter_turns % 2 == 1 {
                half_extents = Vec2::new(half_extents.y, half_extents.x);
            }
            (
                building.placement.id,
                building.placement.centre_metres,
                half_extents + Vec2::splat(PAD_CLEARANCE_METRES),
            )
        })
        .collect::<Vec<_>>();
    for (index, &(id, centre, apron_half_extents)) in footprints.iter().enumerate() {
        if (centre.abs() + apron_half_extents)
            .cmpgt(terrain_half_extent)
            .any()
        {
            return invalid(format!(
                "building {id} terrace apron leaves playable terrain"
            ));
        }
        for &(other_id, other_centre, other_half_extents) in &footprints[index + 1..] {
            if (centre - other_centre)
                .abs()
                .cmplt(apron_half_extents + other_half_extents)
                .all()
            {
                return invalid(format!(
                    "building {id} terrace apron overlaps building {other_id}"
                ));
            }
        }
    }
    Ok(())
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
            quarter_turns: building.placement.quarter_turns,
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
