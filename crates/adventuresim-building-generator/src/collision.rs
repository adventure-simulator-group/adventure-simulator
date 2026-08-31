//! Static tactical collision compiled from accepted semantic wall assemblies.
//!
//! Collision deliberately does not consume render LOD triangles. Wall host
//! solids preserve authoritative opening subtraction, while doors and other
//! operable closures remain available for separate dynamic collision entities.

use std::collections::{BTreeMap, BTreeSet};

use bevy::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{BuildingPlan, ResolvedItemId, ResolvedSolid};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollisionBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl CollisionBounds {
    pub fn centre(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn plan_half_extents(self) -> Vec2 {
        Vec2::new(self.max.x - self.min.x, self.max.z - self.min.z) * 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollisionCuboid {
    pub source: ResolvedItemId,
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw_radians: f32,
    pub crossfall_radians: f32,
    pub longfall_radians: f32,
}

impl CollisionCuboid {
    fn from_solid(solid: &ResolvedSolid) -> Self {
        Self {
            source: solid.id,
            centre: solid.centre,
            size: solid.size,
            yaw_radians: solid.yaw_radians,
            crossfall_radians: solid.crossfall_radians,
            longfall_radians: solid.longfall_radians,
        }
    }

    fn bounds(self) -> CollisionBounds {
        let half = self.size * 0.5;
        let sin = self.yaw_radians.sin().abs();
        let cos = self.yaw_radians.cos().abs();
        let rotated_half = Vec3::new(
            half.x * cos + half.z * sin,
            half.y,
            half.x * sin + half.z * cos,
        );
        CollisionBounds {
            min: self.centre - rotated_half,
            max: self.centre + rotated_half,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildingCollision {
    pub bounds: CollisionBounds,
    pub cuboids: Vec<CollisionCuboid>,
}

/// Compiles static collision from authoritative wall hosts and walkable timber
/// surfaces. Opening closures are intentionally excluded: an operable door is
/// a separate gameplay entity, not permanent masonry and not an LOD concern.
pub fn compile_building_collision(plan: &BuildingPlan) -> BuildingCollision {
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<BTreeMap<_, _>>();
    let mut selected = plan
        .wall_assemblies
        .iter()
        .filter(|wall| wall.replaced_by_owner.is_none())
        .flat_map(|wall| wall.host_solids.iter().copied())
        .collect::<BTreeSet<_>>();
    if let Some(frame) = &plan.timber_frame {
        selected.extend(
            frame
                .floors
                .iter()
                .flat_map(|floor| floor.floor_solids.iter().copied()),
        );
        selected.extend(frame.circulation.stair_solids.iter().copied());
        selected.extend(frame.circulation.landing_solids.iter().copied());
    }
    let cuboids = selected
        .into_iter()
        .filter_map(|id| solids.get(&id).copied())
        .map(CollisionCuboid::from_solid)
        .collect::<Vec<_>>();
    let bounds = collision_bounds(plan, &cuboids);
    BuildingCollision { bounds, cuboids }
}

fn collision_bounds(plan: &BuildingPlan, cuboids: &[CollisionCuboid]) -> CollisionBounds {
    let dimensions = plan.dimensions_metres();
    let fallback = CollisionBounds {
        min: Vec3::ZERO,
        max: Vec3::new(dimensions.x, 0.0, dimensions.y),
    };
    cuboids
        .iter()
        .copied()
        .map(CollisionCuboid::bounds)
        .fold(fallback, |bounds, cuboid| CollisionBounds {
            min: bounds.min.min(cuboid.min),
            max: bounds.max.max(cuboid.max),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingArchetype, BuildingProgram, OpeningUse, generate};

    #[test]
    fn collision_comes_from_wall_hosts_and_preserves_door_voids() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let collision = compile_building_collision(&plan);
        assert!(!collision.cuboids.is_empty());
        let door = plan
            .opening_assemblies
            .iter()
            .find(|opening| opening.use_kind == OpeningUse::Door)
            .expect("town house has a door");
        let door_point = Vec3::new(
            door.frame.origin.x,
            door.sill_elevation_metres + door.profile.clear_height_metres() * 0.45,
            door.frame.origin.y,
        );
        assert!(collision.cuboids.iter().all(|cuboid| {
            let offset = door_point - cuboid.centre;
            let local = bevy::math::Quat::from_rotation_y(-cuboid.yaw_radians) * offset;
            let half = cuboid.size * 0.5 - Vec3::splat(0.01);
            local.x.abs() > half.x || local.y.abs() > half.y || local.z.abs() > half.z
        }));
        assert!(collision.bounds.plan_half_extents().min_element() > 1.0);
    }

    #[test]
    fn timber_floors_and_stair_treads_receive_collision() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let frame = plan.timber_frame.as_ref().expect("merchant house frame");
        let collision = compile_building_collision(&plan);
        let sources = collision
            .cuboids
            .iter()
            .map(|cuboid| cuboid.source)
            .collect::<BTreeSet<_>>();

        assert!(
            frame
                .circulation
                .stair_solids
                .iter()
                .all(|id| sources.contains(id))
        );
        assert!(
            frame
                .circulation
                .landing_solids
                .iter()
                .all(|id| sources.contains(id))
        );
        assert!(
            frame
                .floors
                .iter()
                .all(|floor| { floor.floor_solids.iter().all(|id| sources.contains(id)) })
        );
    }
}
