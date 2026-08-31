//! Operable casements compiled from accepted window opening assemblies.

use std::collections::BTreeMap;

use bevy::math::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    BuildingPlan, ClosureKind, ClosureState, OpeningAssemblyId, OpeningUse, ResolvedItemId,
    ResolvedSolid, SolidRole,
};

const CASEMENT_OPEN_ANGLE_RADIANS: f32 = 80.0 * core::f32::consts::PI / 180.0;

/// One inward-opening glazed casement in building-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowSpec {
    pub opening: OpeningAssemblyId,
    pub source: ResolvedItemId,
    pub closed_centre: Vec3,
    pub hinge_centre: Vec3,
    pub size_metres: Vec3,
    pub closed_yaw_radians: f32,
    pub tangent: Vec2,
    pub outward: Vec2,
    pub open_angle_radians: f32,
    pub barred: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowBarSpec {
    pub source: ResolvedItemId,
    pub centre: Vec3,
    pub size_metres: Vec3,
    pub yaw_radians: f32,
}

pub fn compile_window_bars(plan: &BuildingPlan) -> Vec<WindowBarSpec> {
    plan.opening_assemblies
        .iter()
        .filter(|opening| opening.closure.layers.contains(&ClosureKind::IronBars))
        .flat_map(|opening| {
            let width = opening.profile.interior_width_metres();
            let height = opening.profile.clear_height_metres();
            let count = if width >= 0.9 { 3 } else { 2 };
            (0..count).map(move |index| {
                let fraction = (index + 1) as f32 / (count + 1) as f32;
                let offset = (fraction - 0.5) * width;
                let plan_position = opening.frame.origin + opening.frame.tangent * offset;
                WindowBarSpec {
                    source: ResolvedItemId((7_u64 << 60) | (opening.id.0 << 8) | index as u64),
                    centre: Vec3::new(
                        plan_position.x,
                        opening.sill_elevation_metres + height * 0.5,
                        plan_position.y,
                    ),
                    size_metres: Vec3::new(0.035, height, 0.035),
                    yaw_radians: -opening.frame.tangent.y.atan2(opening.frame.tangent.x),
                }
            })
        })
        .collect()
}

pub fn compile_operable_windows(plan: &BuildingPlan) -> Vec<WindowSpec> {
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<BTreeMap<_, _>>();
    plan.opening_assemblies
        .iter()
        .filter(|opening| {
            opening.use_kind == OpeningUse::Window
                && opening.closure.state == ClosureState::Operable
                && opening.frame.inside_room.is_some()
                && opening.frame.outside_room.is_none()
        })
        .filter_map(|opening| {
            let solid = opening.closure_solids.iter().find_map(|id| {
                solids
                    .get(id)
                    .copied()
                    .filter(|solid| solid.role == SolidRole::LeadedGlazing)
            })?;
            window_from_solid(
                opening.id,
                opening.frame.tangent,
                opening.frame.outward,
                opening.closure.layers.contains(&ClosureKind::IronBars),
                solid,
            )
        })
        .collect()
}

fn window_from_solid(
    opening: OpeningAssemblyId,
    tangent: Vec2,
    outward: Vec2,
    barred: bool,
    solid: &ResolvedSolid,
) -> Option<WindowSpec> {
    let tangent = tangent.normalize_or_zero();
    let outward = outward.normalize_or_zero();
    if tangent == Vec2::ZERO || outward == Vec2::ZERO {
        return None;
    }
    let width = tangent.x.abs() * solid.size.x + tangent.y.abs() * solid.size.z;
    let thickness = outward.x.abs() * solid.size.x + outward.y.abs() * solid.size.z;
    let hinge_centre = solid.centre - Vec3::new(tangent.x, 0.0, tangent.y) * width * 0.5;
    let positive_swing = Quat::from_rotation_y(0.01) * Vec3::new(tangent.x, 0.0, tangent.y);
    let enters_room = Vec2::new(positive_swing.x, positive_swing.z).dot(-outward) > 0.0;
    Some(WindowSpec {
        opening,
        source: solid.id,
        closed_centre: solid.centre,
        hinge_centre,
        size_metres: Vec3::new(width, solid.size.y, thickness.max(0.025)),
        closed_yaw_radians: -tangent.y.atan2(tangent.x),
        tangent,
        outward,
        open_angle_radians: if enters_room {
            CASEMENT_OPEN_ANGLE_RADIANS
        } else {
            -CASEMENT_OPEN_ANGLE_RADIANS
        },
        barred,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingArchetype, BuildingProgram, generate};

    #[test]
    fn civilian_seed_matrix_contains_operable_fixed_and_barred_windows() {
        let mut operable = 0;
        let mut fixed = 0;
        let mut barred = 0;
        for seed in 0..20 {
            let plan = generate(&BuildingProgram::fixture(
                BuildingArchetype::TownHouse,
                seed,
            ))
            .expect("town house");
            let windows = compile_operable_windows(&plan);
            operable += windows.len();
            barred += windows.iter().filter(|window| window.barred).count();
            fixed += plan
                .opening_assemblies
                .iter()
                .filter(|opening| {
                    opening.use_kind == OpeningUse::Window
                        && opening.closure.state == ClosureState::Closed
                })
                .count();
        }
        assert!(operable > 0);
        assert!(fixed > 0);
        assert!(barred > 0);
    }

    #[test]
    fn operable_windows_swing_toward_the_inside() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        for window in compile_operable_windows(&plan) {
            let closed_arm = Vec3::new(window.tangent.x, 0.0, window.tangent.y);
            let open_arm = Quat::from_rotation_y(window.open_angle_radians) * closed_arm;
            assert!(Vec2::new(open_arm.x, open_arm.z).dot(-window.outward) > 0.0);
        }
    }
}
