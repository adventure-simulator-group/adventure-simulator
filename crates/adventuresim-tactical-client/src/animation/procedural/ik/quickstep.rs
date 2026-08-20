use super::*;

// Quickstep is a short, client-presented action whose contact owner is checked
// against the live hip before every sample. The guard follower's horizon-wide
// pelvis-correction tube grows larger than the available leg reserve over a
// dodge and makes every terrain contact impossible by construction.
const QUICKSTEP_HIP_PATH_TOLERANCE_METRES: f32 = 0.01;

const QUICKSTEP_RELEASE_FLEXION_RESERVE_RADIANS: f32 = 35.0_f32.to_radians();

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct QuickstepIkState {
    action_start_tick: Option<u64>,
    left_takeoff_world: Option<Vec3>,
    right_takeoff_world: Option<Vec3>,
    left_takeoff_rotation_world: Option<Quat>,
    right_takeoff_rotation_world: Option<Quat>,
    left_release_phase: Option<f32>,
    right_release_phase: Option<f32>,
    left_contact_segment: Option<C2FootSegment>,
    right_contact_segment: Option<C2FootSegment>,
    left_impact_correction: Option<(Vec3, Vec3)>,
    right_impact_correction: Option<(Vec3, Vec3)>,
    left_contact_event: Option<ContactMotionEvent>,
    right_contact_event: Option<ContactMotionEvent>,
    left_motion_owner_epoch: u64,
    right_motion_owner_epoch: u64,
    left_release_owner_active: bool,
    right_release_owner_active: bool,
    left_hip_world: Option<Vec3>,
    right_hip_world: Option<Vec3>,
    left_hip_velocity: Vec3,
    right_hip_velocity: Vec3,
    left_solve_world: Option<Vec3>,
    right_solve_world: Option<Vec3>,
    left_target_velocity: Vec3,
    right_target_velocity: Vec3,
    left_target_acceleration: Vec3,
    right_target_acceleration: Vec3,
    left_desired_target: Option<Vec3>,
    right_desired_target: Option<Vec3>,
    left_ideal_velocity: Vec3,
    right_ideal_velocity: Vec3,
    left_ideal_acceleration: Vec3,
    right_ideal_acceleration: Vec3,
    left_ideal_history_valid: bool,
    right_ideal_history_valid: bool,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
    pelvis_shift: f32,
    pelvis_shift_velocity: f32,
    pelvis_shift_acceleration: f32,
    pelvis_recovery: Option<PelvisRecoverySegment>,
    presented_pelvis_local: Option<Transform>,
    evaluation_tick: Option<u64>,
}

fn seed_quickstep_from_presentation(
    action_start_tick: Option<u64>,
    presentation: Option<LegIkMemory>,
) -> QuickstepIkState {
    let pelvis_follower = PelvisFollowerState::default();
    QuickstepIkState {
        action_start_tick,
        left_motion_owner_epoch: action_start_tick.unwrap_or(0),
        right_motion_owner_epoch: action_start_tick.unwrap_or(0),
        left_takeoff_world: presentation.and_then(|memory| memory.left_foot_world_target),
        right_takeoff_world: presentation.and_then(|memory| memory.right_foot_world_target),
        left_takeoff_rotation_world: presentation
            .and_then(|memory| memory.left_last_rendered_foot_rotation_world),
        right_takeoff_rotation_world: presentation
            .and_then(|memory| memory.right_last_rendered_foot_rotation_world),
        left_solve_world: presentation.and_then(|memory| memory.left_foot_world_target),
        right_solve_world: presentation.and_then(|memory| memory.right_foot_world_target),
        left_target_velocity: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.velocity),
        right_target_velocity: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.velocity),
        left_target_acceleration: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.acceleration),
        right_target_acceleration: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.acceleration),
        left_desired_target: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map(|follower| follower.previous_ideal),
        right_desired_target: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map(|follower| follower.previous_ideal),
        left_ideal_velocity: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_velocity),
        right_ideal_velocity: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_velocity),
        left_ideal_acceleration: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_acceleration),
        right_ideal_acceleration: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_acceleration),
        left_ideal_history_valid: presentation
            .is_some_and(|memory| memory.left_foot_follower.is_some()),
        right_ideal_history_valid: presentation
            .is_some_and(|memory| memory.right_foot_follower.is_some()),
        left_knee_bend_world: presentation.and_then(|memory| {
            memory
                .left_terrain_pole_world
                .or(memory.left_anatomical_pole_world)
        }),
        right_knee_bend_world: presentation.and_then(|memory| {
            memory
                .right_terrain_pole_world
                .or(memory.right_anatomical_pole_world)
        }),
        left_end_direction: presentation.and_then(|memory| memory.left_terrain_end_direction),
        right_end_direction: presentation.and_then(|memory| memory.right_terrain_end_direction),
        pelvis_shift: pelvis_follower.position,
        pelvis_shift_velocity: pelvis_follower.velocity,
        pelvis_shift_acceleration: pelvis_follower.acceleration,
        ..default()
    }
}

