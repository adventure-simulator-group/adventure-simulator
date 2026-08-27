use super::*;

#[cfg(test)]
mod legacy_tests {
    use super::*;
    use semantic_route::{SemanticRoutePath, SemanticRouteTrace};

    fn route(skeleton: SkeletonState) -> SemanticRouteTrace {
        semantic_route::route_semantic_pose_for_test(In(PresentedSkeleton::new(skeleton, None)))
    }

    #[test]
    fn semantic_router_preserves_ordinary_locomotion_evaluation() {
        let skeleton = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 2.0)
            .with_world_velocity(Vec3::NEG_Z * 2.0);
        let before = AnimationEvaluation::from_skeleton(&skeleton);
        let after = route(skeleton);
        assert_eq!(after.path, SemanticRoutePath::OrdinaryLocomotion);
        assert!(after.runtime_evaluated);
        assert_eq!(before, after.evaluation);
    }

    #[test]
    fn semantic_router_inputs_are_read_only_and_keep_live_attack_movement() {
        let mut skeleton = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_local_velocity(Vec3::NEG_Z * 3.0)
            .with_world_velocity(Vec3::NEG_Z * 3.0);
        skeleton
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 10, 20)
            .unwrap();
        skeleton.advance_action(15);
        let before = serde_json::to_vec(&skeleton).unwrap();
        let presented = PresentedSkeleton::new(skeleton, None);
        let evaluation = AnimationEvaluation::from_skeleton(&presented);
        let inputs = semantic_route::SemanticRouteInputs::from_presented(&presented, &evaluation);

        assert_eq!(inputs.action, SkeletonAction::Attack);
        assert_eq!(inputs.direction, Vec2::NEG_Y);
        assert_eq!(inputs.speed, 3.0);
        assert_eq!(presented.attack_animation(), Some(AttackAnimation::Swing));
        assert_eq!(before, serde_json::to_vec(&presented.state).unwrap());
    }

    #[test]
    fn semantic_router_preserves_raised_attack_contact_timing() {
        for (tick, expected_phase) in [(10, 0.0), (15, 0.25), (20, 0.5), (25, 0.75), (30, 1.0)] {
            let mut skeleton = SkeletonState::default()
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_lead_foot(LeadFoot::Left);
            skeleton
                .begin_attack(AttackSpec::default(), 10, 20)
                .unwrap();
            skeleton.advance_action(tick);
            let presented = PresentedSkeleton::new(skeleton.clone(), None);
            let direct = AnimationEvaluation::from_skeleton(&skeleton);
            let routed = route(skeleton);

            assert_eq!(routed.path, SemanticRoutePath::RaisedGuardAttack);
            assert!(routed.runtime_evaluated);
            assert_eq!(routed.evaluation, direct);
            assert!((routed.inputs.gait_phase - presented.gait_phase).abs() < f32::EPSILON);
            assert!((routed.evaluation.action_phase - expected_phase).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn asset_validation_resolves_deterministic_routes() {
        let asset_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
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
        assert!(
            report
                .route_resolutions
                .iter()
                .any(|resolution| resolution.route == "raised_guard_attack")
        );
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
    fn prone_presentation_predicts_the_authoritative_crawl_cadence() {
        let velocity = Vec3::NEG_Z * 2.0;
        let mut authoritative = SkeletonState::default()
            .with_body_state(BodyState::Prone)
            .with_local_velocity(velocity)
            .with_world_velocity(velocity);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);

        project_skeleton_locomotion(
            &mut authoritative,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: velocity,
                grounded: true,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 1,
            },
        );
        advance_presented_skeleton(&mut presented, &authoritative, 1.0 / LOCOMOTION_SAMPLE_HZ);

        assert!(
            circular_phase_delta(presented.gait_phase, authoritative.gait_phase).abs() < 0.000_01
        );
        assert_eq!(presented.last_phase_correction_delta, 0.0);
        assert_eq!(presented.phase_error_remaining, 0.0);
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
        // The 19 required semantics collapse to 18 independently resolvable
        // variants when the supported whole-body mirror pair appears once.
        let authored_variants = SemanticPose::HUMANOID_REQUIRED
            .into_iter()
            .filter(|pose| {
                pose.mirrored_counterpart()
                    .is_none_or(|counterpart| pose.as_str() < counterpart.as_str())
            })
            .count();
        assert_eq!(authored_variants, 18);
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
            root.poses[&SemanticPose::AttackThrust],
            PoseAnchor {
                motion: "thrust".to_owned(),
                frame: 4,
            }
        );
        assert_eq!(
            root.poses[&SemanticPose::DiveForward],
            PoseAnchor {
                motion: "dive".to_owned(),
                frame: 0,
            }
        );
        for pose in [
            SemanticPose::DiveBackward,
            SemanticPose::DiveLeft,
            SemanticPose::DiveRight,
        ] {
            assert_eq!(root.poses[&pose].motion, "dive");
        }
        assert_eq!(SemanticPose::DiveRight.mirrored_counterpart(), None);
        assert_eq!(root.poses[&SemanticPose::GuardSwing].frame, 0);
        assert_eq!(root.poses[&SemanticPose::GuardThrust].frame, 0);
        assert_eq!(root.poses[&SemanticPose::AttackOffhand].frame, 0);
        assert_eq!(root.motions["swing"].last_frame, 12);
        assert_eq!(root.motions["swing"].required_last_frame, 4);
        assert_eq!(root.motions["thrust"].last_frame, 12);
        assert_eq!(root.motions["thrust"].required_last_frame, 4);
        assert_eq!(root.motions["offhand"].last_frame, 4);
        assert_eq!(root.motions["offhand"].required_last_frame, 0);
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
        let mut assets = Assets::<AnimationClip>::default();
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
            runtime.clips.insert(
                key,
                LoadedClip {
                    handle: assets.add(AnimationClip::default()),
                    duration_seconds: 64.0 / ANIMATION_FPS,
                    layer: ClipLayer::Whole,
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
    fn attack_entry_blends_frame_zero_guard_and_contact_in_one_motion() {
        let catalog = AnimationPackCatalog::default();
        let runtime =
            runtime_with_available([SemanticPose::GuardThrust, SemanticPose::AttackThrust]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::GuardThrust,
                sampling: PoseSampling::Span {
                    end: SemanticPose::AttackThrust,
                    progress: 0.5,
                },
                weight: 1.0,
                mirror_lower_body: false,
            },
        );
        assert_eq!(weighted.len(), 2);
        let thrust = runtime.clips[&(HUMANOID_UNARMED_PACK.to_owned(), "thrust".to_owned())]
            .handle
            .id();
        assert!(
            weighted
                .iter()
                .all(|sample| sample.clip.handle.id() == thrust)
        );
        assert!(
            weighted
                .iter()
                .any(|sample| sample.time_seconds == 0.0 && (sample.weight - 0.5).abs() < 0.0001)
        );
        assert!(
            weighted
                .iter()
                .any(
                    |sample| (sample.time_seconds - 4.0 / ANIMATION_FPS).abs() < 0.0001
                        && (sample.weight - 0.5).abs() < 0.0001
                )
        );
    }

    #[test]
    fn mirrored_gait_endpoint_uses_a_distinct_pre_fk_clip_node() {
        let catalog = AnimationPackCatalog::default();
        let mut runtime =
            runtime_with_available([SemanticPose::RunContact, SemanticPose::RunFlight]);
        let mut assets = Assets::<AnimationClip>::default();
        let mirrored_handle = assets.add(AnimationClip::default());
        runtime.clips.insert(
            (HUMANOID_UNARMED_PACK.to_owned(), "run_mirrored".to_owned()),
            LoadedClip {
                handle: mirrored_handle.clone(),
                duration_seconds: 64.0 / ANIMATION_FPS,
                layer: ClipLayer::Whole,
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
        );

        assert_eq!(weighted.len(), 1);
        assert_eq!(weighted[0].clip.handle.id(), mirrored_handle.id());
        assert!((weighted[0].weight - 0.5).abs() < 0.0001);
    }

    #[test]
    fn zero_clip_base_gets_stable_animation_targets_without_a_player() {
        let mut world = World::new();
        world.init_resource::<AnimationRuntime>();
        let owner = world.spawn_empty().id();
        let rig = world.spawn(AnimationRigScene(owner)).id();
        let skeleton = world.spawn(Name::new("Skeleton")).id();
        let root = world.spawn(Name::new("body_world")).id();
        let pelvis = world.spawn(Name::new("root")).id();
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

        assert_eq!(
            world.get::<AnimationTargetId>(pelvis),
            Some(&AnimationTargetId::from_names(
                [
                    Name::new("Skeleton"),
                    Name::new("body_world"),
                    Name::new("root")
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
    fn composite_mask_keeps_mhr_world_pelvis_and_legs_out_of_the_upper_body() {
        for lower in [
            "Skeleton",
            "body_world",
            "root",
            "l_upleg",
            "r_upleg_twist3_proc",
            "l_lowleg",
            "r_foot",
            "l_talocrural",
            "r_subtalar",
            "l_transversetarsal",
            "l_ball",
        ] {
            assert!(is_lower_body_animation_target(lower), "{lower}");
        }
        for upper in [
            "c_spine0",
            "c_spine2",
            "c_spine3",
            "l_clavicle",
            "r_uparm",
            "c_head",
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
                Name::new("body_world"),
                Name::new("root"),
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
        );
        assert_eq!(weighted.len(), 1);
        assert!(weighted[0].time_seconds.abs() < 0.0001);
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
    fn client_constraint_api_is_reexported() {
        let _: Option<HandIkTarget> = None;
        let _: HumanoidIkTargets = default();
        let _ = [HandSide::Left, HandSide::Right];
        let constraint = HeldWeaponConstraint {
            owner: Entity::PLACEHOLDER,
            primary_hand: HandSide::Right,
            secondary_grip_local: None,
            socket_bind_correction: Transform::IDENTITY,
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
