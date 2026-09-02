use adventuresim_building_generator::{BuildingArchetype, Direction, WALL_THICKNESS_METRES};
use adventuresim_tactical_core::prelude::GeneratedBuilding;
use bevy::prelude::*;

use super::capture_state::{BuildingReviewCamera, PlasterRakingLight};

const PLASTER_REVIEW_CAMERA_DISTANCE_METRES: f32 = 0.82;
const PLASTER_REVIEW_LIGHT_TANGENT_DISTANCE_METRES: f32 = 1.2;
const PLASTER_REVIEW_LIGHT_INCIDENCE_DEGREES: f32 = 10.0;

const REVIEW_ARCHETYPES: [BuildingArchetype; 4] = [
    BuildingArchetype::TownHouse,
    BuildingArchetype::HallHouse,
    BuildingArchetype::FachwerkCottage,
    BuildingArchetype::FachwerkMerchantHouse,
];

pub(super) fn capture_cameras(buildings: &[GeneratedBuilding]) -> Vec<BuildingReviewCamera> {
    let mut cameras = REVIEW_ARCHETYPES
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
        .collect::<Vec<_>>();
    let hall_house = buildings
        .iter()
        .find(|building| building.plan.archetype == BuildingArchetype::HallHouse)
        .expect("interior-review fixture lacks hall-house plaster proof");
    cameras.push(plaster_raking_camera(hall_house));
    let merchant_house = buildings
        .iter()
        .find(|building| building.plan.archetype == BuildingArchetype::FachwerkMerchantHouse)
        .expect("interior-review fixture lacks merchant-house partition proof");
    cameras.push(partition_review_camera(merchant_house));
    cameras
}

fn partition_review_camera(building: &GeneratedBuilding) -> BuildingReviewCamera {
    let storey = &building.plan.storeys[0];
    let room = storey
        .rooms
        .iter()
        .max_by_key(|room| room.cells.len())
        .expect("partition review ground storey has a room");
    let wall = storey
        .walls
        .iter()
        .enumerate()
        .filter(|(_, wall)| wall.inside_room == room.id && !wall.exterior())
        .max_by_key(|(index, _)| {
            usize::from(storey.openings.iter().any(|opening| opening.wall == *index))
        })
        .map(|(_, wall)| *wall)
        .expect("partition review building has an internal wall facing its largest room");
    let centroid =
        room.cells.iter().map(|cell| cell.centre()).sum::<Vec2>() / room.cells.len() as f32;
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
    BuildingReviewCamera {
        position: transform.transform_point(Vec3::new(centroid.x, 1.48, centroid.y) - local_origin),
        target: transform
            .transform_point(Vec3::new(wall.centre().x, 1.38, wall.centre().y) - local_origin),
        plaster_raking_light: None,
    }
}

fn plaster_raking_camera(building: &GeneratedBuilding) -> BuildingReviewCamera {
    let storey = &building.plan.storeys[0];
    let room = storey
        .rooms
        .iter()
        .max_by_key(|room| room.cells.len())
        .expect("hall house ground storey has a room");
    let wall = storey
        .walls
        .iter()
        .enumerate()
        .filter(|(index, wall)| {
            wall.inside_room == room.id
                && !wall.exterior()
                && !storey.openings.iter().any(|opening| opening.wall == *index)
        })
        .map(|(_, wall)| *wall)
        .next()
        .expect("hall house has an unopened interior plaster wall");
    let inward_local = match wall.direction {
        Direction::North => -Vec2::Y,
        Direction::East => -Vec2::X,
        Direction::South => Vec2::Y,
        Direction::West => Vec2::X,
    };
    let tangent_local = if wall.is_horizontal() {
        Vec2::X
    } else {
        Vec2::Y
    };
    let wall_face = wall.centre() + inward_local * (WALL_THICKNESS_METRES * 0.5 + 0.002);
    let local_origin = building.collision.bounds.centre();
    let floor_offset = local_origin.y - building.collision.bounds.min.y;
    let building_transform = Transform::from_xyz(
        building.placement.centre_metres.x,
        building.pad_elevation_metres + floor_offset,
        building.placement.centre_metres.y,
    )
    .with_rotation(Quat::from_rotation_y(
        building.placement.orientation.yaw_radians(),
    ));
    let wall_point = building_transform
        .transform_point(Vec3::new(wall_face.x, 1.42, wall_face.y) - local_origin);
    let inward_normal =
        (building_transform.rotation * Vec3::new(inward_local.x, 0.0, inward_local.y)).normalize();
    let tangent = (building_transform.rotation * Vec3::new(tangent_local.x, 0.0, tangent_local.y))
        .normalize();
    let incidence = PLASTER_REVIEW_LIGHT_INCIDENCE_DEGREES.to_radians();
    let light_direction = tangent * incidence.cos() + inward_normal * incidence.sin();
    let light_position =
        wall_point + light_direction * PLASTER_REVIEW_LIGHT_TANGENT_DISTANCE_METRES;
    BuildingReviewCamera {
        position: wall_point + inward_normal * PLASTER_REVIEW_CAMERA_DISTANCE_METRES,
        target: wall_point,
        plaster_raking_light: Some(PlasterRakingLight {
            wall_point,
            inward_normal,
            position: light_position,
        }),
    }
}

fn camera_for_storey(
    building: &GeneratedBuilding,
    preferred_storey: usize,
    angle_variant: usize,
) -> BuildingReviewCamera {
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
    BuildingReviewCamera {
        position,
        target,
        plaster_raking_light: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_building_generator::{BuildingProgram, compile_building_collision, generate};
    use adventuresim_tactical_core::prelude::{BuildingOrientation, TacticalBuildingPlacement};

    #[test]
    fn city_archetypes_produce_two_finite_interior_cameras_and_one_plaster_proof() {
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

        assert_eq!(cameras.len(), REVIEW_ARCHETYPES.len() * 2 + 2);
        assert!(cameras.iter().all(|camera| {
            camera.position.is_finite()
                && camera.target.is_finite()
                && camera.position.distance(camera.target) > 0.5
        }));
        let proof = &cameras[REVIEW_ARCHETYPES.len() * 2];
        let light = proof.plaster_raking_light.unwrap();
        let light_direction = (light.position - light.wall_point).normalize();
        let incidence_degrees = light_direction.dot(light.inward_normal).asin().to_degrees();
        assert!((5.0..=15.0).contains(&incidence_degrees));
        let view_direction = (proof.position - light.wall_point).normalize();
        assert!(
            view_direction.dot(light.inward_normal) >= 0.99,
            "proof camera must see the lit room side"
        );
        assert!(proof.position.distance(proof.target) < 1.0);
    }
}