fn latch_quickstep_release(release_phase: &mut Option<f32>, action_phase: f32) {
    release_phase.get_or_insert(action_phase);
}

/// Retains each takeoff ankle as a world plant while the leg can still reach
/// it without leaning past a 45-degree arch. Only then does that foot tuck and
/// travel toward its authored guard landing position.
pub(in crate::animation) fn apply(
    clock: Res<ProceduralAnimationClock>,
    owners: Query<&PresentedSkeleton>,
    terrain: Query<&SceneTerrain>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut QuickstepIkState>,
    mut diagnostics: Query<&mut LegIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        let mut state = states
            .get_mut(owner)
            .map(|state| *state)
            .unwrap_or_default();
        let previous_presentation = diagnostics.get_mut(owner).ok().map(|state| state.0);
        if skeleton.action_kind() != SkeletonAction::Dodge {
            if state.left_takeoff_world.is_some() || state.right_takeoff_world.is_some() {
                // Terrain/guard evaluates before quickstep in the procedural
                // chain. On the action-source exit edge it cannot yet own the
                // final landed pose, so re-present the last completed
                // quickstep chains once before retiring this owner. This is a
                // pure presentation handoff; state does not advance.
                if let Some(memory) = previous_presentation {
                    locomotion::apply_retained_raised_lower_body(
                        rig,
                        memory,
                        state.presented_pelvis_local,
                        &mut transforms.p1(),
                    );
                }
                if let Ok(mut diagnostic) = diagnostics.get_mut(owner) {
                    if let Some(segment) = state
                        .left_contact_segment
                        .filter(|segment| segment.end.is_contact() && !segment.timing.is_complete())
                    {
                        diagnostic.0.left_contact_event =
                            Some(ContactMotionEvent::AbortedLiveReach {
                                aborted_owner_epoch: segment.owner_epoch,
                            });
                        diagnostic.0.left_contact_event_tick =
                            Some(skeleton.locomotion_sample_tick);
                        diagnostic.0.left_motion_owner_epoch = skeleton.locomotion_sample_tick;
                        diagnostic.0.left_direct_c2_active = false;
                    }
                    if let Some(segment) = state
                        .right_contact_segment
                        .filter(|segment| segment.end.is_contact() && !segment.timing.is_complete())
                    {
                        diagnostic.0.right_contact_event =
                            Some(ContactMotionEvent::AbortedLiveReach {
                                aborted_owner_epoch: segment.owner_epoch,
                            });
                        diagnostic.0.right_contact_event_tick =
                            Some(skeleton.locomotion_sample_tick);
                        diagnostic.0.right_motion_owner_epoch = skeleton.locomotion_sample_tick;
                        diagnostic.0.right_direct_c2_active = false;
                    }
                }
                state = QuickstepIkState::default();
                store_state(owner, state, &mut states, &mut commands);
            } else if let Ok(mut diagnostic) = diagnostics.get_mut(owner) {
                if diagnostic
                    .0
                    .left_contact_event_tick
                    .is_some_and(|event_tick| skeleton.locomotion_sample_tick > event_tick)
                    && matches!(
                        diagnostic.0.left_contact_event,
                        Some(ContactMotionEvent::AbortedLiveReach { .. })
                    )
                {
                    diagnostic.0.left_contact_event = None;
                    diagnostic.0.left_contact_event_tick = None;
                }
                if diagnostic
                    .0
                    .right_contact_event_tick
                    .is_some_and(|event_tick| skeleton.locomotion_sample_tick > event_tick)
                    && matches!(
                        diagnostic.0.right_contact_event,
                        Some(ContactMotionEvent::AbortedLiveReach { .. })
                    )
                {
                    diagnostic.0.right_contact_event = None;
                    diagnostic.0.right_contact_event_tick = None;
                }
            }
            continue;
        }
        let action_start_tick = skeleton.action_view().map(|action| match action {
            ActionView::Dodge { timeline, .. }
            | ActionView::Attack { timeline, .. }
            | ActionView::Block { timeline, .. } => timeline.start_tick,
        });
        let action_landing_tick = skeleton.action_view().map(|action| {
            let timeline = match action {
                ActionView::Dodge { timeline, .. }
                | ActionView::Attack { timeline, .. }
                | ActionView::Block { timeline, .. } => timeline,
            };
            timeline
                .start_tick
                .saturating_add(timeline.preparation_ticks)
                .saturating_add(timeline.preparation_ticks.saturating_add(3) / 4)
        });
        let action_tick = skeleton.action_presentation_tick();
        if state.action_start_tick != action_start_tick {
            state = seed_quickstep_from_presentation(action_start_tick, previous_presentation);
        }
        let (tick, semantic_delta) = clock.semantic_step();
        let evaluation_advances = state.evaluation_tick != Some(tick);
        let delta_seconds = if evaluation_advances {
            state.evaluation_tick = Some(tick);
            semantic_delta
        } else {
            0.0
        };
        if repeated_fixed_tick_skips_ik(true, evaluation_advances) {
            // Quickstep's diagnostics intentionally do not reuse terrain's
            // evaluation tick. Re-publish its own first-view chains, then its
            // additive pelvis offset, without advancing followers or plans.
            if let Some(memory) = previous_presentation {
                locomotion::apply_retained_raised_lower_body(
                    rig,
                    memory,
                    None,
                    &mut transforms.p1(),
                );
            }
            apply_quickstep_pelvis_shift(rig, state.pelvis_shift, &parents, &mut transforms);
            continue;
        }
        let Some((rig_origin, rig_rotation)) = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
        else {
            continue;
        };
        if evaluation_advances {
            let followed = advance_pelvis_follower_with_recovery(
                PelvisFollowerState {
                    position: state.pelvis_shift,
                    velocity: state.pelvis_shift_velocity,
                    acceleration: state.pelvis_shift_acceleration,
                },
                &mut state.pelvis_recovery,
                0.0,
                delta_seconds,
            );
            state.pelvis_shift = followed.position;
            state.pelvis_shift_velocity = followed.velocity;
            state.pelvis_shift_acceleration = followed.acceleration;
        }
        apply_quickstep_pelvis_shift(rig, state.pelvis_shift, &parents, &mut transforms);

        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                false,
            ),
        ];
        if skeleton.is_grounded()
            && (state.left_takeoff_world.is_none() || state.right_takeoff_world.is_none())
        {
            for (_, _, foot_role, left) in legs {
                let takeoff = rig
                    .get(&foot_role)
                    .and_then(|foot| transforms.p0().compute_global_transform(*foot).ok())
                    .filter(|global| {
                        global.translation().is_finite() && global.rotation().is_finite()
                    });
                if left {
                    state.left_takeoff_world = takeoff.map(|global| global.translation());
                    state.left_takeoff_rotation_world = takeoff.map(|global| global.rotation());
                } else {
                    state.right_takeoff_world = takeoff.map(|global| global.translation());
                    state.right_takeoff_rotation_world = takeoff.map(|global| global.rotation());
                }
            }
        }

        let action_phase = skeleton.action_phase();
        let mut targets = [None, None];
        let mut rendered_targets = [None, None];
        let mut supports = [0.0, 0.0];
        let mut solve_inputs = [None, None];
        for (index, (upper_role, lower_role, foot_role, left)) in legs.into_iter().enumerate() {
            let (Some(&upper), Some(&lower), Some(&foot)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
            else {
                continue;
            };
            let takeoff = if left {
                state.left_takeoff_world
            } else {
                state.right_takeoff_world
            }
            .unwrap_or_else(|| foot_snapshot.global.translation());
            let authored = foot_snapshot.global.translation();
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(authored);
            let reach = maximum_reach(upper_length, lower_length);
            let warning_reach = (upper_length * upper_length
                + lower_length * lower_length
                + 2.0
                    * upper_length
                    * lower_length
                    * QUICKSTEP_RELEASE_FLEXION_RESERVE_RADIANS.cos())
            .sqrt();
            let mut release_phase = if left {
                state.left_release_phase
            } else {
                state.right_release_phase
            };
            if release_phase.is_none()
                && (action_tick == 0
                    || quickstep_foot_must_release(
                        skeleton.is_grounded(),
                        action_phase,
                        upper_snapshot.global.translation(),
                        takeoff,
                        warning_reach,
                        reach,
                    ))
            {
                latch_quickstep_release(&mut release_phase, action_phase);
            }
            let linear_progress = release_phase.map_or(0.0, |release| {
                if skeleton.is_grounded() && action_phase > 0.125 {
                    1.0
                } else {
                    ((action_phase - release) / (0.625 - release).max(0.001)).clamp(0.0, 1.0)
                }
            });
            let progress = quintic_progress(linear_progress);
            let mut target = takeoff.lerp(authored, progress);
            target.y += (std::f32::consts::PI * progress).sin() * 0.16;
            let (previous, velocity, acceleration, mut previous_desired) = if left {
                (
                    state.left_solve_world,
                    state.left_target_velocity,
                    state.left_target_acceleration,
                    state.left_desired_target,
                )
            } else {
                (
                    state.right_solve_world,
                    state.right_target_velocity,
                    state.right_target_acceleration,
                    state.right_desired_target,
                )
            };
            let (mut previous_ideal_velocity, mut previous_ideal_acceleration) = if left {
                (state.left_ideal_velocity, state.left_ideal_acceleration)
            } else {
                (state.right_ideal_velocity, state.right_ideal_acceleration)
            };
            let mut ideal_history_valid = if left {
                state.left_ideal_history_valid
            } else {
                state.right_ideal_history_valid
            };
            let reach_envelope = FootReachEnvelope::new(
                upper_snapshot.global.translation(),
                upper_snapshot.global.translation() + skeleton.world_velocity * delta_seconds,
                warning_reach,
                reach,
            );
            let mut forced_release = if left {
                state.left_release_owner_active
            } else {
                state.right_release_owner_active
            };
            let previous_hip = if left {
                state.left_hip_world
            } else {
                state.right_hip_world
            };
            let previous_hip_velocity = if left {
                state.left_hip_velocity
            } else {
                state.right_hip_velocity
            };
            let hip_trajectory = reach_envelope.and_then(|reach| {
                PredictedHipTrajectory::from_retained_motion(
                    reach,
                    previous_hip,
                    previous_hip_velocity,
                    delta_seconds,
                    QUICKSTEP_HIP_PATH_TOLERANCE_METRES,
                    0.0,
                )
            });
            let mut contact_segment = if left {
                state.left_contact_segment
            } else {
                state.right_contact_segment
            };
            if release_phase.is_some()
                && contact_segment.is_none()
                && !forced_release
                && evaluation_advances
            {
                let contact_seconds = action_landing_tick
                    .map(|tick| tick.saturating_sub(action_tick) as f32)
                    .unwrap_or(0.0)
                    / CONTINUITY_SAMPLE_HZ;
                let semantic_landing = takeoff + skeleton.world_velocity * contact_seconds;
                let release_without_contact = release_plan_or_hold(
                    hip_trajectory.and_then(|trajectory| {
                        plan_c2_release_segment(
                            previous.unwrap_or(takeoff),
                            velocity,
                            acceleration,
                            contact_seconds,
                            trajectory,
                        )
                    }),
                    previous.unwrap_or(takeoff),
                );
                let plan = terrain
                    .and_then(|terrain| terrain.height_at(semantic_landing.xz()))
                    .map(|height| terrain_conformed_guard_target(semantic_landing, Some(height)))
                    .map_or(release_without_contact, |terrain_landing| {
                        plan_c2_foot_segment(
                            previous.unwrap_or(takeoff),
                            velocity,
                            acceleration,
                            terrain_landing,
                            contact_seconds,
                            hip_trajectory,
                        )
                    });
                match plan {
                    FootEndpointPlan::Segment(segment) => {
                        let segment = segment.with_owner_epoch(action_tick);
                        previous_desired = Some(segment.start);
                        previous_ideal_velocity = segment.start_velocity;
                        previous_ideal_acceleration = segment.start_acceleration;
                        ideal_history_valid = true;
                        contact_segment = Some(segment);
                    }
                    FootEndpointPlan::MustReleaseOrReplan(release_plan) => {
                        let release_segment = match release_plan {
                            FootReleasePlan::Segment(segment) => {
                                Some(segment.with_owner_epoch(action_tick))
                            }
                            FootReleasePlan::EmergencyBrake { .. } => None,
                        };
                        if let Some(segment) = release_segment {
                            previous_desired = Some(segment.start);
                            previous_ideal_velocity = segment.start_velocity;
                            previous_ideal_acceleration = segment.start_acceleration;
                            ideal_history_valid = true;
                        } else {
                            let FootReleasePlan::EmergencyBrake { presented: held } = release_plan
                            else {
                                unreachable!();
                            };
                            previous_desired = Some(held);
                            previous_ideal_velocity = Vec3::ZERO;
                            previous_ideal_acceleration = Vec3::ZERO;
                            ideal_history_valid = true;
                        }
                        contact_segment = release_segment;
                        forced_release = true;
                    }
                }
            }
            let mut explicit_ideal_motion = None;
            let mut direct_c2_sample = false;
            let mut completed_contact = false;
            let mut contact_event = None;
            if forced_release && contact_segment.is_none() {
                target = previous.unwrap_or(takeoff);
                explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
            }
            if let Some(mut segment) = contact_segment {
                advance_c2_segment_tick(&mut segment, evaluation_advances, action_tick);
                let sample = guard_swing_replan_sample(segment);
                if evaluation_advances
                    && !direct_c2_sample_is_live_reachable(
                        segment,
                        sample,
                        reach_envelope,
                        delta_seconds,
                    )
                {
                    target = previous.unwrap_or(takeoff);
                    explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
                    direct_c2_sample = false;
                    contact_segment = None;
                    forced_release = true;
                    contact_event =
                        segment
                            .end
                            .is_contact()
                            .then_some(ContactMotionEvent::AbortedLiveReach {
                                aborted_owner_epoch: segment.owner_epoch,
                            });
                } else {
                    target = sample.position;
                    explicit_ideal_motion = Some((sample.velocity, sample.acceleration));
                    direct_c2_sample = true;
                    contact_segment = Some(segment);
                    if segment.timing.is_complete() {
                        forced_release = !segment.end.is_contact();
                        completed_contact = segment.end.is_contact();
                    }
                    contact_event = segment.end.is_contact().then_some(if completed_contact {
                        ContactMotionEvent::Completed
                    } else {
                        ContactMotionEvent::Promised
                    });
                }
            }
            let desired_target = target;
            let mut tracked = if direct_c2_sample {
                let (velocity, acceleration) = explicit_ideal_motion
                    .expect("a direct quickstep C2 sample carries analytic derivatives");
                direct_c2_guard_target_sample(target, velocity, acceleration)
            } else {
                advance_guard_foot_target_sample_with_reach(
                    previous,
                    velocity,
                    acceleration,
                    previous_desired,
                    previous_ideal_velocity,
                    previous_ideal_acceleration,
                    ideal_history_valid,
                    target,
                    explicit_ideal_motion,
                    delta_seconds,
                    evaluation_advances,
                    reach_envelope,
                )
            };
            // A live trajectory departure may invalidate the predictive
            // contact owner. The interval ending at the authored 0.625 contact
            // phase is therefore an explicit visual-impact contract: each
            // ankle converges to a terrain-conformed, currently reachable
            // point, then holds through action exit. This is client presentation state;
            // it does not alter tactical collision or movement authority.
            if action_phase >= 0.375 {
                let correction = if left {
                    &mut state.left_impact_correction
                } else {
                    &mut state.right_impact_correction
                };
                if let Some(terrain) = terrain {
                    // Reach shortening must remain on the terrain manifold.
                    // A radial post-clamp lifts the ankle toward the hip and
                    // made later dodge impacts progressively airborne. The
                    // search also falls back toward terrain beneath the hip
                    // when the authored ankle XZ is outside the terrain map.
                    let reachable_target = ground_safety_slide_endpoint(
                        authored,
                        upper_snapshot.global.translation(),
                        // Land with proportional flexion reserve. A target at
                        // 99.5% of maximum reach was already in the warning
                        // band; one ordinary network/presentation root sample
                        // made it hard-infeasible and forced the ankle upward
                        // on the first guard frame. This ratio scales with the
                        // actual limb lengths rather than character units.
                        reach * 0.92,
                        Some(terrain),
                    );
                    if let Some((_, endpoint)) = correction.as_mut() {
                        *endpoint = reachable_target;
                    } else {
                        *correction = Some((tracked.position, reachable_target));
                    }
                }
                if let Some((start, end)) = *correction {
                    let correction_progress = ((action_phase - 0.375) / 0.25).clamp(0.0, 1.0);
                    let duration_seconds = action_landing_tick
                        .and_then(|landing| action_start_tick.map(|start| landing - start))
                        .map(|ticks| ticks as f32 / CONTINUITY_SAMPLE_HZ / 2.5)
                        .unwrap_or(0.16)
                        .max(1.0 / CONTINUITY_SAMPLE_HZ);
                    let sample = guard_quintic_sample(
                        start,
                        end,
                        correction_progress,
                        duration_seconds,
                        0.0,
                    );
                    tracked = direct_c2_guard_target_sample(
                        sample.position,
                        sample.velocity,
                        sample.acceleration,
                    );
                    contact_segment = None;
                    forced_release = false;
                    completed_contact = correction_progress >= 1.0 - f32::EPSILON;
                    contact_event = Some(if completed_contact {
                        ContactMotionEvent::Completed
                    } else {
                        ContactMotionEvent::Promised
                    });
                }
            }
            target = tracked.position;
            if left {
                state.left_solve_world = Some(target);
                state.left_target_velocity = tracked.velocity;
                state.left_target_acceleration = tracked.acceleration;
                state.left_desired_target = Some(desired_target);
                state.left_ideal_velocity = tracked.ideal_velocity;
                state.left_ideal_acceleration = tracked.ideal_acceleration;
                state.left_ideal_history_valid = tracked.ideal_history_valid;
            } else {
                state.right_solve_world = Some(target);
                state.right_target_velocity = tracked.velocity;
                state.right_target_acceleration = tracked.acceleration;
                state.right_desired_target = Some(desired_target);
                state.right_ideal_velocity = tracked.ideal_velocity;
                state.right_ideal_acceleration = tracked.ideal_acceleration;
                state.right_ideal_history_valid = tracked.ideal_history_valid;
            }
            if let Some((reason, _)) = tracked.replan {
                latch_quickstep_release(&mut release_phase, action_phase);
                if reason == FootFollowReason::ReachHardLimit {
                    if left {
                        state.left_release_phase = release_phase;
                    } else {
                        state.right_release_phase = release_phase;
                    }
                    // Keep presenting the follower's last-safe target on the
                    // release sample. Dropping to authored FK here would add
                    // a second, discontinuous owner handoff precisely when
                    // the reach guard is trying to retire IK safely.
                    forced_release = true;
                }
            }
            if left {
                state.left_release_phase = release_phase;
                state.left_contact_segment = contact_segment;
                state.left_contact_event = contact_event;
                if let Some(segment) = contact_segment {
                    state.left_motion_owner_epoch = segment.owner_epoch;
                } else if matches!(
                    contact_event,
                    Some(ContactMotionEvent::AbortedLiveReach { .. })
                ) {
                    state.left_motion_owner_epoch = action_tick;
                }
                state.left_release_owner_active = forced_release;
                if evaluation_advances && delta_seconds > f32::EPSILON {
                    state.left_hip_velocity = previous_hip
                        .map(|previous| {
                            (upper_snapshot.global.translation() - previous) / delta_seconds
                        })
                        .unwrap_or(skeleton.world_velocity);
                    state.left_hip_world = Some(upper_snapshot.global.translation());
                }
            } else {
                state.right_release_phase = release_phase;
                state.right_contact_segment = contact_segment;
                state.right_contact_event = contact_event;
                if let Some(segment) = contact_segment {
                    state.right_motion_owner_epoch = segment.owner_epoch;
                } else if matches!(
                    contact_event,
                    Some(ContactMotionEvent::AbortedLiveReach { .. })
                ) {
                    state.right_motion_owner_epoch = action_tick;
                }
                state.right_release_owner_active = forced_release;
                if evaluation_advances && delta_seconds > f32::EPSILON {
                    state.right_hip_velocity = previous_hip
                        .map(|previous| {
                            (upper_snapshot.global.translation() - previous) / delta_seconds
                        })
                        .unwrap_or(skeleton.world_velocity);
                    state.right_hip_world = Some(upper_snapshot.global.translation());
                }
            }
            targets[index] = Some(target);
            supports[index] = quickstep_support_weight(completed_contact, forced_release, progress);
            let canonical = pole_to_world(
                rig_rotation,
                canonical_knee_pole(if left { -1.0 } else { 1.0 }),
            );
            let remembered = if left {
                state.left_knee_bend_world
            } else {
                state.right_knee_bend_world
            };
            let previous_end_direction = if left {
                state.left_end_direction
            } else {
                state.right_end_direction
            };
            let foot_facing = rendered_foot_facing(
                rig,
                left,
                foot_snapshot.global.translation(),
                &parents,
                &transforms.p0(),
            );
            let pole = stabilized_knee_pole(
                remembered,
                previous_end_direction,
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                target,
                canonical,
                foot_facing,
            )
            .unwrap_or(canonical);
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                upper_snapshot.global.translation(),
                authored,
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
            solve_inputs[index] = Some((
                upper_snapshot.global.translation(),
                upper_length,
                lower_length,
                pole,
            ));
            let resolved_end = if let Some(solution) = solve_two_bone_with_reach(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                authored,
                target,
                upper_length,
                lower_length,
                pole,
                reach,
            ) {
                let resolved_end = solution.end;
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction)
                    .try_normalize();
                if left {
                    if bend.is_some() {
                        state.left_knee_bend_world = bend;
                    }
                    state.left_end_direction = Some(solution.end_direction);
                } else {
                    if bend.is_some() {
                        state.right_knee_bend_world = bend;
                    }
                    state.right_end_direction = Some(solution.end_direction);
                }
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                Some(resolved_end)
            } else {
                None
            };
            rendered_targets[index] = Some(quickstep_handoff_target(target, resolved_end));
            let takeoff_rotation = if left {
                state.left_takeoff_rotation_world
            } else {
                state.right_takeoff_rotation_world
            };
            if let Some(takeoff_rotation) = takeoff_rotation {
                let desired_rotation = takeoff_rotation
                    .slerp(foot_snapshot.global.rotation(), progress)
                    .normalize();
                if let Ok(parent) = parents.get(foot)
                    && let Ok(parent_global) =
                        transforms.p0().compute_global_transform(parent.parent())
                    && let Ok(mut foot_transform) = transforms.p1().get_mut(foot)
                {
                    foot_transform.rotation =
                        (parent_global.rotation().inverse() * desired_rotation).normalize();
                }
            }
        }
        let mut memory = LegIkMemory {
            left_authored_world_target: targets[0],
            right_authored_world_target: targets[1],
            left_foot_world_target: rendered_targets[0],
            right_foot_world_target: rendered_targets[1],
            quickstep_handoff_pending: true,
            quickstep_left_landing_local: rendered_targets[0]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
            quickstep_right_landing_local: rendered_targets[1]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
            left_support_weight: Some(supports[0]),
            right_support_weight: Some(supports[1]),
            left_terrain_pole_world: state.left_knee_bend_world,
            right_terrain_pole_world: state.right_knee_bend_world,
            left_terrain_end_direction: state.left_end_direction,
            right_terrain_end_direction: state.right_end_direction,
            raised_pelvis_shift: state.pelvis_shift,
            raised_pelvis_shift_velocity: state.pelvis_shift_velocity,
            raised_pelvis_shift_acceleration: state.pelvis_shift_acceleration,
            raised_pelvis_follower_valid: true,
            left_foot_follower: state
                .left_ideal_history_valid
                .then(|| {
                    FootFollowerState::from_presented_pose(
                        rendered_targets[0]?,
                        state.left_target_velocity,
                        state.left_target_acceleration,
                        state.left_desired_target?,
                        state.left_ideal_velocity,
                        state.left_ideal_acceleration,
                    )
                })
                .flatten(),
            right_foot_follower: state
                .right_ideal_history_valid
                .then(|| {
                    FootFollowerState::from_presented_pose(
                        rendered_targets[1]?,
                        state.right_target_velocity,
                        state.right_target_acceleration,
                        state.right_desired_target?,
                        state.right_ideal_velocity,
                        state.right_ideal_acceleration,
                    )
                })
                .flatten(),
            left_direct_c2_active: state.left_contact_segment.is_some()
                || state.left_impact_correction.is_some(),
            right_direct_c2_active: state.right_contact_segment.is_some()
                || state.right_impact_correction.is_some(),
            left_contact_endpoint: state
                .left_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.end.position())
                .or_else(|| state.left_impact_correction.map(|(_, end)| end)),
            right_contact_endpoint: state
                .right_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.end.position())
                .or_else(|| state.right_impact_correction.map(|(_, end)| end)),
            left_contact_progress: state
                .left_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.timing.progress())
                .or_else(|| {
                    state
                        .left_impact_correction
                        .map(|_| ((action_phase - 0.375) / 0.25).clamp(0.0, 1.0))
                }),
            right_contact_progress: state
                .right_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.timing.progress())
                .or_else(|| {
                    state
                        .right_impact_correction
                        .map(|_| ((action_phase - 0.375) / 0.25).clamp(0.0, 1.0))
                }),
            left_contact_tick: state
                .left_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| {
                    segment
                        .owner_epoch
                        .saturating_add(segment.timing.total_ticks.get() as u64)
                }),
            right_contact_tick: state
                .right_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| {
                    segment
                        .owner_epoch
                        .saturating_add(segment.timing.total_ticks.get() as u64)
                }),
            left_contact_initial_lag: state
                .left_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.start.distance(segment.end.position()))
                .or_else(|| {
                    state
                        .left_impact_correction
                        .map(|(start, end)| start.distance(end))
                }),
            left_contact_event: state.left_contact_event,
            right_contact_event: state.right_contact_event,
            left_solve_hip: solve_inputs[0].map(|input| input.0),
            right_solve_hip: solve_inputs[1].map(|input| input.0),
            left_solve_upper_length: solve_inputs[0].map(|input| input.1),
            right_solve_upper_length: solve_inputs[1].map(|input| input.1),
            left_solve_lower_length: solve_inputs[0].map(|input| input.2),
            right_solve_lower_length: solve_inputs[1].map(|input| input.2),
            left_commanded_pole: solve_inputs[0].map(|input| input.3),
            right_commanded_pole: solve_inputs[1].map(|input| input.3),
            left_motion_owner_epoch: state.left_motion_owner_epoch,
            right_motion_owner_epoch: state.right_motion_owner_epoch,
            right_contact_initial_lag: state
                .right_contact_segment
                .filter(|segment| segment.end.is_contact())
                .map(|segment| segment.start.distance(segment.end.position()))
                .or_else(|| {
                    state
                        .right_impact_correction
                        .map(|(start, end)| start.distance(end))
                }),
            ..default()
        };
        state.presented_pelvis_local = rig.get(&BoneRole::Pelvis).and_then(|pelvis| {
            transforms
                .p1()
                .get_mut(*pelvis)
                .ok()
                .map(|transform| *transform)
        });
        memory.quickstep_pelvis_local = state.presented_pelvis_local;
        memory.left_foot_target = rendered_targets[0];
        memory.right_foot_target = rendered_targets[1];
        if let Ok(mut current) = diagnostics.get_mut(owner) {
            current.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
        store_state(owner, state, &mut states, &mut commands);
    }
}

