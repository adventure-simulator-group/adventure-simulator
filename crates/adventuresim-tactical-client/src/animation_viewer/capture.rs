//! ECS fixture driving, sampling, and three-view capture.

use super::*;

pub(super) const CAPTURE_ROOT_GROUND_OFFSET_METRES: f32 = 0.95;
pub(super) const FULL_PLANT_SUPPORT_WEIGHT: f32 = 0.99;
pub(super) const TRACKED_BONE_NAMES: [&str; 17] = [
    "pelvis",
    "chest",
    "head",
    "left_shoulder",
    "right_shoulder",
    "left_elbow",
    "right_elbow",
    "left_hand",
    "right_hand",
    "left_hip",
    "right_hip",
    "left_knee",
    "right_knee",
    "left_foot",
    "right_foot",
    "left_toe",
    "right_toe",
];

#[derive(Component)]
pub(super) struct CaptureSubject;

#[derive(Component)]
pub(super) struct CaptureLabel;

#[derive(Resource)]
pub(super) struct CaptureSequence {
    output: PathBuf,
    settle_frames: u32,
    plan: Vec<PlannedFrame>,
    index: usize,
    view_index: usize,
    applied: bool,
    settled: u32,
    waiting: u32,
    capture_in_flight: bool,
    view_fingerprints: Vec<u64>,
    duplicate_view_frames: Vec<String>,
    samples: Vec<FrameSample>,
    global_bone_frames: Vec<GlobalBoneFrame>,
    presentation_events: Vec<PresentationEventSample>,
    repeated_evaluation_baseline: Option<RepeatedEvaluationSnapshot>,
    repeated_evaluation_valid: bool,
    active_scenario: Option<&'static str>,
    warmup_frames: u32,
    motion_ready_frames: u32,
    simulation_tick: u64,
    scenario_distance: f32,
}

pub(super) struct CompletedCapture {
    pub(super) output: PathBuf,
    pub(super) plan: Vec<PlannedFrame>,
    pub(super) frames: Vec<FrameSample>,
    pub(super) global_bone_frames: Vec<GlobalBoneFrame>,
    pub(super) presentation_events: Vec<PresentationEventSample>,
    pub(super) duplicate_view_frames: Vec<String>,
    pub(super) repeated_evaluation_valid: bool,
    pub(super) playback_backend: &'static str,
    pub(super) pose_buffer_metrics: PoseBufferMetrics,
}

impl CaptureSequence {
    pub(super) fn new(output: PathBuf, settle_frames: u32, scenario: Option<&str>) -> Self {
        let plan = match scenario {
            Some("flat-grid-walk-2.0") => steady_scenario("flat-grid-walk-2.0", 2.0, 3.0),
            Some("flat-grid-run-5.5") => steady_scenario("flat-grid-run-5.5", 5.5, 3.0),
            Some("flat-grid-walk-no-ik") => steady_scenario("flat-grid-walk-no-ik", 2.0, 3.0),
            Some("flat-grid-sprint-no-ik") => {
                steady_scenario("flat-grid-sprint-no-ik", canonical_john_sprint_speed(), 3.0)
            }
            Some("flat-grid-walk-stop") => flat_grid_walk_stop_scenario(),
            Some("full-ragdoll") => full_ragdoll_scenario(),
            _ => capture_plan()
                .into_iter()
                .filter(|frame| scenario.is_none_or(|scenario| frame.scenario == scenario))
                .collect::<Vec<_>>(),
        };
        assert!(
            !plan.is_empty(),
            "requested animation capture scenario is unknown"
        );
        Self {
            output,
            settle_frames: settle_frames.max(1),
            plan,
            index: 0,
            view_index: 0,
            applied: false,
            settled: 0,
            waiting: 0,
            capture_in_flight: false,
            view_fingerprints: Vec::with_capacity(VIEWS.len()),
            duplicate_view_frames: Vec::new(),
            samples: Vec::new(),
            global_bone_frames: Vec::new(),
            presentation_events: Vec::new(),
            repeated_evaluation_baseline: None,
            repeated_evaluation_valid: true,
            active_scenario: None,
            warmup_frames: 0,
            motion_ready_frames: 0,
            simulation_tick: 0,
            scenario_distance: 0.0,
        }
    }

    fn uses_flat_grid(&self) -> bool {
        self.plan
            .iter()
            .all(|frame| frame.scenario.starts_with("flat-grid-"))
    }

    fn complete(
        &mut self,
        playback_backend: &'static str,
        pose_buffer_metrics: PoseBufferMetrics,
    ) -> CompletedCapture {
        CompletedCapture {
            output: std::mem::take(&mut self.output),
            plan: std::mem::take(&mut self.plan),
            frames: std::mem::take(&mut self.samples),
            global_bone_frames: std::mem::take(&mut self.global_bone_frames),
            presentation_events: std::mem::take(&mut self.presentation_events),
            duplicate_view_frames: std::mem::take(&mut self.duplicate_view_frames),
            repeated_evaluation_valid: self.repeated_evaluation_valid,
            playback_backend,
            pose_buffer_metrics,
        }
    }
}

pub(super) fn next_capture_simulation_tick(current: u64, absolute_first_sample: bool) -> u64 {
    if absolute_first_sample {
        current
    } else {
        current.wrapping_add(1)
    }
}

pub(super) struct RepeatedEvaluationSnapshot {
    scenario: &'static str,
    scenario_frame: usize,
    bones: BTreeMap<String, BoneSample>,
    contact_sequence: u64,
    landing_sequence: u64,
    event_count: usize,
    leg_ik: LegIkDiagnostics,
}

pub(super) fn repeated_bone_mismatch(
    expected: &BTreeMap<String, BoneSample>,
    actual: &BTreeMap<String, BoneSample>,
) -> Option<(String, f32, f32)> {
    if let Some(name) = expected
        .keys()
        .find(|name| !actual.contains_key(*name))
        .or_else(|| actual.keys().find(|name| !expected.contains_key(*name)))
    {
        return Some((name.clone(), f32::INFINITY, f32::INFINITY));
    }
    expected.iter().find_map(|(name, expected)| {
        let actual = actual
            .get(name)
            .expect("equal repeated-evaluation bone keys were checked above");
        let position_delta =
            Vec3::from_array(expected.position).distance(Vec3::from_array(actual.position));
        let expected_rotation = Quat::from_array(expected.rotation_xyzw);
        let actual_rotation = Quat::from_array(actual.rotation_xyzw);
        // `acos` quantizes identical f32 quaternions to roughly 0.056-0.079
        // degrees on some frames. Treat a direct dot match as identity, then
        // retain the angular report for genuine changes.
        let rotation_dot = expected_rotation.dot(actual_rotation).abs();
        let rotation_delta = if rotation_dot >= 1.0 - 0.000_001 {
            0.0
        } else {
            expected_rotation
                .angle_between(actual_rotation)
                .to_degrees()
        };
        // Re-evaluating one logical tick must be visually identical. Keep only
        // sub-half-millimetre/sub-twentieth-degree numeric noise.
        (position_delta > 0.0005 || rotation_delta > 0.05).then_some((
            name.clone(),
            position_delta,
            rotation_delta,
        ))
    })
}

