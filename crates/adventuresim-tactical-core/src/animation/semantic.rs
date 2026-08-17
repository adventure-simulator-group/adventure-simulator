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
        assert_eq!(state.action(), ActionState::default());

        state.begin_attack(AttackSpec::default(), 10, 20);
        assert_eq!(state.action_kind(), SkeletonAction::Attack);
        state.begin_dodge(DodgeSpec::default(), 12, 13);
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
        state.begin_block(BlockSpec::default(), u64::MAX, u64::MAX);
        state.advance_action(u64::MAX);
        assert_eq!(state.action(), ActionState::default());
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
        state.begin_attack(AttackSpec::default(), 0, 10);
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
        state.begin_attack(AttackSpec::default(), 1, 2);
        state.transition_body(BodyState::Prone);
        assert_eq!(state.stance(), StanceState::Lowered);
        assert_eq!(state.action(), ActionState::default());
        set_weapon_guard(&mut state, WeaponGuardState::Raised);
        assert_eq!(state.stance(), StanceState::Lowered);
        state.begin_attack(AttackSpec::default(), 3, 4);
        assert_eq!(state.action(), ActionState::default());
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
        state.begin_attack(AttackSpec::default(), 10, 20);
        state.begin_dodge(DodgeSpec { direction: Vec2::X }, 11, 12);
        assert_eq!(state.action_kind(), SkeletonAction::Dodge);
        assert_eq!(state.action_direction(), Vec2::X);
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
        state.begin_attack(AttackSpec::default(), 10, 20);
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
        assert_eq!(normalized.action(), ActionState::default());
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
