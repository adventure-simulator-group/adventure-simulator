//! Authoritative simulation for operable exterior building doors.

use adventuresim_building_generator::{
    BuildingCollision, BuildingPlan, DoorSpec, compile_operable_doors,
};
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;
use bevy::{ecs::system::SystemParam, math::primitives::Cuboid, prelude::*};

const DOOR_DENSITY_KILOGRAMS_PER_CUBIC_METRE: f32 = 150.0;
const DOOR_MOTOR_FREQUENCY_HZ: f32 = 3.0;
const DOOR_OPENING_MAX_TORQUE_NEWTON_METRES: f32 = 40.0;
const DOOR_CLOSING_MAX_TORQUE_NEWTON_METRES: f32 = 7.0;
const DOOR_MAX_ANGULAR_SPEED_RADIANS_PER_SECOND: f32 = 6.0;
const DOOR_MAX_LINEAR_SPEED_METRES_PER_SECOND: f32 = 3.0;
const PASSAGE_RELEASE_OUTSIDE_METRES: f32 = 0.8;
const PASSAGE_RELEASE_INSIDE_METRES: f32 = 2.1;

#[derive(Component)]
pub(crate) struct DoorController {
    joint: Entity,
    doorway_centre: Vec3,
    tangent: Vec3,
    outward: Vec3,
    half_width_metres: f32,
    open_angle_radians: f32,
}

pub(crate) struct DoorServerPlugin;