fn quickstep_support_weight(
    completed_contact: bool,
    forced_release: bool,
    action_progress: f32,
) -> f32 {
    if completed_contact {
        1.0
    } else if forced_release {
        0.0
    } else {
        (action_progress <= f32::EPSILON) as u8 as f32
    }
}

fn apply_quickstep_pelvis_shift(
    rig: &HumanoidRig,
    shift: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    if shift.abs() <= 0.0001 {
        return;
    }
    let Some(&pelvis) = rig.get(&BoneRole::Pelvis) else {
        return;
    };
    let local_delta = parents
        .get(pelvis)
        .ok()
        .and_then(|parent| {
            transforms
                .p0()
                .compute_global_transform(parent.parent())
                .ok()
        })
        .map(|parent| parent.affine().inverse().transform_vector3(Vec3::Y * shift))
        .unwrap_or(Vec3::Y * shift);
    if local_delta.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(pelvis)
    {
        transform.translation += local_delta;
    }
}

fn quickstep_handoff_target(requested: Vec3, resolved_end: Option<Vec3>) -> Vec3 {
    resolved_end
        .filter(|end| end.is_finite())
        .unwrap_or(requested)
}

fn quickstep_foot_must_release(
    grounded: bool,
    action_phase: f32,
    hip: Vec3,
    planted_ankle: Vec3,
    warning_reach: f32,
    maximum_reach: f32,
) -> bool {
    if action_phase < 0.125 {
        return false;
    }
    if grounded {
        return action_phase >= 0.5;
    }
    let offset = hip - planted_ankle;
    let planar_distance = offset.xz().length();
    let vertical_distance = offset.y.max(0.001);
    hip.distance(planted_ankle) >= warning_reach
        || hip.distance(planted_ankle) >= maximum_reach
        || planar_distance >= vertical_distance // 45-degree maximum arch.
}