pub(super) fn repeated_leg_ik_matches(
    expected: LegIkDiagnostics,
    actual: LegIkDiagnostics,
) -> bool {
    let option_vec3_matches =
        |expected: Option<Vec3>, actual: Option<Vec3>| match (expected, actual) {
            (Some(expected), Some(actual)) => expected.distance(actual) <= 0.0005,
            (None, None) => true,
            _ => false,
        };
    let option_scalar_matches =
        |expected: Option<f32>, actual: Option<f32>| match (expected, actual) {
            (Some(expected), Some(actual)) => (expected - actual).abs() <= 0.001,
            (None, None) => true,
            _ => false,
        };
    option_vec3_matches(expected.left_authored_target, actual.left_authored_target)
        && option_vec3_matches(expected.right_authored_target, actual.right_authored_target)
        && option_vec3_matches(expected.left_planned_contact, actual.left_planned_contact)
        && option_vec3_matches(expected.right_planned_contact, actual.right_planned_contact)
        && option_vec3_matches(expected.settle_capture_point, actual.settle_capture_point)
        && option_vec3_matches(expected.left_solve_target, actual.left_solve_target)
        && option_vec3_matches(expected.right_solve_target, actual.right_solve_target)
        && (expected.left_support_weight - actual.left_support_weight).abs() <= 0.001
        && (expected.right_support_weight - actual.right_support_weight).abs() <= 0.001
        && expected.left_release_active == actual.left_release_active
        && expected.right_release_active == actual.right_release_active
        && option_vec3_matches(expected.left_release_target, actual.left_release_target)
        && option_vec3_matches(expected.right_release_target, actual.right_release_target)
        && option_scalar_matches(expected.settle_progress, actual.settle_progress)
        && (expected.left_knee_foot_yaw_offset_degrees - actual.left_knee_foot_yaw_offset_degrees)
            .abs()
            <= 0.05
        && (expected.right_knee_foot_yaw_offset_degrees - actual.right_knee_foot_yaw_offset_degrees)
            .abs()
            <= 0.05
}

