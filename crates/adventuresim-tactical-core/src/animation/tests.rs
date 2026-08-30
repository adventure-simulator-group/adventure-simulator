// Retained temporarily as historical fixtures while the sparse-contract tests
// below replace assumptions about the removed authored state space.
#[cfg(test)]
mod legacy_tests {
    use super::*;

    fn raised_intent(local_velocity: Vec3) -> RaisedLocomotionIntent {
        let speed = local_velocity.xz().length();
        RaisedLocomotionIntent::moving(Vec2::new(local_velocity.x, local_velocity.z), speed)
    }

    fn pack(
        id: &str,
        fallback: Option<&str>,
        clips: impl IntoIterator<Item = SemanticPose>,
    ) -> AnimationPack {
        AnimationPack {
            id: id.to_owned(),
            skeleton_family: "humanoid".to_owned(),
            fallback: fallback.map(str::to_owned),
            clips: clips.into_iter().collect(),
        }
    }

    #[test]
    fn pack_then_semantic_fallback_is_deterministic() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::WalkContact]))
            .unwrap();
        library
            .insert(pack("rapier", Some("unarmed"), [SemanticPose::RunFlight]))
            .unwrap();

        assert_eq!(
            library.resolve("rapier", SemanticPose::RunContact),
            ResolvedPose::Clip {
                pack_id: "unarmed",
                semantic: SemanticPose::WalkContact,
                pose: SemanticPose::WalkContact,
                mirrored: false,
            }
        );
        assert_eq!(
            library.resolve("rapier", SemanticPose::RunFlight),
            ResolvedPose::Clip {
                pack_id: "rapier",
                semantic: SemanticPose::RunFlight,
                pose: SemanticPose::RunFlight,
                mirrored: false,
            }
        );
    }

    #[test]
    fn mirrored_semantic_counterparts_are_involutions() {
        for pose in SemanticPose::HUMANOID_REQUIRED {
            let Some(counterpart) = pose.mirrored_counterpart() else {
                continue;
            };
            assert_ne!(pose, counterpart);
            assert_eq!(counterpart.mirrored_counterpart(), Some(pose));
        }
    }

    #[test]
    fn leaving_grounded_upright_plants_raised_movement() {
        let body = BodyState::Airborne;
        let mut state = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::NEG_Y, 2.0));

        state.transition_body(body);

        assert_eq!(state.body(), body);
        assert_eq!(state.weapon_guard(), WeaponGuardState::Raised);
        assert!(!state.raised_locomotion().is_moving());
        let rebuilt = state.with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::X, 3.0));
        assert!(!rebuilt.raised_locomotion().is_moving());
    }

    #[test]
    fn deserialization_plants_invalid_raised_movement_body_combinations() {
        let moving = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::NEG_Y, 2.0));

        let body = BodyState::Airborne;
        let mut wire = serde_json::to_value(&moving).unwrap();
        wire["body"] = serde_json::to_value(body).unwrap();
        let state: SkeletonState = serde_json::from_value(wire).unwrap();

        assert_eq!(state.body(), body);
        assert_eq!(state.weapon_guard(), WeaponGuardState::Raised);
        assert!(!state.raised_locomotion().is_moving());
    }

    #[test]
    fn child_attack_set_is_all_or_nothing_and_empty_child_inherits() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack(
                "unarmed",
                None,
                [
                    SemanticPose::AttackSwing,
                    SemanticPose::RecoverSwing,
                    SemanticPose::ContinueSwing,
                    SemanticPose::AttackThrust,
                    SemanticPose::AttackOffhand,
                ],
            ))
            .unwrap();
        library
            .insert(pack("sword", Some("unarmed"), [SemanticPose::AttackSwing]))
            .unwrap();
        library
            .insert(pack("shield", Some("unarmed"), [SemanticPose::GuardThrust]))
            .unwrap();

        assert_eq!(
            library.attack_animations("sword"),
            AttackAnimations {
                swing: true,
                swing_continuation: false,
                thrust_continuation: false,
                offhand: true,
                offhand_preparation: false,
                thrust: false,
            }
        );
        assert_eq!(
            library.attack_animations("shield"),
            AttackAnimations {
                swing: true,
                swing_continuation: true,
                thrust_continuation: false,
                offhand: true,
                offhand_preparation: false,
                thrust: true,
            }
        );
    }

    #[test]
    fn empty_or_unknown_pack_uses_bind_pose_t() {
        let mut library = AnimationPackLibrary::default();
        library.insert(pack("empty", None, [])).unwrap();
        assert_eq!(
            library.resolve("empty", SemanticPose::AirborneTravel),
            ResolvedPose::BindPoseT
        );
        assert_eq!(
            library.resolve("missing", SemanticPose::IdleRelaxed),
            ResolvedPose::BindPoseT
        );
    }

    #[test]
    fn missing_dive_pose_falls_back_to_guard() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(pack("unarmed", None, [SemanticPose::GuardThrust]))
            .unwrap();
        assert!(matches!(
            library.resolve("unarmed", SemanticPose::DiveLeft),
            ResolvedPose::Clip {
                semantic: SemanticPose::GuardThrust,
                pose: SemanticPose::GuardThrust,
                ..
            }
        ));
    }

    #[test]
    fn invalid_fallback_graph_is_rejected() {
        let mut library = AnimationPackLibrary::default();
        library.insert(pack("a", Some("b"), [])).unwrap();
        library.insert(pack("b", Some("a"), [])).unwrap();
        assert_eq!(
            library.validate_structure(),
            Err(PackValidationError::FallbackCycle("a".to_owned()))
        );
    }

    #[test]
    fn locomotion_projection_uses_controller_frame_and_fixed_stride() {
        let mut state = SkeletonState::default();
        let orientation = Quat::from_rotation_y(std::f32::consts::PI);
        let local_velocity = Vec3::NEG_Z * 2.0;
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: orientation * local_velocity,
                grounded: true,
                delta_seconds: 1.0 / 64.0,
                tick: 1,
            },
        );

        assert!(state.is_grounded());
        assert_eq!(state.posture(), Posture::Upright);
        assert!((state.local_velocity - local_velocity).length() < 0.0001);
        assert!(
            (state.gait_phase - gait_cycle_phase_delta(walk_locomotion_profile(), 2.0, 1.0 / 64.0))
                .abs()
                < 0.0001
        );
        assert_eq!(state.lead_foot, LeadFoot::Left);
    }

    #[test]
    fn shared_profiles_own_cadence_support_and_flight() {
        assert!(
            (ordinary_step_distance(2.0) - walk_locomotion_profile().step_distance).abs() < 0.0001
        );
        assert!(
            (ordinary_step_distance(5.5) - run_locomotion_profile().step_distance).abs() < 0.0001
        );
        let (walk_left, walk_right) = gait_support_weights(walk_locomotion_profile(), 0.25);
        assert!(walk_left + walk_right > 0.0);
        assert_eq!(
            gait_support_weights(run_locomotion_profile(), 0.25),
            (0.0, 0.0)
        );
        assert_eq!(run_locomotion_profile().flight_apex_metres, 0.12);
    }

    #[test]
    fn locomotion_style_uses_current_physical_speed() {
        let mut state = SkeletonState::default();
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                delta_seconds: 1.0 / locomotion_sample_hz(),
                tick: 1,
            },
        );

        assert_eq!(
            state.animation_speed(),
            walk_locomotion_profile().reference_speed
        );
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert!(evaluation.base.iter().all(|sample| matches!(
            sample.pose,
            SemanticPose::WalkContact | SemanticPose::WalkPassing
        )));
        assert_eq!(
            evaluation
                .base
                .iter()
                .map(|sample| sample.weight)
                .sum::<f32>(),
            1.0
        );
    }

    #[test]
    fn projector_sequences_contacts_acceleration_and_one_landing_edge() {
        let mut state = SkeletonState::default()
            .with_gait_phase(0.49)
            .with_locomotion_sample_tick(1)
            .with_local_velocity(Vec3::new(0.0, -4.0, -1.0))
            .with_world_velocity(Vec3::new(0.0, -4.0, -1.0))
            .with_body_state(BodyState::Airborne);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                delta_seconds: 0.1,
                tick: 2,
            },
        );
        assert_eq!(state.contact_sequence, 1);
        assert_eq!(state.contact_foot, LeadFoot::Right);
        assert_eq!(state.landing_sequence, 1);
        assert_eq!(state.landing_impact_speed, 4.0);
        assert!(state.world_acceleration.length() > 0.0);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                delta_seconds: 0.1,
                tick: 3,
            },
        );
        assert_eq!(state.landing_sequence, 1);
    }

    #[test]
    fn quickstep_contact_returns_to_raised_guard_without_discarding_momentum() {
        let mut state = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_locomotion_sample_tick(10)
            .with_world_velocity(Vec3::new(3.0, -2.0, 0.0))
            .with_body_state(BodyState::Airborne);
        state
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();

        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::X * 2.5,
                grounded: true,
                delta_seconds: 1.0 / locomotion_sample_hz(),
                tick: 11,
            },
        );

        assert_eq!(state.action_kind(), SkeletonAction::None);
        assert_eq!(state.body(), BodyState::Grounded(GroundedPosture::Upright));
        assert_eq!(state.world_velocity, Vec3::X * 2.5);
        assert!(state.raised_locomotion().is_moving());
        assert_eq!(
            AnimationEvaluation::from_skeleton(&state).base[0].pose,
            SemanticPose::GuardThrust
        );
    }

    #[test]
    fn turning_acceleration_is_differenced_in_one_world_frame() {
        let previous_velocity = Vec3::NEG_Z * 5.5;
        let orientation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let current_velocity = controller_yaw(orientation) * Vec3::NEG_Z * 5.5;
        let mut state = SkeletonState::default()
            .with_locomotion_sample_tick(4)
            .with_local_velocity(Vec3::NEG_Z * 5.5)
            .with_world_velocity(previous_velocity);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: current_velocity,
                grounded: true,
                delta_seconds: 1.0 / locomotion_sample_hz(),
                tick: 5,
            },
        );
        let expected =
            ((current_velocity - previous_velocity) * locomotion_sample_hz()).clamp_length_max(80.0);
        assert!(state.world_acceleration.abs_diff_eq(expected, 0.0001));
    }

    #[test]
    fn planar_projection_ignores_camera_pitch() {
        let yaw = 0.7;
        let orientation = Quat::from_euler(EulerRot::YXZ, yaw, 1.25, 0.0);
        let world_velocity = Quat::from_rotation_y(yaw) * Vec3::NEG_Z * 3.0;
        let mut state = SkeletonState::default();
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: world_velocity,
                grounded: true,
                delta_seconds: 1.0 / 64.0,
                tick: 1,
            },
        );
        assert!(state.local_velocity.abs_diff_eq(Vec3::NEG_Z * 3.0, 0.0001));
    }

    #[test]
    fn raised_guard_freezes_lead_and_plans_each_direction_explicitly() {
        let input = |linear_velocity| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity,
            grounded: true,
            delta_seconds: 0.1,
            tick: 1,
        };
        let mut forward = SkeletonState::default()
            .with_lead_foot(LeadFoot::Left)
            .with_gait_phase(0.25)
            .with_weapon_guard(WeaponGuardState::Raised);
        let mut retreat = forward.clone();
        project_skeleton_locomotion(&mut forward, input(Vec3::NEG_Z * 2.0));
        project_skeleton_locomotion(&mut retreat, input(Vec3::Z * 2.0));

        assert_eq!(forward.lead_foot, LeadFoot::Left);
        assert_eq!(retreat.lead_foot, LeadFoot::Left);
        let forward_step = forward.raised_footwork().step().unwrap();
        let retreat_step = retreat.raised_footwork().step().unwrap();
        assert!(forward_step.direction().dot(retreat_step.direction()) < -0.99);

        let mut lowered = SkeletonState::default()
            .with_lead_foot(LeadFoot::Left)
            .with_gait_phase(0.49);
        project_skeleton_locomotion(&mut lowered, input(Vec3::NEG_Z * 2.0));
        assert_eq!(lowered.lead_foot, LeadFoot::Right);
    }

    #[test]
    fn body_facing_is_bounded_and_uses_stable_half_turn() {
        let first = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            Vec3::NEG_Z,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0 / 64.0,
        );
        let angle = Quat::IDENTITY.angle_between(first);
        assert!((angle - body_turn_speed_radians() / 64.0).abs() < 0.0001);
        assert!(
            (first * Vec3::Z).x > 0.0,
            "exact reversal chooses positive yaw"
        );

        let completed = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            Vec3::NEG_Z,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            0.25,
        );
        assert!((Quat::IDENTITY.angle_between(completed) - std::f32::consts::PI).abs() < 0.0001);
    }

    #[test]
    fn dive_launch_root_maps_every_procedural_axis_to_camera_relative_travel() {
        let orientation = Quat::from_euler(EulerRot::YXZ, 0.83, -0.4, 0.2);
        let root = dive_launch_root_rotation(orientation);
        let camera = controller_yaw(orientation);
        for (authored_axis, travel) in [
            (Vec3::Z, camera * Vec3::NEG_Z),
            (Vec3::NEG_Z, camera * Vec3::Z),
            (Vec3::X, camera * Vec3::NEG_X),
            (Vec3::NEG_X, camera * Vec3::X),
        ] {
            assert!((root * authored_axis).abs_diff_eq(travel, 0.000_01));
        }
    }

    #[test]
    fn camera_yaw_maps_quarter_and_half_turns_to_downed_roll() {
        let body = Quat::IDENTITY;
        let same_heading = Quat::from_rotation_y(std::f32::consts::PI);
        let side_heading = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(downed_camera_roll_target(body, same_heading).abs() < 0.0001);
        assert!((downed_camera_roll_target(body, side_heading).abs() - 0.5).abs() < 0.0001);
        assert!((downed_camera_roll_target(body, Quat::IDENTITY) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn lateral_dive_lands_at_its_half_roll_without_crossing_prone_idle() {
        for (direction, expected_half_turns, expected_pose) in [
            (DiveDirection::Left, -0.5, SemanticPose::ProneSupineRollLeft),
            (
                DiveDirection::Right,
                0.5,
                SemanticPose::ProneSupineRollRight,
            ),
        ] {
            let mut state = SkeletonState::default();
            assert!(state.begin_posture_transition(
                PostureTransitionKind::DiveToDowned {
                    direction,
                    trajectory: DiveTrajectory::Airborne,
                },
                0,
                10,
            ));
            state.transition_body(BodyState::Airborne);
            state.advance_posture_transition(1);
            state.transition_body(BodyState::Grounded(GroundedPosture::Upright));
            state.advance_posture_transition(2);
            state.advance_posture_transition(12);

            assert_eq!(state.body(), BodyState::Prone);
            assert_eq!(
                state.downed_facing().map(DownedFacingState::half_turns),
                Some(expected_half_turns)
            );
            let evaluation = AnimationEvaluation::from_skeleton(&state);
            assert_eq!(evaluation.action[0].pose, expected_pose);
        }
    }

    #[test]
    fn backward_dive_landing_transfers_the_supine_half_turn_to_the_root() {
        let mut state = SkeletonState::default();
        assert!(state.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
                trajectory: DiveTrajectory::Airborne,
            },
            0,
            8,
        ));
        state.transition_body(BodyState::Airborne);
        state.advance_posture_transition(1);
        state.transition_body(BodyState::Grounded(GroundedPosture::Upright));
        state.advance_posture_transition(2);
        let previous = state.posture_transition();
        state.advance_posture_transition(6);

        let delta = dive_landing_facing_delta(previous, state.posture_transition());
        let expected_linear = (0.5 - 0.18) / (0.92 - 0.18);
        let expected_progress = expected_linear * expected_linear * (3.0 - 2.0 * expected_linear);
        assert!(
            (Quat::IDENTITY.angle_between(delta) - std::f32::consts::PI * expected_progress).abs()
                < 0.0001
        );
    }

    #[test]
    fn downed_camera_alignment_turns_slowly_and_stops_at_the_observed_step() {
        let first = advance_downed_body_facing(Quat::IDENTITY, Quat::IDENTITY, 0.5);
        assert!((Quat::IDENTITY.angle_between(first) - std::f32::consts::FRAC_PI_4).abs() < 0.0001);
        let unchanged_without_another_step = first;
        assert_eq!(first, unchanged_without_another_step);
    }

    #[test]
    fn guard_faces_look_while_locomotion_faces_world_velocity() {
        let look = Quat::from_rotation_y(0.8);
        let guard = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::Block,
            WeaponGuardState::Lowered,
            1.0,
        );
        assert!((guard * Vec3::Z).abs_diff_eq(look * Vec3::NEG_Z, 0.0001));
        let travel = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0,
        );
        assert!((travel * Vec3::Z).abs_diff_eq(Vec3::X, 0.0001));
        let raised = advance_body_facing(
            Quat::IDENTITY,
            look,
            Vec3::X,
            SkeletonAction::None,
            WeaponGuardState::Raised,
            1.0,
        );
        assert!((raised * Vec3::Z).abs_diff_eq(look * Vec3::NEG_Z, 0.0001));
    }

    #[test]
    fn locomotion_shares_phase_across_walk_and_run() {
        let state = SkeletonState::default()
            .with_local_velocity(Vec3::new(0.0, 0.0, 3.75))
            .with_gait_phase(0.25);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(evaluation.base.len(), 2);
        assert!(evaluation.base.iter().any(|sample| {
            sample.pose == SemanticPose::WalkContact
                && sample.sampling == PoseSampling::Cycle { phase: 0.25 }
                && !sample.mirror_lower_body
                && sample.weight == 0.5
        }));
        assert!(evaluation.base.iter().any(|sample| {
            sample.pose == SemanticPose::RunContact
                && sample.sampling == PoseSampling::Cycle { phase: 0.25 }
                && !sample.mirror_lower_body
                && sample.weight == 0.5
        }));
    }

    #[test]
    fn gait_phase_spans_two_steps_at_run_speed() {
        let speed = 5.5;
        let cycle_seconds = run_locomotion_profile().step_distance * 2.0 / speed;
        assert!(
            (gait_cycle_phase_delta(run_locomotion_profile(), speed, cycle_seconds) - 1.0).abs()
                < 0.0001
        );
    }

    #[test]
    fn raised_guard_locomotion_keeps_guard_upper_body_and_authored_combat_legs() {
        let evaluate = |velocity| {
            AnimationEvaluation::from_skeleton(
                &SkeletonState::default()
                    .with_lead_foot(LeadFoot::Left)
                    .with_local_velocity(velocity)
                    .with_weapon_guard(WeaponGuardState::Raised)
                    .with_gait_phase(0.25)
                    .with_raised_locomotion(raised_intent(velocity)),
            )
        };
        let idle = evaluate(Vec3::ZERO);
        assert_eq!(idle.base[0].pose, SemanticPose::GuardThrust);
        assert_eq!(idle.base[0].sampling, PoseSampling::Anchor);
        assert_eq!(idle.lower_body[0].pose, SemanticPose::CombatStance);

        for (velocity, lower_pose, expected_phase) in [
            (Vec3::NEG_Z, SemanticPose::SkipCycle, 0.0),
            (Vec3::Z, SemanticPose::SkipCycle, 0.5),
            (Vec3::NEG_X, SemanticPose::StrafeCycle, 0.5),
            (Vec3::X, SemanticPose::StrafeCycle, 0.5),
        ] {
            let evaluation = evaluate(velocity);
            assert_eq!(evaluation.base.len(), 1);
            assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
            assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
            assert_eq!(evaluation.lower_body[0].pose, lower_pose);
            assert_eq!(
                evaluation.lower_body[0].sampling,
                PoseSampling::Cycle {
                    phase: expected_phase
                }
            );
        }
    }

    #[test]
    fn raised_guard_diagonal_blends_strafe_and_skip_beneath_static_guard() {
        let evaluation = AnimationEvaluation::from_skeleton(
            &SkeletonState::default()
                .with_lead_foot(LeadFoot::Right)
                .with_local_velocity(Vec3::new(-3.0, 0.0, -1.0))
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_gait_phase(0.75)
                .with_raised_locomotion(raised_intent(Vec3::new(-3.0, 0.0, -1.0))),
        );
        assert_eq!(evaluation.base.len(), 1);
        assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
        assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
        assert_eq!(evaluation.base[0].weight, 1.0);
        assert_eq!(evaluation.lower_body.len(), 2);
        assert_eq!(evaluation.lower_body[0].pose, SemanticPose::StrafeCycle);
        assert_eq!(evaluation.lower_body[0].weight, 0.75);
        assert_eq!(evaluation.lower_body[1].pose, SemanticPose::SkipCycle);
        assert_eq!(evaluation.lower_body[1].weight, 0.25);
    }

    #[test]
    fn forward_diagonals_pair_skip_with_the_opposite_foot_order() {
        let phases = |velocity| {
            AnimationEvaluation::from_skeleton(
                &SkeletonState::default()
                    .with_local_velocity(velocity)
                    .with_weapon_guard(WeaponGuardState::Raised)
                    .with_gait_phase(0.0)
                    .with_raised_locomotion(raised_intent(velocity)),
            )
            .lower_body
            .into_iter()
            .map(|sample| match sample.sampling {
                PoseSampling::Cycle { phase } => (sample.pose, phase),
                _ => panic!("combat locomotion must be a cycle"),
            })
            .collect::<Vec<_>>()
        };

        assert_eq!(
            phases(Vec3::new(1.0, 0.0, 1.0)),
            vec![
                (SemanticPose::StrafeCycle, 0.25),
                (SemanticPose::SkipCycle, 0.25),
            ]
        );
        assert_eq!(
            phases(Vec3::new(-1.0, 0.0, 1.0)),
            vec![
                (SemanticPose::StrafeCycle, 0.75),
                (SemanticPose::SkipCycle, 0.25),
            ]
        );
        assert_eq!(
            phases(Vec3::new(1.0, 0.0, -1.0)),
            vec![
                (SemanticPose::StrafeCycle, 0.25),
                (SemanticPose::SkipCycle, 0.75),
            ]
        );
        assert_eq!(
            phases(Vec3::new(-1.0, 0.0, -1.0)),
            vec![
                (SemanticPose::StrafeCycle, 0.75),
                (SemanticPose::SkipCycle, 0.75),
            ]
        );
    }

    #[test]
    fn left_strafe_reverses_the_single_authored_strafe_cycle() {
        let evaluate = |velocity| {
            AnimationEvaluation::from_skeleton(
                &SkeletonState::default()
                    .with_local_velocity(velocity)
                    .with_weapon_guard(WeaponGuardState::Raised)
                    .with_gait_phase(0.0)
                    .with_raised_locomotion(raised_intent(velocity)),
            )
        };

        assert_eq!(
            evaluate(Vec3::X).lower_body[0].sampling,
            PoseSampling::Cycle { phase: 0.25 }
        );
        assert_eq!(
            evaluate(Vec3::NEG_X).lower_body[0].sampling,
            PoseSampling::Cycle { phase: 0.75 }
        );
    }

    #[test]
    fn raised_guard_upper_body_stays_at_guard_through_authored_step_cycle() {
        for phase in [0.0, 0.5, 0.999] {
            let evaluation = AnimationEvaluation::from_skeleton(
                &SkeletonState::default()
                    .with_lead_foot(LeadFoot::Right)
                    .with_local_velocity(Vec3::NEG_Z)
                    .with_weapon_guard(WeaponGuardState::Raised)
                    .with_gait_phase(phase)
                    .with_raised_locomotion(raised_intent(Vec3::NEG_Z)),
            );
            assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
            assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
            assert_eq!(evaluation.lower_body[0].pose, SemanticPose::SkipCycle);
            assert_eq!(
                evaluation.lower_body[0].sampling,
                PoseSampling::Cycle {
                    phase: (phase + 0.75).rem_euclid(1.0)
                }
            );
        }
    }

    #[test]
    fn raised_guard_jog_uses_walk_run_lower_body_blend() {
        let evaluation = AnimationEvaluation::from_skeleton(
            &SkeletonState::default()
                .with_local_velocity(Vec3::new(0.0, 0.0, -3.75))
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_gait_phase(0.25)
                .with_guarded_sprint_locomotion(true)
                .with_raised_locomotion(raised_intent(Vec3::new(0.0, 0.0, -3.75))),
        );
        assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
        assert_eq!(evaluation.lower_body.len(), 2);
        assert_eq!(evaluation.lower_body[0].pose, SemanticPose::WalkContact);
        assert_eq!(evaluation.lower_body[1].pose, SemanticPose::RunContact);
    }

    #[test]
    fn ordinary_guard_movement_uses_authored_skip_even_above_guard_speed() {
        let evaluation = AnimationEvaluation::from_skeleton(
            &SkeletonState::default()
                .with_local_velocity(Vec3::new(0.0, 0.0, -3.75))
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_gait_phase(0.25)
                .with_raised_locomotion(raised_intent(Vec3::new(0.0, 0.0, -3.75))),
        );

        assert_eq!(evaluation.lower_body.len(), 1);
        assert_eq!(evaluation.lower_body[0].pose, SemanticPose::SkipCycle);
        assert_eq!(evaluation.base.len(), 1);
        assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
    }

    #[test]
    fn entering_raised_guard_resets_to_static_guard_endpoint_once() {
        let mut state = SkeletonState::default()
            .with_gait_phase(0.63)
            .with_lead_foot(LeadFoot::Right);
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.gait_phase, 0.0);
        assert_eq!(state.lead_foot, LeadFoot::Right);

        state.gait_phase = 0.25;
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.gait_phase, 0.25);
        set_weapon_guard(&mut state, WeaponGuardState::Lowered);
        assert_eq!(state.gait_phase, 0.25);
    }

    #[test]
    fn raised_guard_release_finishes_the_authoritative_in_flight_step() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, tick| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            delta_seconds: 1.0 / locomotion_sample_hz(),
            tick,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0, 1));
        assert!(state.raised_locomotion().is_moving());
        let step = state.raised_footwork().step().unwrap();

        project_skeleton_locomotion(&mut state, input(Vec3::ZERO, step.contact_tick() - 1));
        assert!(state.raised_locomotion().is_moving());
        assert_eq!(state.raised_locomotion().local_direction(), Vec2::NEG_Y);
        project_skeleton_locomotion(&mut state, input(Vec3::ZERO, step.contact_tick()));
        assert!(!state.raised_locomotion().is_moving());
        assert!(matches!(
            state.raised_footwork(),
            GuardFootworkPlan::Planted { .. }
        ));
    }

    #[test]
    fn raised_guard_direction_change_places_a_new_contact_when_support_is_behind() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, tick| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            delta_seconds: 1.0 / locomotion_sample_hz(),
            tick,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_X * 2.0, 1));
        let previous_sequence = state.contact_sequence;
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_Z * 2.0, 2));
        assert_eq!(state.raised_locomotion().local_direction(), Vec2::NEG_Y);
        let replanned = state.raised_footwork().step().unwrap();
        assert!(state.contact_sequence > previous_sequence);
        assert!(replanned.direction().dot(Vec2::NEG_Y) > 0.99);
        assert_eq!(state.lead_foot, LeadFoot::Left);
    }

    #[test]
    fn raised_guard_reversal_updates_motion_without_snapping_visual_phase() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, tick| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            delta_seconds: 0.05,
            tick,
        };
        project_skeleton_locomotion(&mut state, input(Vec3::NEG_X * 2.0, 1));
        let committed = state.raised_footwork().step().unwrap();
        project_skeleton_locomotion(&mut state, input(Vec3::X * 2.0, 2));
        assert_eq!(state.raised_locomotion().local_direction(), Vec2::X);
        let retained = state.raised_footwork().step().unwrap();
        assert_eq!(retained.start_tick(), committed.start_tick());
        assert_eq!(retained.landing(), committed.landing() - Vec2::X * 0.1);
    }

    #[test]
    fn raised_guard_step_deadline_tightens_with_speed() {
        let plan = |speed| {
            let mut state = SkeletonState::default();
            set_weapon_guard(&mut state, WeaponGuardState::Raised);
            for tick in 1..=80 {
                project_skeleton_locomotion(
                    &mut state,
                    SkeletonLocomotionInput {
                        orientation: Quat::IDENTITY,
                        linear_velocity: Vec3::NEG_Z * speed,
                        grounded: true,
                        delta_seconds: 1.0 / locomotion_sample_hz(),
                        tick,
                    },
                );
                if state.contact_sequence > 0 {
                    break;
                }
            }
            state.raised_footwork().step().unwrap()
        };
        let slow = plan(0.1);
        let fast = plan(2.0);
        assert!(fast.contact_tick() - fast.start_tick() < slow.contact_tick() - slow.start_tick());
    }

    #[test]
    fn raised_guard_movement_front_foot_follows_direction() {
        let lead = LeadFoot::Left;
        assert_eq!(guard_movement_front_foot(lead, Vec2::NEG_Y), lead);
        assert_eq!(guard_movement_front_foot(lead, Vec2::Y), LeadFoot::Right);
        assert_eq!(guard_movement_front_foot(lead, Vec2::NEG_X), LeadFoot::Left);
        assert_eq!(guard_movement_front_foot(lead, Vec2::X), LeadFoot::Right);
        assert!(
            (guard_maximum_lateral_foot_separation(0.840_348)
                - guard_maximum_foot_separation(0.840_348))
            .abs()
                < 0.0001
        );
        assert!(guard_contact_travel_distance(0.840_348, Vec2::X) > 0.34);
    }

    #[test]
    fn held_guard_input_survives_zero_velocity_until_front_contact() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        let input = |velocity, tick, delta_seconds| SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: velocity,
            grounded: true,
            delta_seconds,
            tick,
        };
        project_skeleton_locomotion_with_intent(
            &mut state,
            input(Vec3::NEG_Z * 0.1, 1, 0.05),
            Some(Vec2::NEG_Y),
        );
        assert_eq!(state.contact_foot, LeadFoot::Right);

        project_skeleton_locomotion_with_intent(
            &mut state,
            input(Vec3::ZERO, 2, guard_maximum_unsupported_contact_seconds()),
            Some(Vec2::NEG_Y),
        );
        assert!(state.raised_locomotion().is_moving());
        assert_eq!(state.contact_foot, state.lead_foot);
    }

    #[test]
    fn raised_guard_contacts_are_committed_one_planned_landing_at_a_time() {
        let mut state = SkeletonState::default();
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        for tick in 1..=80 {
            project_skeleton_locomotion(
                &mut state,
                SkeletonLocomotionInput {
                    orientation: Quat::IDENTITY,
                    linear_velocity: Vec3::NEG_Z * 2.0,
                    grounded: true,
                    delta_seconds: 1.0 / locomotion_sample_hz(),
                    tick,
                },
            );
        }
        assert!(state.contact_sequence >= 2);
        let contacts = state.raised_footwork().contacts().unwrap();
        assert!(contacts.left().is_finite() && contacts.right().is_finite());
    }

    #[test]
    fn raised_guard_contacts_never_trail_the_body_and_handoffs_land_immediately() {
        for velocity in [
            Vec3::NEG_Z * 2.0,
            Vec3::Z * 2.0,
            Vec3::NEG_X * 2.0,
            Vec3::X * 2.0,
        ] {
            let mut state = SkeletonState::default();
            set_weapon_guard(&mut state, WeaponGuardState::Raised);
            let direction = velocity.xz().normalize();
            let mut previous_sequence = state.contact_sequence;
            for tick in 1..=80 {
                if tick == 20 {
                    state.begin_attack(AttackSpec::default(), tick, 30).unwrap();
                }
                project_skeleton_locomotion(
                    &mut state,
                    SkeletonLocomotionInput {
                        orientation: Quat::IDENTITY,
                        linear_velocity: velocity,
                        grounded: true,
                        delta_seconds: 1.0 / locomotion_sample_hz(),
                        tick,
                    },
                );
                let contacts = state.raised_footwork().contacts().unwrap();
                assert!(
                    contacts
                        .left()
                        .dot(direction)
                        .max(contacts.right().dot(direction))
                        >= 0.0
                );
                if state.contact_sequence != previous_sequence {
                    let landed = match state.contact_foot {
                        LeadFoot::Left => contacts.left(),
                        LeadFoot::Right => contacts.right(),
                    };
                    assert!(landed.dot(direction) >= guard_contact_margin_metres() - 0.0001);
                    previous_sequence = state.contact_sequence;
                }
            }
        }
    }

    #[test]
    fn raised_guard_preserves_airborne_posture() {
        let airborne = AnimationEvaluation::from_skeleton(
            &SkeletonState::default()
                .with_local_velocity(Vec3::Y)
                .with_body_state(BodyState::Airborne)
                .with_weapon_guard(WeaponGuardState::Raised),
        );
        assert_eq!(airborne.base[0].pose, SemanticPose::AirborneCenter);
    }

    #[test]
    fn low_speed_idle_and_complete_cycle_remain_unmirrored() {
        let evaluation = AnimationEvaluation::from_skeleton(
            &SkeletonState::default()
                .with_local_velocity(Vec3::new(0.0, 0.0, 0.25))
                .with_gait_phase(0.375),
        );
        assert!(evaluation.base.len() >= 2);
        assert!(!evaluation.base[0].mirror_lower_body);
        assert!(
            evaluation
                .base
                .iter()
                .all(|sample| !sample.mirror_lower_body)
        );
        assert!(evaluation.base.iter().any(|sample| matches!(
            sample.sampling,
            PoseSampling::Cycle { phase } if (phase - 0.375).abs() < 0.0001
        )));
    }

    #[test]
    fn ordinary_locomotion_blends_strafe_while_hips_turn_toward_velocity() {
        let input = SkeletonLocomotionInput {
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::X * 2.0,
            grounded: true,
            delta_seconds: 1.0 / locomotion_sample_hz(),
            tick: 1,
        };

        let mut lateral = SkeletonState::default();
        project_skeleton_locomotion_with_body_rotation(
            &mut lateral,
            input,
            Quat::IDENTITY,
            None,
        );
        let lateral_evaluation = AnimationEvaluation::from_skeleton(&lateral);
        assert!(lateral.local_velocity.abs_diff_eq(Vec3::X * 2.0, 0.0001));
        assert_eq!(lateral_evaluation.base.len(), 1);
        assert_eq!(lateral_evaluation.base[0].pose, SemanticPose::StrafeCycle);

        let mut turning = SkeletonState::default();
        project_skeleton_locomotion_with_body_rotation(
            &mut turning,
            input,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            None,
        );
        let turning_evaluation = AnimationEvaluation::from_skeleton(&turning);
        assert!(turning_evaluation.base.iter().any(|sample| {
            matches!(sample.pose, SemanticPose::WalkContact | SemanticPose::RunContact)
        }));
        assert!(
            turning_evaluation
                .base
                .iter()
                .any(|sample| sample.pose == SemanticPose::StrafeCycle)
        );

        let mut aligned = SkeletonState::default();
        project_skeleton_locomotion_with_body_rotation(
            &mut aligned,
            input,
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            None,
        );
        let aligned_evaluation = AnimationEvaluation::from_skeleton(&aligned);
        assert!(
            aligned_evaluation
                .base
                .iter()
                .all(|sample| sample.pose != SemanticPose::StrafeCycle)
        );
    }

    #[test]
    fn gait_constructs_four_quarters_from_sparse_authoritative_anchors() {
        let samples = [0.0, 0.25, 0.5, 0.75]
            .map(|phase| gait_pair(phase, SemanticPose::WalkContact, SemanticPose::WalkPassing)[0]);
        assert_eq!(
            samples.map(|sample| sample.pose),
            [
                SemanticPose::WalkContact,
                SemanticPose::WalkPassing,
                SemanticPose::WalkContact,
                SemanticPose::WalkPassing,
            ]
        );
        assert_eq!(
            samples.map(|sample| sample.sampling),
            [PoseSampling::Anchor; 4]
        );
        assert_eq!(
            samples.map(|sample| sample.mirror_lower_body),
            [false, false, true, true]
        );
        let between = gait_pair(0.375, SemanticPose::WalkContact, SemanticPose::WalkPassing);
        assert_eq!(between.len(), 2);
        assert_eq!(
            between
                .iter()
                .map(|sample| (sample.pose, sample.mirror_lower_body, sample.weight))
                .collect::<Vec<_>>(),
            vec![
                (SemanticPose::WalkPassing, false, 0.5),
                (SemanticPose::WalkContact, true, 0.5),
            ]
        );
    }

    #[test]
    fn gait_never_fractionally_mirrors_an_fk_blend() {
        for frame in 0..=256 {
            let samples = gait_pair(
                frame as f32 / 256.0,
                SemanticPose::RunContact,
                SemanticPose::RunFlight,
            );
            assert!(samples.len() <= 2);
            assert!((samples.iter().map(|sample| sample.weight).sum::<f32>() - 1.0).abs() < 0.0001);
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.sampling == PoseSampling::Anchor)
            );
        }
    }

    #[test]
    fn gait_anchor_weights_use_monotone_cubic_quarters() {
        for frames_per_cycle in [20, 40] {
            let step = 1.0 / frames_per_cycle as f32;
            let samples = (0..frames_per_cycle)
                .map(|frame| {
                    let phase = frame as f32 * step;
                    let quarter = phase.rem_euclid(1.0) * 4.0;
                    let gait =
                        gait_pair(phase, SemanticPose::WalkContact, SemanticPose::WalkPassing);
                    let end_weight = gait.get(1).map_or(0.0, |sample| sample.weight);
                    (quarter.floor() as u8, end_weight)
                })
                .collect::<Vec<_>>();
            let travel = samples
                .windows(2)
                .map(|pair| {
                    let (before_segment, before) = pair[0];
                    let (after_segment, after) = pair[1];
                    if before_segment == after_segment {
                        (after - before).abs()
                    } else {
                        assert_eq!((after_segment + 4 - before_segment) % 4, 1);
                        1.0 - before + after
                    }
                })
                .collect::<Vec<_>>();
            assert!(
                travel.iter().all(|delta| *delta >= 0.0 && *delta < 0.3),
                "non-monotone or discontinuous gait travel at {frames_per_cycle} frames/cycle: {travel:?}"
            );
            let start = gait_pair(0.0, SemanticPose::WalkContact, SemanticPose::WalkPassing);
            let near_start = gait_pair(step, SemanticPose::WalkContact, SemanticPose::WalkPassing);
            let start_delta = near_start.get(1).map_or(0.0, |sample| sample.weight)
                - start.get(1).map_or(0.0, |sample| sample.weight);
            let middle = gait_pair(0.125, SemanticPose::WalkContact, SemanticPose::WalkPassing);
            let near_middle = gait_pair(
                0.125 + step,
                SemanticPose::WalkContact,
                SemanticPose::WalkPassing,
            );
            let middle_delta = near_middle.get(1).map_or(0.0, |sample| sample.weight)
                - middle.get(1).map_or(0.0, |sample| sample.weight);
            assert!(
                start_delta < middle_delta,
                "Hermite endpoints ease into motion"
            );
        }
    }

    #[test]
    fn attack_curve_reaches_contact_then_recovers_through_guard_contact_axis() {
        let mut state = SkeletonState::default();
        state
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 0, 100)
            .unwrap();
        state.advance_action(100);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(
            evaluation.action,
            vec![PoseSample {
                pose: SemanticPose::GuardSwing,
                sampling: PoseSampling::CurveSpan {
                    end: SemanticPose::AttackSwing,
                    coordinate: 1.0,
                },
                weight: 1.0,
                mirror_lower_body: false,
            }]
        );

        state.advance_action(199);
        let end = AnimationEvaluation::from_skeleton(&state);
        let PoseSampling::CurveSpan { end, coordinate } = end.action[0].sampling else {
            panic!("attack recovery should remain an extrapolating curve span");
        };
        assert_eq!(end, SemanticPose::AttackSwing);
        assert!(coordinate > 0.0 && coordinate < 0.01);
    }

    #[test]
    fn preferred_attack_falls_back_to_the_available_family() {
        let mut state = SkeletonState::default();
        state.attack_animations = AttackAnimations {
            swing: false,
            swing_continuation: false,
            thrust: true,
            thrust_continuation: false,
            offhand: true,
            offhand_preparation: false,
        };
        assert_eq!(
            state.available_strike_family(StrikeFamily::Swing),
            Some(StrikeFamily::Thrust)
        );
        assert_eq!(
            state.select_main_attack(StrikeFamily::Thrust),
            Some(AttackSpec::main(StrikeFamily::Thrust, false))
        );
    }

    #[test]
    fn continuation_waits_for_full_recovery_then_enters_follow_up_preparation() {
        let mut state = SkeletonState::default();
        state.attack_animations = AttackAnimations {
            swing: true,
            swing_continuation: true,
            thrust: true,
            thrust_continuation: true,
            offhand: true,
            offhand_preparation: false,
        };
        state
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 10, 20)
            .unwrap();
        state.advance_action(19);
        assert_eq!(state.select_main_attack(StrikeFamily::Swing), None);
        state.advance_action(20);
        assert_eq!(
            state.select_main_attack(StrikeFamily::Swing),
            Some(AttackSpec::main(StrikeFamily::Swing, true))
        );
        state
            .begin_attack_timed(
                AttackSpec::main(StrikeFamily::Swing, true),
                30,
                40,
                50,
            )
            .unwrap();
        assert!(!state.attack_is_continuation());
        assert_eq!(state.select_main_attack(StrikeFamily::Swing), None);

        state.advance_action(29);
        assert!(!state.attack_is_continuation());
        let recovery = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(recovery.action[0].pose, SemanticPose::GuardSwing);
        let PoseSampling::CurveSpan { end, coordinate } = recovery.action[0].sampling else {
            panic!("queued recovery should remain a frame-0-to-frame-4 extrapolation");
        };
        assert_eq!(end, SemanticPose::AttackSwing);
        assert_eq!(coordinate, 1.0 + state.attack_curve().overshoot);

        state.advance_action(30);
        assert!(state.attack_is_continuation());
        assert_eq!(state.action_phase(), 0.0);
        let follow_up_start = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(follow_up_start.action.len(), 2);
        assert_eq!(
            follow_up_start.action[0].sampling,
            PoseSampling::CurveSpan {
                end: SemanticPose::AttackSwing,
                coordinate: 1.0 + state.attack_curve().overshoot,
            }
        );
        assert_eq!(follow_up_start.action[0].weight, 1.0);
        assert_eq!(follow_up_start.action[1].pose, SemanticPose::RecoverSwing);
        assert_eq!(follow_up_start.action[1].sampling, PoseSampling::Anchor);
        assert_eq!(follow_up_start.action[1].weight, 0.0);

        state.advance_action(35);
        let prepared = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(prepared.action[0].pose, SemanticPose::RecoverSwing);
        assert_eq!(
            prepared.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::ContinueSwing,
                progress: 0.0,
            }
        );
        assert_eq!(state.select_main_attack(StrikeFamily::Swing), None);
    }

    #[test]
    fn attacking_and_ordinary_raised_guard_use_identical_locomotion() {
        let mut guard = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        let mut attack = guard.clone();
        attack
            .begin_attack(AttackSpec::new(AttackAnimation::Thrust), 10, 1000)
            .unwrap();

        for tick in 11..80 {
            let input = SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                delta_seconds: 1.0 / locomotion_sample_hz(),
                tick,
            };
            project_skeleton_locomotion(&mut guard, input);
            project_skeleton_locomotion(&mut attack, input);

            assert_eq!(attack.local_velocity, guard.local_velocity);
            assert_eq!(attack.gait_phase, guard.gait_phase);
            assert_eq!(attack.raised_locomotion(), guard.raised_locomotion());
            assert_eq!(attack.lead_foot, guard.lead_foot);
        }
        assert_eq!(attack.action_kind(), SkeletonAction::Attack);
        assert_eq!(attack.attack_animation(), Some(AttackAnimation::Thrust));
    }

    #[test]
    fn horizontal_speed_drives_continuous_airborne_span() {
        for (speed, progress) in [(0.0, 0.0), (2.0, 1.0)] {
            let evaluation = AnimationEvaluation::from_skeleton(
                &SkeletonState::default()
                    .with_local_velocity(Vec3::new(speed, 0.0, 0.0))
                    .with_body_state(BodyState::Airborne),
            );
            assert_eq!(evaluation.base[0].pose, SemanticPose::AirborneCenter);
            assert_eq!(
                evaluation.base[0].sampling,
                PoseSampling::Span {
                    end: SemanticPose::AirborneTravel,
                    progress
                }
            );
        }
    }
    #[test]
    fn quickstep_holds_guard_while_block_returns_to_guard() {
        let mut dodge_state = SkeletonState::default();
        dodge_state
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();
        dodge_state.advance_action(150);
        let dodge = AnimationEvaluation::from_skeleton(&dodge_state);
        assert_eq!(dodge.base[0].pose, SemanticPose::GuardThrust);
        assert!(dodge.action.is_empty());

        let mut right_lead_dodge_state = SkeletonState::default().with_lead_foot(LeadFoot::Right);
        right_lead_dodge_state
            .begin_dodge(DodgeSpec::quickstep(Vec2::NEG_X).unwrap(), 0, 100)
            .unwrap();
        right_lead_dodge_state.advance_action(50);
        let right_lead_dodge = AnimationEvaluation::from_skeleton(&right_lead_dodge_state);
        assert_eq!(right_lead_dodge.base[0].pose, SemanticPose::GuardThrust);
        assert!(right_lead_dodge.action.is_empty());

        dodge_state.transition_body(BodyState::Airborne);
        assert_eq!(
            AnimationEvaluation::from_skeleton(&dodge_state).base[0].pose,
            SemanticPose::GuardThrust
        );

        let mut block_state = SkeletonState::default().with_lead_foot(LeadFoot::Left);
        block_state
            .begin_block(BlockSpec::default(), 0, 100)
            .unwrap();
        block_state.advance_action(150);
        let block = AnimationEvaluation::from_skeleton(&block_state);
        assert!(block.action.is_empty());
    }

    #[test]
    fn quickstep_samples_the_complete_authored_timeline() {
        let mut state = SkeletonState::default();
        state
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();

        for (tick, expected_progress) in [(0, 0.0), (50, 0.25), (100, 0.5), (150, 0.75), (200, 1.0)]
        {
            state.advance_action(tick);
            let evaluation = AnimationEvaluation::from_skeleton(&state);
            assert_eq!(evaluation.lower_body.len(), 1);
            assert_eq!(
                evaluation.lower_body[0].pose,
                SemanticPose::QuickstepRightTakeoff
            );
            assert_eq!(
                evaluation.lower_body[0].sampling,
                PoseSampling::Timeline {
                    progress: expected_progress
                }
            );
        }
    }

    #[test]
    fn authoritative_action_clock_centers_contact_and_finishes_recovery() {
        let mut state = SkeletonState::default();
        state.begin_attack(AttackSpec::default(), 10, 20).unwrap();
        state.advance_action(15);
        assert_eq!(state.action_phase(), 0.25);
        state.advance_action(20);
        assert_eq!(state.action_phase(), 0.5);
        state.advance_action(25);
        assert_eq!(state.action_phase(), 0.75);
        state.advance_action(30);
        assert_eq!(state.action_phase(), 1.0);
        state.advance_action(31);
        assert_eq!(state.action(), ActionState::default());
    }

    #[test]
    fn authoritative_attack_clock_supports_asymmetric_recovery() {
        let mut state = SkeletonState::default();
        state
            .begin_attack_timed(AttackSpec::default(), 10, 20, 26)
            .unwrap();
        state.advance_action(23);
        assert_eq!(state.action_phase(), 0.75);
        state.advance_action(26);
        assert_eq!(state.action_phase(), 1.0);
        state.advance_action(27);
        assert_eq!(state.action(), ActionState::default());
    }

    #[test]
    fn poor_handling_increases_readable_drawback_and_follow_through() {
        let controlled = AttackCurve::from_handling(0.03, 5.0);
        let uncontrolled = AttackCurve::from_handling(1.2, 1.0);
        assert!(uncontrolled.tell_fraction > controlled.tell_fraction);
        assert!(uncontrolled.coordinate(0.15) < controlled.coordinate(0.15));
        assert!(uncontrolled.coordinate(0.6) > controlled.coordinate(0.6));
        assert_eq!(uncontrolled.coordinate(0.5), 1.0);
        assert!(uncontrolled.coordinate(1.0).abs() < 1.0e-6);
    }

    #[test]
    fn ordinary_attack_crosses_contact_without_stopping_or_changing_velocity() {
        for curve in [
            AttackCurve::from_handling(0.03, 5.0),
            AttackCurve::default(),
            AttackCurve::from_handling(1.2, 1.0),
        ] {
            let step = 0.0025;
            let contact = curve.coordinate(0.5);
            let left_velocity = (contact - curve.coordinate(0.5 - step)) / step;
            let right_velocity = (curve.coordinate(0.5 + step) - contact) / step;
            assert!(left_velocity > 0.1, "contact must retain forward velocity");
            assert!(right_velocity > 0.1, "follow-through must continue forward");
            assert!(
                (left_velocity - right_velocity).abs() < 0.01,
                "contact velocity changed from {left_velocity} to {right_velocity}"
            );
        }
    }

    #[test]
    fn ordinary_attack_remains_monotone_from_drawback_through_follow_through() {
        for curve in [
            AttackCurve::from_handling(0.03, 5.0),
            AttackCurve::default(),
            AttackCurve::from_handling(1.2, 1.0),
        ] {
            let tell_end = curve.tell_fraction * 0.5;
            let follow_through_end = 0.5 + curve.follow_through_fraction * 0.5;
            let mut previous = curve.coordinate(tell_end);
            for sample in 1..=128 {
                let phase = tell_end
                    + (follow_through_end - tell_end) * sample as f32 / 128.0;
                let coordinate = curve.coordinate(phase);
                assert!(
                    coordinate + 1.0e-5 >= previous,
                    "attack reversed at phase {phase}: {previous} -> {coordinate}"
                );
                previous = coordinate;
            }
        }
    }

    #[test]
    fn jump_anticipation_does_not_change_upright_posture() {
        let mut state = SkeletonState::default();
        state.set_jump_anticipation(true);
        assert_eq!(state.jump_anticipation(), JumpAnticipation::Charging);
        assert_eq!(state.body(), BodyState::Grounded(GroundedPosture::Upright));
        assert_eq!(state.posture(), Posture::Upright);

        state.transition_body(BodyState::Airborne);
        assert_eq!(state.jump_anticipation(), JumpAnticipation::Inactive);
    }
}
