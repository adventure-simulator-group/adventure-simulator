use super::*;

/// Stable semantic names used by animation packs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPose {
    IdleRelaxed,
    WalkContact,
    WalkPassing,
    RunContact,
    RunFlight,
    CrouchIdle,
    DuckForward,
    DuckBackward,
    DuckLeft,
    DuckRight,
    DiveForward,
    DiveBackward,
    DiveLeft,
    DiveRight,
    AirborneCenter,
    AirborneTravel,
    ProneIdle,
    SupineIdle,
    ProneCrawlContact,
    SupineScamperContact,
    ProneTransition,
    ProneSupineRollLeft,
    ProneSupineRollRight,
    SupineTransition,
    Guard,
    AttackSwing,
    AttackSwingFollow,
    AttackThrust,
    BlockCutLeft,
    BlockCutRight,
    BlockThrust,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod contract_tests {
    use super::*;

    #[test]
    fn humanoid_contract_resolves_from_twenty_five_authored_poses() {
        assert_eq!(SemanticPose::HUMANOID_REQUIRED.len(), 28);
        let authored = SemanticPose::HUMANOID_REQUIRED
            .into_iter()
            .filter(|pose| {
                pose.mirrored_counterpart()
                    .is_none_or(|other| pose.as_str() < other.as_str())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(authored.len(), 25);
        let mut library = AnimationPackLibrary::default();
        library
            .insert(AnimationPack {
                id: "root".into(),
                skeleton_family: "humanoid".into(),
                fallback: None,
                clips: authored,
            })
            .unwrap();
        assert_eq!(library.validate_complete("root"), Ok(()));
    }

    #[test]
    fn complete_validation_requires_a_terminal_family_root() {
        let mut library = AnimationPackLibrary::default();
        library
            .insert(AnimationPack {
                id: "root".into(),
                skeleton_family: "humanoid".into(),
                fallback: Some("parent".into()),
                clips: BTreeSet::new(),
            })
            .unwrap();
        library
            .insert(AnimationPack {
                id: "parent".into(),
                skeleton_family: "humanoid".into(),
                fallback: None,
                clips: BTreeSet::new(),
            })
            .unwrap();
        assert_eq!(
            library.validate_complete("root"),
            Err(PackValidationError::RootHasFallback("root".into()))
        );
    }

    #[test]
    fn body_and_action_discriminants_own_their_valid_payloads() {
        let mut state = SkeletonState::default();
        assert_eq!(state.body(), BodyState::Grounded(GroundedPosture::Upright));
        assert!(state.is_grounded());
        assert!(state.action_view().is_none());

        state.begin_attack(AttackSpec::default(), 10, 20).unwrap();
        assert_eq!(state.action_kind(), SkeletonAction::Attack);
        state.begin_dodge(DodgeSpec::default(), 12, 13).unwrap();
        assert_eq!(state.action_kind(), SkeletonAction::Dodge);
    }

    #[test]
    fn lowered_stance_cannot_retain_raised_motion_and_inputs_are_normalized() {
        let raised = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_raised_locomotion(RaisedLocomotionIntent::moving(
                Vec2::new(10.0, 0.0),
                2.0,
                LeadFoot::Left,
                0,
            ));
        assert_eq!(raised.raised_locomotion().local_direction(), Vec2::X);
        let lowered = raised.with_weapon_guard(WeaponGuardState::Lowered);
        assert_eq!(lowered.stance(), StanceState::Lowered);
        assert_eq!(
            lowered.raised_locomotion(),
            RaisedLocomotionIntent::default()
        );
    }

    #[test]
    fn action_timeline_uses_saturating_arithmetic_at_u64_max() {
        let mut state = SkeletonState::default();
        state
            .begin_block(BlockSpec::default(), u64::MAX, u64::MAX)
            .unwrap();
        state.advance_action(u64::MAX);
        assert!(state.action_view().is_none());
        assert_eq!(state.action_phase(), 0.0);
    }

    #[test]
    fn projection_owns_grounded_body_transitions_and_sanitizes_motion() {
        let mut state = SkeletonState::default();
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::splat(f32::NAN),
                grounded: false,
                crouching: true,
                delta_seconds: f32::NAN,
                tick: 1,
            },
        );
        assert_eq!(state.body(), BodyState::Airborne);
        assert!(!state.is_grounded());
        assert_eq!(state.world_velocity, Vec3::ZERO);
    }

    #[test]
    fn sparse_attack_evaluation_uses_guard_contact_guard() {
        let mut state = SkeletonState::default();
        state.begin_attack(AttackSpec::default(), 0, 10).unwrap();
        state.advance_action(5);
        let early = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(early.action[0].pose, SemanticPose::Guard);
        assert_eq!(
            early.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::AttackThrust,
                progress: 0.5,
            }
        );
        state.advance_action(15);
        let recovery = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(recovery.action[0].pose, SemanticPose::AttackThrust);
    }

    #[test]
    fn repeated_guard_writes_preserve_live_raised_footwork() {
        let intent = RaisedLocomotionIntent::moving(Vec2::X, 2.0, LeadFoot::Right, 17);
        let mut server_like = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_raised_locomotion(intent);
        server_like.gait_phase = 0.37;
        set_weapon_guard(&mut server_like, WeaponGuardState::Raised);
        assert_eq!(server_like.raised_locomotion(), intent);
        assert_eq!(server_like.gait_phase, 0.37);

        let mut viewer_like = server_like.clone();
        for _ in 0..8 {
            set_weapon_guard(&mut viewer_like, WeaponGuardState::Raised);
        }
        assert_eq!(viewer_like.raised_locomotion(), intent);
        assert_eq!(viewer_like.gait_phase, 0.37);

        set_weapon_guard(&mut viewer_like, WeaponGuardState::Lowered);
        let lowered_phase = viewer_like.gait_phase;
        set_weapon_guard(&mut viewer_like, WeaponGuardState::Lowered);
        assert_eq!(viewer_like.gait_phase, lowered_phase);
        set_weapon_guard(&mut viewer_like, WeaponGuardState::Raised);
        assert_eq!(viewer_like.gait_phase, 0.0);
        assert_eq!(
            viewer_like.raised_locomotion(),
            RaisedLocomotionIntent::default()
        );
    }

    #[test]
    fn downed_body_transition_clears_action_and_raised_stance() {
        let mut state = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        state.begin_attack(AttackSpec::default(), 1, 2).unwrap();
        state.transition_body(BodyState::Prone);
        assert_eq!(state.stance(), StanceState::Lowered);
        assert!(state.action_view().is_none());
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.stance(), StanceState::Lowered);
        assert_eq!(
            state.begin_attack(AttackSpec::default(), 3, 4),
            Err(ActionAdmissionError::BodyCannotAct(BodyState::Prone))
        );
        assert!(state.action_view().is_none());
    }

    #[test]
    fn roll_transition_uses_directional_midpoint_and_commits_supine() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(state.begin_posture_transition(
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Right,
            },
            10,
            20,
        ));
        state.advance_posture_transition(20);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(evaluation.action.len(), 1);
        assert_eq!(
            evaluation.action[0].pose,
            SemanticPose::ProneSupineRollRight
        );
        assert_eq!(
            evaluation.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::SupineIdle,
                progress: 0.0,
            }
        );
        state.advance_posture_transition(30);
        assert_eq!(state.body(), BodyState::Supine);
        assert!(state.posture_transition().is_none());
    }

    #[test]
    fn aimed_downed_facing_holds_half_roll_and_reaches_supine() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(state.advance_downed_facing(0.5, true, 1.0));
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.5);
        assert_eq!(state.downed_lateral_motion(), 1.0);
        assert_eq!(state.body(), BodyState::Prone);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(
            evaluation.action[0].pose,
            SemanticPose::ProneSupineRollRight
        );

        assert!(state.advance_downed_facing(1.0, true, 1.0));
        assert_eq!(state.body(), BodyState::Supine);
        assert_eq!(state.downed_facing().unwrap().half_turns(), 1.0);
        assert!(AnimationEvaluation::from_skeleton(&state).action.is_empty());
    }

    #[test]
    fn aimed_downed_facing_uses_four_discrete_sticky_sectors() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);

        // The prone sector nominally ends at 0.25 half-turns (45 degrees),
        // but remains committed through the ten-degree sticky margin.
        assert!(state.advance_downed_facing(0.30, true, 1.0));
        assert_eq!(
            state.downed_facing().unwrap().target(),
            DownedFacingPose::Prone
        );
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.0);
        assert!(AnimationEvaluation::from_skeleton(&state).action.is_empty());

        assert!(state.advance_downed_facing(0.31, true, 1.0));
        assert_eq!(
            state.downed_facing().unwrap().target(),
            DownedFacingPose::RollRight
        );
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.5);

        // Reversing across the nominal boundary does not chatter back. It
        // must clear the opposite side of the hysteresis deadband first.
        assert!(state.advance_downed_facing(0.24, true, 1.0));
        assert_eq!(
            state.downed_facing().unwrap().target(),
            DownedFacingPose::RollRight
        );
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.5);
        assert!(state.advance_downed_facing(0.19, true, 1.0));
        assert_eq!(
            state.downed_facing().unwrap().target(),
            DownedFacingPose::Prone
        );
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.0);
    }

    #[test]
    fn aimed_downed_facing_interpolates_only_after_sector_commit() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(state.advance_downed_facing(0.5, true, 0.125));
        let transition = state.downed_facing().unwrap();
        assert_eq!(transition.target(), DownedFacingPose::RollRight);
        assert_eq!(transition.half_turns(), 0.125);
        assert_eq!(transition.lateral_motion(), 1.0);
        assert!(!AnimationEvaluation::from_skeleton(&state).action.is_empty());

        for _ in 0..3 {
            state.advance_downed_facing(0.5, true, 0.125);
        }
        assert_eq!(state.downed_facing().unwrap().half_turns(), 0.5);
        state.advance_downed_facing(0.55, true, 0.125);
        let settled = state.downed_facing().unwrap();
        assert_eq!(settled.target(), DownedFacingPose::RollRight);
        assert_eq!(settled.half_turns(), 0.5);
        assert_eq!(settled.lateral_motion(), 0.0);
    }

    #[test]
    fn downed_turning_advances_contact_gait_without_planar_velocity() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Supine);
        state.set_downed_turning(true);
        assert_eq!(state.animation_speed(), 0.8);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::ZERO,
                grounded: true,
                crouching: true,
                delta_seconds: 0.25,
                tick: 1,
            },
        );
        let expected = gait_cycle_phase_delta(SUPINE_LOCOMOTION_PROFILE, 1.6, 0.25);
        assert!((state.gait_phase - expected).abs() < 0.000_01);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert!(
            evaluation
                .base
                .iter()
                .all(|sample| sample.pose == SemanticPose::SupineScamperContact)
        );
    }

    #[test]
    fn prone_cadence_follows_its_authoritative_posture_speed() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: true,
                crouching: true,
                delta_seconds: 0.25,
                tick: 1,
            },
        );
        let expected = gait_cycle_phase_delta(PRONE_LOCOMOTION_PROFILE, 2.0, 0.25);
        assert!((state.gait_phase - expected).abs() < 0.000_01);
    }

    #[test]
    fn releasing_aim_settles_downed_roll_to_nearest_contact() {
        let mut toward_prone = SkeletonState::default().with_body_state(BodyState::Prone);
        toward_prone.advance_downed_facing(0.4, true, 1.0);
        assert!(!toward_prone.advance_downed_facing(0.4, false, 1.0));
        assert_eq!(toward_prone.body(), BodyState::Prone);
        assert!(toward_prone.downed_facing().is_none());

        let mut toward_supine = SkeletonState::default().with_body_state(BodyState::Prone);
        toward_supine.advance_downed_facing(1.0, true, 0.6);
        assert!(!toward_supine.advance_downed_facing(0.0, false, 1.0));
        assert_eq!(toward_supine.body(), BodyState::Supine);
        assert!(toward_supine.downed_facing().is_none());

        let mut exact_half_roll = SkeletonState::default().with_body_state(BodyState::Prone);
        exact_half_roll.advance_downed_facing(0.5, true, 1.0);
        exact_half_roll.advance_downed_facing(0.5, false, 1.0);
        assert_eq!(exact_half_roll.body(), BodyState::Prone);
    }

    #[test]
    fn dive_holds_takeoff_until_contact_then_recovers_to_prone() {
        let mut state = SkeletonState::default();
        assert!(state.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Forward,
            },
            0,
            11,
        ));
        state.advance_posture_transition(5);
        assert!(state.posture_transition().unwrap().phase() < 0.5);

        state.transition_body(BodyState::Airborne);
        state.advance_posture_transition(6);
        assert_eq!(state.posture_transition().unwrap().phase(), 0.5);
        state.advance_posture_transition(100);
        assert_eq!(state.posture_transition().unwrap().phase(), 0.5);

        state.transition_body(BodyState::Grounded(GroundedPosture::Crouched));
        state.advance_posture_transition(101);
        assert_eq!(state.posture_transition().unwrap().phase(), 0.5);
        state.advance_posture_transition(106);
        assert!(state.posture_transition().unwrap().phase() > 0.5);
        state.advance_posture_transition(112);
        assert_eq!(state.body(), BodyState::Prone);
        assert!(state.posture_transition().is_none());
    }

    #[test]
    fn backward_dive_recovers_to_supine_after_contact() {
        let mut state = SkeletonState::default();
        assert!(state.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
            },
            0,
            11,
        ));
        state.transition_body(BodyState::Airborne);
        state.advance_posture_transition(6);
        state.transition_body(BodyState::Grounded(GroundedPosture::Crouched));
        state.advance_posture_transition(7);
        state.advance_posture_transition(18);
        assert_eq!(state.body(), BodyState::Supine);
        assert!(state.posture_transition().is_none());
    }

    #[test]
    fn supported_prone_projection_does_not_repeat_landing_or_restore_upright() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Prone);
        state.landing_sequence = 7;
        project_skeleton_locomotion(
            &mut state,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::ZERO,
                grounded: true,
                crouching: true,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 1,
            },
        );
        assert_eq!(state.body(), BodyState::Prone);
        assert_eq!(state.landing_sequence, 7);
    }

    #[test]
    fn prone_crawl_blends_directly_between_mirrored_contacts() {
        let state = SkeletonState::default()
            .with_body_state(BodyState::Prone)
            .with_local_velocity(Vec3::NEG_Z)
            .with_gait_phase(0.25);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(evaluation.base.len(), 2);
        assert!(evaluation.base.iter().all(|sample| {
            sample.pose == SemanticPose::ProneCrawlContact
                && sample.sampling == PoseSampling::Anchor
        }));
        assert!(!evaluation.base[0].mirror_lower_body);
        assert!(evaluation.base[1].mirror_lower_body);
        assert!((evaluation.base[0].weight - 0.5).abs() < 0.0001);
        assert!((evaluation.base[1].weight - 0.5).abs() < 0.0001);
    }

    #[test]
    fn action_replacement_is_explicitly_last_writer_wins() {
        let mut state = SkeletonState::default();
        let first = state.begin_attack(AttackSpec::default(), 10, 20).unwrap();
        let replacement = state
            .begin_dodge(DodgeSpec { direction: Vec2::X }, 11, 12)
            .unwrap();
        assert_eq!(first.kind(), SkeletonAction::Attack);
        assert_eq!(first.start_tick(), 10);
        assert_eq!(replacement.kind(), SkeletonAction::Dodge);
        assert_eq!(state.action_kind(), SkeletonAction::Dodge);
        assert_eq!(
            state.dodge_view().map(|(direction, _)| direction),
            Some(Vec2::X)
        );
    }

    #[test]
    fn action_admission_rejects_all_downed_bodies_without_mutating_action() {
        for body in [BodyState::Prone, BodyState::Supine, BodyState::Ragdolled] {
            let mut state = SkeletonState::default().with_body_state(body);
            assert_eq!(
                state.begin_attack(AttackSpec::default(), 10, 20),
                Err(ActionAdmissionError::BodyCannotAct(body))
            );
            assert!(state.action_view().is_none());
        }
    }

    #[test]
    fn ragdoll_never_enters_authored_downed_controls() {
        let mut state = SkeletonState::default().with_body_state(BodyState::Ragdolled);
        assert_eq!(state.body().downed_contact(), None);
        assert!(!state.advance_downed_facing(0.5, true, 0.1));
        assert!(!state.begin_posture_transition(
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Left,
            },
            0,
            10,
        ));
        assert!(state.posture_transition().is_none());

        let mut interrupted = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(interrupted.advance_downed_facing(0.5, true, 0.5));
        interrupted.set_downed_turning(true);
        assert!(interrupted.downed_facing().is_some());
        assert!(interrupted.downed_turning());
        interrupted.transition_body(BodyState::Ragdolled);
        assert!(interrupted.downed_facing().is_none());
        assert!(!interrupted.downed_turning());

        let mut acting = SkeletonState::default();
        acting.begin_attack(AttackSpec::default(), 1, 4).unwrap();
        acting.transition_body(BodyState::Ragdolled);
        assert!(acting.action_view().is_none());
    }

    #[test]
    fn typed_action_views_expose_only_the_active_payload() {
        let mut state = SkeletonState::default();
        state
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 10, 20)
            .unwrap();
        assert_eq!(
            state.attack_view(),
            Some((
                0.5,
                AttackAnimation::Swing,
                ActionTimelineView {
                    start_tick: 10,
                    preparation_ticks: 10,
                    phase: 0.0,
                }
            ))
        );
        assert!(state.dodge_view().is_none());
        assert!(state.block_view().is_none());
    }

    #[test]
    fn transition_body_matrix_round_trips_and_normalizes_invalid_wire_pairs() {
        let cases = [
            (BodyState::default(), PostureTransitionKind::UprightToProne),
            (BodyState::Prone, PostureTransitionKind::ProneToUpright),
            (
                BodyState::Prone,
                PostureTransitionKind::ProneToSupine {
                    direction: RollDirection::Left,
                },
            ),
            (
                BodyState::Supine,
                PostureTransitionKind::SupineToProne {
                    direction: RollDirection::Right,
                },
            ),
            (BodyState::Supine, PostureTransitionKind::SupineToUpright),
        ];
        for (body, transition) in cases {
            let mut state = SkeletonState::default().with_body_state(body);
            assert!(state.begin_posture_transition(transition, 4, 12));
            let mut wire = serde_json::to_value(&state).unwrap();
            wire["posture_transition"]["Timed"]["duration_ticks"] = serde_json::json!(0);
            wire["posture_transition"]["Timed"]["phase"] = serde_json::json!(2.0);
            let rebuilt: SkeletonState = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(rebuilt.body(), body);
            assert_eq!(rebuilt.posture_transition().unwrap().kind(), transition);
            assert_eq!(rebuilt.posture_transition().unwrap().phase(), 1.0);

            let mut invalid = wire;
            invalid["body"] = serde_json::to_value(BodyState::Ragdolled).unwrap();
            let interrupted: SkeletonState = serde_json::from_value(invalid).unwrap();
            assert_eq!(interrupted.body(), BodyState::Ragdolled);
            assert!(interrupted.posture_transition().is_none());
        }

        let mut dive = SkeletonState::default();
        assert!(dive.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Forward,
            },
            4,
            12,
        ));
        dive.transition_body(BodyState::Airborne);
        let wire = serde_json::to_value(&dive).unwrap();
        assert!(serde_json::from_value::<SkeletonState>(wire).is_ok());
    }

    #[test]
    fn public_body_interruptions_keep_every_reachable_state_round_trippable() {
        let mut ledge_fall = SkeletonState::default();
        assert!(ledge_fall.begin_posture_transition(PostureTransitionKind::UprightToProne, 0, 10,));
        ledge_fall.transition_body(BodyState::Airborne);
        assert!(ledge_fall.posture_transition().is_none());
        let encoded = postcard::to_allocvec(&ledge_fall).unwrap();
        assert_eq!(
            postcard::from_bytes::<SkeletonState>(&encoded).unwrap(),
            ledge_fall
        );

        let mut ragdoll = SkeletonState::default();
        assert!(ragdoll.begin_posture_transition(PostureTransitionKind::UprightToProne, 0, 10,));
        ragdoll.transition_body(BodyState::Ragdolled);
        assert!(ragdoll.posture_transition().is_none());
        assert!(ragdoll.action_view().is_none());
        assert!(ragdoll.downed_facing().is_none());
        assert!(!ragdoll.downed_turning());
        let encoded = postcard::to_allocvec(&ragdoll).unwrap();
        assert_eq!(
            postcard::from_bytes::<SkeletonState>(&encoded).unwrap(),
            ragdoll
        );

        let mut dive = SkeletonState::default();
        assert!(dive.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Left,
            },
            0,
            10,
        ));
        dive.transition_body(BodyState::Airborne);
        assert!(dive.posture_transition().is_some());
        dive.transition_body(BodyState::Ragdolled);
        assert!(dive.posture_transition().is_none());
    }

    #[test]
    fn transition_reconstruction_clears_forbidden_orthogonal_dimensions() {
        let mut active = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        active.set_jump_anticipation(true);
        active
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 2, 8)
            .unwrap();
        let mut transition = SkeletonState::default();
        assert!(transition.begin_posture_transition(PostureTransitionKind::UprightToProne, 3, 12,));

        let mut adversarial = serde_json::to_value(&active).unwrap();
        adversarial["posture_transition"] =
            serde_json::to_value(transition.posture_transition()).unwrap();
        adversarial["jump_anticipation"] = serde_json::json!("Charging");
        let rebuilt: SkeletonState = serde_json::from_value(adversarial).unwrap();
        assert!(rebuilt.posture_transition().is_some());
        assert_eq!(rebuilt.stance(), StanceState::Lowered);
        assert_eq!(rebuilt.jump_anticipation(), JumpAnticipation::Inactive);
        assert!(rebuilt.action_view().is_none());

        let encoded = postcard::to_allocvec(&rebuilt).unwrap();
        let replicated: SkeletonState = postcard::from_bytes(&encoded).unwrap();
        assert_eq!(replicated, rebuilt);
    }

    #[test]
    fn valid_dive_lifecycle_stages_round_trip_through_postcard() {
        let mut dive = SkeletonState::default();
        assert!(dive.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
            },
            4,
            12,
        ));
        for stage in 0..3 {
            if stage == 1 {
                dive.transition_body(BodyState::Airborne);
                dive.advance_posture_transition(5);
            } else if stage == 2 {
                dive.transition_body(BodyState::Grounded(GroundedPosture::Upright));
                dive.advance_posture_transition(20);
            }
            let encoded = postcard::to_allocvec(&dive).unwrap();
            let rebuilt: SkeletonState = postcard::from_bytes(&encoded).unwrap();
            assert_eq!(rebuilt, dive);
        }
    }

    #[test]
    fn impossible_dive_bookkeeping_is_normalized_for_the_physical_body() {
        let mut dive = SkeletonState::default();
        assert!(dive.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Forward,
            },
            4,
            12,
        ));
        let mut wire = serde_json::to_value(&dive).unwrap();
        wire["body"] = serde_json::to_value(BodyState::Airborne).unwrap();
        wire["posture_transition"]["Dive"]["phase"] = serde_json::json!(0.9);
        wire["posture_transition"]["Dive"]["was_airborne"] = serde_json::json!(false);
        wire["posture_transition"]["Dive"]["landing_tick"] = serde_json::json!(8);

        let rebuilt: SkeletonState = serde_json::from_value(wire).unwrap();
        assert_eq!(rebuilt.body(), BodyState::Airborne);
        assert_eq!(rebuilt.posture_transition().unwrap().phase(), 0.5);
        let normalized = serde_json::to_value(&rebuilt).unwrap();
        assert_eq!(
            normalized["posture_transition"]["Dive"]["was_airborne"],
            serde_json::json!(true)
        );
        assert_eq!(
            normalized["posture_transition"]["Dive"]["landing_tick"],
            serde_json::Value::Null
        );

        let encoded = postcard::to_allocvec(&rebuilt).unwrap();
        assert_eq!(
            postcard::from_bytes::<SkeletonState>(&encoded).unwrap(),
            rebuilt
        );
    }

    #[test]
    fn ragdoll_locomotion_projection_is_inert() {
        let mut ragdoll = SkeletonState::default().with_body_state(BodyState::Ragdolled);
        let before = ragdoll.clone();
        project_skeleton_locomotion(
            &mut ragdoll,
            SkeletonLocomotionInput {
                orientation: Quat::from_rotation_y(1.0),
                linear_velocity: Vec3::new(3.0, -2.0, 1.0),
                grounded: true,
                crouching: true,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 99,
            },
        );
        assert_eq!(ragdoll, before);
    }

    #[test]
    fn raised_intent_round_trip_preserves_valid_variants_and_normalizes_wire_input() {
        let moving = RaisedLocomotionIntent::moving(Vec2::new(3.0, 0.0), 2.0, LeadFoot::Right, 9);
        let json = serde_json::to_string(&moving).unwrap();
        assert_eq!(
            serde_json::from_str::<RaisedLocomotionIntent>(&json).unwrap(),
            moving
        );

        let invalid = r#"{"moving":{"local_direction":[null,0.0],"speed":2.0,"swing_foot":"Left","step_sequence":9}}"#;
        assert!(serde_json::from_str::<RaisedLocomotionIntent>(invalid).is_err());
        assert_eq!(
            RaisedLocomotionIntent::moving(Vec2::ZERO, f32::NAN, LeadFoot::Left, 9),
            RaisedLocomotionIntent::planted(9)
        );

        let mut state = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        state.begin_attack(AttackSpec::default(), 10, 20).unwrap();
        let mut wire = serde_json::to_value(&state).unwrap();
        wire["body"] = serde_json::json!("Prone");
        wire["action"]["Attack"]["timeline"]["preparation_ticks"] = serde_json::json!(0);
        wire["action"]["Attack"]["timeline"]["phase"] = serde_json::json!(2.0);
        let normalized_action: ActionState =
            serde_json::from_value(wire["action"].clone()).unwrap();
        assert_eq!(normalized_action.phase(), 1.0);
        let normalized: SkeletonState = serde_json::from_value(wire).unwrap();
        assert_eq!(normalized.body(), BodyState::Prone);
        assert_eq!(normalized.stance(), StanceState::Lowered);
        assert!(normalized.action_view().is_none());
    }

    #[test]
    fn raised_stance_round_trips_through_replication_codec() {
        for locomotion in [
            RaisedLocomotionIntent::planted(7),
            RaisedLocomotionIntent::moving(Vec2::Y, 2.0, LeadFoot::Right, 8),
        ] {
            let state = SkeletonState::default()
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_raised_locomotion(locomotion);
            let encoded = postcard::to_allocvec(&state).unwrap();
            let decoded: SkeletonState = postcard::from_bytes(&encoded).unwrap();
            assert_eq!(decoded, state);
        }
    }
}