impl Plugin for DoorServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(log_door_collision_start);
        app.add_systems(
            FixedPostUpdate,
            (
                update_door_passages
                    .before(AhoySystems::MoveCharacters)
                    .before(PhysicsSystems::Prepare),
                report_unseated_doors.after(PhysicsSystems::StepSimulation),
            ),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PassageDecision {
    None,
    Retain,
    Revoke,
}

impl PassageDecision {
    fn holds_door_open(self) -> bool {
        matches!(self, Self::Retain)
    }
}

#[derive(SystemParam)]
pub(crate) struct DoorGrabber<'w, 's> {
    doors: Query<'w, 's, (&'static SceneDoor, &'static DoorController)>,
    characters: Query<
        'w,
        's,
        (
            &'static Transform,
            &'static mut CharacterController,
            &'static mut DoorPassageExemptions,
        ),
        With<Player>,
    >,
}

impl DoorGrabber<'_, '_> {
    pub(crate) fn try_open_from_inside(&mut self, actor: Entity, door_entity: Entity) -> bool {
        let Ok((door, _controller)) = self.doors.get(door_entity) else {
            return false;
        };
        let Ok((actor_transform, mut character_controller, mut exemptions)) =
            self.characters.get_mut(actor)
        else {
            return false;
        };
        if !can_grab_door_from_inside(
            actor_transform.translation,
            door.doorway_centre_metres,
            door.tangent,
            door.outward,
            door.size_metres.x * 0.5,
        ) {
            return false;
        }
        exemptions.grant(door_entity);
        character_controller
            .filter
            .excluded_entities
            .insert(door_entity);
        // The motor target is derived from this exemption during the next
        // fixed update, keeping all hinge mutation in one system.
        debug!(
            actor = ?actor,
            door = ?door_entity,
            building_id = door.building_id,
            opening_id = door.opening_id,
            "Accepted interior door grab"
        );
        true
    }
}

pub(crate) fn spawn_building_doors(
    commands: &mut Commands,
    building_entity: Entity,
    building: &SceneBuilding,
    building_transform: &Transform,
    plan: &BuildingPlan,
    collision: &BuildingCollision,
) {
    let collision_origin = collision.bounds.centre();
    for door in compile_operable_doors(plan) {
        spawn_door(
            commands,
            building_entity,
            building,
            building_transform,
            collision_origin,
            door,
        );
    }
}

fn spawn_door(
    commands: &mut Commands,
    building_entity: Entity,
    building: &SceneBuilding,
    building_transform: &Transform,
    collision_origin: Vec3,
    door: DoorSpec,
) {
    let closed_centre = building_transform.transform_point(door.closed_centre - collision_origin);
    let hinge_centre = building_transform.transform_point(door.hinge_centre - collision_origin);
    let closed_rotation =
        building_transform.rotation * Quat::from_rotation_y(door.closed_yaw_radians);
    let tangent = building_transform.rotation * Vec3::new(door.tangent.x, 0.0, door.tangent.y);
    let outward = building_transform.rotation * Vec3::new(door.outward.x, 0.0, door.outward.y);
    let doorway_centre = hinge_centre + tangent * door.size_metres.x * 0.5;
    let leaf = commands
        .spawn((
            Name::new(format!(
                "Building {} door {} leaf",
                building.id, door.opening.0
            )),
            Replicated,
            SceneDoor {
                building_id: building.id,
                opening_id: door.opening.0,
                size_metres: door.size_metres,
                doorway_centre_metres: doorway_centre,
                tangent,
                outward,
            },
            RigidBody::Dynamic,
            Collider::cuboid(door.size_metres.x, door.size_metres.y, door.size_metres.z),
            MassPropertiesBundle::from_shape(
                &Cuboid::from_size(door.size_metres),
                DOOR_DENSITY_KILOGRAMS_PER_CUBIC_METRE,
            ),
            // The frame is represented by several independent terrain-layer
            // colliders. Letting the leaf contact them turns a character push
            // into an over-constrained solver island and can launch the door.
            CollisionLayers::new(TACTICAL_DOOR_LAYER, LayerMask::DEFAULT),
            ActiveCollisionHooks::FILTER_PAIRS,
            CollisionEventsEnabled,
            MaxAngularSpeed(DOOR_MAX_ANGULAR_SPEED_RADIANS_PER_SECOND),
            MaxLinearSpeed(DOOR_MAX_LINEAR_SPEED_METRES_PER_SECOND),
            AngularDamping(2.5),
            LinearDamping(1.0),
            SleepingDisabled,
            Transform::from_translation(closed_centre).with_rotation(closed_rotation),
        ))
        .id();
    let (minimum_angle, maximum_angle) = if door.open_angle_radians.is_sign_positive() {
        (0.0, door.open_angle_radians)
    } else {
        (door.open_angle_radians, 0.0)
    };
    let joint = commands
        .spawn((
            Name::new(format!(
                "Building {} door {} hinge",
                building.id, door.opening.0
            )),
            door_joint(
                building_entity,
                leaf,
                hinge_centre,
                closed_rotation,
                minimum_angle,
                maximum_angle,
            )
            .with_motor(door_motor(0.0, DOOR_CLOSING_MAX_TORQUE_NEWTON_METRES)),
            JointCollisionDisabled,
        ))
        .id();
    commands.entity(leaf).insert(DoorController {
        joint,
        doorway_centre,
        tangent,
        outward,
        half_width_metres: door.size_metres.x * 0.5,
        open_angle_radians: door.open_angle_radians,
    });
}

fn door_joint(
    building: Entity,
    leaf: Entity,
    hinge_centre: Vec3,
    closed_rotation: Quat,
    minimum_angle: f32,
    maximum_angle: f32,
) -> RevoluteJoint {
    RevoluteJoint::new(building, leaf)
        .with_hinge_axis(Vec3::Y)
        .with_anchor(hinge_centre)
        .with_basis(closed_rotation)
        .with_angle_limits(minimum_angle, maximum_angle)
}

fn log_door_collision_start(
    event: On<CollisionStart>,
    doors: Query<(
        &SceneDoor,
        &Position,
        &Rotation,
        &LinearVelocity,
        &AngularVelocity,
    )>,
) {
    let Ok((door, position, rotation, linear_velocity, angular_velocity)) =
        doors.get(event.collider1)
    else {
        return;
    };
    debug!(
        building_id = door.building_id,
        opening_id = door.opening_id,
        other = ?event.collider2,
        position = ?position.0,
        rotation = ?rotation.0,
        linear_velocity = ?linear_velocity.0,
        angular_velocity = ?angular_velocity.0,
        "Door collision started"
    );
}

fn report_unseated_doors(
    doors: Query<(
        &SceneDoor,
        &DoorController,
        &Position,
        &Rotation,
        &LinearVelocity,
        &AngularVelocity,
    )>,
) {
    const MAX_HINGE_SEPARATION_METRES: f32 = 0.03;
    const MIN_VERTICAL_ALIGNMENT: f32 = 0.995;

    for (door, controller, position, rotation, linear_velocity, angular_velocity) in &doors {
        let leaf_hinge = position.0 + rotation.0 * Vec3::new(-door.size_metres.x * 0.5, 0.0, 0.0);
        let hinge_separation = leaf_hinge.distance(
            controller.doorway_centre - controller.tangent * controller.half_width_metres,
        );
        let vertical_alignment = (rotation.0 * Vec3::Y).dot(Vec3::Y);
        if hinge_separation > MAX_HINGE_SEPARATION_METRES
            || vertical_alignment < MIN_VERTICAL_ALIGNMENT
        {
            warn!(
                building_id = door.building_id,
                opening_id = door.opening_id,
                hinge_separation_metres = hinge_separation,
                vertical_alignment,
                position = ?position.0,
                rotation = ?rotation.0,
                linear_velocity = ?linear_velocity.0,
                angular_velocity = ?angular_velocity.0,
                "Door escaped its vertical hinge"
            );
        }
    }
}

fn door_motor(target_position: f32, maximum_torque: f32) -> AngularMotor {
    AngularMotor::new(MotorModel::SpringDamper {
        frequency: DOOR_MOTOR_FREQUENCY_HZ,
        damping_ratio: 1.0,
    })
    .with_target_position(target_position)
    .with_max_torque(maximum_torque)
}

pub(crate) fn update_door_passages(
    doors: Query<(Entity, &DoorController)>,
    mut joints: Query<&mut RevoluteJoint>,
    mut characters: Query<(
        Entity,
        &Transform,
        &mut CharacterController,
        &mut DoorPassageExemptions,
    )>,
) {
    for (door_entity, door) in &doors {
        let mut should_open = false;
        for (_character_entity, character_transform, mut controller, mut exemptions) in
            &mut characters
        {
            let offset = character_transform.translation - door.doorway_centre;
            let signed_depth = offset.dot(door.outward);
            let lateral_distance = offset.dot(door.tangent).abs();
            let within_passage =
                lateral_distance <= door.half_width_metres + DOOR_GRAB_LATERAL_MARGIN_METRES;
            let was_exempt = exemptions.contains(door_entity);
            let decision = passage_decision(signed_depth, within_passage, was_exempt);

            match decision {
                PassageDecision::Retain => {
                    controller.filter.excluded_entities.insert(door_entity);
                }
                PassageDecision::Revoke => {
                    exemptions.revoke(door_entity);
                    controller.filter.excluded_entities.remove(&door_entity);
                }
                PassageDecision::None => {}
            }
            should_open |= decision.holds_door_open();
        }

        if let Ok(mut joint) = joints.get_mut(door.joint) {
            joint.motor = if should_open {
                door_motor(
                    door.open_angle_radians,
                    DOOR_OPENING_MAX_TORQUE_NEWTON_METRES,
                )
            } else {
                door_motor(0.0, DOOR_CLOSING_MAX_TORQUE_NEWTON_METRES)
            };
        }
    }
}

fn passage_decision(
    signed_depth_metres: f32,
    within_passage: bool,
    was_exempt: bool,
) -> PassageDecision {
    if was_exempt
        && (!within_passage
            || signed_depth_metres > PASSAGE_RELEASE_OUTSIDE_METRES
            || signed_depth_metres < -PASSAGE_RELEASE_INSIDE_METRES)
    {
        PassageDecision::Revoke
    } else if was_exempt {
        PassageDecision::Retain
    } else {
        PassageDecision::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::time::TimeUpdateStrategy;
    use core::time::Duration;

    #[test]
    fn closing_motor_is_weaker_than_automatic_opening() {
        let opening = door_motor(1.0, DOOR_OPENING_MAX_TORQUE_NEWTON_METRES);
        let closing = door_motor(0.0, DOOR_CLOSING_MAX_TORQUE_NEWTON_METRES);

        assert!(opening.max_torque > closing.max_torque);
        assert_eq!(opening.target_position, 1.0);
        assert_eq!(closing.target_position, 0.0);
    }

    #[test]
    fn door_collides_with_characters_but_not_its_multi_collider_frame() {
        let door = CollisionLayers::new(TACTICAL_DOOR_LAYER, LayerMask::DEFAULT);
        let character = CollisionLayers::DEFAULT;
        let frame = CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL);

        assert!(door.interacts_with(character));
        assert!(!door.interacts_with(frame));
    }

    #[test]
    fn proximity_alone_never_grants_a_passage_exemption() {
        assert_eq!(passage_decision(-1.0, true, false), PassageDecision::None);
        assert_eq!(passage_decision(0.4, true, false), PassageDecision::None);
        assert_eq!(passage_decision(0.4, true, true), PassageDecision::Retain);
        assert_eq!(passage_decision(1.0, true, true), PassageDecision::Revoke);
        assert_eq!(passage_decision(-1.0, false, true), PassageDecision::Revoke);
    }

    #[test]
    fn grabbed_inside_door_is_exempt_until_clear_of_the_exterior() {
        let mut world = World::new();
        let anchor = world.spawn_empty().id();
        let leaf = world.spawn_empty().id();
        let joint = world.spawn(RevoluteJoint::new(anchor, leaf)).id();
        world.entity_mut(leaf).insert(DoorController {
            joint,
            doorway_centre: Vec3::ZERO,
            tangent: Vec3::X,
            outward: Vec3::Z,
            half_width_metres: 0.5,
            open_angle_radians: 1.0,
        });
        let character = world
            .spawn((
                Transform::from_xyz(0.0, 0.0, -1.0),
                CharacterController::default(),
                DoorPassageExemptions::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(update_door_passages);

        world
            .get_mut::<DoorPassageExemptions>(character)
            .unwrap()
            .grant(leaf);
        world
            .get_mut::<CharacterController>(character)
            .unwrap()
            .filter
            .excluded_entities
            .insert(leaf);

        schedule.run(&mut world);
        assert!(
            world
                .get::<DoorPassageExemptions>(character)
                .unwrap()
                .contains(leaf)
        );
        assert!(
            world
                .get::<CharacterController>(character)
                .unwrap()
                .filter
                .excluded_entities
                .contains(&leaf)
        );
        assert_eq!(
            world
                .get::<RevoluteJoint>(joint)
                .unwrap()
                .motor
                .target_position,
            1.0
        );

        world.get_mut::<Transform>(character).unwrap().translation.z = 0.4;
        world
            .get_mut::<CharacterController>(character)
            .unwrap()
            .filter
            .excluded_entities
            .remove(&leaf);
        schedule.run(&mut world);
        assert!(
            world
                .get::<CharacterController>(character)
                .unwrap()
                .filter
                .excluded_entities
                .contains(&leaf)
        );

        world.get_mut::<Transform>(character).unwrap().translation.z = 1.0;
        schedule.run(&mut world);
        assert!(
            !world
                .get::<DoorPassageExemptions>(character)
                .unwrap()
                .contains(leaf)
        );
        assert_eq!(
            world
                .get::<RevoluteJoint>(joint)
                .unwrap()
                .motor
                .target_position,
            0.0
        );
    }

    #[test]
    fn authoritative_inside_grab_grants_collision_exemption() {
        let mut world = World::new();
        let joint = world.spawn_empty().id();
        let door = world
            .spawn((
                SceneDoor {
                    building_id: 1,
                    opening_id: 2,
                    size_metres: Vec3::new(1.0, 2.0, 0.08),
                    doorway_centre_metres: Vec3::ZERO,
                    tangent: Vec3::X,
                    outward: Vec3::Z,
                },
                DoorController {
                    joint,
                    doorway_centre: Vec3::ZERO,
                    tangent: Vec3::X,
                    outward: Vec3::Z,
                    half_width_metres: 0.5,
                    open_angle_radians: 1.0,
                },
            ))
            .id();
        let actor = world
            .spawn((
                Player {
                    name: "Door test actor".to_owned(),
                },
                Transform::from_xyz(0.0, 0.0, -1.0),
                CharacterController::default(),
                DoorPassageExemptions::default(),
            ))
            .id();

        let accepted = world
            .run_system_once(move |mut grabber: DoorGrabber| {
                grabber.try_open_from_inside(actor, door)
            })
            .unwrap();

        assert!(accepted);
        assert!(
            world
                .get::<DoorPassageExemptions>(actor)
                .unwrap()
                .contains(door)
        );
        assert!(
            world
                .get::<CharacterController>(actor)
                .unwrap()
                .filter
                .excluded_entities
                .contains(&door)
        );
    }

    #[test]
    fn door_remains_seated_and_upright_after_a_character_scale_collision() {
        const TIMESTEP_SECONDS: f32 = 1.0 / 64.0;
        const DOOR_WIDTH_METRES: f32 = 1.0;
        const DOOR_HEIGHT_METRES: f32 = 2.0;
        const DOOR_THICKNESS_METRES: f32 = 0.08;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PhysicsPlugins::default(), TransformPlugin));
        app.insert_resource(SubstepCount(20));
        app.insert_resource(Time::<Fixed>::from_duration(Duration::from_secs_f32(
            TIMESTEP_SECONDS,
        )));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            TIMESTEP_SECONDS,
        )));
        app.finish();

        let hinge = Vec3::new(0.0, DOOR_HEIGHT_METRES * 0.5, 0.0);
        let building = app
            .world_mut()
            .spawn((RigidBody::Static, Position(Vec3::ZERO), Rotation::default()))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Position(hinge + Vec3::X * DOOR_WIDTH_METRES * 0.5),
                Rotation::default(),
                Collider::cuboid(DOOR_WIDTH_METRES, DOOR_HEIGHT_METRES, DOOR_THICKNESS_METRES),
                MassPropertiesBundle::from_shape(
                    &Cuboid::new(DOOR_WIDTH_METRES, DOOR_HEIGHT_METRES, DOOR_THICKNESS_METRES),
                    DOOR_DENSITY_KILOGRAMS_PER_CUBIC_METRE,
                ),
                CollisionLayers::new(TACTICAL_DOOR_LAYER, LayerMask::DEFAULT),
                MaxAngularSpeed(DOOR_MAX_ANGULAR_SPEED_RADIANS_PER_SECOND),
                MaxLinearSpeed(DOOR_MAX_LINEAR_SPEED_METRES_PER_SECOND),
                SleepingDisabled,
            ))
            .id();
        let joint = app
            .world_mut()
            .spawn((
                door_joint(building, leaf, hinge, Quat::IDENTITY, -1.75, 0.0),
                JointCollisionDisabled,
            ))
            .id();
        app.world_mut().spawn((
            RigidBody::Kinematic,
            Position(Vec3::new(0.75, 1.0, 1.0)),
            Rotation::default(),
            Collider::capsule(0.35, 1.1),
            LinearVelocity(Vec3::NEG_Z * 1.5),
        ));
        // The production frame is compiled into independent wall-host
        // colliders immediately around the opening. They must not compete
        // with the hinge constraint when the character pushes the leaf.
        for x in [-0.08, DOOR_WIDTH_METRES + 0.08] {
            app.world_mut().spawn((
                RigidBody::Static,
                Position(Vec3::new(x, DOOR_HEIGHT_METRES * 0.5, 0.0)),
                Rotation::default(),
                Collider::cuboid(0.16, DOOR_HEIGHT_METRES, 0.2),
                CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
            ));
        }

        app.update();
        app.update();
        let resolved_joint = app
            .world()
            .get::<RevoluteJoint>(joint)
            .expect("door joint")
            .clone();
        assert!(resolved_joint.local_anchor1().is_some());
        assert!(resolved_joint.local_anchor2().is_some());
        for _ in 2..192 {
            app.update();
        }

        let door = app.world().entity(leaf);
        let position = door.get::<Position>().expect("door position").0;
        let rotation = door.get::<Rotation>().expect("door rotation").0;
        let leaf_hinge = position + rotation * Vec3::new(-DOOR_WIDTH_METRES * 0.5, 0.0, 0.0);
        assert!(
            leaf_hinge.distance(hinge) < 0.015,
            "hinge drifted by {} m; position={position:?}; rotation={rotation:?}; joint={resolved_joint:?}",
            leaf_hinge.distance(hinge),
        );
        assert!(
            (rotation * Vec3::Y).dot(Vec3::Y) > 0.999,
            "door tilted away from its vertical hinge: {rotation:?}"
        );
    }
}
