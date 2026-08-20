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
const MINIMUM_ATTACK_PRESENTATION_PREPARATION_TICKS: u64 = 20;

/// Client-only locomotion state used for rendering between replicated server
/// samples. Gameplay semantics and presentation events continue to read the
/// authoritative `SkeletonState` directly.
#[derive(Component, Debug, Clone)]
pub(crate) struct PresentedSkeleton {
    pub(crate) state: SkeletonState,
    source_tick: u64,
    presentation_tick: Option<u64>,
    raised_phase_ahead: f32,
    pub(crate) phase_error_remaining: f32,
    pub(crate) last_phase_prediction_delta: f32,
    pub(crate) last_phase_correction_delta: f32,
    pub(crate) last_phase_measurement_error: Option<f32>,
    pub(crate) last_phase_source_changed: bool,
    action_clock: Option<PresentedActionClock>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PresentedActionSpec {
    Dodge {
        direction: Vec2,
        preparation_ticks: u64,
    },
    Attack {
        target_height: f32,
        animation: AttackAnimation,
        preparation_ticks: u64,
    },
    Block {
        incoming_line: AttackLine,
        preparation_ticks: u64,
    },
}

impl PresentedActionSpec {
    fn same_visual_action(self, other: Self) -> bool {
        match (self, other) {
            (Self::Dodge { direction: a, .. }, Self::Dodge { direction: b, .. }) => {
                a.distance_squared(b) <= 1.0e-6
            }
            (
                Self::Attack {
                    target_height: a,
                    animation: a_animation,
                    ..
                },
                Self::Attack {
                    target_height: b,
                    animation: b_animation,
                    ..
                },
            ) => a_animation == b_animation && (a - b).abs() <= 1.0e-4,
            (
                Self::Block {
                    incoming_line: a, ..
                },
                Self::Block {
                    incoming_line: b, ..
                },
            ) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PresentedActionClock {
    spec: PresentedActionSpec,
    source_start_tick: u64,
    elapsed_ticks: u64,
    completed: bool,
    terminal_contact_ticks_remaining: u8,
}

impl PresentedActionClock {
    fn new(spec: PresentedActionSpec, source_start_tick: u64) -> Self {
        Self {
            spec,
            source_start_tick,
            elapsed_ticks: 0,
            completed: false,
            // Dodge landing is a contact contract, not merely the final
            // authored sample. Keep that exact pose for one additional
            // semantic tick so lower-body ownership transfers after impact.
            terminal_contact_ticks_remaining: u8::from(matches!(
                spec,
                PresentedActionSpec::Dodge { .. }
            )),
        }
    }

    fn dodge_terminal_tick(self) -> Option<u64> {
        match self.spec {
            PresentedActionSpec::Dodge {
                preparation_ticks, ..
            } => Some(preparation_ticks.saturating_mul(2)),
            _ => None,
        }
    }
}

impl PresentedSkeleton {
    pub(crate) fn new(state: SkeletonState, presentation_tick: Option<u64>) -> Self {
        let source_tick = state.locomotion_sample_tick;
        Self {
            state,
            source_tick,
            presentation_tick,
            raised_phase_ahead: 0.0,
            phase_error_remaining: 0.0,
            last_phase_prediction_delta: 0.0,
            last_phase_correction_delta: 0.0,
            last_phase_measurement_error: None,
            last_phase_source_changed: false,
            action_clock: None,
        }
    }

    pub(crate) fn action_presentation_tick(&self) -> u64 {
        self.action_clock
            .map_or(self.state.locomotion_sample_tick, |clock| {
                clock.elapsed_ticks
            })
    }

    pub(crate) fn cadence_diagnostic_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "source_tick": self.source_tick,
            "presentation_tick": self.presentation_tick,
            "source_age_ticks": self.presentation_tick.map(|tick| tick.saturating_sub(self.source_tick)),
            "raised_phase_ahead": self.raised_phase_ahead,
            "authoritative_sample_tick": self.state.locomotion_sample_tick,
            "raised_step_sequence": self.state.raised_locomotion().step_sequence(),
            "raised_swing_foot": format!("{:?}", self.state.raised_locomotion().swing_foot()),
            "raised_phase": self.state.gait_phase,
        })
    }
}

fn presented_action_spec(state: &SkeletonState) -> Option<(PresentedActionSpec, u64)> {
    match state.action_view()? {
        ActionView::Dodge {
            direction,
            timeline,
        } => Some((
            PresentedActionSpec::Dodge {
                direction,
                preparation_ticks: timeline.preparation_ticks,
            },
            timeline.start_tick,
        )),
        ActionView::Attack {
            target_height,
            animation,
            timeline,
        } => Some((
            PresentedActionSpec::Attack {
                target_height,
                animation,
                preparation_ticks: timeline
                    .preparation_ticks
                    .max(MINIMUM_ATTACK_PRESENTATION_PREPARATION_TICKS),
            },
            timeline.start_tick,
        )),
        ActionView::Block {
            incoming_line,
            timeline,
        } => Some((
            PresentedActionSpec::Block {
                incoming_line,
                preparation_ticks: timeline.preparation_ticks,
            },
            timeline.start_tick,
        )),
    }
}

fn apply_presented_action(state: &mut SkeletonState, clock: PresentedActionClock) {
    state.clear_action_for_presentation();
    let start = 0;
    let admitted = match clock.spec {
        PresentedActionSpec::Dodge {
            direction,
            preparation_ticks,
        } => state.begin_dodge(
            DodgeSpec { direction },
            start,
            start.saturating_add(preparation_ticks),
        ),
        PresentedActionSpec::Attack {
            target_height,
            animation,
            preparation_ticks,
        } => state.begin_attack(
            AttackSpec {
                target_height,
                animation,
            },
            start,
            start.saturating_add(preparation_ticks),
        ),
        PresentedActionSpec::Block {
            incoming_line,
            preparation_ticks,
        } => state.begin_block(
            BlockSpec { incoming_line },
            start,
            start.saturating_add(preparation_ticks),
        ),
    };
    debug_assert!(admitted.is_ok());
    state.advance_action(clock.elapsed_ticks);
}

fn advance_presented_action(
    clock: &mut Option<PresentedActionClock>,
    previous: &SkeletonState,
    authoritative: &SkeletonState,
    next: &mut SkeletonState,
    delta_seconds: f32,
) {
    let authoritative_action = presented_action_spec(authoritative);
    let mut newly_started = false;
    match (*clock, authoritative_action) {
        (None, Some((spec, source_start_tick))) => {
            *clock = Some(PresentedActionClock::new(spec, source_start_tick));
            newly_started = true;
        }
        (Some(mut active), Some((spec, source_start_tick)))
            if active.spec.same_visual_action(spec) =>
        {
            // A server echo can replace the locally predicted start tick. It
            // is still the same visual action and must not restart or jump.
            active.source_start_tick = source_start_tick;
            *clock = Some(active);
        }
        (Some(_), Some((spec, source_start_tick))) => {
            *clock = Some(PresentedActionClock::new(spec, source_start_tick));
            newly_started = true;
        }
        (Some(active), None) if active.completed => *clock = None,
        (Some(_), None) | (None, None) => {}
    }

    let Some(mut active) = *clock else {
        return;
    };
    if !newly_started && !active.completed {
        let elapsed = (delta_seconds * LOCOMOTION_SAMPLE_HZ).round().max(1.0) as u64;
        active.elapsed_ticks = active.elapsed_ticks.saturating_add(elapsed);
    }
    let reached_dodge_contact = active
        .dodge_terminal_tick()
        .is_some_and(|terminal| active.elapsed_ticks >= terminal);
    if let Some(terminal) = active.dodge_terminal_tick() {
        active.elapsed_ticks = active.elapsed_ticks.min(terminal);
    }
    if active.completed {
        next.clear_action_for_presentation();
    } else {
        apply_presented_action(next, active);
        if reached_dodge_contact {
            if active.terminal_contact_ticks_remaining == 0 {
                active.completed = true;
            } else {
                active.terminal_contact_ticks_remaining -= 1;
            }
        } else {
            active.completed = next.action_kind() == SkeletonAction::None;
        }
    }
    // A local action remains visually authoritative through sparse or early
    // authoritative retirement. The previous parameter documents that this
    // is a presentation handoff, not a gameplay-state mutation.
    let _ = previous;
    *clock = Some(active);
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
    let authoritative_phase = authoritative.gait_phase;
    let authoritative_raised = authoritative.raised_locomotion();

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
        let newly_presented_phase = if source_changed {
            circular_phase_delta(authoritative_phase, next.gait_phase).max(0.0)
        } else {
            circular_phase_delta(previous.gait_phase, next.gait_phase).max(0.0)
        };
        presented.raised_phase_ahead = if source_changed {
            newly_presented_phase
        } else {
            presented.raised_phase_ahead + newly_presented_phase
        };
        predict_raised_cadence_identity(
            authoritative_phase,
            authoritative_raised,
            presented.raised_phase_ahead,
            &mut next,
        );
    } else {
        presented.phase_error_remaining = 0.0;
        presented.raised_phase_ahead = 0.0;
    }

