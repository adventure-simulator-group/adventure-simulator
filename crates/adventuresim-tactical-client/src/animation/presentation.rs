use super::*;

pub struct TacticalAnimationPlugin;

pub(super) const PRESENTATION_VELOCITY_RESPONSE_PER_SECOND: f32 = 18.0;
// Replicated phase samples naturally arrive a few render frames behind the
// presentation clock. Treat that delivery-time offset as jitter unless it
// persists beyond a meaningful fraction of a quarter-cycle.
pub(super) const PRESENTATION_PHASE_CORRECTION_RATE_PER_SECOND: f32 = 0.05;
pub(super) const PRESENTATION_PHASE_DRIFT_DEADBAND: f32 = 0.04;
pub(super) const PRESENTATION_PHASE_DRIFT_MEASUREMENT_BLEND: f32 = 0.15;
pub(super) const PRESENTATION_PHASE_SNAP_ERROR: f32 = 0.20;
pub(super) const MAX_PRESENTATION_SOURCE_GAP_TICKS: u64 = 32;

/// Client-only locomotion state used for rendering between replicated server
/// samples. Gameplay semantics and presentation events continue to read the
/// authoritative `SkeletonState` directly.
#[derive(Component, Debug, Clone)]
pub(crate) struct PresentedSkeleton {
    pub(crate) state: SkeletonState,
    source_tick: u64,
    presentation_tick: Option<u64>,
    pub(crate) phase_error_remaining: f32,
    pub(crate) last_phase_prediction_delta: f32,
    pub(crate) last_phase_correction_delta: f32,
    pub(crate) last_phase_measurement_error: Option<f32>,
    pub(crate) last_phase_source_changed: bool,
    authored_cadence: Option<AuthoredCadence>,
    quickstep_phase: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthoredCadenceKind {
    Ordinary,
    Combat,
}

#[derive(Debug, Clone, Copy)]
struct AuthoredCadence {
    kind: AuthoredCadenceKind,
    phase: f32,
}

impl PresentedSkeleton {
    pub(super) fn new(state: SkeletonState, presentation_tick: Option<u64>) -> Self {
        let source_tick = state.locomotion_sample_tick;
        Self {
            state,
            source_tick,
            presentation_tick,
            phase_error_remaining: 0.0,
            last_phase_prediction_delta: 0.0,
            last_phase_correction_delta: 0.0,
            last_phase_measurement_error: None,
            last_phase_source_changed: false,
            authored_cadence: None,
            quickstep_phase: None,
        }
    }
}

impl std::ops::Deref for PresentedSkeleton {
    type Target = SkeletonState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

pub(super) fn circular_phase_delta(from: f32, to: f32) -> f32 {
    (to - from + 0.5).rem_euclid(1.0) - 0.5
}

pub(super) fn can_predict_locomotion(
    previous: &SkeletonState,
    authoritative: &SkeletonState,
) -> bool {
    let action_is_predictable = authoritative.action_kind() == SkeletonAction::None
        || (authoritative.weapon_guard() == WeaponGuardState::Raised
            && authoritative.action_kind() == SkeletonAction::Attack);
    authoritative.is_surface_supported()
        && action_is_predictable
        && authoritative.animation_speed() > 0.05
        && previous.posture() == authoritative.posture()
        && previous.weapon_guard() == authoritative.weapon_guard()
        && previous.action_kind() == authoritative.action_kind()
        && previous.is_surface_supported() == authoritative.is_surface_supported()
        && previous.animation_pack == authoritative.animation_pack
        && previous.contact_sequence == authoritative.contact_sequence
        && same_guard_step(previous, authoritative)
}

fn same_guard_step(previous: &SkeletonState, authoritative: &SkeletonState) -> bool {
    match (
        previous.raised_footwork().step(),
        authoritative.raised_footwork().step(),
    ) {
        (Some(previous), Some(authoritative)) => {
            previous.swing_foot() == authoritative.swing_foot()
                && previous.start_tick() == authoritative.start_tick()
                && previous.contact_tick() == authoritative.contact_tick()
        }
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn advance_presented_skeleton(
    presented: &mut PresentedSkeleton,
    authoritative: &SkeletonState,
    delta_seconds: f32,
) {
    advance_presented_skeleton_with_strides(
        presented,
        authoritative,
        delta_seconds,
        &AuthoredLocomotionStrides::default(),
    );
}

fn advance_presented_skeleton_with_strides(
    presented: &mut PresentedSkeleton,
    authoritative: &SkeletonState,
    delta_seconds: f32,
    authored_strides: &AuthoredLocomotionStrides,
) {
    let delta_seconds = delta_seconds.clamp(0.0, 0.1);
    let source_changed = presented.source_tick != authoritative.locomotion_sample_tick;
    presented.last_phase_prediction_delta = 0.0;
    presented.last_phase_correction_delta = 0.0;
    presented.last_phase_measurement_error = None;
    presented.last_phase_source_changed = source_changed;
    let source_gap = authoritative
        .locomotion_sample_tick
        .checked_sub(presented.source_tick);
    let previous = presented.state.clone();
    let mut next = authoritative.clone();

    let source_is_contiguous =
        source_gap.is_some_and(|gap| gap <= MAX_PRESENTATION_SOURCE_GAP_TICKS);
    if source_is_contiguous && can_predict_locomotion(&previous, authoritative) {
        let response = 1.0 - (-PRESENTATION_VELOCITY_RESPONSE_PER_SECOND * delta_seconds).exp();
        next.local_velocity = previous
            .local_velocity
            .lerp(authoritative.local_velocity, response);
        next.world_velocity = previous
            .world_velocity
            .lerp(authoritative.world_velocity, response);

        let prediction_delta = if next.weapon_guard() == WeaponGuardState::Raised
            && let Some(step) = next.raised_footwork().step()
        {
            let duration_ticks = step.contact_tick().saturating_sub(step.start_tick()).max(1);
            delta_seconds * LOCOMOTION_SAMPLE_HZ * 0.5 / duration_ticks as f32
        } else {
            let speed = presentation_phase_speed(&next);
            gait_cycle_phase_delta(locomotion_profile(&next), speed, delta_seconds)
        };
        let predicted = (previous.gait_phase + prediction_delta).rem_euclid(1.0);
        presented.last_phase_prediction_delta = prediction_delta;
        let error = circular_phase_delta(predicted, authoritative.gait_phase);
        let discontinuous = error.abs() > PRESENTATION_PHASE_SNAP_ERROR;
        if source_changed && discontinuous {
            next.gait_phase = authoritative.gait_phase;
            presented.phase_error_remaining = 0.0;
            presented.last_phase_measurement_error = Some(error);
        } else {
            if source_changed {
                presented.last_phase_measurement_error = Some(error);
                let measured_drift = if error.abs() <= PRESENTATION_PHASE_DRIFT_DEADBAND {
                    0.0
                } else {
                    error - error.signum() * PRESENTATION_PHASE_DRIFT_DEADBAND
                };
                // Keep one continuous correction accumulator. Replacing it
                // with every packet's error makes packet timing modulate the
                // displayed gait speed even when physical speed is constant.
                presented.phase_error_remaining += (measured_drift
                    - presented.phase_error_remaining)
                    * PRESENTATION_PHASE_DRIFT_MEASUREMENT_BLEND;
            }
            let maximum_correction = PRESENTATION_PHASE_CORRECTION_RATE_PER_SECOND * delta_seconds;
            let correction = presented
                .phase_error_remaining
                .clamp(-maximum_correction, maximum_correction);
            next.gait_phase = (predicted + correction).rem_euclid(1.0);
            presented.phase_error_remaining -= correction;
            presented.last_phase_correction_delta = correction;
        }
    } else {
        presented.phase_error_remaining = 0.0;
    }

    advance_presented_quickstep(
        &previous,
        authoritative,
        &mut next,
        &mut presented.quickstep_phase,
        delta_seconds,
    );
    apply_authored_cadence(
        &previous,
        &mut next,
        &mut presented.authored_cadence,
        authored_strides,
        delta_seconds,
    );
    presented.state = next;
    presented.source_tick = authoritative.locomotion_sample_tick;
}

/// Keep authored quicksteps on a render-time timeline. Authoritative tactical
/// ticks can be coalesced between render observations (including phase 0.13 →
/// 1.0); copying that discontinuity directly skips the authored frame-3 tuck.
/// Gameplay still owns action admission/contact. Presentation only bounds how
/// quickly the already-authoritative action phase may advance visually.
fn advance_presented_quickstep(
    previous: &SkeletonState,
    authoritative: &SkeletonState,
    next: &mut SkeletonState,
    phase: &mut Option<f32>,
    delta_seconds: f32,
) {
    let continuing = previous.is_quickstep();
    if !authoritative.is_quickstep() && !continuing {
        *phase = None;
        return;
    }
    if !authoritative.is_quickstep() && phase.is_some_and(|phase| phase >= 1.0) {
        *phase = None;
        return;
    }

    let source = if authoritative.is_quickstep() {
        authoritative
    } else {
        previous
    };
    let preparation_ticks = source.action_preparation_ticks().unwrap_or(1).max(1);
    let entering = phase.is_none() && !previous.is_quickstep() && authoritative.is_quickstep();
    let visual_phase = phase.get_or_insert_with(|| {
        if previous.is_quickstep() {
            previous.action_phase().clamp(0.0, 1.0)
        } else {
            source.action_phase().clamp(0.0, 1.0)
        }
    });
    if !entering {
        *visual_phase = (*visual_phase
            + delta_seconds.max(0.0) * LOCOMOTION_SAMPLE_HZ / (2 * preparation_ticks) as f32)
            .min(1.0);
    }

    *next = source.clone();
    let start_tick = source.action_start_tick().unwrap_or(0);
    let elapsed_ticks = (*visual_phase * (2 * preparation_ticks) as f32).round() as u64;
    next.advance_action(
        start_tick
            .saturating_add(elapsed_ticks)
            .min(start_tick.saturating_add(2 * preparation_ticks)),
    );
}

fn apply_authored_cadence(
    previous: &SkeletonState,
    next: &mut SkeletonState,
    cadence: &mut Option<AuthoredCadence>,
    strides: &AuthoredLocomotionStrides,
    delta_seconds: f32,
) {
    if next.posture() != Posture::Upright || next.is_quickstep() {
        *cadence = None;
        return;
    }
    let kind =
        if next.weapon_guard() == WeaponGuardState::Raised && !next.guarded_sprint_locomotion() {
            AuthoredCadenceKind::Combat
        } else {
            AuthoredCadenceKind::Ordinary
        };
    let speed = next.animation_speed();
    let stride = match kind {
        AuthoredCadenceKind::Ordinary => strides.ordinary(speed),
        AuthoredCadenceKind::Combat => strides.combat(next.raised_locomotion().local_direction()),
    };
    let continuing = cadence.filter(|current| current.kind == kind);
    if stride.is_none() && !(speed <= 0.05 && continuing.is_some()) {
        *cadence = None;
        return;
    }
    let mut phase = continuing.map_or(previous.gait_phase, |current| current.phase);
    if speed > 0.05
        && let Some(stride) = stride
    {
        phase = (phase + speed * delta_seconds.max(0.0) / (stride.max(0.01) * 2.0)).rem_euclid(1.0);
    }
    next.gait_phase = phase;
    *cadence = Some(AuthoredCadence { kind, phase });
}

fn presentation_phase_speed(skeleton: &SkeletonState) -> f32 {
    let speed = skeleton.animation_speed();
    if skeleton.downed_turning() {
        speed * 2.0
    } else {
        match skeleton.body() {
            // Keep presentation prediction identical to the authoritative
            // projector. Prone crawl contacts track physical travel directly;
            // retaining the old two-thirds multiplier here made every server
            // sample correct the displayed phase and visibly cut the crawl.
            BodyState::Prone => speed,
            BodyState::Supine => speed * (2.0 / 3.0),
            _ => speed,
        }
    }
}

pub(super) fn update_presented_skeletons(
    mut commands: Commands,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    authored_strides: Res<AuthoredLocomotionStrides>,
    mut players: Query<(Entity, &SkeletonState, Option<&mut PresentedSkeleton>), With<Player>>,
) {
    for (entity, authoritative, presented) in &mut players {
        let Some(mut presented) = presented else {
            commands.entity(entity).insert(PresentedSkeleton::new(
                authoritative.clone(),
                procedural_clock.fixed_step().map(|(tick, _)| tick),
            ));
            continue;
        };
        let delta_seconds = if let Some((tick, fixed_delta)) = procedural_clock.fixed_step() {
            let advances = presented.presentation_tick != Some(tick);
            presented.presentation_tick = Some(tick);
            if advances { fixed_delta } else { 0.0 }
        } else {
            time.delta_secs()
        };
        advance_presented_skeleton_with_strides(
            &mut presented,
            authoritative,
            delta_seconds,
            &authored_strides,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocomotionPresentationEventKind {
    Contact(LeadFoot),
    Landing,
}

#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct LocomotionPresentationEvent {
    pub owner: Entity,
    pub sequence: u64,
    /// Authoritative sample tick when this sequence state was observed. For a
    /// coalesced gap this is not a reconstructed historical contact time.
    pub sample_tick: u64,
    pub kind: LocomotionPresentationEventKind,
}

pub(super) const MAX_COALESCED_PRESENTATION_EVENTS: u64 = 8;

pub(super) fn bounded_forward_sequence_delta(previous: u64, current: u64) -> Option<u64> {
    current
        .checked_sub(previous)
        .filter(|delta| *delta <= MAX_COALESCED_PRESENTATION_EVENTS)
}

pub(super) fn latest_coalesced_landing(previous: u64, current: u64) -> Option<u64> {
    bounded_forward_sequence_delta(previous, current)
        .filter(|delta| *delta > 0)
        .map(|_| current)
}

pub(super) fn coalesced_contacts(
    previous: u64,
    current: u64,
    latest_foot: LeadFoot,
) -> Option<Vec<(u64, LeadFoot)>> {
    let delta = bounded_forward_sequence_delta(previous, current)?;
    Some(
        (0..delta)
            .rev()
            .map(|offset| {
                let foot = if offset % 2 == 0 {
                    latest_foot
                } else {
                    match latest_foot {
                        LeadFoot::Left => LeadFoot::Right,
                        LeadFoot::Right => LeadFoot::Left,
                    }
                };
                (current - offset, foot)
            })
            .collect(),
    )
}

#[derive(Component, Debug, Clone, Copy, Default)]
struct LocomotionEventCursor {
    initialized: bool,
    contact_sequence: u64,
    landing_sequence: u64,
}

impl Plugin for TacticalAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimationPackCatalog>()
            .init_resource::<pose_buffer::PoseBufferMetrics>()
            .init_resource::<pose_buffer::RigDefinitions>()
            .init_resource::<pose_buffer::BakedClipBank>()
            .init_resource::<AuthoredLocomotionStrides>()
            .init_resource::<AnimationRuntime>()
            .init_resource::<semantic_route::SemanticRouteTelemetry>()
            .init_resource::<TerrainIkEnabled>()
            .init_resource::<ProceduralAnimationClock>()
            .init_resource::<procedural::FixedTickPoseCache>()
            .init_resource::<secondary_physics::SecondaryPhysicsTelemetry>()
            .register_required_components::<procedural::HumanoidBone, secondary_physics::SecondaryBoneDynamics>()
            .add_message::<LocomotionPresentationEvent>()
            .add_systems(Startup, request_animation_packs)
            .add_observer(on_successful_attack)
            .add_systems(
                Update,
                (
                    collect_loaded_packs,
                    attach_loaded_rig_scenes,
                    update_presented_skeletons,
                    establish_animation_targets,
                    procedural::bind_humanoid_bones,
                    procedural::cache_humanoid_rigs,
                    full_ragdoll::sync_full_ragdolls,
                    full_ragdoll::resolve_ragdoll_terrain_contacts,
                    capture_authored_bind_transforms,
                    procedural::capture_humanoid_rig_axes,
                    semantic_route::evaluate_semantic_route_paths,
                    evaluate_skeletons,
                    tick_impact_reactions,
                    pose_buffer::update_pose_buffers,
                    pose_buffer::calibrate_authored_locomotion_strides,
                    update_rig_visibility,
                    emit_locomotion_presentation_events,
                    trace_locomotion_presentation_events,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (
                    procedural::restore_procedural_look_base,
                    pose_buffer::apply_pose_buffers,
                    restore_authored_bind_pose,
                    procedural::apply_pose_mirroring,
                    procedural::apply_procedural_dive_lower_body,
                    procedural::apply_locomotion_height,
                    procedural::orient_guarded_run_lower_body,
                    procedural::apply_landing_leg_compression,
                    procedural::apply_locomotion_body_response,
                    procedural::apply_jump_anticipation,
                    procedural::apply_head_and_torso_look,
                    secondary_physics::apply_secondary_bone_physics,
                    procedural::apply_terrain_leg_ik,
                    procedural::enforce_anatomical_knee_yaw,
                    procedural::apply_arm_and_weapon_constraints,
                    full_ragdoll::apply_full_ragdoll_pose,
                    procedural::stabilize_repeated_fixed_tick_pose,
                )
                    .chain()
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (
                    procedural::refresh_raised_support_after_propagation,
                    // Diagnostics must observe the final global transforms
                    // that the renderer receives, including procedural IK.
                    log_animation_diagnostics,
                )
                    .chain()
                    .after(TransformSystems::Propagate),
            );
    }
}

pub(super) fn trace_locomotion_presentation_events(
    mut messages: MessageReader<LocomotionPresentationEvent>,
) {
    for message in messages.read() {
        trace!(
            owner = ?message.owner,
            sequence = message.sequence,
            sample_tick = message.sample_tick,
            kind = ?message.kind,
            "locomotion presentation event"
        );
    }
}

fn emit_locomotion_presentation_events(
    mut commands: Commands,
    mut messages: MessageWriter<LocomotionPresentationEvent>,
    mut skeletons: Query<(Entity, &SkeletonState, Option<&mut LocomotionEventCursor>)>,
) {
    for (owner, skeleton, cursor) in &mut skeletons {
        let mut next = cursor.as_deref().copied().unwrap_or_default();
        if !next.initialized {
            next.initialized = true;
            next.contact_sequence = skeleton.contact_sequence;
            next.landing_sequence = skeleton.landing_sequence;
        } else {
            if let Some(contacts) = coalesced_contacts(
                next.contact_sequence,
                skeleton.contact_sequence,
                skeleton.contact_foot,
            ) {
                for (sequence, foot) in contacts {
                    messages.write(LocomotionPresentationEvent {
                        owner,
                        sequence,
                        sample_tick: skeleton.locomotion_sample_tick,
                        kind: LocomotionPresentationEventKind::Contact(foot),
                    });
                }
            }
            if let Some(sequence) =
                latest_coalesced_landing(next.landing_sequence, skeleton.landing_sequence)
            {
                messages.write(LocomotionPresentationEvent {
                    owner,
                    sequence,
                    sample_tick: skeleton.locomotion_sample_tick,
                    kind: LocomotionPresentationEventKind::Landing,
                });
            }
            next.contact_sequence = skeleton.contact_sequence;
            next.landing_sequence = skeleton.landing_sequence;
        }
        if let Some(mut cursor) = cursor {
            *cursor = next;
        } else {
            commands.entity(owner).insert(next);
        }
    }
}

#[cfg(test)]
mod authored_cadence_tests {
    use super::*;

    #[test]
    fn authored_walk_stride_controls_visual_cycle_rate() {
        let state = SkeletonState::default().with_local_velocity(Vec3::NEG_Z * 2.0);
        let mut presented = PresentedSkeleton::new(state.clone(), None);
        let strides = AuthoredLocomotionStrides {
            walk: Some(1.0),
            run: Some(2.0),
            ..default()
        };

        advance_presented_skeleton_with_strides(&mut presented, &state, 0.1, &strides);

        // One cycle owns two one-metre steps: 2 m/s advances 0.1 cycle in 0.1 s.
        assert!((presented.gait_phase - 0.1).abs() < 0.0001);
    }

    #[test]
    fn diagonal_combat_cadence_blends_strafe_and_skip_stride() {
        let state = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_local_velocity(Vec3::new(1.0, 0.0, 1.0))
            .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::ONE, 2.0));
        let mut presented = PresentedSkeleton::new(state.clone(), None);
        let strides = AuthoredLocomotionStrides {
            strafe: Some(0.25),
            skip: Some(0.75),
            ..default()
        };

        advance_presented_skeleton_with_strides(&mut presented, &state, 0.1, &strides);

        assert!((presented.gait_phase - 0.2).abs() < 0.0001);
    }

    #[test]
    fn coalesced_quickstep_phase_still_traverses_the_authored_tuck() {
        let mut authoritative =
            SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        authoritative
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 10, 20)
            .unwrap();
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);

        // Simulate a coalesced authoritative observation that jumps directly
        // to contact/recovery complete. Presentation must still visit frame 3,
        // the midpoint of the authoritative 0/3/6 source timeline.
        authoritative.advance_action(30);
        let strides = AuthoredLocomotionStrides::default();
        for _ in 0..15 {
            advance_presented_skeleton_with_strides(
                &mut presented,
                &authoritative,
                1.0 / LOCOMOTION_SAMPLE_HZ,
                &strides,
            );
        }
        let evaluation = AnimationEvaluation::from_skeleton(&presented.state);
        assert!((presented.action_phase() - 0.75).abs() < 0.051);
        assert!(evaluation.lower_body.iter().any(|sample| matches!(
            sample.sampling,
            PoseSampling::Timeline { progress } if (progress - 0.5).abs() < 0.11
        )));
    }

