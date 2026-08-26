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
    authoritative.is_surface_supported()
        && authoritative.action_kind() == SkeletonAction::None
        && authoritative.animation_speed() > 0.05
        && previous.posture() == authoritative.posture()
        && previous.weapon_guard() == authoritative.weapon_guard()
        && previous.action_kind() == authoritative.action_kind()
        && previous.is_surface_supported() == authoritative.is_surface_supported()
        && previous.animation_pack == authoritative.animation_pack
}

pub(super) fn advance_presented_skeleton(
    presented: &mut PresentedSkeleton,
    authoritative: &SkeletonState,
    delta_seconds: f32,
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

        let speed = presentation_phase_speed(&next);
        let prediction_delta =
            gait_cycle_phase_delta(locomotion_profile(&next), speed, delta_seconds);
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

    presented.state = next;
    presented.source_tick = authoritative.locomotion_sample_tick;
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
        advance_presented_skeleton(&mut presented, authoritative, delta_seconds);
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
            .init_resource::<AnimationRuntime>()
            .init_resource::<semantic_route::SemanticRouteTelemetry>()
            .init_resource::<TerrainIkEnabled>()
            .init_resource::<ProceduralAnimationClock>()
            .init_resource::<procedural::FixedTickPoseCache>()
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
                    procedural::stabilize_locomotion_torso,
                    procedural::apply_landing_leg_compression,
                    procedural::apply_locomotion_body_response,
                    procedural::apply_jump_anticipation,
                    procedural::apply_head_and_torso_look,
                    secondary_physics::apply_secondary_bone_physics,
                    procedural::apply_ordinary_locomotion_ik,
                    procedural::apply_terrain_leg_ik,
                    procedural::apply_quickstep_ik,
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
            )
            .add_systems(Update, super::diagnostics::report_system_spikes);
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