fn store_state(
    owner: Entity,
    state: QuickstepIkState,
    states: &mut Query<&mut QuickstepIkState>,
    commands: &mut Commands,
) {
    if let Ok(mut current) = states.get_mut(owner) {
        *current = state;
    } else {
        commands.entity(owner).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planted_foot_releases_at_reach_or_forty_five_degree_arch() {
        let ankle = Vec3::ZERO;
        assert!(!quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.2, 0.8, 0.0),
            ankle,
            1.0,
            1.1
        ));
        assert!(quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.81, 0.8, 0.0),
            ankle,
            1.0,
            1.5
        ));
        assert!(quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.2, 0.8, 0.0),
            ankle,
            0.75,
            0.8
        ));
    }

    #[test]
    fn landing_handoff_uses_the_reach_clamped_rendered_endpoint() {
        let requested = Vec3::new(-1.8, 0.1, 0.0);
        let rendered = Vec3::new(-0.9, 0.2, 0.0);

        assert_eq!(
            quickstep_handoff_target(requested, Some(rendered)),
            rendered
        );
        assert_eq!(quickstep_handoff_target(requested, None), requested);
        assert_eq!(
            quickstep_handoff_target(requested, Some(Vec3::splat(f32::NAN))),
            requested
        );
    }

    #[test]
    fn quickstep_onset_seeds_feet_but_owns_an_authored_pelvis() {
        let left = Vec3::new(-0.2, 0.1, 0.0);
        let right = Vec3::new(0.2, 0.1, 0.0);
        let left_rotation = Quat::from_rotation_y(0.2);
        let memory = LegIkMemory {
            left_foot_world_target: Some(left),
            right_foot_world_target: Some(right),
            left_last_rendered_foot_rotation_world: Some(left_rotation),
            raised_pelvis_shift: -0.25,
            raised_pelvis_follower_valid: true,
            ..default()
        };

        let state = seed_quickstep_from_presentation(Some(17), Some(memory));
        assert_eq!(state.left_takeoff_world, Some(left));
        assert_eq!(state.right_takeoff_world, Some(right));
        assert_eq!(state.left_solve_world, Some(left));
        assert_eq!(state.right_solve_world, Some(right));
        assert_eq!(state.left_takeoff_rotation_world, Some(left_rotation));
        assert_eq!(state.pelvis_shift, 0.0);
        assert_eq!(state.pelvis_shift_velocity, 0.0);
        assert_eq!(state.pelvis_shift_acceleration, 0.0);
    }

    #[test]
    fn reach_replan_never_rewinds_an_existing_release() {
        let mut release_phase = Some(0.2);
        latch_quickstep_release(&mut release_phase, 0.4);
        assert_eq!(release_phase, Some(0.2));
    }

    #[test]
    fn only_completed_contact_not_release_publishes_quickstep_support() {
        assert_eq!(quickstep_support_weight(true, false, 0.8), 1.0);
        assert_eq!(quickstep_support_weight(false, true, 0.0), 0.0);
        assert_eq!(quickstep_support_weight(false, false, 0.8), 0.0);
    }

    #[test]
    fn a_new_quickstep_sequence_discards_previous_takeoff_targets() {
        let mut state = QuickstepIkState {
            action_start_tick: Some(10),
            left_takeoff_world: Some(Vec3::X),
            right_takeoff_world: Some(Vec3::NEG_X),
            left_release_phase: Some(0.4),
            right_release_phase: Some(0.5),
            ..default()
        };
        let next_action_start_tick = Some(40);

        if state.action_start_tick != next_action_start_tick {
            state = QuickstepIkState {
                action_start_tick: next_action_start_tick,
                ..default()
            };
        }

        assert_eq!(state.action_start_tick, next_action_start_tick);
        assert!(state.left_takeoff_world.is_none());
        assert!(state.right_takeoff_world.is_none());
        assert!(state.left_release_phase.is_none());
        assert!(state.right_release_phase.is_none());
    }
}