    #[test]
    fn released_guard_cannot_replace_presented_quickstep_before_frame_six() {
        let mut authoritative =
            SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        authoritative
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 10, 20)
            .unwrap();
        authoritative.advance_action(20);
        let mut presented = PresentedSkeleton::new(authoritative.clone(), None);
        let strides = AuthoredLocomotionStrides::default();

        // The server has already observed landing and guard release. The
        // presentation timeline still owns the complete authored airborne
        // output before it may route back to ordinary/combat locomotion.
        authoritative.advance_action(31);
        authoritative = authoritative.with_weapon_guard(WeaponGuardState::Lowered);
        authoritative = authoritative.with_local_velocity(Vec3::X * 2.0);

        let initial = AnimationEvaluation::from_skeleton(&presented.state);
        let mut progresses = initial
            .lower_body
            .iter()
            .filter_map(|sample| match sample.sampling {
                PoseSampling::Timeline { progress } => Some(progress),
                _ => None,
            })
            .collect::<Vec<_>>();
        while presented.is_quickstep() {
            advance_presented_skeleton_with_strides(
                &mut presented,
                &authoritative,
                1.0 / LOCOMOTION_SAMPLE_HZ,
                &strides,
            );
            let evaluation = AnimationEvaluation::from_skeleton(&presented.state);
            if presented.is_quickstep() {
                assert!(!evaluation.lower_body.is_empty());
                for sample in &evaluation.lower_body {
                    assert_eq!(sample.pose, SemanticPose::QuickstepRightTakeoff);
                    let PoseSampling::Timeline { progress } = sample.sampling else {
                        panic!("quickstep lower body must remain timeline sampled")
                    };
                    progresses.push(progress);
                }
            }
            assert!(progresses.len() < 64, "presented quickstep did not finish");
        }

        assert!(progresses.first().is_some_and(|progress| *progress <= 0.01));
        assert!(
            progresses
                .iter()
                .any(|progress| (*progress - 0.5).abs() <= 0.11)
        );
        assert!(progresses.last().is_some_and(|progress| *progress >= 0.99));
        let released = AnimationEvaluation::from_skeleton(&presented.state);
        assert!(released.lower_body.iter().all(|sample| !matches!(
            sample.pose,
            SemanticPose::QuickstepRightTakeoff
                | SemanticPose::StrafeCycle
                | SemanticPose::SkipCycle
        )));
    }
}
