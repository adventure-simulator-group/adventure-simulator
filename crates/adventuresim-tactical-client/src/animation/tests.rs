use super::*;

#[cfg(test)]
mod legacy_tests {
    use std::path::Path;

    use super::*;
    use bevy_animation_graph::core::animation_graph::{DEFAULT_OUTPUT_POSE, TargetPin};
    use semantic_graph::{SemanticGraphPath, SemanticGraphTrace};

    fn graph_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy_animation_graph::AnimationGraphPlugin::default())
            .init_resource::<semantic_graph::SemanticGraphLibrary>();
        app
    }

    fn route(app: &mut App, skeleton: SkeletonState) -> SemanticGraphTrace {
        app.world_mut()
            .run_system_cached_with(
                semantic_graph::route_semantic_graph_for_test,
                (PresentedSkeleton::new(skeleton, None), Entity::PLACEHOLDER),
            )
            .unwrap()
    }

    #[test]
    fn ordinary_semantic_graph_runtime_drives_legacy_equivalent_samples() {
        let mut app = graph_test_app();
        let skeleton = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 2.0)
            .with_world_velocity(Vec3::NEG_Z * 2.0);
        let before = AnimationEvaluation::from_skeleton(&skeleton);
        let after = route(&mut app, skeleton);
        assert_eq!(after.path, SemanticGraphPath::OrdinaryLocomotion);
        assert!(after.runtime_evaluated);
        assert_eq!(before, after.evaluation);
    }

    #[test]
    fn semantic_graph_inputs_are_read_only_and_cover_attack_capture() {
        let mut skeleton = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_local_velocity(Vec3::NEG_Z * 3.0)
            .with_world_velocity(Vec3::NEG_Z * 3.0);
        skeleton.begin_attack(
            AttackSpec {
                step: AttackStep::Forward,
                step_speed: 3.0,
                movement_direction: Vec2::Y,
                movement_speed: 3.0,
                ..default()
            },
            10,
            20,
        );
        skeleton.advance_action(15);
        let before = serde_json::to_vec(&skeleton).unwrap();
        let presented = PresentedSkeleton::new(skeleton, None);
        let evaluation = AnimationEvaluation::from_skeleton(&presented);
        let inputs = semantic_graph::SemanticGraphInputs::from_presented(&presented, &evaluation);

        assert_eq!(inputs.action, SkeletonAction::Attack);
        assert_eq!(inputs.captured_step, AttackStep::Forward);
        assert_eq!(inputs.captured_step_direction, Vec2::Y);
        assert_eq!(inputs.captured_step_speed, 3.0);
        assert_eq!(before, serde_json::to_vec(&presented.state).unwrap());
    }

    #[test]
    fn semantic_graph_routes_raised_attack_without_retiming_contact() {
        let mut app = graph_test_app();
        for (tick, expected_phase) in [(10, 0.0), (15, 0.25), (20, 0.5), (25, 0.75), (30, 1.0)] {
            let mut skeleton = SkeletonState::default()
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_lead_foot(LeadFoot::Left);
            skeleton.begin_attack(AttackSpec::default(), 10, 20);
            skeleton.advance_action(tick);
            let presented = PresentedSkeleton::new(skeleton.clone(), None);
            let legacy = AnimationEvaluation::from_skeleton(&skeleton);
            let routed = route(&mut app, skeleton);

            assert_eq!(routed.path, SemanticGraphPath::RaisedGuardAttack);
            assert!(routed.runtime_evaluated);
            assert_eq!(routed.evaluation, legacy);
            assert!((routed.inputs.gait_phase - presented.gait_phase).abs() < f32::EPSILON);
            assert!((routed.evaluation.action_phase - expected_phase).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn missing_dependency_graph_output_falls_back_to_legacy() {
        let mut app = graph_test_app();
        let handle = app
            .world()
            .resource::<semantic_graph::SemanticGraphLibrary>()
            .ordinary
            .clone();
        app.world_mut()
            .resource_mut::<Assets<bevy_animation_graph::core::animation_graph::AnimationGraph>>()
            .get_mut(&handle)
            .unwrap()
            .remove_edge_by_target(&TargetPin::OutputData(DEFAULT_OUTPUT_POSE.into()));

        let skeleton = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z)
            .with_world_velocity(Vec3::NEG_Z);
        let legacy = AnimationEvaluation::from_skeleton(&skeleton);
        let routed = route(&mut app, skeleton);

        assert_eq!(routed.path, SemanticGraphPath::LegacyFallback);
        assert!(!routed.runtime_evaluated);
        assert_eq!(routed.evaluation, legacy);
    }

    #[test]
    fn dropped_dependency_graph_asset_falls_back_to_legacy() {
        let mut app = graph_test_app();
        let handle = app
            .world()
            .resource::<semantic_graph::SemanticGraphLibrary>()
            .raised
            .clone();
        app.world_mut()
            .resource_mut::<Assets<bevy_animation_graph::core::animation_graph::AnimationGraph>>()
            .remove(&handle);

        let skeleton = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        let legacy = AnimationEvaluation::from_skeleton(&skeleton);
        let routed = route(&mut app, skeleton);

        assert_eq!(routed.path, SemanticGraphPath::LegacyFallback);
        assert!(!routed.runtime_evaluated);
        assert_eq!(routed.evaluation, legacy);
    }

    #[test]
    fn malformed_late_graph_marker_discards_all_partial_decode_changes() {
        let mut app = graph_test_app();
        app.world_mut()
            .resource_mut::<semantic_graph::SemanticGraphLibrary>()
            .corrupt_last_marker = true;
        let skeleton = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 2.0)
            .with_world_velocity(Vec3::NEG_Z * 2.0);
        let legacy = AnimationEvaluation::from_skeleton(&skeleton);
        let routed = route(&mut app, skeleton);

        assert_eq!(routed.requested_path, SemanticGraphPath::OrdinaryLocomotion);
        assert_eq!(routed.path, SemanticGraphPath::LegacyFallback);
        assert!(!routed.runtime_evaluated);
        assert_eq!(routed.evaluation, legacy);
    }

    #[test]
    fn non_identity_graph_blend_changes_weighted_fk_playback_input() {
        let mut skeleton = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 3.75)
            .with_world_velocity(Vec3::NEG_Z * 3.75);
        skeleton.gait_phase = 0.25;
        assert!(AnimationEvaluation::from_skeleton(&skeleton).base.len() > 1);

        let mut production_app = graph_test_app();
        let production = route(&mut production_app, skeleton.clone());
        let mut changed_app = graph_test_app();
        let active_anchors = AnimationEvaluation::from_skeleton(&skeleton)
            .base
            .iter()
            .map(|sample| match sample.sampling {
                PoseSampling::Anchor | PoseSampling::Cycle { .. } => 1,
                PoseSampling::Span { .. } => 2,
            })
            .sum::<usize>();
        let mut factors = [0.0; semantic_graph::MAX_GRAPH_ANCHORS - 1];
        factors[..active_anchors.saturating_sub(1)].fill(0.25);
        changed_app
            .world_mut()
            .resource_mut::<semantic_graph::SemanticGraphLibrary>()
            .factor_override = Some(factors);
        let changed = route(&mut changed_app, skeleton);

        assert!(production.runtime_evaluated);
        assert!(changed.runtime_evaluated);
        assert_ne!(production.evaluation, changed.evaluation);

        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::WalkContact,
            SemanticPose::WalkPassing,
            SemanticPose::RunContact,
            SemanticPose::RunFlight,
        ]);
        let resolve_playback = |evaluation: &AnimationEvaluation| {
            let samples = if evaluation.action.is_empty() {
                &evaluation.base
            } else {
                &evaluation.action
            };
            let mut clips = Vec::new();
            for sample in samples {
                append_resolved_sample(
                    &mut clips,
                    &runtime,
                    &catalog,
                    HUMANOID_UNARMED_PACK,
                    *sample,
                    None,
                );
            }
            AnimationPlayback { clips, ..default() }
        };
        let production_playback = resolve_playback(&production.evaluation);
        let changed_playback = resolve_playback(&changed.evaluation);
        let production_weights = production_playback
            .clips
            .iter()
            .map(|clip| (clip.clip.node, clip.weight))
            .collect::<Vec<_>>();
        let changed_weights = changed_playback
            .clips
            .iter()
            .map(|clip| (clip.clip.node, clip.weight))
            .collect::<Vec<_>>();
        assert_ne!(production_weights, changed_weights);
    }

    #[test]
    fn non_identity_attack_graph_blend_changes_span_fk_playback_input() {
        let mut skeleton = SkeletonState::default().with_lead_foot(LeadFoot::Left);
        skeleton.begin_attack(AttackSpec::default(), 10, 20);
        skeleton.advance_action(15);
        let legacy = AnimationEvaluation::from_skeleton(&skeleton);
        assert!(matches!(
            legacy.action[0].sampling,
            PoseSampling::Span { progress, .. } if progress > 0.0 && progress < 1.0
        ));

        let mut production_app = graph_test_app();
        let production = route(&mut production_app, skeleton.clone());
        let mut changed_app = graph_test_app();
        changed_app
            .world_mut()
            .resource_mut::<semantic_graph::SemanticGraphLibrary>()
            .factor_override = Some([0.0; semantic_graph::MAX_GRAPH_ANCHORS - 1]);
        let changed = route(&mut changed_app, skeleton);

        assert!(production.runtime_evaluated);
        assert!(changed.runtime_evaluated);
        assert_ne!(production.evaluation.action, changed.evaluation.action);

        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::AttackThrustLeadLeftContact,
        ]);
        let resolve = |sample: PoseSample| {
            let mut weighted = Vec::new();
            append_resolved_sample(
                &mut weighted,
                &runtime,
                &catalog,
                HUMANOID_UNARMED_PACK,
                sample,
                None,
            );
            weighted
                .into_iter()
                .map(|clip| (clip.clip.node, clip.weight))
                .collect::<Vec<_>>()
        };
        assert_ne!(
            resolve(production.evaluation.action[0]),
            resolve(changed.evaluation.action[0])
        );
    }

    #[test]
    fn editor_preflight_resolves_deterministic_routes_and_mirror_fallback() {
        let asset_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("assets");
        let report = catalog::validate_editor_asset_root(&asset_root).unwrap();
        assert!(report.motion_count > 0);
        assert!(report.missing_motion_count > 0);
        assert_eq!(report.missing_motion_count, report.warnings.len());
        assert!(
            report
                .route_resolutions
                .iter()
                .any(|resolution| resolution.route == "ordinary_locomotion")
        );
        assert!(report.route_resolutions.iter().any(|resolution| {
            resolution.route == "raised_guard_attack" && resolution.mirrored
        }));

        let mut app = graph_test_app();
        let graph_routes = app
            .world_mut()
            .run_system_cached(semantic_graph::editor_graph_preflight)
            .unwrap()
            .unwrap();
        assert_eq!(graph_routes.len(), 2);
        assert!(graph_routes.iter().all(|route| {
            route.requested_path == route.selected_path && route.sample_count > 0
        }));
        assert!(graph_routes.iter().any(|route| {
            route.label.contains("right-lead")
                && route.selected_path == SemanticGraphPath::RaisedGuardAttack
        }));
    }

    #[test]
    fn runtime_catalog_registers_both_downed_gait_mirror_endpoints() {
        let catalog = catalog::AnimationPackCatalog::default();
        let pack = catalog.packs.get(HUMANOID_UNARMED_PACK).unwrap();
        assert!(pack.motions.contains_key("prone_crawl_mirrored"));
        assert!(pack.motions.contains_key("supine_scamper_mirrored"));
    }

    #[test]
    fn terrain_ik_defaults_on() {
        assert!(TerrainIkEnabled::default().0);
    }

    #[test]
    fn presentation_phase_advances_between_sparse_authoritative_samples() {
        let mut authoritative = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 5.5)
            .with_world_velocity(Vec3::NEG_Z * 5.5);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);
        let mut replicated = authoritative.clone();
        let mut previous_phase = presented.gait_phase;
        let mut largest_step = 0.0_f32;
        let mut advanced_without_sample = false;

        for tick in 1..=64 {
            project_skeleton_locomotion(
                &mut authoritative,
                SkeletonLocomotionInput {
                    orientation: Quat::IDENTITY,
                    linear_velocity: Vec3::NEG_Z * 5.5,
                    grounded: true,
                    crouching: false,
                    delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                    tick,
                },
            );
            if tick % 4 == 0 {
                replicated = authoritative.clone();
            }
            advance_presented_skeleton(&mut presented, &replicated, 1.0 / LOCOMOTION_SAMPLE_HZ);
            let step = circular_phase_delta(previous_phase, presented.gait_phase).abs();
            largest_step = largest_step.max(step);
            if tick % 4 != 0 && step > 0.001 {
                advanced_without_sample = true;
            }
            previous_phase = presented.gait_phase;
        }

        assert!(advanced_without_sample);
        assert!(largest_step < 0.06, "largest phase step was {largest_step}");
        assert!(circular_phase_delta(presented.gait_phase, authoritative.gait_phase).abs() < 0.08);
    }

    #[test]
    fn presentation_phase_correction_is_rate_limited_across_packet_jitter() {
        let velocity = Vec3::NEG_Z * 5.5;
        let authoritative = SkeletonState::default()
            .with_local_velocity(velocity)
            .with_world_velocity(velocity)
            .with_gait_phase(0.12)
            .with_locomotion_sample_tick(1);
        let mut presented = PresentedSkeleton::new(
            SkeletonState::default()
                .with_local_velocity(velocity)
                .with_world_velocity(velocity),
            None,
        );
        let render_deltas = [1.0 / 60.0, 1.0 / 90.0, 1.0 / 45.0, 1.0 / 72.0];
        let mut previous_phase = presented.gait_phase;

        for delta_seconds in render_deltas.into_iter().cycle().take(16) {
            advance_presented_skeleton(&mut presented, &authoritative, delta_seconds);
            let actual = circular_phase_delta(previous_phase, presented.gait_phase);
            let predicted = gait_cycle_phase_delta(
                locomotion_profile(&presented.state),
                presented.animation_speed(),
                delta_seconds,
            );
            let maximum = predicted
                + PRESENTATION_PHASE_CORRECTION_RATE_PER_SECOND * delta_seconds
                + 0.000_01;
            assert!(actual >= 0.0, "phase moved backwards by {actual}");
            assert!(actual <= maximum, "phase step {actual} exceeded {maximum}");
            previous_phase = presented.gait_phase;
        }
    }

    #[test]
    fn presentation_phase_ignores_minor_authoritative_packet_jitter() {
        let velocity = Vec3::NEG_Z * 5.5;
        let mut authoritative = SkeletonState::default()
            .with_local_velocity(velocity)
            .with_world_velocity(velocity);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);
        let delta_seconds = 1.0 / 60.0;

        for tick in 1..=32 {
            let previous_phase = presented.gait_phase;
            let predicted_delta = gait_cycle_phase_delta(
                locomotion_profile(&presented.state),
                presented.animation_speed(),
                delta_seconds,
            );
            let jitter = if tick % 2 == 0 { 0.03 } else { -0.03 };
            authoritative.gait_phase = (previous_phase + predicted_delta + jitter).rem_euclid(1.0);
            authoritative.locomotion_sample_tick = tick;

            advance_presented_skeleton(&mut presented, &authoritative, delta_seconds);

            let actual_delta = circular_phase_delta(previous_phase, presented.gait_phase);
            assert!((actual_delta - predicted_delta).abs() < 0.000_01);
            assert_eq!(presented.last_phase_correction_delta, 0.0);
            assert_eq!(presented.phase_error_remaining, 0.0);
            assert!(presented.last_phase_source_changed);
            assert!(presented.last_phase_measurement_error.is_some());
        }
    }

    #[test]
    fn presentation_phase_filters_persistent_drift_before_bounded_correction() {
        let velocity = Vec3::NEG_Z * 5.5;
        let mut authoritative = SkeletonState::default()
            .with_local_velocity(velocity)
            .with_world_velocity(velocity);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);
        let delta_seconds = 1.0 / 60.0;
        let maximum_correction = PRESENTATION_PHASE_CORRECTION_RATE_PER_SECOND * delta_seconds;

        for tick in 1..=8 {
            let predicted_delta = gait_cycle_phase_delta(
                locomotion_profile(&presented.state),
                presented.animation_speed(),
                delta_seconds,
            );
            authoritative.gait_phase =
                (presented.gait_phase + predicted_delta + 0.10).rem_euclid(1.0);
            authoritative.locomotion_sample_tick = tick;

            advance_presented_skeleton(&mut presented, &authoritative, delta_seconds);

            assert!(presented.last_phase_correction_delta > 0.0);
            assert!(presented.last_phase_correction_delta <= maximum_correction + 0.000_01);
            let measured = presented
                .last_phase_measurement_error
                .expect("new authoritative sample should be measured");
            assert!((measured - 0.10).abs() < 0.000_01);
            assert!(presented.phase_error_remaining < 0.10 - PRESENTATION_PHASE_DRIFT_DEADBAND);
        }
    }

    #[test]
    fn presentation_velocity_smooths_a_sparse_turn_without_changing_authority() {
        let authoritative = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 5.5)
            .with_world_velocity(Vec3::NEG_Z * 5.5);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);
        let turned = authoritative
            .clone()
            .with_local_velocity(Vec3::X * 5.5)
            .with_world_velocity(Vec3::X * 5.5)
            .with_locomotion_sample_tick(4)
            .with_gait_phase(0.1);

        advance_presented_skeleton(&mut presented, &turned, 1.0 / LOCOMOTION_SAMPLE_HZ);

        assert_eq!(turned.local_velocity, Vec3::X * 5.5);
        assert!(presented.local_velocity.x > 0.0);
        assert!(presented.local_velocity.x < 5.5);
        assert!(presented.local_velocity.z < 0.0);
    }

    fn spawn_test_t_pose(
        In(owner): In<Entity>,
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        commands.entity(owner).with_children(|parent| {
            spawn_fallback_t_pose(parent, owner, Color::WHITE, &mut meshes, &mut materials);
        });
    }

    #[test]
    fn default_catalog_owns_all_required_poses_once() {
        let catalog = AnimationPackCatalog::default();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        let mut library = AnimationPackLibrary::default();
        library
            .insert(AnimationPack {
                id: HUMANOID_UNARMED_PACK.to_owned(),
                skeleton_family: root.skeleton_family.clone(),
                fallback: root.fallback.clone(),
                clips: root.poses.keys().copied().collect(),
            })
            .unwrap();
        for required in SemanticPose::HUMANOID_REQUIRED {
            assert!(
                matches!(
                    library.resolve(HUMANOID_UNARMED_PACK, required),
                    ResolvedPose::Clip { semantic, .. } if semantic == required
                ),
                "required pose {required:?} did not resolve"
            );
        }
        // The 40 required semantics collapse to 31 authored variants when
        // each supported whole-body mirror pair is represented once.
        let authored_variants = SemanticPose::HUMANOID_REQUIRED
            .into_iter()
            .filter(|pose| {
                pose.mirrored_counterpart()
                    .is_none_or(|counterpart| pose.as_str() < counterpart.as_str())
            })
            .count();
        assert_eq!(authored_variants, 31);
        assert_eq!(
            root.motions["walk"].path,
            "animations/biped/unarmed/walk.glb"
        );
        assert_eq!(
            root.poses[&SemanticPose::WalkPassing],
            PoseAnchor {
                motion: "walk".to_owned(),
                frame: 16,
            }
        );
        assert_eq!(
            root.poses[&SemanticPose::AttackThrustLeadLeftContact],
            PoseAnchor {
                motion: "attack_thrust_lead_left_contact".to_owned(),
                frame: 0,
            }
        );
        assert_eq!(
            root.poses[&SemanticPose::DuckLeadLeftBackward],
            PoseAnchor {
                motion: "duck_lead_left_backward".to_owned(),
                frame: 0,
            }
        );
        assert_eq!(
            root.poses[&SemanticPose::DuckLeadLeftForward],
            PoseAnchor {
                motion: "duck_lead_left_forward".to_owned(),
                frame: 0,
            }
        );
        assert_eq!(root.motions["duck_lead_left_forward"].last_frame, 0);
        assert_eq!(
            root.poses[&SemanticPose::DiveForward],
            PoseAnchor {
                motion: "dive_forward".to_owned(),
                frame: 0,
            }
        );
        assert_eq!(
            SemanticPose::DiveRight.mirrored_counterpart(),
            Some(SemanticPose::DiveLeft)
        );
        for pose in [
            SemanticPose::GuardWalkLeadLeft,
            SemanticPose::GuardWalkLeadRight,
            SemanticPose::GuardStrafeLeadLeftLeft,
            SemanticPose::GuardStrafeLeadLeftRight,
            SemanticPose::GuardStrafeLeadRightLeft,
            SemanticPose::GuardStrafeLeadRightRight,
        ] {
            let anchor = &root.poses[&pose];
            assert_eq!(anchor.frame, 0);
            assert_eq!(root.motions[&anchor.motion].last_frame, 0);
        }
    }

    #[test]
    fn duplicate_authoritative_pose_is_rejected() {
        let mut builder = PackBuilder::new("test", "humanoid", None, "animations/test");
        builder.motion("one", 0);
        builder.motion("two", 0);
        builder.pose("one", 0, SemanticPose::IdleRelaxed).unwrap();
        assert_eq!(
            builder.pose("two", 0, SemanticPose::IdleRelaxed),
            Err(CatalogError::DuplicatePose(SemanticPose::IdleRelaxed))
        );
    }

    #[test]
    fn pack_builder_supports_specialized_pack_paths_and_fallbacks() {
        let mut builder = PackBuilder::new(
            "armored",
            "humanoid",
            Some(HUMANOID_UNARMED_PACK),
            "animations/armored",
        );
        builder.motion("idle", 0);
        builder.pose("idle", 0, SemanticPose::IdleRelaxed).unwrap();
        let (id, pack) = builder.finish();
        assert_eq!(id, "armored");
        assert_eq!(pack.fallback.as_deref(), Some(HUMANOID_UNARMED_PACK));
        assert_eq!(pack.motions["idle"].path, "animations/armored/idle.glb");
    }

    #[test]
    fn frame_eight_is_sampled_at_thirty_fps() {
        assert!((frame_seconds(8) - 8.0 / 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sole_clip_accepts_named_or_unnamed_animation_equally() {
        let one = [Handle::<AnimationClip>::default()];
        let two = [
            Handle::<AnimationClip>::default(),
            Handle::<AnimationClip>::default(),
        ];
        assert!(sole_animation(&[]).is_none());
        assert!(sole_animation(&one).is_some());
        assert!(sole_animation(&two).is_none());
    }

    fn runtime_with_available(poses: impl IntoIterator<Item = SemanticPose>) -> AnimationRuntime {
        let catalog = AnimationPackCatalog::default();
        let poses = poses.into_iter().collect::<BTreeSet<_>>();
        let mut library = AnimationPackLibrary::default();
        library
            .insert(AnimationPack {
                id: HUMANOID_UNARMED_PACK.to_owned(),
                skeleton_family: "humanoid".to_owned(),
                fallback: None,
                clips: poses.clone(),
            })
            .unwrap();
        let mut runtime = AnimationRuntime {
            library,
            ..default()
        };
        for pose in poses {
            let anchor = &catalog.packs[HUMANOID_UNARMED_PACK].poses[&pose];
            let key = (HUMANOID_UNARMED_PACK.to_owned(), anchor.motion.clone());
            if runtime.clips.contains_key(&key) {
                continue;
            }
            let node_base = runtime.clips.len() * 256;
            let pack = &catalog.packs[HUMANOID_UNARMED_PACK];
            let anchor_nodes: BTreeMap<u16, AnimationNodeIndex> = pack
                .poses
                .values()
                .filter(|candidate| candidate.motion == anchor.motion)
                .map(|candidate| candidate.frame)
                .chain(
                    pack.references
                        .get(&anchor.motion)
                        .into_iter()
                        .flatten()
                        .map(|reference| reference.frame),
                )
                .collect::<BTreeSet<_>>()
                .into_iter()
                .enumerate()
                .map(|(index, frame)| (frame, AnimationNodeIndex::new(node_base + index + 1)))
                .collect();
            runtime.clips.insert(
                key,
                LoadedClip {
                    node: AnimationNodeIndex::new(node_base),
                    duration_seconds: 64.0 / ANIMATION_FPS,
                    anchor_nodes: anchor_nodes.clone(),
                    upper_node: AnimationNodeIndex::new(node_base),
                    upper_anchor_nodes: anchor_nodes.clone(),
                    lower_node: AnimationNodeIndex::new(node_base),
                    lower_anchor_nodes: anchor_nodes,
                },
            );
        }
        runtime
    }

    #[test]
    fn same_motion_span_blends_only_the_two_exact_anchor_frames() {
        let catalog = AnimationPackCatalog::default();
        let runtime =
            runtime_with_available([SemanticPose::WalkContact, SemanticPose::WalkPassing]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::WalkContact,
                sampling: PoseSampling::Span {
                    end: SemanticPose::WalkPassing,
                    progress: 0.5,
                },
                weight: 1.0,
                mirror_lower_body: false,
            },
            None,
        );
        assert_eq!(weighted.len(), 2);
        assert!(
            weighted.iter().any(|sample| {
                sample.time_seconds == 0.0 && (sample.weight - 0.5).abs() < 0.0001
            })
        );
        assert!(weighted.iter().any(|sample| {
            (sample.time_seconds - 16.0 / ANIMATION_FPS).abs() < 0.0001
                && (sample.weight - 0.5).abs() < 0.0001
        }));
    }

    #[test]
    fn attack_entry_blends_guard_and_contact_motions() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::AttackThrustLeadLeftContact,
        ]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::GuardLeadLeft,
                sampling: PoseSampling::Span {
                    end: SemanticPose::AttackThrustLeadLeftContact,
                    progress: 0.5,
                },
                weight: 1.0,
                mirror_lower_body: false,
            },
            None,
        );
        assert_eq!(weighted.len(), 2);
        assert!(
            weighted
                .iter()
                .all(|sample| sample.time_seconds == 0.0 && (sample.weight - 0.5).abs() < 0.0001)
        );
        let guard = runtime.clips[&(
            HUMANOID_UNARMED_PACK.to_owned(),
            "guard_lead_left".to_owned(),
        )]
            .at_anchor(0)
            .node;
        let contact = runtime.clips[&(
            HUMANOID_UNARMED_PACK.to_owned(),
            "attack_thrust_lead_left_contact".to_owned(),
        )]
            .at_anchor(0)
            .node;
        assert!(weighted.iter().any(|sample| sample.clip.node == guard));
        assert!(weighted.iter().any(|sample| sample.clip.node == contact));
    }

    #[test]
    fn mirrored_gait_endpoint_uses_a_distinct_pre_fk_clip_node() {
        let catalog = AnimationPackCatalog::default();
        let mut runtime =
            runtime_with_available([SemanticPose::RunContact, SemanticPose::RunFlight]);
        let mirrored_node = AnimationNodeIndex::new(9_001);
        runtime.clips.insert(
            (HUMANOID_UNARMED_PACK.to_owned(), "run_mirrored".to_owned()),
            LoadedClip {
                node: AnimationNodeIndex::new(9_000),
                duration_seconds: 64.0 / ANIMATION_FPS,
                anchor_nodes: BTreeMap::from([(0, mirrored_node)]),
                upper_node: AnimationNodeIndex::new(9_000),
                upper_anchor_nodes: BTreeMap::from([(0, mirrored_node)]),
                lower_node: AnimationNodeIndex::new(9_000),
                lower_anchor_nodes: BTreeMap::from([(0, mirrored_node)]),
            },
        );
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::RunContact,
                sampling: PoseSampling::Anchor,
                weight: 0.5,
                mirror_lower_body: true,
            },
            None,
        );

        assert_eq!(weighted.len(), 1);
        assert_eq!(weighted[0].clip.node, mirrored_node);
        assert!((weighted[0].weight - 0.5).abs() < 0.0001);
    }

    #[test]
    fn zero_clip_base_gets_one_canonical_player_and_stable_targets() {
        let mut world = World::new();
        world.init_resource::<AnimationRuntime>();
        let owner = world.spawn_empty().id();
        let rig = world.spawn(AnimationRigScene(owner)).id();
        let skeleton = world.spawn(Name::new("Skeleton")).id();
        let root = world.spawn(Name::new("root")).id();
        let pelvis = world.spawn(Name::new("pelvis")).id();
        world.entity_mut(rig).add_child(skeleton);
        world.entity_mut(skeleton).add_child(root);
        world.entity_mut(root).add_child(pelvis);

        world
            .run_system_cached(establish_animation_targets)
            .unwrap();
        world.flush();
        world
            .run_system_cached(establish_animation_targets)
            .unwrap();
        world.flush();

        assert_eq!(world.query::<&AnimationPlayer>().iter(&world).count(), 1);
        assert_eq!(
            world.get::<AnimatedBy>(pelvis).map(|link| link.0),
            Some(skeleton)
        );
        assert_eq!(
            world.get::<AnimationTargetId>(pelvis),
            Some(&AnimationTargetId::from_names(
                [
                    Name::new("Skeleton"),
                    Name::new("root"),
                    Name::new("pelvis")
                ]
                .iter()
            ))
        );
        assert_eq!(
            world.resource::<AnimationRuntime>().canonical_targets.len(),
            3
        );
    }

    #[test]
    fn composite_mask_keeps_root_pelvis_and_legs_out_of_the_upper_body() {
        for lower in [
            "Skeleton",
            "root",
            "pelvis",
            "thigh.L",
            "thigh_twist.R",
            "shin.L",
            "foot.R",
            "toe.L",
        ] {
            assert!(is_lower_body_animation_target(lower), "{lower}");
        }
        for upper in [
            "stomach_01",
            "stomach_02",
            "chest",
            "clavicle.L",
            "upper_arm.R",
            "head",
        ] {
            assert!(!is_lower_body_animation_target(upper), "{upper}");
        }
    }

    #[test]
    fn authored_rig_attaches_to_a_player_with_skeleton_state() {
        let mut world = World::new();
        let runtime = AnimationRuntime {
            base_scene: Some(Handle::default()),
            ..default()
        };
        world.insert_resource(runtime);
        let owner = world
            .spawn((Player::default(), SkeletonState::default()))
            .id();

        world.run_system_cached(attach_loaded_rig_scenes).unwrap();
        world.flush();

        assert!(world.get::<AnimationRigAttached>(owner).is_some());
        let rig = world
            .query::<(Entity, &AnimationRigScene)>()
            .iter(&world)
            .find_map(|(entity, scene)| (scene.0 == owner).then_some(entity))
            .expect("client authored rig");
        assert_eq!(
            world.get::<Transform>(rig).unwrap().translation.y,
            PLAYER_VISUAL_Y_OFFSET
        );
    }

    #[test]
    fn incompatible_motion_target_set_is_rejected_independently() {
        let root = AnimationTargetId::from_names([Name::new("Skeleton")].iter());
        let pelvis = AnimationTargetId::from_names(
            [
                Name::new("Skeleton"),
                Name::new("root"),
                Name::new("pelvis"),
            ]
            .iter(),
        );
        let foreign = AnimationTargetId::from_names([Name::new("OtherRig")].iter());
        let base = HashSet::from([root, pelvis]);
        assert!(targets_match_base([&root, &pelvis].into_iter(), &base));
        assert!(!targets_match_base([&root, &foreign].into_iter(), &base));
        assert!(!targets_match_base([].iter(), &base));
    }

    #[test]
    fn unavailable_motion_uses_similar_pose_fallback() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([SemanticPose::WalkContact]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::RunContact,
                sampling: PoseSampling::Anchor,
                weight: 1.0,
                mirror_lower_body: false,
            },
            None,
        );
        assert_eq!(weighted.len(), 1);
        assert!(weighted[0].time_seconds.abs() < 0.0001);
    }

    #[test]
    fn missing_opposite_guard_uses_mirrored_same_pack_anchor() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([SemanticPose::GuardLeadLeft]);
        let resolved = resolve_anchor(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            SemanticPose::GuardLeadRight,
        )
        .expect("mirrored guard fallback");

        assert!(resolved.mirrored);
        assert_eq!(
            resolved.anchor,
            &catalog.packs[HUMANOID_UNARMED_PACK].poses[&SemanticPose::GuardLeadLeft]
        );

        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::GuardLeadRight,
                sampling: PoseSampling::Anchor,
                weight: 0.75,
                mirror_lower_body: false,
            },
            None,
        );
        assert_eq!(weighted.len(), 1);
        assert!((weighted[0].mirrored_weight - 0.75).abs() < 0.0001);
    }

    #[test]
    fn partial_guard_diagonal_assets_keep_one_coherent_exact_orientation() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::GuardWalkLeadLeft,
            SemanticPose::GuardStrafeLeadRightRight,
        ]);
        let requested = [
            (SemanticPose::GuardWalkLeadLeft, 0.25),
            (SemanticPose::GuardStrafeLeadLeftLeft, 0.75),
        ];
        let samples = requested.map(|(pose, weight)| PoseSample {
            pose: SemanticPose::GuardLeadLeft,
            sampling: PoseSampling::Span {
                end: pose,
                progress: 0.5,
            },
            weight,
            mirror_lower_body: false,
        });
        let movement = requested.map(|item| item.0);
        let exact = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            false,
        );
        let mirrored = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            true,
        );
        let parity = mirrored > exact;
        assert!(!parity);

        let mut weighted = Vec::new();
        for sample in samples {
            append_resolved_sample(
                &mut weighted,
                &runtime,
                &catalog,
                HUMANOID_UNARMED_PACK,
                sample,
                Some(parity),
            );
        }
        assert!(!weighted.is_empty());
        assert!(weighted.iter().all(|clip| clip.mirrored_weight == 0.0));
        assert!((weighted.iter().map(|clip| clip.weight).sum::<f32>() - 1.0).abs() < 0.0001);
        let exact_nodes = [
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_lead_left".to_owned(),
            )]
                .anchor_nodes[&0],
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_walk_lead_left".to_owned(),
            )]
                .anchor_nodes[&0],
        ];
        assert!(
            weighted
                .iter()
                .all(|clip| exact_nodes.contains(&clip.clip.node))
        );
    }

    #[test]
    fn coherent_opposite_parity_preserves_complete_diagonal_semantics() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::GuardLeadRight,
            SemanticPose::GuardWalkLeadLeft,
            SemanticPose::GuardWalkLeadRight,
            SemanticPose::GuardStrafeLeadRightRight,
        ]);
        let samples = [
            PoseSample {
                pose: SemanticPose::GuardLeadLeft,
                sampling: PoseSampling::Span {
                    end: SemanticPose::GuardWalkLeadLeft,
                    progress: 0.5,
                },
                weight: 0.5,
                mirror_lower_body: false,
            },
            PoseSample {
                pose: SemanticPose::GuardLeadLeft,
                sampling: PoseSampling::Span {
                    end: SemanticPose::GuardStrafeLeadLeftLeft,
                    progress: 0.5,
                },
                weight: 0.5,
                mirror_lower_body: false,
            },
        ];
        let movement = [
            SemanticPose::GuardWalkLeadLeft,
            SemanticPose::GuardStrafeLeadLeftLeft,
        ];
        let exact = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            false,
        );
        let mirrored = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            true,
        );
        assert!(mirrored > exact);

        let mut weighted = Vec::new();
        for sample in samples {
            append_resolved_sample(
                &mut weighted,
                &runtime,
                &catalog,
                HUMANOID_UNARMED_PACK,
                sample,
                Some(true),
            );
        }
        let expected_nodes = [
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_lead_right".to_owned(),
            )]
                .anchor_nodes[&0],
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_walk_lead_right".to_owned(),
            )]
                .anchor_nodes[&0],
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_strafe_lead_right_right".to_owned(),
            )]
                .anchor_nodes[&0],
        ];
        assert!(
            weighted
                .iter()
                .all(|clip| expected_nodes.contains(&clip.clip.node))
        );
        assert!(
            weighted
                .iter()
                .all(|clip| (clip.mirrored_weight - clip.weight).abs() < 0.0001)
        );
    }

    #[test]
    fn exact_cardinal_guard_motion_wins_a_complete_parity_tie() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::GuardLeadRight,
            SemanticPose::GuardWalkLeadLeft,
            SemanticPose::GuardWalkLeadRight,
        ]);
        let samples = [PoseSample {
            pose: SemanticPose::GuardLeadLeft,
            sampling: PoseSampling::Span {
                end: SemanticPose::GuardWalkLeadLeft,
                progress: 0.5,
            },
            weight: 1.0,
            mirror_lower_body: false,
        }];
        let movement = [SemanticPose::GuardWalkLeadLeft];
        let exact = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            false,
        );
        let mirrored = guard_parity_score(
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            &samples,
            &movement,
            true,
        );
        assert_eq!(exact, mirrored);
        assert!(!(mirrored > exact));
    }

    #[test]
    fn absent_guard_locomotion_assets_degrade_without_a_partial_clip() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::GuardLeadLeft,
                sampling: PoseSampling::Span {
                    end: SemanticPose::GuardStrafeLeadLeftRight,
                    progress: 0.5,
                },
                weight: 1.0,
                mirror_lower_body: false,
            },
            Some(false),
        );
        assert!(weighted.is_empty());
    }

    #[test]
    fn out_of_range_catalog_frame_is_unavailable() {
        assert!(frame_fits_clip(8, 8.0 / 30.0));
        assert!(!frame_fits_clip(20, 0.1));
    }

    #[test]
    fn missing_base_keeps_generated_mannequin_visible() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        let owner = world.spawn_empty().id();
        world
            .run_system_cached_with(spawn_test_t_pose, owner)
            .unwrap();
        world.flush();
        world.run_system_cached(update_rig_visibility).unwrap();
        let (_, visibility) = world
            .query::<(&FallbackAnimationRig, &Visibility)>()
            .single(&world)
            .unwrap();
        assert_eq!(*visibility, Visibility::Inherited);
    }

    #[test]
    fn authored_zero_motion_rig_hides_mannequin_and_shows_bind_pose() {
        let mut world = World::new();
        let owner = world.spawn(AnimationPlayback::default()).id();
        let fallback = world
            .spawn((FallbackAnimationRig(owner), Visibility::Inherited))
            .id();
        let authored = world
            .spawn((AnimationRigScene(owner), Visibility::Hidden))
            .id();
        world.run_system_cached(update_rig_visibility).unwrap();
        assert_eq!(
            *world.get::<Visibility>(fallback).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *world.get::<Visibility>(authored).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn unresolved_motion_restores_authored_bind_transform() {
        let mut world = World::new();
        let owner = world.spawn(AnimationPlayback::default()).id();
        let bind = Transform::from_rotation(Quat::from_rotation_x(0.4));
        let node = world
            .spawn((
                AuthoredBindTransform { owner, local: bind },
                Transform::from_rotation(Quat::from_rotation_y(1.2)),
            ))
            .id();
        world.run_system_cached(restore_authored_bind_pose).unwrap();
        assert_eq!(*world.get::<Transform>(node).unwrap(), bind);
    }

    #[test]
    fn partial_motion_begins_from_bind_every_frame() {
        let mut world = World::new();
        let owner = world
            .spawn(AnimationPlayback {
                use_authored_bind_pose: false,
                ..default()
            })
            .id();
        let bind = Transform::from_xyz(0.0, 0.25, 0.0);
        let node = world
            .spawn((
                AuthoredBindTransform { owner, local: bind },
                Transform::from_xyz(3.0, 4.0, 5.0),
            ))
            .id();
        world
            .run_system_cached(reset_authored_bind_before_fk)
            .unwrap();
        assert_eq!(*world.get::<Transform>(node).unwrap(), bind);
        world.get_mut::<Transform>(node).unwrap().translation = Vec3::splat(9.0);
        world
            .run_system_cached(reset_authored_bind_before_fk)
            .unwrap();
        assert_eq!(*world.get::<Transform>(node).unwrap(), bind);
    }

    fn mirror_test_pose(mirror: f32) -> PlaybackPose {
        PlaybackPose {
            clips: Vec::new(),
            use_authored_bind_pose: true,
            whole_body_mirror: mirror,
            foot_ik_weights: Vec2::ZERO,
        }
    }

    #[test]
    fn guard_crossfade_activates_at_the_current_effective_pose() {
        let mut playback = AnimationPlayback {
            whole_body_mirror: 0.8,
            ..default()
        };
        let mut clock = ProceduralAnimationClock::default();
        clock.set_fixed_tick(10, 0.1);

        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );

        assert!(playback.presentation_transition.is_some());
        assert!((playback.whole_body_mirror - 0.8).abs() < 0.0001);
    }

    #[test]
    fn guard_crossfade_completes_at_the_latest_target_pose() {
        let mut playback = AnimationPlayback {
            whole_body_mirror: 0.8,
            ..default()
        };
        let mut clock = ProceduralAnimationClock::default();
        clock.set_fixed_tick(10, 0.1);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );
        clock.set_fixed_tick(11, PRESENTATION_CROSSFADE_SECONDS);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );

        assert!(playback.presentation_transition.is_none());
        assert!((playback.whole_body_mirror - 0.2).abs() < 0.0001);
    }

    #[test]
    fn hard_stop_retains_then_releases_the_effective_locomotion_pose() {
        let mut playback = AnimationPlayback {
            whole_body_mirror: 0.8,
            ordinary_locomotion_active: true,
            ..default()
        };
        let mut clock = ProceduralAnimationClock::default();
        clock.set_fixed_tick(10, 1.0 / LOCOMOTION_SAMPLE_HZ);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.0),
            WeaponGuardState::Lowered,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );
        assert!(playback.presentation_transition.is_some());
        assert!((playback.whole_body_mirror - 0.8).abs() < 0.0001);

        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.0),
            WeaponGuardState::Lowered,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            1.0,
        );
        assert!((playback.whole_body_mirror - 0.8).abs() < 0.0001);

        clock.set_fixed_tick(11, PRESENTATION_CROSSFADE_SECONDS);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.0),
            WeaponGuardState::Lowered,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );
        assert!(playback.presentation_transition.is_none());
        assert!(playback.whole_body_mirror.abs() < 0.0001);
    }

    #[test]
    fn reversing_guard_mid_crossfade_has_no_presentation_jump() {
        let mut playback = AnimationPlayback {
            whole_body_mirror: 0.8,
            ..default()
        };
        let mut clock = ProceduralAnimationClock::default();
        clock.set_fixed_tick(10, 0.09);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );
        clock.set_fixed_tick(11, 0.09);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );
        let before_reversal = playback.whole_body_mirror;

        clock.set_fixed_tick(12, 0.09);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.8),
            WeaponGuardState::Lowered,
            false,
            PRESENTATION_CROSSFADE_SECONDS,
            &clock,
            0.0,
        );

        let transition = playback.presentation_transition.as_ref().unwrap();
        assert!((playback.whole_body_mirror - before_reversal).abs() < 0.0001);
        assert!((transition.from.whole_body_mirror - before_reversal).abs() < 0.0001);
    }

    #[test]
    fn client_constraint_api_is_reexported() {
        let _: Option<HandIkTarget> = None;
        let _: HumanoidIkTargets = default();
        let _ = [HandSide::Left, HandSide::Right];
        let constraint = HeldWeaponConstraint {
            owner: Entity::PLACEHOLDER,
            primary_hand: HandSide::Right,
            secondary_grip_local: None,
        };
        assert_eq!(constraint.primary_hand, HandSide::Right);
    }

    #[test]
    fn presentation_sequence_gaps_are_bounded_ordered_and_reset_safe() {
        assert_eq!(
            coalesced_contacts(10, 13, LeadFoot::Right),
            Some(vec![
                (11, LeadFoot::Right),
                (12, LeadFoot::Left),
                (13, LeadFoot::Right),
            ])
        );
        assert_eq!(coalesced_contacts(13, 13, LeadFoot::Right), Some(vec![]));
        assert_eq!(coalesced_contacts(13, 2, LeadFoot::Left), None);
        assert_eq!(coalesced_contacts(2, 20, LeadFoot::Left), None);
        assert_eq!(bounded_forward_sequence_delta(7, 9), Some(2));
        assert_eq!(bounded_forward_sequence_delta(9, 7), None);
        assert_eq!(latest_coalesced_landing(7, 9), Some(9));
        assert_eq!(latest_coalesced_landing(9, 7), None);
        assert_eq!(latest_coalesced_landing(2, 20), None);
    }

    #[test]
    fn downed_camera_alignment_enters_the_idle_locomotion_crossfade() {
        let idle = SkeletonState::default().with_body_state(BodyState::Supine);
        assert!(!ordinary_locomotion_candidate(&idle));

        let mut turning = idle;
        turning.set_downed_turning(true);
        assert!(ordinary_locomotion_candidate(&turning));
    }
}
