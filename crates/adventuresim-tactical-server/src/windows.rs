//! Authoritative interaction and constrained motion for building casements.

use adventuresim_building_generator::{
    BuildingCollision, BuildingPlan, WindowSpec, compile_operable_windows,
};
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;
use bevy::{ecs::system::SystemParam, prelude::*};

const WINDOW_ANGULAR_SPEED_RADIANS_PER_SECOND: f32 = 2.8;

#[derive(Component)]
struct WindowController {
    hinge_centre: Vec3,
    closed_rotation: Quat,
    open_angle_radians: f32,
    current_angle_radians: f32,
    open: bool,
}

pub(crate) struct WindowServerPlugin;

impl Plugin for WindowServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPostUpdate,
            animate_windows.before(PhysicsSystems::Prepare),
        );
    }
}

#[derive(SystemParam)]
pub(crate) struct WindowGrabber<'w, 's> {
    windows: Query<'w, 's, (&'static SceneWindow, &'static mut WindowController)>,
    characters: Query<'w, 's, &'static Transform, With<Player>>,
}

impl WindowGrabber<'_, '_> {
    pub(crate) fn try_toggle_from_inside(&mut self, actor: Entity, window_entity: Entity) -> bool {
        let Ok(actor_transform) = self.characters.get(actor) else {
            return false;
        };
        let Ok((window, mut controller)) = self.windows.get_mut(window_entity) else {
            return false;
        };
        if !can_grab_window_from_inside(
            actor_transform.translation,
            window.opening_centre_metres,
            window.tangent,
            window.outward,
            window.size_metres.x * 0.5,
        ) {
            return false;
        }
        controller.open = !controller.open;
        debug!(
            actor = ?actor,
            window = ?window_entity,
            building_id = window.building_id,
            opening_id = window.opening_id,
            open = controller.open,
            "Toggled interior window catch"
        );
        true
    }
}

pub(crate) fn spawn_building_windows(
    commands: &mut Commands,
    building: &SceneBuilding,
    building_transform: &Transform,
    plan: &BuildingPlan,
    collision: &BuildingCollision,
) {
    let collision_origin = collision.bounds.centre();
    for window in compile_operable_windows(plan) {
        spawn_window(
            commands,
            building,
            building_transform,
            collision_origin,
            window,
        );
    }
}

fn spawn_window(
    commands: &mut Commands,
    building: &SceneBuilding,
    building_transform: &Transform,
    collision_origin: Vec3,
    window: WindowSpec,
) {
    let closed_centre = building_transform.transform_point(window.closed_centre - collision_origin);
    let hinge_centre = building_transform.transform_point(window.hinge_centre - collision_origin);
    let closed_rotation =
        building_transform.rotation * Quat::from_rotation_y(window.closed_yaw_radians);
    let tangent = building_transform.rotation * Vec3::new(window.tangent.x, 0.0, window.tangent.y);
    let outward = building_transform.rotation * Vec3::new(window.outward.x, 0.0, window.outward.y);
    commands.spawn((
        Name::new(format!(
            "Building {} window {} casement",
            building.id, window.opening.0
        )),
        Replicated,
        SceneWindow {
            building_id: building.id,
            opening_id: window.opening.0,
            size_metres: window.size_metres,
            opening_centre_metres: closed_centre,
            tangent,
            outward,
            barred: window.barred,
        },
        RigidBody::Kinematic,
        Collider::cuboid(
            window.size_metres.x,
            window.size_metres.y,
            window.size_metres.z,
        ),
        CollisionLayers::new(TACTICAL_WINDOW_LAYER, LayerMask::DEFAULT),
        Transform::from_translation(closed_centre).with_rotation(closed_rotation),
        WindowController {
            hinge_centre,
            closed_rotation,
            open_angle_radians: window.open_angle_radians,
            current_angle_radians: 0.0,
            open: false,
        },
    ));
}

fn animate_windows(
    time: Res<Time>,
    mut windows: Query<(&SceneWindow, &mut WindowController, &mut Transform)>,
) {
    for (window, mut controller, mut transform) in &mut windows {
        let target = if controller.open {
            controller.open_angle_radians
        } else {
            0.0
        };
        let maximum_step = WINDOW_ANGULAR_SPEED_RADIANS_PER_SECOND * time.delta_secs();
        let delta = (target - controller.current_angle_radians).clamp(-maximum_step, maximum_step);
        controller.current_angle_radians += delta;
        transform.rotation =
            Quat::from_rotation_y(controller.current_angle_radians) * controller.closed_rotation;
        transform.translation =
            controller.hinge_centre + transform.rotation * Vec3::X * window.size_metres.x * 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn only_an_inside_actor_can_toggle_the_window_catch() {
        let mut world = World::new();
        let window = world
            .spawn((
                SceneWindow {
                    building_id: 1,
                    opening_id: 2,
                    size_metres: Vec3::new(1.0, 1.0, 0.025),
                    opening_centre_metres: Vec3::ZERO,
                    tangent: Vec3::X,
                    outward: Vec3::Z,
                    barred: false,
                },
                WindowController {
                    hinge_centre: Vec3::NEG_X * 0.5,
                    closed_rotation: Quat::IDENTITY,
                    open_angle_radians: 1.0,
                    current_angle_radians: 0.0,
                    open: false,
                },
            ))
            .id();
        let inside = world
            .spawn((
                Player {
                    name: "Inside".to_owned(),
                },
                Transform::from_xyz(0.0, 0.0, -0.5),
            ))
            .id();
        let outside = world
            .spawn((
                Player {
                    name: "Outside".to_owned(),
                },
                Transform::from_xyz(0.0, 0.0, 0.5),
            ))
            .id();

        assert!(
            world
                .run_system_once(move |mut windows: WindowGrabber| {
                    windows.try_toggle_from_inside(inside, window)
                })
                .unwrap()
        );
        assert!(world.get::<WindowController>(window).unwrap().open);
        assert!(
            !world
                .run_system_once(move |mut windows: WindowGrabber| {
                    windows.try_toggle_from_inside(outside, window)
                })
                .unwrap()
        );
    }
}
