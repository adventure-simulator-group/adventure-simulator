use adventuresim_building_generator::BuildingArchetype;
use adventuresim_tactical_core::prelude::GeneratedBuilding;
use bevy::prelude::*;

use super::capture_state::InteriorCaptureCamera;

const REVIEW_ARCHETYPES: [BuildingArchetype; 4] = [
    BuildingArchetype::TownHouse,
    BuildingArchetype::HallHouse,
    BuildingArchetype::FachwerkCottage,
    BuildingArchetype::FachwerkMerchantHouse,
];

pub(super) fn capture_cameras(buildings: &[GeneratedBuilding]) -> Vec<InteriorCaptureCamera> {
    REVIEW_ARCHETYPES
        .into_iter()
        .flat_map(|archetype| {
            let building = buildings
                .iter()
                .find(|building| building.plan.archetype == archetype)
                .unwrap_or_else(|| panic!("interior-review fixture lacks {}", archetype.slug()));
            [
                camera_for_storey(building, 0, 0),
                camera_for_storey(building, 1, 1),
            ]
        })
        .collect()
}

fn camera_for_storey(
    building: &GeneratedBuilding,
    preferred_storey: usize,
    angle_variant: usize,
) -> InteriorCaptureCamera {
    let storey = building
        .plan
        .storeys
        .get(preferred_storey)
        .unwrap_or(&building.plan.storeys[0]);
    let room = storey
        .rooms
        .iter()
        .max_by_key(|room| room.cells.len())
        .expect("generated building storey has a room");
    let centroid =
        room.cells.iter().map(|cell| cell.centre()).sum::<Vec2>() / room.cells.len() as f32;
    let corner_axis = [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0)][angle_variant];
    let camera_cell = room
        .cells
        .iter()
        .min_by(|left, right| {
            left.centre()
                .dot(corner_axis)
                .total_cmp(&right.centre().dot(corner_axis))
        })
        .expect("generated room has a cell");
    let eye_height = f32::from(storey.level) * building.plan.storey_height_metres + 1.65;
    let local_origin = building.collision.bounds.centre();
    let floor_offset = local_origin.y - building.collision.bounds.min.y;
    let transform = Transform::from_xyz(
        building.placement.centre_metres.x,
        building.pad_elevation_metres + floor_offset,
        building.placement.centre_metres.y,
    )
    .with_rotation(Quat::from_rotation_y(
        building.placement.orientation.yaw_radians(),
    ));
    let eye = camera_cell.centre();
    let position = transform.transform_point(Vec3::new(eye.x, eye_height, eye.y) - local_origin);
    let target = transform
        .transform_point(Vec3::new(centroid.x, eye_height + 0.08, centroid.y) - local_origin);
    InteriorCaptureCamera { position, target }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_building_generator::{BuildingProgram, compile_building_collision, generate};
    use adventuresim_tactical_core::prelude::{BuildingOrientation, TacticalBuildingPlacement};

    #[test]
    fn city_archetypes_produce_two_finite_interior_cameras_each() {
        let placements = REVIEW_ARCHETYPES
            .into_iter()
            .enumerate()
            .map(|(index, archetype)| TacticalBuildingPlacement {
                id: index as u64 + 1,
                program: BuildingProgram::fixture(archetype, 42),
                centre_metres: Vec2::new(index as f32 * 30.0, 0.0),
                orientation: BuildingOrientation::IDENTITY,
            })
            .collect::<Vec<_>>();
        let buildings = placements
            .into_iter()
            .map(|placement| {
                let plan = generate(&placement.program).unwrap();
                let collision = compile_building_collision(&plan);
                GeneratedBuilding {
                    placement,
                    plan,
                    collision,
                    pad_elevation_metres: 0.0,
                }
            })
            .collect::<Vec<_>>();
        let cameras = capture_cameras(&buildings);

        assert_eq!(cameras.len(), REVIEW_ARCHETYPES.len() * 2);
        assert!(cameras.iter().all(|camera| {
            camera.position.is_finite()
                && camera.target.is_finite()
                && camera.position.distance(camera.target) > 0.5
        }));
    }
}