pub(super) fn setup_viewer(mut commands: Commands, sequence: Res<CaptureSequence>) {
    let default_player = Player::default();
    let mut generator = TerrainGenerator::new(0xA11C_E5E1);
    generator.period = 200.0;
    let terrain = generator.generate(100, if sequence.uses_flat_grid() { 0 } else { 30 }, 100);
    let spawn_height =
        terrain.height_at(Vec2::ZERO).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES;
    commands.spawn((
        Name::new(if sequence.uses_flat_grid() {
            "Animation review flat-grid scene"
        } else {
            "Animation review hills scene"
        }),
        SceneId("hills".to_owned()),
        SceneEnvironmentFixture::TemperateHills.snapshot("hills"),
        terrain,
        Transform::default(),
    ));

    commands.spawn((
        Name::new(default_player.name),
        CaptureSubject,
        Player::default(),
        CharacterId(default_tactical_character_id()),
        CharacterLook::default(),
        SkeletonState::default(),
        Transform::from_xyz(0.0, spawn_height, 0.0),
        Collider::cylinder(0.4, 1.9),
        CollisionMargin(0.01),
        tactical_character_controller(),
    ));
    commands.spawn((
        Name::new("Animation review fill light"),
        DirectionalLight {
            illuminance: 35_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        CaptureLabel,
        Text::new("Loading authored animation rig..."),
        TextFont::from_font_size(22.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

/// Screenshot completion advances capture views asynchronously. Reassert the
/// planned look on every render of a logical sample so all three views enter
/// procedural PostUpdate with identical presentation input.
pub(super) fn freeze_capture_look(
    sequence: Res<CaptureSequence>,
    mut subjects: Query<&mut CharacterLook, With<CaptureSubject>>,
) {
    let Some(frame) = sequence.plan.get(sequence.index) else {
        return;
    };
    for mut look in &mut subjects {
        look.yaw = frame.camera_yaw;
        look.pitch = frame.camera_pitch;
    }
}

#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "the Bevy capture driver independently controls timing, IK modes, animation input, subject state, and labels"
)]
pub(super) fn drive_sequence(
    mut sequence: ResMut<CaptureSequence>,
    mut procedural_clock: ResMut<ProceduralAnimationClock>,
    mut terrain_ik: ResMut<TerrainIkEnabled>,
    mut guard_input: ResMut<WeaponGuardInputState>,
    animation_runtime: Res<AnimationRuntime>,
    terrain: Single<&SceneTerrain>,
    mut subjects: Query<
        (
            &mut SkeletonState,
            &mut Transform,
            &mut CharacterLook,
            Option<&AnimationPlayback>,
            Option<&mut LegIkState>,
            Option<&mut ArmIkState>,
            Option<&mut RaisedFootworkState>,
            Option<&mut LocomotionHeightState>,
            Option<&mut LocomotionBodyResponseState>,
        ),
        With<CaptureSubject>,
    >,
    mut labels: Query<&mut Text, With<CaptureLabel>>,
) {
    if sequence.applied || sequence.capture_in_flight || sequence.index >= sequence.plan.len() {
        return;
    }
    let frame = sequence.plan[sequence.index].clone();
    let metadata = scenario_metadata(frame.scenario);
    let mut gait_phase = 0.0;
    let mut presentation_settled = false;
    for (
        mut skeleton,
        mut transform,
        mut look,
        playback,
        ik_state,
        arm_ik_state,
        raised_footwork,
        height_state,
        body_response,
    ) in &mut subjects
    {
        let Some(playback) = playback else {
            return;
        };
        presentation_settled = playback.presentation_is_settled();
        if frame.scenario != "full-ragdoll" && !playback.authored_pose_is_ready() {
            return;
        }

        if sequence.active_scenario != Some(frame.scenario) {
            sequence.active_scenario = Some(frame.scenario);
            sequence.scenario_distance = 0.0;
            sequence.motion_ready_frames = 0;
            *skeleton = SkeletonState::default();
            *guard_input = WeaponGuardInputState::default();
            if let Some(mut ik_state) = ik_state {
                *ik_state = LegIkState::default();
            }
            if let Some(mut arm_ik_state) = arm_ik_state {
                *arm_ik_state = ArmIkState::default();
            }
            if let Some(mut raised_footwork) = raised_footwork {
                *raised_footwork = RaisedFootworkState::default();
            }
            if let Some(mut height_state) = height_state {
                *height_state = LocomotionHeightState::default();
            }
            if let Some(mut body_response) = body_response {
                *body_response = LocomotionBodyResponseState::default();
            }
            let ground = terrain.height_at(Vec2::ZERO).unwrap_or_default();
            transform.translation = Vec3::new(0.0, ground + CAPTURE_ROOT_GROUND_OFFSET_METRES, 0.0);
            transform.rotation = Quat::from_rotation_y(std::f32::consts::PI);
            if frame.scenario.starts_with("raised-guard-tap-stop-") {
                // Prime the static combat stance and its terrain-conformed IK
                // before judging the later six-frame movement tap. A live
                // character has already held this stance; capturing component
                // allocation and pelvis acquisition would test viewer spawn,
                // not the authored-to-procedural stop transition.
                sequence.warmup_frames = 8;
            }
            if let Some((start_body, _)) = transition_for_scenario(frame.scenario) {
                skeleton.transition_body(start_body);
                // Prime the authored endpoint before beginning the transition.
                // Live characters have already evaluated their prone/supine or
                // upright base; a fresh viewer subject otherwise crossfades
                // from its default standing pose during the first samples.
                sequence.warmup_frames = 8;
            }
        }

        if let Some(body) = downed_body_for_scenario(frame.scenario) {
            skeleton.transition_body(body);
            let preload_locomotion = frame.scenario_frame == 0
                && matches!(
                    required_motion_for_scenario(frame.scenario),
                    Some("prone_crawl" | "supine_scamper")
                )
                && !required_motion_for_scenario(frame.scenario)
                    .is_some_and(|motion| animation_runtime.motion_is_processed(motion));
            skeleton.set_downed_turning(
                frame.scenario != "downed-prone-look-at"
                    && (preload_locomotion || (frame.scenario_frame >= 4 && frame.speed <= 0.05)),
            );
        }

        let orientation =
            Quat::from_euler(EulerRot::YXZ, frame.camera_yaw, frame.camera_pitch, 0.0);
        look.yaw = frame.camera_yaw;
        look.pitch = frame.camera_pitch;
        let attack_start_frame = frame.scenario.starts_with("attack-live-").then_some(8);
        if frame.action != SkeletonAction::Attack
            || attack_start_frame == Some(frame.scenario_frame)
        {
            skeleton.lead_foot = frame.lead_foot;
        }
        terrain_ik.0 = terrain_ik_enabled_for_frame(&frame);
        guard_input.desired = frame.weapon_guard;
        set_weapon_guard(&mut skeleton, guard_input.desired);
        if frame.scenario == "downed-prone-look-at" {
            let target = downed_camera_roll_target(transform.rotation, orientation);
            skeleton.advance_downed_facing(
                target,
                true,
                if frame.scenario_frame == 0 {
                    0.0
                } else {
                    1.0 / 84.0
                },
            );
        }
        let dive_impact = frame.scenario.ends_with("-impact");
        let quickstep = is_quickstep_scenario(frame.scenario);
        let grounded = if quickstep {
            frame.scenario_frame < quickstep_release_frame()
                || frame.scenario_frame >= quickstep_landing_frame()
        } else if dive_impact {
            frame.scenario_frame == 0 || frame.scenario_frame >= 17
        } else {
            metadata.kind != ScenarioKind::Landing || frame.scenario_frame >= 32
        };
        let vertical_velocity = if quickstep && frame.scenario_frame < quickstep_landing_frame() {
            quickstep_fixture_vertical_state(frame.scenario_frame).1
        } else if (metadata.kind == ScenarioKind::Landing || dive_impact) && !grounded {
            -4.5
        } else {
            0.0
        };
        let requested_local_velocity = Vec3::new(
            frame.local_direction.x * frame.speed,
            vertical_velocity,
            frame.local_direction.y * frame.speed,
        );
        let local_velocity = requested_local_velocity;
        let world_velocity = controller_yaw(orientation) * local_velocity;
        sequence.simulation_tick = next_capture_simulation_tick(
            sequence.simulation_tick,
            sequence.index == 0 && sequence.warmup_frames == 0,
        );
        if sequence.warmup_frames == 0
            && frame.scenario_frame == 0
            && let Some((start_body, transition)) = transition_for_scenario(frame.scenario)
        {
            skeleton.transition_body(start_body);
            if frame.scenario.ends_with("-aimed-impact") {
                // Match the authoritative launch seam: velocity and authored
                // direction capture one camera frame even if the previously
                // displayed root had not finished turning toward it.
                transform.rotation =
                    dive_launch_root_rotation(Quat::from_rotation_y(frame.camera_yaw));
            }
            // Matches the live server's terrain-contact dive recovery.
            let duration = if frame.scenario.starts_with("dive-backward") {
                32
            } else if dive_impact {
                20
            } else {
                84
            };
            skeleton.begin_posture_transition(transition, sequence.simulation_tick, duration);
        }
        let delta_seconds = if frame.scenario_frame == 0 {
            0.0
        } else {
            1.0 / locomotion_sample_hz()
        };
        procedural_clock.fixed_tick = Some((sequence.simulation_tick, delta_seconds.max(0.0)));
        let horizontal = transform.translation.xz() + world_velocity.xz() * delta_seconds;
        let vertical = if quickstep {
            let ground = terrain.height_at(horizontal).unwrap_or_default()
                + CAPTURE_ROOT_GROUND_OFFSET_METRES;
            ground
                + quickstep_fixture_vertical_state(frame.scenario_frame)
                    .0
                    .max(0.0)
        } else if terrain_ik.0 {
            terrain.height_at(horizontal).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES
        } else {
            transform.translation.y
        };
        transform.translation = Vec3::new(horizontal.x, vertical, horizontal.y);
        let action_starts_now = match frame.action {
            SkeletonAction::Attack => attack_start_frame == Some(frame.scenario_frame),
            SkeletonAction::Dodge if quickstep => frame.scenario_frame == 0,
            SkeletonAction::Dodge | SkeletonAction::Block => skeleton.action_kind() != frame.action,
            SkeletonAction::None => false,
        };
        if action_starts_now {
            let start = sequence.simulation_tick;
            let contact = start
                + if frame.action == SkeletonAction::Attack {
                    19
                } else {
                    64
                };
            match frame.action {
                SkeletonAction::Attack => {
                    let attack = if frame.scenario == "attack-live-stationary-swing" {
                        AttackSpec::new(AttackAnimation::Swing)
                    } else {
                        AttackSpec::new(AttackAnimation::Thrust)
                    };
                    skeleton
                        .begin_attack(attack, start, contact)
                        .expect("viewer attack transition must be admitted");
                }
                SkeletonAction::Dodge => {
                    let spec = if quickstep {
                        DodgeSpec::quickstep(frame.local_direction)
                            .expect("quickstep scenario direction must be non-zero")
                    } else {
                        DodgeSpec::default()
                    };
                    skeleton
                        .begin_dodge(
                            spec,
                            start,
                            if quickstep {
                                start + (quickstep_action_ticks() / 2) as u64
                            } else {
                                contact
                            },
                        )
                        .expect("viewer dodge transition must be admitted");
                }
                SkeletonAction::Block => {
                    skeleton
                        .begin_block(BlockSpec::default(), start, contact)
                        .expect("viewer block transition must be admitted");
                }
                SkeletonAction::None => {}
            }
        }
        if !skeleton.is_posture_transitioning() {
            transform.rotation = advance_body_facing(
                transform.rotation,
                orientation,
                world_velocity,
                frame.action,
                skeleton.weapon_guard(),
                delta_seconds,
            );
        }
        if frame.scenario == "full-ragdoll" {
            let fall = ((frame.scenario_frame + 4) as f32 / 8.0).clamp(0.0, 1.0);
            transform.rotation = Quat::from_rotation_y(std::f32::consts::PI)
                * Quat::from_rotation_x(1.25 * smoothstep01(fall));
            // The production root becomes a coarse dynamic body while the
            // client ragdoll is active. Reproduce its descent in this focused
            // viewer so the first sixty-frame settle window actually exercises
            // whole-body terrain contact instead of suspending the pelvis at
            // the standing controller height.
            let ground = terrain.height_at(Vec2::ZERO).unwrap_or_default();
            transform.translation.y =
                ground + CAPTURE_ROOT_GROUND_OFFSET_METRES - 0.62 * smoothstep01(fall);
        }
        sequence.scenario_distance += frame.speed * delta_seconds;
        let jump_charging =
            frame.scenario == "jump-charge-anticipation" && (4..48).contains(&frame.scenario_frame);
        project_skeleton_locomotion_with_body_rotation(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: world_velocity,
                grounded,
                delta_seconds,
                tick: sequence.simulation_tick,
            },
            transform.rotation,
            None,
        );
        skeleton.set_jump_anticipation(jump_charging);
        if sequence.warmup_frames == 0 && transition_for_scenario(frame.scenario).is_some() {
            let previous_transition = skeleton.posture_transition();
            skeleton.advance_posture_transition(sequence.simulation_tick);
            transform.rotation = (transform.rotation
                * dive_landing_facing_delta(previous_transition, skeleton.posture_transition())
                * supine_get_up_counter_yaw_delta(
                    previous_transition,
                    skeleton.posture_transition(),
                ))
            .normalize();
        }
        gait_phase = skeleton.gait_phase;
    }
    if sequence.warmup_frames > 0 {
        sequence.warmup_frames -= 1;
        return;
    }
    if frame.scenario_frame == 0
        && let Some(motion) = required_motion_for_scenario(frame.scenario)
    {
        if !animation_runtime.motion_is_processed(motion) || !presentation_settled {
            sequence.motion_ready_frames = 0;
            return;
        }
        sequence.motion_ready_frames += 1;
        if sequence.motion_ready_frames < 2 {
            return;
        }
    }
    for mut label in &mut labels {
        **label = format!(
            "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 64 Hz frame {}",
            frame.scenario,
            frame.speed,
            gait_phase,
            VIEWS[sequence.view_index].slug(),
            frame.scenario_frame,
        );
    }
    sequence.applied = true;
    sequence.settled = 0;
}

pub(super) fn position_capture_camera(
    sequence: Res<CaptureSequence>,
    subjects: Query<(&Transform, &PresentedSkeleton), With<CaptureSubject>>,
    mut cameras: Query<&mut Transform, (With<TacticalGameplayCamera>, Without<CaptureSubject>)>,
    mut labels: Query<(&mut Text, &mut Visibility), With<CaptureLabel>>,
) {
    let (Ok((subject, skeleton)), Ok(mut camera)) = (subjects.single(), cameras.single_mut())
    else {
        return;
    };
    let focus = subject.translation + Vec3::Y * 0.95;
    let view = VIEWS[sequence.view_index.min(VIEWS.len() - 1)];
    match view {
        CaptureView::Gameplay => {
            // Physics simulation is disabled in this fixture, so ahoy does
            // not refresh its controller-follow base transform. Reconstruct
            // that default base and apply the exact gameplay camera offset;
            // otherwise the offset accumulates and the first raw frame is a
            // pelvis-level/empty view.
            camera.translation =
                subject.translation + animation_capture_camera_offset(Quat::IDENTITY);
            camera.rotation = Quat::IDENTITY;
        }
        CaptureView::Side => {
            camera.translation = focus + Vec3::new(5.0, 0.45, 0.0);
            camera.look_at(focus, Vec3::Y);
        }
        CaptureView::Front => {
            camera.translation = focus + Vec3::new(0.0, 0.45, -5.0);
            camera.look_at(focus, Vec3::Y);
        }
    }
    if sequence.applied
        && let Some(frame) = sequence.plan.get(sequence.index)
    {
        for (mut label, mut visibility) in &mut labels {
            *visibility = if matches!(view, CaptureView::Gameplay) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            **label = format!(
                "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 64 Hz frame {}",
                frame.scenario,
                frame.speed,
                skeleton.gait_phase,
                view.slug(),
                frame.scenario_frame,
            );
        }
    }
}

pub(super) fn draw_skeleton_overlay(
    sequence: Res<CaptureSequence>,
    mut gizmos: Gizmos,
    subjects: Query<(Entity, &PresentedSkeleton), With<CaptureSubject>>,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
) {
    if matches!(
        VIEWS[sequence.view_index.min(VIEWS.len() - 1)],
        CaptureView::Gameplay
    ) {
        return;
    }
    let Ok((subject, skeleton)) = subjects.single() else {
        return;
    };
    let positions = bones
        .iter()
        .filter(|(bone, _)| bone.owner == subject)
        .map(|(bone, transform)| (bone.role, transform.translation()))
        .collect::<BTreeMap<_, _>>();
    let connections = [
        (BoneRole::Pelvis, BoneRole::Chest),
        (BoneRole::Chest, BoneRole::Head),
        (BoneRole::Chest, BoneRole::UpperArmLeft),
        (BoneRole::UpperArmLeft, BoneRole::ForearmLeft),
        (BoneRole::ForearmLeft, BoneRole::HandLeft),
        (BoneRole::Chest, BoneRole::UpperArmRight),
        (BoneRole::UpperArmRight, BoneRole::ForearmRight),
        (BoneRole::ForearmRight, BoneRole::HandRight),
        (BoneRole::Pelvis, BoneRole::ThighLeft),
        (BoneRole::ThighLeft, BoneRole::ShinLeft),
        (BoneRole::ShinLeft, BoneRole::FootLeft),
        (BoneRole::Pelvis, BoneRole::ThighRight),
        (BoneRole::ThighRight, BoneRole::ShinRight),
        (BoneRole::ShinRight, BoneRole::FootRight),
    ];
    for (start, end) in connections {
        if let (Some(&start), Some(&end)) = (positions.get(&start), positions.get(&end)) {
            gizmos.line(start, end, Color::srgba(0.1, 0.9, 1.0, 0.8));
        }
    }
    let (left_support, right_support) = locomotion_support_weights(skeleton);
    for (role, support) in [
        (BoneRole::FootLeft, left_support),
        (BoneRole::FootRight, right_support),
    ] {
        let Some(&position) = positions.get(&role) else {
            continue;
        };
        let color = if support >= 0.55 {
            Color::srgb(1.0, 0.8, 0.05)
        } else {
            Color::srgb(1.0, 0.1, 0.7)
        };
        gizmos.line(position - Vec3::X * 0.09, position + Vec3::X * 0.09, color);
        gizmos.line(position - Vec3::Z * 0.09, position + Vec3::Z * 0.09, color);
        gizmos.line(
            position,
            position + Vec3::Y * (0.05 + support * 0.15),
            color,
        );
    }
}

pub(super) fn draw_flat_ground_grid(sequence: Res<CaptureSequence>, mut gizmos: Gizmos) {
    let Some(frame) = sequence.plan.get(sequence.index) else {
        return;
    };
    if !frame.scenario.starts_with("flat-grid-") {
        return;
    }

    const HALF_EXTENT_METRES: i32 = 20;
    const SUBDIVISIONS_PER_METRE: i32 = 4;
    let half_steps = HALF_EXTENT_METRES * SUBDIVISIONS_PER_METRE;
    let height = 0.012;
    for step in -half_steps..=half_steps {
        let coordinate = step as f32 / SUBDIVISIONS_PER_METRE as f32;
        let whole_metre = step % SUBDIVISIONS_PER_METRE == 0;
        let color = if step == 0 {
            Color::srgba(1.0, 0.45, 0.12, 0.95)
        } else if whole_metre {
            Color::srgba(0.82, 0.86, 0.92, 0.80)
        } else {
            Color::srgba(0.42, 0.47, 0.55, 0.48)
        };
        gizmos.line(
            Vec3::new(coordinate, height, -HALF_EXTENT_METRES as f32),
            Vec3::new(coordinate, height, HALF_EXTENT_METRES as f32),
            color,
        );
        gizmos.line(
            Vec3::new(-HALF_EXTENT_METRES as f32, height, coordinate),
            Vec3::new(HALF_EXTENT_METRES as f32, height, coordinate),
            color,
        );
    }
}

pub(super) fn collect_locomotion_presentation_events(
    mut events: MessageReader<LocomotionPresentationEvent>,
    mut sequence: ResMut<CaptureSequence>,
) {
    if sequence.index >= sequence.plan.len() {
        return;
    }
    let scenario = sequence.plan[sequence.index].scenario.to_owned();
    let scenario_frame = sequence.plan[sequence.index].scenario_frame;
    sequence
        .presentation_events
        .extend(events.read().map(move |event| PresentationEventSample {
            scenario: scenario.clone(),
            scenario_frame,
            owner: capture_entity_id(event.owner),
            sequence: event.sequence,
            sample_tick: event.sample_tick,
            kind: match event.kind {
                LocomotionPresentationEventKind::Contact(foot) => {
                    format!("contact_{}", foot.id())
                }
                LocomotionPresentationEventKind::Landing => "landing".to_owned(),
            },
        }));
}

#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "the Bevy capture system records each independent animation state and diagnostic source"
)]
pub(super) fn capture_frame(
    mut commands: Commands,
    mut sequence: ResMut<CaptureSequence>,
    pose_buffer_metrics: Res<PoseBufferMetrics>,
    secondary_physics_telemetry: Res<SecondaryPhysicsTelemetry>,
    terrain_ik: Res<TerrainIkEnabled>,
    subjects: Query<
        (
            Entity,
            &PresentedSkeleton,
            &GlobalTransform,
            Option<&AnimationPlayback>,
            Option<&RaisedFootworkState>,
            Option<&LocomotionBodyResponseState>,
            Option<&LocomotionHeightState>,
            Option<&LegIkState>,
            Option<&SemanticRouteTrace>,
        ),
        With<CaptureSubject>,
    >,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
    animation_bones: Query<(
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        &GlobalTransform,
    )>,
    terrain: Single<&SceneTerrain>,
    mut exit: MessageWriter<AppExit>,
) {
    let playback_backend = "pose_buffer";
    let pose_buffer_metrics = *pose_buffer_metrics;
    if !sequence.applied || sequence.capture_in_flight {
        return;
    }
    let Ok((
        subject,
        skeleton,
        subject_global,
        playback,
        raised_footwork,
        body_response,
        height_state,
        leg_ik,
        semantic_route,
    )) = subjects.single()
    else {
        wait_or_fail(&mut sequence, "capture subject is missing", &mut exit);
        return;
    };
    let Some(playback) = playback else {
        wait_or_fail(
            &mut sequence,
            "capture subject has no AnimationPlayback",
            &mut exit,
        );
        return;
    };
    let Some(semantic_route) = semantic_route else {
        wait_or_fail(
            &mut sequence,
            "capture subject has no semantic route trace",
            &mut exit,
        );
        return;
    };
    if sequence.active_scenario != Some("full-ragdoll") && !playback.authored_pose_is_ready() {
        wait_or_fail(
            &mut sequence,
            "authored locomotion clip has not resolved",
            &mut exit,
        );
        return;
    }
    sequence.waiting = 0;
    sequence.settled += 1;
    let required = if sequence.index == 0 {
        sequence.settle_frames.max(60)
    } else {
        // A view change needs one complete render before requesting the next
        // asynchronous screenshot; otherwise two paths may receive the same
        // previously rendered camera image.
        sequence.settle_frames.max(2)
    };
    if sequence.settled < required {
        return;
    }

    let frame = sequence.plan[sequence.index].clone();
    let view = VIEWS[sequence.view_index];
    let file_name = format!(
        "{}-{:04}-{}.png",
        frame.scenario,
        frame.scenario_frame,
        view.slug()
    );
    let path = sequence.output.join(&file_name);
    let evaluation_bones = collect_bones(
        subject,
        &bones,
        &terrain,
        (!terrain_ik.0)
            .then_some(subject_global.translation().y - CAPTURE_ROOT_GROUND_OFFSET_METRES),
    );
    let evaluation_leg_ik = leg_ik.map(LegIkState::diagnostics).unwrap_or_default();
    if sequence.view_index == 0 {
        sequence.repeated_evaluation_baseline = Some(RepeatedEvaluationSnapshot {
            scenario: frame.scenario,
            scenario_frame: frame.scenario_frame,
            bones: evaluation_bones.clone(),
            contact_sequence: skeleton.contact_sequence,
            landing_sequence: skeleton.landing_sequence,
            event_count: sequence.presentation_events.len(),
            leg_ik: evaluation_leg_ik,
        });
    } else if let Some(baseline) = &sequence.repeated_evaluation_baseline {
        let bone_mismatch = repeated_bone_mismatch(&baseline.bones, &evaluation_bones);
        let bones_match = bone_mismatch.is_none();
        let repeated_evaluation_matches = baseline.scenario == frame.scenario
            && baseline.scenario_frame == frame.scenario_frame
            && bones_match
            && baseline.contact_sequence == skeleton.contact_sequence
            && baseline.landing_sequence == skeleton.landing_sequence
            && baseline.event_count == sequence.presentation_events.len()
            && repeated_leg_ik_matches(baseline.leg_ik, evaluation_leg_ik);
        if !repeated_evaluation_matches
            && let Some((bone, position_delta, rotation_delta)) = &bone_mismatch
        {
            warn!(
                scenario = frame.scenario,
                scenario_frame = frame.scenario_frame,
                bone,
                position_delta,
                rotation_delta,
                "repeated animation evaluation changed a captured bone"
            );
        }
        if !repeated_evaluation_matches && bone_mismatch.is_none() {
            warn!(
                scenario = frame.scenario,
                scenario_frame = frame.scenario_frame,
                baseline_contact = baseline.contact_sequence,
                contact = skeleton.contact_sequence,
                baseline_landing = baseline.landing_sequence,
                landing = skeleton.landing_sequence,
                baseline_events = baseline.event_count,
                events = sequence.presentation_events.len(),
                "repeated animation evaluation changed non-bone state"
            );
        }
        sequence.repeated_evaluation_valid &= repeated_evaluation_matches;
    }
    if sequence.view_index == 0 {
        sequence.global_bone_frames.push(GlobalBoneFrame {
            scenario: frame.scenario.to_owned(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            action: skeleton.action_kind(),
            action_phase: skeleton.action_phase(),
            subject_translation: subject_global.translation().to_array(),
            subject_rotation_xyzw: subject_global.rotation().to_array(),
            bones: collect_global_bone_transforms(subject, &animation_bones),
        });
        let cadence_support = locomotion_support_weights(skeleton);
        let root_distance_metres = sequence.scenario_distance;
        let (desired_left_foot_target, desired_right_foot_target) = raised_footwork
            .map(|state| (state.left_solve_target, state.right_solve_target))
            .unwrap_or_default();
        let leg_ik = evaluation_leg_ik;
        let ik_support =
            if leg_ik.left_solve_target.is_some() || leg_ik.right_solve_target.is_some() {
                (leg_ik.left_support_weight, leg_ik.right_support_weight)
            } else {
                cadence_support
            };
        let (left_support_weight, right_support_weight) = raised_footwork
            .filter(|state| state.initialized())
            .map(|state| (state.left_support_weight, state.right_support_weight))
            .unwrap_or(ik_support);
        sequence.samples.push(FrameSample {
            scenario: frame.scenario.to_owned(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            speed_metres_per_second: frame.speed,
            gait_phase: skeleton.gait_phase,
            locomotion_sample_tick: skeleton.locomotion_sample_tick,
            body_acceleration: (subject_global.rotation().inverse() * skeleton.world_acceleration)
                .to_array(),
            world_acceleration: skeleton.world_acceleration.to_array(),
            secondary_upper_body_bone_count: secondary_physics_telemetry
                .simulated_upper_body_bones,
            secondary_upper_body_mean_blend_weight: secondary_physics_telemetry
                .mean_upper_body_blend_weight,
            secondary_upper_body_maximum_pose_lag_degrees: secondary_physics_telemetry
                .maximum_pose_lag_degrees,
            secondary_upper_body_maximum_inertial_acceleration_radians_per_second_squared:
                secondary_physics_telemetry
                    .maximum_inertial_acceleration_radians_per_second_squared,
            // Raised guard owns its visual contacts locally. Segment its
            // diagnostics by the sequence that actually changed the rendered
            // support foot, not by the replicated locomotion cadence.
            // RaisedFootworkState retains the presentation-owned contact
            // counter even while its actual step is deliberately
            // uninitialized across an authored/no-IK handoff. Falling back to
            // the replicated idle counter here invents a 1 -> 0 reset that
            // neither the rendered feet nor the local stepper performed.
            contact_sequence: raised_footwork.map_or(
                skeleton.contact_sequence,
                RaisedFootworkState::step_sequence,
            ),
            contact_foot: raised_footwork
                .and_then(RaisedFootworkState::contact_foot)
                .unwrap_or(skeleton.contact_foot),
            landing_sequence: skeleton.landing_sequence,
            landing_impact_speed: skeleton.landing_impact_speed,
            body_lean_pitch_degrees: body_response
                .map_or(0.0, |state| state.pitch_radians.to_degrees()),
            body_lean_roll_degrees: body_response
                .map_or(0.0, |state| state.roll_radians.to_degrees()),
            landing_compression_metres: height_state.map_or(0.0, |state| state.landing_compression),
            root_distance_metres,
            root_position_metres: subject_global.translation().to_array(),
            world_travel_direction: (controller_yaw(Quat::from_rotation_y(frame.camera_yaw))
                * Vec3::new(frame.local_direction.x, 0.0, frame.local_direction.y))
            .normalize_or_zero()
            .to_array(),
            desired_body_forward_direction: if frame.weapon_guard == WeaponGuardState::Raised
                || matches!(frame.action, SkeletonAction::Attack | SkeletonAction::Block)
            {
                (controller_yaw(Quat::from_rotation_y(frame.camera_yaw)) * Vec3::NEG_Z).to_array()
            } else {
                (controller_yaw(Quat::from_rotation_y(frame.camera_yaw))
                    * Vec3::new(frame.local_direction.x, 0.0, frame.local_direction.y))
                .normalize_or_zero()
                .to_array()
            },
            body_forward_direction: (subject_global.rotation() * Vec3::Z).to_array(),
            body_rotation_xyzw: subject_global.rotation().to_array(),
            weapon_guard: frame.weapon_guard,
            lead_foot: skeleton.lead_foot,
            action: skeleton.action_kind(),
            action_phase: skeleton.action_phase(),
            attack_animation: skeleton.attack_animation(),
            strike_family: skeleton.strike_family(),
            guard_action: frame.weapon_guard == WeaponGuardState::Raised
                || matches!(
                    frame.action,
                    SkeletonAction::Dodge | SkeletonAction::Attack | SkeletonAction::Block
                ),
            left_support_weight,
            right_support_weight,
            desired_left_foot_target: desired_left_foot_target.map(|value| value.to_array()),
            desired_right_foot_target: desired_right_foot_target.map(|value| value.to_array()),
            ik_left_authored_target: leg_ik.left_authored_target.map(|value| value.to_array()),
            ik_right_authored_target: leg_ik.right_authored_target.map(|value| value.to_array()),
            ik_left_planned_contact: leg_ik.left_planned_contact.map(|value| value.to_array()),
            ik_right_planned_contact: leg_ik.right_planned_contact.map(|value| value.to_array()),
            ik_settle_capture_point: leg_ik.settle_capture_point.map(|value| value.to_array()),
            ik_left_solve_target: leg_ik.left_solve_target.map(|value| value.to_array()),
            ik_right_solve_target: leg_ik.right_solve_target.map(|value| value.to_array()),
            ik_left_support_weight: leg_ik.left_support_weight,
            ik_right_support_weight: leg_ik.right_support_weight,
            ik_left_release_active: leg_ik.left_release_active,
            ik_right_release_active: leg_ik.right_release_active,
            ik_left_release_target: leg_ik.left_release_target.map(|value| value.to_array()),
            ik_right_release_target: leg_ik.right_release_target.map(|value| value.to_array()),
            ik_settle_progress: leg_ik.settle_progress,
            ik_left_knee_foot_yaw_offset_degrees: leg_ik.left_knee_foot_yaw_offset_degrees,
            ik_right_knee_foot_yaw_offset_degrees: leg_ik.right_knee_foot_yaw_offset_degrees,
            semantic_route_requested_path: semantic_route.requested_path,
            semantic_route_selected_path: semantic_route.path,
            semantic_route_runtime_evaluated: semantic_route.runtime_evaluated,
            screenshots: VIEWS
                .into_iter()
                .map(|view| {
                    (
                        view.slug().to_owned(),
                        format!(
                            "{}-{:04}-{}.png",
                            frame.scenario,
                            frame.scenario_frame,
                            view.slug()
                        ),
                    )
                })
                .collect(),
            bones: evaluation_bones,
        });
    }
    sequence.capture_in_flight = true;
    let frame_key = format!("{}:{}", frame.scenario, frame.scenario_frame);
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut sequence: ResMut<CaptureSequence>,
              mut exit: MessageWriter<AppExit>| {
            sequence
                .view_fingerprints
                .push(visual_fingerprint(&captured.image));
            save_to_disk(&path)(captured);
            sequence.capture_in_flight = false;
            sequence.view_index += 1;
            sequence.settled = 0;
            if sequence.view_index < VIEWS.len() {
                return;
            }
            if sequence
                .view_fingerprints
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != VIEWS.len()
            {
                sequence.duplicate_view_frames.push(frame_key.clone());
            }
            sequence.view_fingerprints.clear();
            sequence.view_index = 0;
            sequence.index += 1;
            sequence.applied = false;
            if sequence.index == sequence.plan.len() {
                let completed = sequence.complete(playback_backend, pose_buffer_metrics);
                exit.write(write_completed_capture(completed));
            }
        },
    );
}

pub(super) fn visual_fingerprint(image: &Image) -> u64 {
    let Some(data) = image.data.as_deref() else {
        return 0;
    };
    let width = image.texture_descriptor.size.width as usize;
    let height = image.texture_descriptor.size.height as usize;
    let stride = width.saturating_mul(4);
    if stride == 0 || data.len() < stride.saturating_mul(height) {
        return 0;
    }
    // Ignore the top UI label and hash a regular sample of the rendered 3D
    // view. This catches accidentally identical camera outputs cheaply.
    let mut hash = 0xcbf29ce484222325_u64;
    for y in (96.min(height)..height).step_by(8) {
        for x in (0..width).step_by(8) {
            for channel in 0..3 {
                hash ^= data[y * stride + x * 4 + channel] as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

pub(super) fn collect_bones(
    subject: Entity,
    bones: &Query<(&HumanoidBone, &GlobalTransform)>,
    terrain: &SceneTerrain,
    flat_ground_height: Option<f32>,
) -> BTreeMap<String, BoneSample> {
    bones
        .iter()
        .filter(|(bone, _)| bone.owner == subject && tracked_bone(bone.role).is_some())
        .map(|(bone, transform)| {
            let rotation = transform.rotation();
            (
                tracked_bone(bone.role).unwrap().to_owned(),
                BoneSample {
                    position: transform.translation().to_array(),
                    rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
                    terrain_clearance_metres: flat_ground_height
                        .or_else(|| terrain.height_at(transform.translation().xz()))
                        .map(|height| transform.translation().y - height),
                },
            )
        })
        .collect()
}

pub(super) fn collect_global_bone_transforms(
    subject: Entity,
    bones: &Query<(
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        &GlobalTransform,
    )>,
) -> Vec<GlobalBoneTransformSample> {
    let mut samples = bones
        .iter()
        .filter(|(_, bind, _, _)| bind.owner == subject)
        .map(|(target, _, name, transform)| {
            let (scale, rotation, translation) = transform.to_scale_rotation_translation();
            let sample = GlobalBoneTransformSample {
                name: name.map_or_else(|| "<unnamed>".to_owned(), |name| name.as_str().to_owned()),
                target_id: capture_animation_target_id(*target),
                translation: translation.to_array(),
                rotation_xyzw: rotation.to_array(),
                scale: scale.to_array(),
            };
            (*target, sample)
        })
        .collect::<Vec<_>>();
    samples.sort_by(|(left_target, left), (right_target, right)| {
        (&left.name, left_target).cmp(&(&right.name, right_target))
    });
    samples.into_iter().map(|(_, sample)| sample).collect()
}

pub(super) fn tracked_bone(role: BoneRole) -> Option<&'static str> {
    Some(match role {
        BoneRole::Pelvis => "pelvis",
        BoneRole::Chest => "chest",
        BoneRole::Head => "head",
        BoneRole::UpperArmLeft => "left_shoulder",
        BoneRole::UpperArmRight => "right_shoulder",
        BoneRole::ForearmLeft => "left_elbow",
        BoneRole::ForearmRight => "right_elbow",
        BoneRole::HandLeft => "left_hand",
        BoneRole::HandRight => "right_hand",
        BoneRole::ThighLeft => "left_hip",
        BoneRole::ThighRight => "right_hip",
        BoneRole::ShinLeft => "left_knee",
        BoneRole::ShinRight => "right_knee",
        BoneRole::FootLeft => "left_foot",
        BoneRole::FootRight => "right_foot",
        BoneRole::ToeLeft => "left_toe",
        BoneRole::ToeRight => "right_toe",
        _ => return None,
    })
}

pub(super) fn jitter_frames(frames: &[FrameSample]) -> Vec<JitterFrame> {
    let mut previous: Option<&FrameSample> = None;
    let mut analysis_segment = 0_u64;
    frames
        .iter()
        // A ragdoll is intentionally non-smooth at the animation-to-physics
        // handoff and does not obey authored locomotion jerk thresholds.
        .filter(|frame| frame.scenario != "full-ragdoll")
        .map(|frame| {
            if let Some(previous_frame) = previous {
                let action_transition = previous_frame.action != frame.action;
                let landing = previous_frame.landing_sequence != frame.landing_sequence;
                let foot_contact = previous_frame.contact_sequence != frame.contact_sequence;
                let guard_stop_handoff = is_guard_stop_transition(&frame.scenario)
                    && previous_frame.speed_metres_per_second > 0.05
                    && frame.speed_metres_per_second <= 0.05;
                let quickstep_takeoff = is_quickstep_scenario(&frame.scenario)
                    && previous_frame.left_support_weight >= FULL_PLANT_SUPPORT_WEIGHT
                    && previous_frame.right_support_weight >= FULL_PLANT_SUPPORT_WEIGHT
                    && frame.left_support_weight <= 0.01
                    && frame.right_support_weight <= 0.01;
                if previous_frame.scenario != frame.scenario {
                    analysis_segment = 0;
                } else if action_transition
                    || landing
                    || foot_contact
                    || guard_stop_handoff
                    || quickstep_takeoff
                {
                    analysis_segment = analysis_segment.wrapping_add(1);
                }
            }
            previous = Some(frame);
            JitterFrame {
                scenario: frame.scenario.clone(),
                analysis_segment,
                scenario_frame: frame.scenario_frame,
                time_seconds: frame.time_seconds,
                bones: frame
                    .bones
                    .iter()
                    .map(|(name, bone)| {
                        let position = if name == "pelvis" {
                            // The capture root is authoritative locomotion, not
                            // a skeletal joint. Exclude its world translation
                            // from limb jitter while retaining pelvis rotation.
                            Vec3::ZERO
                        } else {
                            Vec3::from_array(bone.position)
                        };
                        let rotation = Quat::from_array(bone.rotation_xyzw);
                        let (position, rotation) = parent_bone(name)
                            .and_then(|parent| frame.bones.get(parent))
                            .map_or((position, rotation), |parent| {
                                let parent_position = Vec3::from_array(parent.position);
                                let parent_rotation = Quat::from_array(parent.rotation_xyzw);
                                (
                                    parent_rotation.inverse() * (position - parent_position),
                                    parent_rotation.inverse() * rotation,
                                )
                            });
                        (
                            name.clone(),
                            JitterBone {
                                position: position.to_array(),
                                rotation_xyzw: rotation.to_array(),
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

pub(super) fn parent_bone(name: &str) -> Option<&'static str> {
    Some(match name {
        "chest" | "left_hip" | "right_hip" => "pelvis",
        "head" | "left_shoulder" | "right_shoulder" => "chest",
        "left_elbow" => "left_shoulder",
        "right_elbow" => "right_shoulder",
        "left_hand" => "left_elbow",
        "right_hand" => "right_elbow",
        "left_knee" => "left_hip",
        "right_knee" => "right_hip",
        "left_foot" => "left_knee",
        "right_foot" => "right_knee",
        "left_toe" => "left_foot",
        "right_toe" => "right_foot",
        "pelvis" => return None,
        _ => return None,
    })
}

pub(super) fn wait_or_fail(
    sequence: &mut CaptureSequence,
    reason: &str,
    exit: &mut MessageWriter<AppExit>,
) {
    sequence.waiting += 1;
    if sequence.waiting < 1200 {
        return;
    }
    let message = format!(
        "animation viewer timed out after {} rendered frames: {reason}\n",
        sequence.waiting
    );
    let path = sequence.output.join("failure.txt");
    fs::write(&path, &message).unwrap_or_else(|error| panic!("failed to write {path:?}: {error}"));
    error!(%reason, path = ?path, "Animation capture failed");
    exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_ticks_remain_unique_across_consecutive_scenario_boundaries() {
        let first = next_capture_simulation_tick(0, true);
        let second = next_capture_simulation_tick(first, false);
        let next_scenario_frame_zero = next_capture_simulation_tick(second, false);
        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(next_scenario_frame_zero, 2);
    }

    #[test]
    fn repeated_evaluation_rejects_missing_or_extra_tracked_bones() {
        let sample = BoneSample {
            position: Vec3::ZERO.to_array(),
            rotation_xyzw: Quat::IDENTITY.to_array(),
            terrain_clearance_metres: Some(0.0),
        };
        let expected = BTreeMap::from([("left_foot".to_owned(), sample)]);
        let missing = BTreeMap::new();
        assert!(repeated_bone_mismatch(&expected, &missing).is_some());
        assert!(repeated_bone_mismatch(&missing, &expected).is_some());
        assert!(repeated_bone_mismatch(&expected, &expected).is_none());
    }

    #[test]
    fn repeated_evaluation_diagnostics_detect_hidden_ik_state_mutation() {
        let baseline = LegIkDiagnostics::default();
        let changed = LegIkDiagnostics {
            left_support_weight: 1.0,
            left_release_active: true,
            left_planned_contact: Some(Vec3::NEG_Z),
            settle_progress: Some(0.5),
            ..default()
        };
        assert_ne!(baseline, changed);
    }

    #[test]
    fn flat_grid_scenarios_are_opt_in_complete_cycles_with_explicit_ik_ownership() {
        for (scenario, speed) in [("flat-grid-walk-2.0", 2.0), ("flat-grid-run-5.5", 5.5)] {
            let sequence = CaptureSequence::new(PathBuf::new(), 1, Some(scenario));
            assert!(sequence.uses_flat_grid());
            assert!(sequence.plan.len() > 64);
            assert!(sequence.plan.iter().all(|frame| {
                frame.scenario == scenario
                    && frame.speed == speed
                    && terrain_ik_enabled_for_frame(frame)
            }));
        }

        let sprint = CaptureSequence::new(PathBuf::new(), 1, Some("flat-grid-sprint-no-ik"));
        assert!(sprint.uses_flat_grid());
        assert!(sprint.plan.len() > 64);
        assert!(sprint.plan.iter().all(|frame| {
            frame.scenario == "flat-grid-sprint-no-ik"
                && (frame.speed - canonical_john_sprint_speed()).abs() < f32::EPSILON
                && !terrain_ik_enabled_for_frame(frame)
        }));
        assert!((canonical_john_sprint_speed() - 8.957_856).abs() < 0.000_01);

        let walk = CaptureSequence::new(PathBuf::new(), 1, Some("flat-grid-walk-no-ik"));
        assert!(walk.uses_flat_grid());
        assert!(walk.plan.len() > 64);
        assert!(walk.plan.iter().all(|frame| {
            frame.scenario == "flat-grid-walk-no-ik"
                && (frame.speed - 2.0).abs() < f32::EPSILON
                && !terrain_ik_enabled_for_frame(frame)
        }));

        let ordinary = CaptureSequence::new(PathBuf::new(), 1, Some("steady-walk-2.0"));
        assert!(!ordinary.uses_flat_grid());

        let stop = CaptureSequence::new(PathBuf::new(), 1, Some("flat-grid-walk-stop"));
        assert!(stop.uses_flat_grid());
        assert!(stop.plan[..48].iter().all(|frame| frame.speed == 2.0));
        assert!(stop.plan[56..].iter().all(|frame| frame.speed == 0.0));
        assert!(
            stop.plan[48..=56]
                .windows(2)
                .all(|pair| pair[1].speed <= pair[0].speed)
        );
    }

    #[test]
    fn completed_capture_owns_the_finished_ecs_state() {
        let output = PathBuf::from("owned-capture");
        let mut sequence = CaptureSequence::new(output.clone(), 1, Some("flat-grid-walk-2.0"));
        let plan_len = sequence.plan.len();

        let completed = sequence.complete("pose_buffer", PoseBufferMetrics::default());

        assert_eq!(completed.output, output);
        assert_eq!(completed.plan.len(), plan_len);
        assert_eq!(completed.playback_backend, "pose_buffer");
        assert!(sequence.output.as_os_str().is_empty());
        assert!(sequence.plan.is_empty());
    }
}