    advance_presented_action(
        &mut presented.action_clock,
        &previous,
        authoritative,
        &mut next,
        delta_seconds,
    );

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
    procedural_clock: Res<ProceduralAnimationClock>,
    mut players: Query<(Entity, &SkeletonState, Option<&mut PresentedSkeleton>), With<Player>>,
) {
    for (entity, authoritative, presented) in &mut players {
        let Some(mut presented) = presented else {
            commands.entity(entity).insert(PresentedSkeleton::new(
                authoritative.clone(),
                Some(procedural_clock.semantic_step().0),
            ));
            continue;
        };
        let (tick, semantic_delta) = procedural_clock.semantic_step();
        let advances = presented.presentation_tick != Some(tick);
        if advances {
            presented.presentation_tick = Some(tick);
            advance_presented_skeleton(&mut presented, authoritative, semantic_delta);
        }
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
            .init_resource::<JointJitterDiagnostics>()
            .add_message::<LocomotionPresentationEvent>()
            .add_systems(Startup, request_animation_packs)
            .add_observer(on_successful_attack)
            .add_systems(
                Update,
                (
                    collect_loaded_packs,
                    attach_loaded_rig_scenes,
                    procedural::advance_procedural_animation_clock,
                    update_presented_skeletons,
                    establish_animation_targets,
                    procedural::bind_humanoid_bones,
                    procedural::cache_humanoid_rigs,
                    capture_authored_bind_transforms,
                    procedural::capture_humanoid_rig_axes,
                    semantic_route::evaluate_semantic_route_paths,
                    evaluate_skeletons,
                    tick_impact_reactions,
                    pose_buffer::update_pose_buffers,
                    jitter::advance_jitter_diagnostic_clock,
                    update_rig_visibility,
                    emit_locomotion_presentation_events,
                    trace_locomotion_presentation_events,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (
                    pose_buffer::apply_pose_buffers,
                    restore_authored_bind_pose,
                    procedural::stabilize_repeated_fixed_tick_pose,
                    jitter::sample_authored_pose_jitter,
                    procedural::apply_pose_mirroring,
                    procedural::stabilize_locomotion_torso,
                    procedural::apply_landing_leg_compression,
                    procedural::apply_locomotion_body_response,
                    procedural::apply_jump_charge_crouch,
                    procedural::apply_head_and_torso_look,
                    procedural::apply_impact_reaction,
                    procedural::apply_ordinary_locomotion_ik,
                    procedural::apply_terrain_leg_ik,
                    procedural::apply_quickstep_ik,
                    procedural::enforce_anatomical_knee_yaw,
                    procedural::apply_arm_and_weapon_constraints,
                    jitter::sample_final_pose_jitter,
                )
                    .chain()
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (
                    procedural::refresh_raised_support_after_propagation,
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

fn predict_raised_cadence_identity(
    authoritative_phase: f32,
    authoritative_intent: RaisedLocomotionIntent,
    presented_phase_ahead: f32,
    next: &mut SkeletonState,
) {
    if next.weapon_guard() != WeaponGuardState::Raised || !next.raised_locomotion().is_moving() {
        return;
    }
    // Never replace a newly replicated sequence/swing with the preceding
    // predicted identity. Predict only the bounded forward phase remaining
    // beyond this authoritative sample.
    let mut intent = authoritative_intent;
    let handoffs = (((authoritative_phase + presented_phase_ahead.max(0.0)) * 2.0).floor()
        - (authoritative_phase * 2.0).floor())
    .max(0.0) as u32;
    if handoffs % 2 == 1 {
        let swing = match intent.swing_foot().unwrap_or(next.lead_foot()) {
            LeadFoot::Left => LeadFoot::Right,
            LeadFoot::Right => LeadFoot::Left,
        };
        intent = RaisedLocomotionIntent::moving(
            intent.local_direction(),
            intent.speed(),
            swing,
            intent.step_sequence().wrapping_add(handoffs),
        );
    } else if handoffs > 0 {
        intent = RaisedLocomotionIntent::moving(
            intent.local_direction(),
            intent.speed(),
            intent.swing_foot().unwrap_or(next.lead_foot()),
            intent.step_sequence().wrapping_add(handoffs),
        );
    }
    *next = next.clone().with_raised_locomotion(intent);
}

#[cfg(test)]
mod schedule_contract_tests {
    #[test]
    fn repeated_authored_input_is_restored_before_procedural_evaluation() {
        let source = include_str!("presentation.rs");
        let bind_restore = source.find("restore_authored_bind_pose,").unwrap();
        let fixed_input = source
            .find("procedural::stabilize_repeated_fixed_tick_pose,")
            .unwrap();
        let authored_sample = source.find("jitter::sample_authored_pose_jitter,").unwrap();
        let first_procedural = source.find("procedural::apply_pose_mirroring,").unwrap();
        let final_sample = source.find("jitter::sample_final_pose_jitter,").unwrap();
        assert!(bind_restore < fixed_input);
        assert!(fixed_input < authored_sample);
        assert!(authored_sample < first_procedural);
        assert!(fixed_input < final_sample);
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
