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
    DuckLeadLeftBackward,
    DuckLeadLeftLeft,
    DuckLeadLeftRight,
    DuckLeadRightBackward,
    DuckLeadRightLeft,
    DuckLeadRightRight,
    AirborneCenter,
    AirborneTravel,
    ProneIdle,
    SupineIdle,
    ProneCrawlContact,
    ProneCrawlPassing,
    SupineScamperContact,
    SupineScamperPassing,
    UprightProneTransition,
    DiveImpact,
    GuardLeadLeft,
    GuardLeadRight,
    GuardWalkLeadLeft,
    GuardWalkLeadRight,
    GuardStrafeLeadLeftLeft,
    GuardStrafeLeadLeftRight,
    GuardStrafeLeadRightLeft,
    GuardStrafeLeadRightRight,
    AttackThrustLeadLeftContact,
    AttackThrustLeadRightContact,
    AttackSlashLeadLeftContact,
    AttackSlashLeadRightContact,
    BlockCutLeftLeadLeft,
    BlockCutLeftLeadRight,
    BlockCutRightLeadLeft,
    BlockCutRightLeadRight,
    BlockThrustLeadLeft,
    BlockThrustLeadRight,
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn humanoid_contract_resolves_from_twenty_eight_authored_poses() {
        assert_eq!(SemanticPose::HUMANOID_REQUIRED.len(), 34);
        let authored = SemanticPose::HUMANOID_REQUIRED
            .into_iter()
            .filter(|pose| {
                pose.mirrored_counterpart()
                    .is_none_or(|other| pose.as_str() < other.as_str())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(authored.len(), 28);
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
        assert_eq!(early.action[0].pose, SemanticPose::GuardLeadLeft);
        assert_eq!(
            early.action[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::AttackThrustLeadLeftContact,
                progress: 0.5,
            }
        );
        state.advance_action(15);
        let recovery = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(
            recovery.action[0].pose,
            SemanticPose::AttackThrustLeadLeftContact
        );
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
    pub const ALL: [Self; 40] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::DuckLeadLeftBackward,
        Self::DuckLeadLeftLeft,
        Self::DuckLeadLeftRight,
        Self::DuckLeadRightBackward,
        Self::DuckLeadRightLeft,
        Self::DuckLeadRightRight,
        Self::AirborneCenter,
        Self::AirborneTravel,
        Self::ProneIdle,
        Self::SupineIdle,
        Self::ProneCrawlContact,
        Self::ProneCrawlPassing,
        Self::SupineScamperContact,
        Self::SupineScamperPassing,
        Self::UprightProneTransition,
        Self::DiveImpact,
        Self::GuardLeadLeft,
        Self::GuardLeadRight,
        Self::GuardWalkLeadLeft,
        Self::GuardWalkLeadRight,
        Self::GuardStrafeLeadLeftLeft,
        Self::GuardStrafeLeadLeftRight,
        Self::GuardStrafeLeadRightLeft,
        Self::GuardStrafeLeadRightRight,
        Self::AttackThrustLeadLeftContact,
        Self::AttackThrustLeadRightContact,
        Self::AttackSlashLeadLeftContact,
        Self::AttackSlashLeadRightContact,
        Self::BlockCutLeftLeadLeft,
        Self::BlockCutLeftLeadRight,
        Self::BlockCutRightLeadLeft,
        Self::BlockCutRightLeadRight,
        Self::BlockThrustLeadLeft,
        Self::BlockThrustLeadRight,
    ];
    /// The 34 semantics every complete humanoid family must resolve. A root
    /// pack may author only 28 files because six pairs permit mirroring.
    pub const HUMANOID_REQUIRED: [Self; 34] = [
        Self::IdleRelaxed,
        Self::WalkContact,
        Self::WalkPassing,
        Self::RunContact,
        Self::RunFlight,
        Self::CrouchIdle,
        Self::DuckLeadLeftBackward,
        Self::DuckLeadLeftLeft,
        Self::DuckLeadLeftRight,
        Self::DuckLeadRightBackward,
        Self::DuckLeadRightLeft,
        Self::DuckLeadRightRight,
        Self::AirborneCenter,
        Self::AirborneTravel,
        Self::ProneIdle,
        Self::SupineIdle,
        Self::ProneCrawlContact,
        Self::ProneCrawlPassing,
        Self::SupineScamperContact,
        Self::SupineScamperPassing,
        Self::UprightProneTransition,
        Self::DiveImpact,
        Self::GuardLeadLeft,
        Self::GuardLeadRight,
        Self::AttackThrustLeadLeftContact,
        Self::AttackThrustLeadRightContact,
        Self::AttackSlashLeadLeftContact,
        Self::AttackSlashLeadRightContact,
        Self::BlockCutLeftLeadLeft,
        Self::BlockCutLeftLeadRight,
        Self::BlockCutRightLeadLeft,
        Self::BlockCutRightLeadRight,
        Self::BlockThrustLeadLeft,
        Self::BlockThrustLeadRight,
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
            DuckLeadLeftBackward => "duck_lead_left_backward",
            DuckLeadLeftLeft => "duck_lead_left_left",
            DuckLeadLeftRight => "duck_lead_left_right",
            DuckLeadRightBackward => "duck_lead_right_backward",
            DuckLeadRightLeft => "duck_lead_right_left",
            DuckLeadRightRight => "duck_lead_right_right",
            AirborneCenter => "airborne_center",
            AirborneTravel => "airborne_travel",
            ProneIdle => "prone_idle",
            SupineIdle => "supine_idle",
            ProneCrawlContact => "prone_crawl_contact",
            ProneCrawlPassing => "prone_crawl_passing",
            SupineScamperContact => "supine_scamper_contact",
            SupineScamperPassing => "supine_scamper_passing",
            UprightProneTransition => "upright_prone_transition",
            DiveImpact => "dive_impact",
            GuardLeadLeft => "guard_lead_left",
            GuardLeadRight => "guard_lead_right",
            GuardWalkLeadLeft => "guard_walk_lead_left",
            GuardWalkLeadRight => "guard_walk_lead_right",
            GuardStrafeLeadLeftLeft => "guard_strafe_lead_left_left",
            GuardStrafeLeadLeftRight => "guard_strafe_lead_left_right",
            GuardStrafeLeadRightLeft => "guard_strafe_lead_right_left",
            GuardStrafeLeadRightRight => "guard_strafe_lead_right_right",
            AttackThrustLeadLeftContact => "attack_thrust_lead_left_contact",
            AttackThrustLeadRightContact => "attack_thrust_lead_right_contact",
            AttackSlashLeadLeftContact => "attack_slash_lead_left_contact",
            AttackSlashLeadRightContact => "attack_slash_lead_right_contact",
            BlockCutLeftLeadLeft => "block_cut_left_lead_left",
            BlockCutLeftLeadRight => "block_cut_left_lead_right",
            BlockCutRightLeadLeft => "block_cut_right_lead_left",
            BlockCutRightLeadRight => "block_cut_right_lead_right",
            BlockThrustLeadLeft => "block_thrust_lead_left",
            BlockThrustLeadRight => "block_thrust_lead_right",
        }
    }

    /// Authored whole-body counterpart that may satisfy this pose by
    /// reflection when the exact pose is absent from the same pack. Exact
    /// authored clips always win, so handed packs opt out simply by exporting
    /// both sides.
    pub fn mirrored_counterpart(self) -> Option<Self> {
        use SemanticPose::*;
        Some(match self {
            DuckLeadLeftBackward => DuckLeadRightBackward,
            DuckLeadLeftLeft => DuckLeadRightRight,
            DuckLeadLeftRight => DuckLeadRightLeft,
            DuckLeadRightBackward => DuckLeadLeftBackward,
            DuckLeadRightLeft => DuckLeadLeftRight,
            DuckLeadRightRight => DuckLeadLeftLeft,
            GuardLeadLeft => GuardLeadRight,
            GuardLeadRight => GuardLeadLeft,
            GuardWalkLeadLeft => GuardWalkLeadRight,
            GuardWalkLeadRight => GuardWalkLeadLeft,
            GuardStrafeLeadLeftLeft => GuardStrafeLeadRightRight,
            GuardStrafeLeadLeftRight => GuardStrafeLeadRightLeft,
            GuardStrafeLeadRightLeft => GuardStrafeLeadLeftRight,
            GuardStrafeLeadRightRight => GuardStrafeLeadLeftLeft,
            AttackThrustLeadLeftContact => AttackThrustLeadRightContact,
            AttackThrustLeadRightContact => AttackThrustLeadLeftContact,
            AttackSlashLeadLeftContact => AttackSlashLeadRightContact,
            AttackSlashLeadRightContact => AttackSlashLeadLeftContact,
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
            DuckLeadLeftBackward | DuckLeadLeftLeft | DuckLeadLeftRight => GuardLeadLeft,
            DuckLeadRightBackward | DuckLeadRightLeft | DuckLeadRightRight => GuardLeadRight,
            AirborneCenter => RunFlight,
            AirborneTravel => AirborneCenter,
            ProneIdle | SupineIdle => CrouchIdle,
            ProneCrawlContact | ProneCrawlPassing => ProneIdle,
            SupineScamperContact | SupineScamperPassing => SupineIdle,
            UprightProneTransition => CrouchIdle,
            DiveImpact => AirborneTravel,
            GuardLeadLeft => IdleRelaxed,
            GuardLeadRight => IdleRelaxed,
            GuardWalkLeadLeft => GuardLeadLeft,
            GuardWalkLeadRight => GuardLeadRight,
            GuardStrafeLeadLeftLeft | GuardStrafeLeadLeftRight => GuardWalkLeadLeft,
            GuardStrafeLeadRightLeft | GuardStrafeLeadRightRight => GuardWalkLeadRight,
            AttackThrustLeadLeftContact => AttackSlashLeadLeftContact,
            AttackThrustLeadRightContact => AttackSlashLeadRightContact,
            AttackSlashLeadLeftContact => GuardLeadLeft,
            AttackSlashLeadRightContact => GuardLeadRight,
            BlockCutLeftLeadLeft => BlockThrustLeadLeft,
            BlockCutLeftLeadRight => BlockThrustLeadRight,
            BlockCutRightLeadLeft => BlockCutLeftLeadLeft,
            BlockCutRightLeadRight => BlockCutLeftLeadRight,
            BlockThrustLeadLeft => GuardLeadLeft,
            BlockThrustLeadRight => GuardLeadRight,
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
