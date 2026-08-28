use super::*;

/// Marks the interval where Avian, rather than Ahoy, owns the combatant root.
/// Detailed skeletal ragdolls remain client presentation; this coarse dynamic
/// body authoritatively owns displacement and recovery orientation.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct AuthoritativeRagdoll;

fn recovery_body(rotation: Quat) -> BodyState {
    let anterior = rotation * Vec3::NEG_Z;
    if anterior.dot(Vec3::Y) >= 0.0 {
        BodyState::Supine
    } else {
        BodyState::Prone
    }
}

fn recovery_yaw(rotation: Quat) -> Quat {
    let headward = (rotation * Vec3::Y).xz().normalize_or_zero();
    let forward = if headward == Vec2::ZERO {
        (rotation * Vec3::NEG_Z).xz().normalize_or_zero()
    } else {
        headward
    };
    if forward == Vec2::ZERO {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y((-forward.x).atan2(-forward.y))
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the Bevy system must update the complete authoritative ragdoll boundary atomically"
)]
pub(super) fn update_authoritative_ragdoll_lifecycle(
    mut commands: Commands,
    mut combatants: Query<
        (
            Entity,
            &TacticalCombatState,
            &mut SkeletonState,
            &mut Transform,
            &mut Rotation,
            &LinearVelocity,
            &mut AngularVelocity,
            &mut CharacterControllerState,
            &mut input::AccumulatedInput,
            Option<&AuthoritativeRagdoll>,
        ),
        With<Player>,
    >,
) {
    for (
        entity,
        combat,
        mut skeleton,
        mut transform,
        mut physics_rotation,
        velocity,
        mut angular_velocity,
        mut controller_state,
        mut input,
        ragdoll,
    ) in &mut combatants
    {
        match (combat.is_incapacitated(), ragdoll.is_some()) {
            (true, false) => {
                skeleton.transition_body(BodyState::Ragdolled);
                angular_velocity.0 =
                    Vec3::new(velocity.z, 0.35, -velocity.x).clamp_length_max(8.0) + Vec3::X * 1.1;
                input.last_movement = None;
                input.jumped = None;
                commands
                    .entity(entity)
                    .remove::<(CharacterController, CustomPositionIntegration)>()
                    .insert((
                        AuthoritativeRagdoll,
                        RigidBody::Dynamic,
                        LinearDamping(0.35),
                        AngularDamping(1.4),
                    ));
            }
            (false, true) => {
                let body = recovery_body(transform.rotation);
                let upright = recovery_yaw(transform.rotation);
                transform.rotation = upright;
                physics_rotation.0 = upright;
                angular_velocity.0 = Vec3::ZERO;
                *controller_state = CharacterControllerState {
                    orientation: upright,
                    ..default()
                };
                input.last_movement = None;
                input.jumped = None;
                input.crouched = true;
                skeleton.transition_body(body);
                commands
                    .entity(entity)
                    .remove::<(AuthoritativeRagdoll, LinearDamping, AngularDamping)>()
                    .insert((
                        RigidBody::Kinematic,
                        tactical_character_controller(),
                        CustomPositionIntegration,
                    ));
            }
            (true, true) => {
                skeleton.transition_body(BodyState::Ragdolled);
            }
            (false, false) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_uses_pelvis_anterior_orientation() {
        assert_eq!(recovery_body(Quat::from_rotation_x(1.5)), BodyState::Supine);
        assert_eq!(recovery_body(Quat::from_rotation_x(-1.5)), BodyState::Prone);
    }

    #[test]
    fn recovery_yaw_is_finite_for_degenerate_orientation() {
        assert!(recovery_yaw(Quat::IDENTITY).is_finite());
        assert!(recovery_yaw(Quat::from_rotation_x(1.5)).is_finite());
    }

    #[test]
    fn lifecycle_hands_root_between_kcc_and_dynamic_body() {
        let mut app = App::new();
        app.add_systems(Update, update_authoritative_ragdoll_lifecycle);
        let entity = app
            .world_mut()
            .spawn((
                Player::default(),
                TacticalCombatState {
                    incapacitation: 1.0,
                    ..default()
                },
                SkeletonState::default(),
                Transform::default(),
                Rotation::default(),
                LinearVelocity(Vec3::Z * 2.0),
                AngularVelocity::ZERO,
                RigidBody::Kinematic,
                CharacterControllerState::default(),
                input::AccumulatedInput::default(),
                tactical_character_controller(),
            ))
            .id();

        app.update();
        let entered = app.world().entity(entity);
        assert!(entered.contains::<AuthoritativeRagdoll>());
        assert!(!entered.contains::<CharacterController>());
        assert_eq!(entered.get::<RigidBody>(), Some(&RigidBody::Dynamic));
        assert_eq!(
            entered.get::<SkeletonState>().unwrap().body(),
            BodyState::Ragdolled
        );

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<TacticalCombatState>()
            .unwrap()
            .incapacitation = 0.99;
        app.update();
        let recovered = app.world().entity(entity);
        assert!(!recovered.contains::<AuthoritativeRagdoll>());
        assert!(recovered.contains::<CharacterController>());
        assert_eq!(recovered.get::<RigidBody>(), Some(&RigidBody::Kinematic));
        assert!(matches!(
            recovered.get::<SkeletonState>().unwrap().body(),
            BodyState::Prone | BodyState::Supine
        ));
    }
}
