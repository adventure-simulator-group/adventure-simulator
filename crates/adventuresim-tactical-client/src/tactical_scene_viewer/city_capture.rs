use adventuresim_tactical_core::prelude::{DistantBuildingPlacement, GeneratedBuilding};
use bevy::prelude::*;

use super::capture_state::BuildingReviewCamera;

const FACADE_REVIEW_DISTANCE_METRES: f32 = 7.0;
const STREET_REVIEW_DISTANCE_METRES: f32 = 17.0;

pub(super) fn capture_cameras(
    buildings: &[GeneratedBuilding],
    distant_buildings: &[DistantBuildingPlacement],
    profile: &str,
) -> Vec<BuildingReviewCamera> {
    if profile != super::CITY_REVIEW_PROFILE {
        return Vec::new();
    }
    let focus = buildings
        .iter()
        .min_by(|left, right| {
            left.placement
                .centre_metres
                .length_squared()
                .total_cmp(&right.placement.centre_metres.length_squared())
        })
        .expect("city-review fixture lacks a playable building");
    let local_origin = focus.collision.bounds.centre();
    let floor_offset = local_origin.y - focus.collision.bounds.min.y;
    let transform = Transform::from_xyz(
        focus.placement.centre_metres.x,
        focus.pad_elevation_metres + floor_offset,
        focus.placement.centre_metres.y,
    )
    .with_rotation(Quat::from_rotation_y(
        focus.placement.orientation.yaw_radians(),
    ));
    let facade_target_local = Vec3::new(0.0, 2.4, focus.collision.bounds.max.z) - local_origin;
    let facade_target = transform.transform_point(facade_target_local);
    let outward = transform.rotation * Vec3::Z;
    let side = transform.rotation * Vec3::X;

    let playable_bounds = Bounds2::from_points(
        buildings
            .iter()
            .map(|building| building.placement.centre_metres),
    );
    let city_bounds = Bounds2::from_points(
        buildings
            .iter()
            .map(|building| building.placement.centre_metres)
            .chain(
                distant_buildings
                    .iter()
                    .map(|building| building.centre_metres),
            ),
    );

    vec![
        BuildingReviewCamera {
            position: facade_target + outward * FACADE_REVIEW_DISTANCE_METRES,
            target: facade_target,
            plaster_raking_light: None,
        },
        BuildingReviewCamera {
            position: facade_target
                + outward * STREET_REVIEW_DISTANCE_METRES
                + side * STREET_REVIEW_DISTANCE_METRES * 0.45,
            target: facade_target + Vec3::Y,
            plaster_raking_light: None,
        },
        playable_bounds.oblique_camera(0.95, 0.8),
        city_bounds.oblique_camera(0.72, 0.42),
        city_bounds.edge_camera(),
        city_bounds.aerial_camera(),
        city_bounds.horizon_camera(),
    ]
}

#[derive(Clone, Copy)]
struct Bounds2 {
    min: Vec2,
    max: Vec2,
}

impl Bounds2 {
    fn from_points(points: impl Iterator<Item = Vec2>) -> Self {
        let bounds = points.fold(None::<Self>, |bounds, point| {
            Some(match bounds {
                None => Self {
                    min: point,
                    max: point,
                },
                Some(bounds) => Self {
                    min: bounds.min.min(point),
                    max: bounds.max.max(point),
                },
            })
        });
        bounds.expect("city-review fixture lacks building placements")
    }

    fn centre(self) -> Vec3 {
        let centre = (self.min + self.max) * 0.5;
        Vec3::new(centre.x, 4.0, centre.y)
    }

    fn radius(self) -> f32 {
        ((self.max - self.min).max_element() * 0.5).max(20.0)
    }

    fn oblique_camera(self, distance_scale: f32, height_scale: f32) -> BuildingReviewCamera {
        let target = self.centre();
        let radius = self.radius();
        BuildingReviewCamera {
            position: target + Vec3::new(radius * distance_scale, radius * height_scale, radius),
            target,
            plaster_raking_light: None,
        }
    }

    fn edge_camera(self) -> BuildingReviewCamera {
        let target = self.centre();
        let radius = self.radius();
        BuildingReviewCamera {
            position: target + Vec3::new(radius * 1.45, radius * 0.18, radius * 0.35),
            target,
            plaster_raking_light: None,
        }
    }

    fn aerial_camera(self) -> BuildingReviewCamera {
        let target = self.centre();
        let radius = self.radius();
        BuildingReviewCamera {
            position: target + Vec3::new(0.0, radius * 1.7, radius * 0.18),
            target,
            plaster_raking_light: None,
        }
    }

    fn horizon_camera(self) -> BuildingReviewCamera {
        let target = self.centre();
        let radius = self.radius();
        BuildingReviewCamera {
            position: target + Vec3::new(0.0, radius * 0.12, radius * 2.35),
            target,
            plaster_raking_light: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_building_generator::{
        BuildingArchetype, BuildingProgram, compile_building_collision, generate,
    };
    use adventuresim_tactical_core::prelude::{BuildingOrientation, TacticalBuildingPlacement};

    #[test]
    fn city_packet_spans_close_and_whole_settlement_views() {
        let placement = TacticalBuildingPlacement {
            id: 1,
            program: BuildingProgram::fixture(BuildingArchetype::TownHouse, 42),
            centre_metres: Vec2::ZERO,
            orientation: BuildingOrientation::IDENTITY,
        };
        let plan = generate(&placement.program).unwrap();
        let buildings = [GeneratedBuilding {
            placement,
            collision: compile_building_collision(&plan),
            plan,
            pad_elevation_metres: 0.0,
        }];
        let distant = [DistantBuildingPlacement {
            id: 2,
            archetype: BuildingArchetype::FachwerkCottage,
            seed: 7,
            centre_metres: Vec2::new(300.0, 240.0),
            base_elevation_metres: 0.0,
            orientation: BuildingOrientation::IDENTITY,
        }];

        let cameras = capture_cameras(
            &buildings,
            &distant,
            crate::tactical_scene_viewer::CITY_REVIEW_PROFILE,
        );

        assert_eq!(cameras.len(), 7);
        assert!(cameras.iter().all(|camera| {
            camera.position.is_finite()
                && camera.target.is_finite()
                && camera.position.distance(camera.target) > 0.5
        }));
        assert!(cameras[5].position.y > cameras[0].position.y + 100.0);
    }
}