impl SemanticPose {
    pub const ALL: [Self; 31] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::DuckForward,
        Self::DuckBackward,
        Self::DuckLeft,
        Self::DuckRight,
        Self::DiveForward,
        Self::DiveBackward,
        Self::DiveLeft,
        Self::DiveRight,
        Self::AirborneCenter,
        Self::AirborneTravel,
        Self::ProneIdle,
        Self::SupineIdle,
        Self::ProneCrawlContact,
        Self::SupineScamperContact,
        Self::ProneTransition,
        Self::ProneSupineRollLeft,
        Self::ProneSupineRollRight,
        Self::SupineTransition,
        Self::Guard,
        Self::AttackSwing,
        Self::AttackSwingFollow,
        Self::AttackThrust,
        Self::BlockCutLeft,
        Self::BlockCutRight,
        Self::BlockThrust,
    ];
    /// Non-attack semantics every complete humanoid family must resolve.
    /// Attack clips are capabilities: a pack may deliberately omit any or all
    /// of them, and gameplay respects that absence.
    pub const HUMANOID_REQUIRED: [Self; 28] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::DuckForward,
        Self::DuckBackward,
        Self::DuckLeft,
        Self::DuckRight,
        Self::DiveForward,
        Self::DiveBackward,
        Self::DiveLeft,
        Self::DiveRight,
        Self::AirborneCenter,
        Self::AirborneTravel,
        Self::ProneIdle,
        Self::SupineIdle,
        Self::ProneCrawlContact,
        Self::SupineScamperContact,
        Self::ProneTransition,
        Self::ProneSupineRollLeft,
        Self::ProneSupineRollRight,
        Self::SupineTransition,
        Self::Guard,
        Self::BlockCutLeft,
        Self::BlockCutRight,
        Self::BlockThrust,
    ];

    pub fn as_str(self) -> &'static str {
        use SemanticPose::*;
        match self {
            IdleRelaxed => "idle_relaxed",
            WalkContact => "walk_contact",
            WalkPassing => "walk_passing",
            RunContact => "run_contact",
            RunFlight => "run_flight",
            CrouchIdle => "crouch_idle",
            DuckForward => "duck_forward",
            DuckBackward => "duck_backward",
            DuckLeft => "duck_left",
            DuckRight => "duck_right",
            DiveForward => "dive_forward",
            DiveBackward => "dive_backward",
            DiveLeft => "dive_left",
            DiveRight => "dive_right",
            AirborneCenter => "airborne_center",
            AirborneTravel => "airborne_travel",
            ProneIdle => "prone_idle",
            SupineIdle => "supine_idle",
            ProneCrawlContact => "prone_crawl_contact",
            SupineScamperContact => "supine_scamper_contact",
            ProneTransition => "prone_transition",
            ProneSupineRollLeft => "prone_supine_roll_left",
            ProneSupineRollRight => "prone_supine_roll_right",
            SupineTransition => "supine_transition",
            Guard => "guard",
            AttackSwing => "swing",
            AttackSwingFollow => "swing_follow",
            AttackThrust => "thrust",
            BlockCutLeft => "block_cut_left",
            BlockCutRight => "block_cut_right",
            BlockThrust => "block_thrust",
        }
    }

    /// Authored whole-body counterpart that may satisfy this pose by
    /// reflection when the exact pose is absent from the same pack. Exact
    /// authored clips always win, so handed packs opt out simply by exporting
    /// both sides.
    pub fn mirrored_counterpart(self) -> Option<Self> {
        use SemanticPose::*;
        Some(match self {
            DuckLeft => DuckRight,
            DuckRight => DuckLeft,
            DiveLeft => DiveRight,
            DiveRight => DiveLeft,
            ProneSupineRollLeft => ProneSupineRollRight,
            ProneSupineRollRight => ProneSupineRollLeft,
            _ => return None,
        })
    }

    /// The next closest semantic pose. A miss restarts lookup at the selected
    /// animation pack, so specialized packs can supply a useful substitute.
    pub fn fallback(self) -> Option<Self> {
        use SemanticPose::*;
        Some(match self {
            IdleRelaxed => return None,
            WalkContact => IdleRelaxed,
            WalkPassing => WalkContact,
            RunContact => WalkContact,
            RunFlight => WalkPassing,
            CrouchIdle => IdleRelaxed,
            DuckForward | DuckBackward | DuckLeft | DuckRight => Guard,
            DiveForward | DiveBackward | DiveLeft | DiveRight => AirborneTravel,
            AirborneCenter => RunFlight,
            AirborneTravel => AirborneCenter,
            ProneIdle | SupineIdle => CrouchIdle,
            ProneCrawlContact => ProneIdle,
            SupineScamperContact => SupineIdle,
            ProneTransition => CrouchIdle,
            ProneSupineRollLeft | ProneSupineRollRight => ProneIdle,
            SupineTransition => CrouchIdle,
            Guard => IdleRelaxed,
            AttackSwing | AttackSwingFollow | AttackThrust => Guard,
            BlockCutLeft | BlockCutRight => BlockThrust,
            BlockThrust => Guard,
        })
    }
}

impl FromStr for SemanticPose {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|pose| pose.as_str() == value)
            .ok_or(())
    }
}
