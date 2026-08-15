use std::collections::{BTreeMap, BTreeSet, HashSet};

use adventuresim_tactical_core::prelude::*;
#[cfg(not(target_family = "wasm"))]
use adventuresim_tactical_netcode::message::PlayerInputRequest;
use adventuresim_tactical_netcode::message::SuccessfulAttackResponse;
use bevy::{
    animation::{AnimatedBy, AnimationTargetId},
    app::AnimationSystems,
    asset::LoadState,
    ecs::hierarchy::ChildSpawnerCommands,
    gltf::Gltf,
    prelude::*,
};
use clap::ValueEnum;

pub(crate) mod pose_buffer;
mod procedural;
#[cfg(all(not(target_family = "wasm"), feature = "animation-graph-physics"))]
pub(crate) mod ragdoll;

#[allow(unused_imports)]
pub(crate) use procedural::{
    ArmIkState, AttackFootworkState, BoneRole, HandIkTarget, HandSide, HeldWeaponConstraint,
    HumanoidBone, HumanoidIkTargets, HumanoidRig, LegIkDiagnostics, LegIkState,
    LocomotionBodyResponseState, LocomotionHeightState, MEASURED_ANKLE_SOLE_OFFSET_METRES,
    ProceduralAnimationClock, RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES,
    locomotion_support_weights,
};
const HUMANOID_UNARMED_PACK: &str = "humanoid_unarmed";
const BIPED_BASE_GLB: &str = "animations/biped/unarmed/base.glb";
const ANIMATION_FPS: f32 = 30.0;
const PRESENTATION_CROSSFADE_SECONDS: f32 = 0.18;
const DOWNED_PRESENTATION_CROSSFADE_SECONDS: f32 = 0.5;
const LOWER_BODY_MASK_GROUP: u32 = 0;
const UPPER_BODY_MASK_GROUP: u32 = 1;
// Player transforms sit at the center of the 1.9 m server collider, while
// authored rigs use a floor-level origin. Keep visual feet on the collider's
// lower face so the first-person camera lands at the authored head.
const PLAYER_VISUAL_Y_OFFSET: f32 = -0.95;

/// Client-only authored-pose playback implementation. Both variants consume
/// the same [`PresentedSkeleton`] and semantic animation-pack resolution.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum AnimationBackend {
    #[default]
    Graph,
    PoseBuffer,
}

impl AnimationBackend {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Graph => "graph",
            Self::PoseBuffer => "pose_buffer",
        }
    }
}

mod diagnostics;
use diagnostics::log_animation_diagnostics;
#[cfg(not(target_family = "wasm"))]
#[allow(unused_imports)] // Gameplay diagnostics consume these; the viewer target does not.
pub(crate) use diagnostics::{
    AnimationDiagnosticLog, DiagnosticInputStatus, RenderScheduleTelemetry,
};
pub(crate) mod semantic_graph;

fn animation_asset_path(path: &str) -> String {
    #[cfg(not(target_family = "wasm"))]
    {
        format!("workspace://{path}")
    }
    #[cfg(target_family = "wasm")]
    {
        path.to_owned()
    }
}

mod presentation;
pub(crate) use presentation::*;

/// Runtime switch for terrain height, slope, and pelvis conformity. This is on
/// by default; debug builds expose F8 to compare against authored FK.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainIkEnabled(pub bool);

impl Default for TerrainIkEnabled {
    fn default() -> Self {
        Self(true)
    }
}

pub(crate) mod catalog;
pub use catalog::AnimationPackCatalog;
use catalog::*;

#[derive(Debug, Clone)]
struct LoadedClip {
    handle: Handle<AnimationClip>,
    /// Dedicated graph node for continuous timeline playback. A Bevy graph
    /// node has one seek position, so sparse pose anchors use separate nodes
    /// below when two frames from the same source clip must be blended.
    node: AnimationNodeIndex,
    duration_seconds: f32,
    anchor_nodes: BTreeMap<u16, AnimationNodeIndex>,
    upper_node: AnimationNodeIndex,
    upper_anchor_nodes: BTreeMap<u16, AnimationNodeIndex>,
    lower_node: AnimationNodeIndex,
    lower_anchor_nodes: BTreeMap<u16, AnimationNodeIndex>,
    layer: ClipLayer,
}

impl LoadedClip {
    fn at_anchor(&self, frame: u16) -> Self {
        self.at_anchor_layer(frame, ClipLayer::Whole)
    }

    fn at_anchor_layer(&self, frame: u16, layer: ClipLayer) -> Self {
        let mut clip = self.clone();
        clip.layer = layer;
        clip.node = match layer {
            ClipLayer::Whole => self.anchor_nodes.get(&frame).copied().unwrap_or(self.node),
            ClipLayer::Upper => self
                .upper_anchor_nodes
                .get(&frame)
                .copied()
                .unwrap_or(self.upper_node),
            ClipLayer::Lower => self
                .lower_anchor_nodes
                .get(&frame)
                .copied()
                .unwrap_or(self.lower_node),
        };
        clip
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClipLayer {
    Whole,
    Upper,
    Lower,
}

#[derive(Resource, Default)]
pub(super) struct AnimationRuntime {
    requested_base: Option<Handle<Gltf>>,
    base_processed: bool,
    base_failed: bool,
    base_scene: Option<Handle<WorldAsset>>,
    requested_motions: BTreeMap<(String, String), Handle<Gltf>>,
    processed_motions: BTreeSet<(String, String)>,
    unavailable_motions: BTreeSet<(String, String)>,
    clip_handles: BTreeMap<(String, String), Handle<AnimationClip>>,
    clips: BTreeMap<(String, String), LoadedClip>,
    library: AnimationPackLibrary,
    graph: Option<Handle<AnimationGraph>>,
    revision: u64,
    canonical_targets: HashSet<AnimationTargetId>,
    lower_body_targets: HashSet<AnimationTargetId>,
    upper_body_targets: HashSet<AnimationTargetId>,
}

impl AnimationRuntime {
    pub(super) fn motion_is_processed(&self, motion: &str) -> bool {
        self.processed_motions
            .iter()
            .any(|(_, processed)| processed == motion)
    }
}

#[derive(Component, Debug)]
pub(super) struct AnimationPlayback {
    clips: Vec<WeightedClip>,
    use_authored_bind_pose: bool,
    pub(super) whole_body_mirror: f32,
    pub(super) foot_ik_weights: Vec2,
    weapon_guard: WeaponGuardState,
    ordinary_locomotion_active: bool,
    presentation_transition: Option<PlaybackTransition>,
    evaluation_tick: Option<u64>,
}

impl AnimationPlayback {
    #[allow(dead_code)] // Used by the standalone animation-viewer binary.
    pub(super) fn authored_pose_is_ready(&self) -> bool {
        !self.use_authored_bind_pose && !self.clips.is_empty()
    }

    pub(super) fn presentation_is_settled(&self) -> bool {
        self.authored_pose_is_ready() && self.presentation_transition.is_none()
    }
}

impl Default for AnimationPlayback {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            use_authored_bind_pose: true,
            whole_body_mirror: 0.0,
            foot_ik_weights: Vec2::ZERO,
            weapon_guard: WeaponGuardState::Lowered,
            ordinary_locomotion_active: false,
            presentation_transition: None,
            evaluation_tick: None,
        }
    }
}

#[derive(Debug, Clone)]
struct WeightedClip {
    clip: LoadedClip,
    weight: f32,
    time_seconds: f32,
    mirrored_weight: f32,
}

#[derive(Debug, Clone)]
struct PlaybackPose {
    clips: Vec<WeightedClip>,
    use_authored_bind_pose: bool,
    whole_body_mirror: f32,
    foot_ik_weights: Vec2,
}

#[derive(Debug, Clone)]
struct PlaybackTransition {
    from: PlaybackPose,
    elapsed_seconds: f32,
    duration_seconds: f32,
}

#[derive(Component)]
pub struct FallbackAnimationRig(pub Entity);

#[derive(Component)]
pub(super) struct AnimationRigScene(pub Entity);

#[derive(Component)]
struct AnimationRigAttached;

#[derive(Component)]
struct RigAnimationTargetsBound;

#[derive(Component)]
struct AnimationPlayerOwner(pub Entity);

#[derive(Component, Default)]
struct AnimationGraphRevision(u64);

#[derive(Component, Debug, Clone, Copy)]
struct AuthoredBindTransform {
    pub(super) owner: Entity,
    pub(super) local: Transform,
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ImpactReaction {
    pub(super) remaining: f32,
    pub(super) duration: f32,
    pub(super) strength: f32,
}

mod loading;
use loading::*;
fn evaluate_skeletons(
    mut commands: Commands,
    backend: Res<AnimationBackend>,
    catalog: Res<AnimationPackCatalog>,
    runtime: Res<AnimationRuntime>,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    players: Query<
        (
            Entity,
            &PresentedSkeleton,
            &semantic_graph::SemanticGraphTrace,
            Option<&mut AnimationPlayback>,
        ),
        With<Player>,
    >,
) {
    for (entity, skeleton, graph_trace, playback) in players {
        // This evaluation contains weights/progress decoded from the dependency
        // graph's returned Pose. The preceding chained system always inserts a
        // trace, including an explicit legacy fallback trace on graph errors.
        let evaluation = graph_trace.evaluation.clone();
        let samples = if evaluation.action.is_empty() {
            &evaluation.base
        } else {
            &evaluation.action
        };
        let coherent_guard_parity = samples
            .iter()
            .map(guard_locomotion_resolution_pose)
            .collect::<Option<Vec<_>>>()
            .map(|resolution_poses| {
                let exact = guard_parity_score(
                    &runtime,
                    &catalog,
                    &skeleton.animation_pack,
                    samples,
                    &resolution_poses,
                    false,
                );
                let mirrored = guard_parity_score(
                    &runtime,
                    &catalog,
                    &skeleton.animation_pack,
                    samples,
                    &resolution_poses,
                    true,
                );
                // Preserve the most requested directional semantics and guard
                // endpoints. Exact orientation wins a complete tie, retaining
                // exact cardinal authorship without sacrificing a coherent
                // opposite-side diagonal set.
                mirrored > exact
            });
        let mut weighted = Vec::<WeightedClip>::new();
        let base_layer = if evaluation.action.is_empty() && !evaluation.lower_body.is_empty() {
            ClipLayer::Upper
        } else {
            ClipLayer::Whole
        };
        for sample in samples {
            append_resolved_sample_layer(
                &mut weighted,
                &runtime,
                &catalog,
                &skeleton.animation_pack,
                *sample,
                coherent_guard_parity,
                base_layer,
            );
        }
        if evaluation.action.is_empty() {
            for sample in &evaluation.lower_body {
                append_resolved_sample_layer(
                    &mut weighted,
                    &runtime,
                    &catalog,
                    &skeleton.animation_pack,
                    *sample,
                    None,
                    ClipLayer::Lower,
                );
            }
        }
        let target = PlaybackPose {
            use_authored_bind_pose: weighted.is_empty(),
            whole_body_mirror: {
                let total = weighted.iter().map(|clip| clip.weight).sum::<f32>();
                if total > f32::EPSILON {
                    (weighted
                        .iter()
                        .map(|clip| clip.mirrored_weight)
                        .sum::<f32>()
                        / total)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                }
            },
            foot_ik_weights: graph_foot_ik_weights(&evaluation),
            clips: weighted,
        };
        let ordinary_locomotion_candidate = ordinary_locomotion_candidate(skeleton);
        if let Some(mut playback) = playback {
            // Hysteresis prevents sub-threshold velocity noise from repeatedly
            // restarting the idle/locomotion presentation transition.
            let locomotion_threshold = if playback.ordinary_locomotion_active {
                0.03
            } else {
                0.08
            };
            let ordinary_locomotion_active =
                ordinary_locomotion_candidate && skeleton.animation_speed() > locomotion_threshold;
            if *backend == AnimationBackend::Graph {
                update_presentation_crossfade(
                    &mut playback,
                    target,
                    skeleton.weapon_guard(),
                    ordinary_locomotion_active,
                    if skeleton.body().is_downed() {
                        DOWNED_PRESENTATION_CROSSFADE_SECONDS
                    } else {
                        PRESENTATION_CROSSFADE_SECONDS
                    },
                    &procedural_clock,
                    time.delta_secs(),
                );
            } else {
                // Pose-buffer playback captures the visible joint pose and
                // velocity when this plan changes. Feeding it an outgoing
                // clip blend would double-smooth the transition and retime
                // authoritative gait/action phases.
                playback.presentation_transition = None;
                playback.weapon_guard = skeleton.weapon_guard();
                playback.ordinary_locomotion_active = ordinary_locomotion_active;
                apply_playback_pose(&mut playback, target);
            }
        } else {
            let ordinary_locomotion_active =
                ordinary_locomotion_candidate && skeleton.animation_speed() > 0.05;
            commands.entity(entity).insert(AnimationPlayback {
                clips: target.clips,
                use_authored_bind_pose: target.use_authored_bind_pose,
                whole_body_mirror: target.whole_body_mirror,
                foot_ik_weights: target.foot_ik_weights,
                weapon_guard: skeleton.weapon_guard(),
                ordinary_locomotion_active,
                presentation_transition: None,
                evaluation_tick: procedural_clock.fixed_step().map(|(tick, _)| tick),
            });
        }
    }
}

fn ordinary_locomotion_candidate(skeleton: &SkeletonState) -> bool {
    !skeleton.is_posture_transitioning()
        && ((skeleton.is_grounded()
            && skeleton.action_kind() == SkeletonAction::None
            && skeleton.weapon_guard() == WeaponGuardState::Lowered)
            || skeleton.downed_turning())
}

fn update_presentation_crossfade(
    playback: &mut AnimationPlayback,
    target: PlaybackPose,
    weapon_guard: WeaponGuardState,
    ordinary_locomotion_active: bool,
    transition_duration_seconds: f32,
    procedural_clock: &ProceduralAnimationClock,
    render_delta_seconds: f32,
) {
    let delta_seconds = match procedural_clock.fixed_step() {
        Some((tick, _)) if playback.evaluation_tick == Some(tick) => 0.0,
        Some((tick, delta_seconds)) => {
            playback.evaluation_tick = Some(tick);
            delta_seconds
        }
        None => render_delta_seconds.max(0.0),
    };
    let guard_changed = playback.weapon_guard != weapon_guard;
    let locomotion_changed =
        playback.ordinary_locomotion_active != ordinary_locomotion_active && !guard_changed;
    let transition_started = guard_changed || locomotion_changed;
    if transition_started {
        playback.presentation_transition = Some(PlaybackTransition {
            from: playback_pose(playback),
            elapsed_seconds: 0.0,
            duration_seconds: transition_duration_seconds.max(f32::EPSILON),
        });
        playback.weapon_guard = weapon_guard;
    }
    playback.ordinary_locomotion_active = ordinary_locomotion_active;

    let mut transition_complete = false;
    let resolved = if let Some(transition) = playback.presentation_transition.as_mut() {
        // The state change is first presented at the exact old pose. Time
        // begins accumulating on the next simulation/render observation, so
        // a replicated guard or locomotion edge cannot consume the preceding
        // frame's dt.
        if !transition_started {
            transition.elapsed_seconds += delta_seconds;
        }
        let progress = (transition.elapsed_seconds / transition.duration_seconds).clamp(0.0, 1.0);
        let pose = blend_playback_poses(&transition.from, &target, progress);
        transition_complete = progress >= 1.0;
        pose
    } else {
        target
    };
    if transition_complete {
        playback.presentation_transition = None;
    }
    apply_playback_pose(playback, resolved);
}

fn playback_pose(playback: &AnimationPlayback) -> PlaybackPose {
    PlaybackPose {
        clips: playback.clips.clone(),
        use_authored_bind_pose: playback.use_authored_bind_pose,
        whole_body_mirror: playback.whole_body_mirror,
        foot_ik_weights: playback.foot_ik_weights,
    }
}

fn apply_playback_pose(playback: &mut AnimationPlayback, pose: PlaybackPose) {
    playback.clips = pose.clips;
    playback.use_authored_bind_pose = pose.use_authored_bind_pose;
    playback.whole_body_mirror = pose.whole_body_mirror;
    playback.foot_ik_weights = pose.foot_ik_weights;
}

fn blend_playback_poses(from: &PlaybackPose, to: &PlaybackPose, progress: f32) -> PlaybackPose {
    let progress = progress.clamp(0.0, 1.0);
    let mut clips = Vec::new();
    append_scaled_playback_clips(&mut clips, &from.clips, 1.0 - progress);
    append_scaled_playback_clips(&mut clips, &to.clips, progress);
    PlaybackPose {
        use_authored_bind_pose: clips.is_empty(),
        clips,
        whole_body_mirror: from.whole_body_mirror.lerp(to.whole_body_mirror, progress),
        foot_ik_weights: from.foot_ik_weights.lerp(to.foot_ik_weights, progress),
    }
}

/// IK ownership follows the dependency graph's returned locomotion samples.
/// The curve shapes are animation metadata: walk keeps one supported foot,
/// run has genuine intervals with neither foot loaded, and idle loads both.
fn graph_foot_ik_weights(evaluation: &AnimationEvaluation) -> Vec2 {
    let samples = if evaluation.action.is_empty() {
        &evaluation.base
    } else {
        return Vec2::ZERO;
    };
    let mut result = Vec2::ZERO;
    let mut total = 0.0;
    for sample in samples {
        let mut weights = match (sample.pose, sample.sampling) {
            (SemanticPose::IdleRelaxed | SemanticPose::CrouchIdle, _) => Vec2::ONE,
            (SemanticPose::WalkContact, PoseSampling::Cycle { phase }) => {
                let (left, right) = gait_support_weights(WALK_LOCOMOTION_PROFILE, phase);
                Vec2::new(left, right)
            }
            (SemanticPose::RunContact, PoseSampling::Cycle { phase }) => {
                // Simulation support spans the complete stance interval. IK
                // is animation metadata and should only become strong near
                // the authored contact itself, after the foot has descended.
                let contact_profile = LocomotionProfile {
                    support_phase_radius: 0.11,
                    ..RUN_LOCOMOTION_PROFILE
                };
                let (left, right) = gait_support_weights(contact_profile, phase);
                Vec2::new(left, right)
            }
            _ => Vec2::ZERO,
        };
        if sample.mirror_lower_body {
            weights = Vec2::new(weights.y, weights.x);
        }
        result += weights * sample.weight;
        total += sample.weight;
    }
    if total > f32::EPSILON {
        result / total
    } else {
        Vec2::ZERO
    }
    .clamp(Vec2::ZERO, Vec2::ONE)
}

fn append_scaled_playback_clips(
    combined: &mut Vec<WeightedClip>,
    source: &[WeightedClip],
    scale: f32,
) {
    if scale <= f32::EPSILON {
        return;
    }
    for source_clip in source {
        let weight = source_clip.weight * scale;
        if weight <= f32::EPSILON {
            continue;
        }
        if let Some(existing) = combined.iter_mut().find(|existing| {
            existing.clip.node == source_clip.clip.node
                && (existing.time_seconds - source_clip.time_seconds).abs() < 0.0001
        }) {
            existing.weight += weight;
            existing.mirrored_weight += source_clip.mirrored_weight * scale;
        } else {
            combined.push(WeightedClip {
                clip: source_clip.clip.clone(),
                weight,
                time_seconds: source_clip.time_seconds,
                mirrored_weight: source_clip.mirrored_weight * scale,
            });
        }
    }
}

fn capture_authored_bind_transforms(
    mut commands: Commands,
    nodes: Query<(Entity, &Transform), (Added<Transform>, Without<AuthoredBindTransform>)>,
    parents: Query<&ChildOf>,
    roots: Query<&AnimationRigScene>,
) {
    for (entity, transform) in &nodes {
        let mut current = entity;
        for _ in 0..64 {
            if let Ok(root) = roots.get(current) {
                commands.entity(entity).insert(AuthoredBindTransform {
                    owner: root.0,
                    local: *transform,
                });
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

fn reset_authored_bind_before_fk(
    playbacks: Query<&AnimationPlayback>,
    mut nodes: Query<(&AuthoredBindTransform, &mut Transform)>,
) {
    for (bind, mut transform) in &mut nodes {
        if playbacks.get(bind.owner).is_ok() {
            *transform = bind.local;
        }
    }
}

fn restore_authored_bind_pose(
    playbacks: Query<&AnimationPlayback>,
    mut nodes: Query<(&AuthoredBindTransform, &mut Transform)>,
) {
    for (bind, mut transform) in &mut nodes {
        if playbacks
            .get(bind.owner)
            .is_ok_and(|playback| playback.use_authored_bind_pose)
        {
            *transform = bind.local;
        }
    }
}

fn on_successful_attack(event: On<SuccessfulAttackResponse>, mut commands: Commands) {
    let strength = (event.total_damage() / 100.0).clamp(0.15, 1.0);
    for entity in &event.hit {
        commands.entity(*entity).insert(ImpactReaction {
            remaining: 0.22,
            duration: 0.22,
            strength,
        });
    }
}

fn tick_impact_reactions(
    mut commands: Commands,
    time: Res<Time>,
    mut reactions: Query<(Entity, &mut ImpactReaction)>,
) {
    for (entity, mut reaction) in &mut reactions {
        reaction.remaining -= time.delta_secs();
        if reaction.remaining <= 0.0 {
            commands.entity(entity).remove::<ImpactReaction>();
        }
    }
}

#[derive(Clone)]
struct ResolvedAnchor<'a> {
    clip: &'a LoadedClip,
    anchor: &'a PoseAnchor,
    pack_id: &'a str,
    semantic: SemanticPose,
    mirrored: bool,
}

fn resolve_anchor<'a>(
    runtime: &'a AnimationRuntime,
    catalog: &'a AnimationPackCatalog,
    requested_pack: &str,
    pose: SemanticPose,
) -> Option<ResolvedAnchor<'a>> {
    let ResolvedPose::Clip {
        pack_id,
        pose: resolved_pose,
        mirrored,
        ..
    } = runtime.library.resolve(requested_pack, pose)
    else {
        return None;
    };
    let pack = catalog.packs.get(pack_id)?;
    let anchor = pack.poses.get(&resolved_pose)?;
    pack.motions.get(&anchor.motion)?;
    let clip = runtime
        .clips
        .get(&(pack_id.to_owned(), anchor.motion.clone()))?;
    Some(ResolvedAnchor {
        clip,
        anchor,
        pack_id,
        semantic: resolved_pose,
        mirrored,
    })
}

fn select_gait_endpoint_parity<'a>(
    runtime: &'a AnimationRuntime,
    resolved: ResolvedAnchor<'a>,
    mirrored: bool,
) -> Option<ResolvedAnchor<'a>> {
    if !mirrored {
        return Some(resolved);
    }
    let motion = format!("{}_mirrored", resolved.anchor.motion);
    let clip = runtime.clips.get(&(resolved.pack_id.to_owned(), motion))?;
    Some(ResolvedAnchor { clip, ..resolved })
}

fn is_guard_locomotion_pose(pose: SemanticPose) -> bool {
    matches!(
        pose,
        SemanticPose::GuardLeadLeft
            | SemanticPose::GuardLeadRight
            | SemanticPose::GuardWalkLeadLeft
            | SemanticPose::GuardWalkLeadRight
            | SemanticPose::GuardStrafeLeadLeftLeft
            | SemanticPose::GuardStrafeLeadLeftRight
            | SemanticPose::GuardStrafeLeadRightLeft
            | SemanticPose::GuardStrafeLeadRightRight
    )
}

fn is_guard_movement_pose(pose: SemanticPose) -> bool {
    matches!(
        pose,
        SemanticPose::GuardWalkLeadLeft
            | SemanticPose::GuardWalkLeadRight
            | SemanticPose::GuardStrafeLeadLeftLeft
            | SemanticPose::GuardStrafeLeadLeftRight
            | SemanticPose::GuardStrafeLeadRightLeft
            | SemanticPose::GuardStrafeLeadRightRight
    )
}

fn guard_locomotion_resolution_pose(sample: &PoseSample) -> Option<SemanticPose> {
    if let PoseSampling::Span { end, .. } = sample.sampling
        && is_guard_movement_pose(end)
    {
        Some(end)
    } else {
        is_guard_locomotion_pose(sample.pose).then_some(sample.pose)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct GuardParityScore {
    movement_semantics: usize,
    guard_semantics: usize,
    resolved_anchors: usize,
}

fn parity_pose(pose: SemanticPose, mirrored: bool) -> SemanticPose {
    if mirrored {
        pose.mirrored_counterpart().unwrap_or(pose)
    } else {
        pose
    }
}

fn guard_parity_score(
    runtime: &AnimationRuntime,
    catalog: &AnimationPackCatalog,
    pack: &str,
    samples: &[PoseSample],
    movement_poses: &[SemanticPose],
    mirrored: bool,
) -> GuardParityScore {
    let movements = movement_poses.iter().copied().collect::<BTreeSet<_>>();
    let guards = samples
        .iter()
        .map(|sample| sample.pose)
        .collect::<BTreeSet<_>>();
    let mut score = GuardParityScore::default();
    for pose in movements {
        if let Some(resolved) = resolve_anchor_with_parity(runtime, catalog, pack, pose, mirrored) {
            score.resolved_anchors += 1;
            score.movement_semantics += (resolved.semantic == parity_pose(pose, mirrored)) as usize;
        }
    }
    for pose in guards {
        if let Some(resolved) = resolve_anchor_with_parity(runtime, catalog, pack, pose, mirrored) {
            score.resolved_anchors += 1;
            score.guard_semantics += (resolved.semantic == parity_pose(pose, mirrored)) as usize;
        }
    }
    score
}

fn resolve_anchor_with_parity<'a>(
    runtime: &'a AnimationRuntime,
    catalog: &'a AnimationPackCatalog,
    requested_pack: &str,
    requested_pose: SemanticPose,
    mirrored: bool,
) -> Option<ResolvedAnchor<'a>> {
    // The core resolver deliberately chooses same-pack counterparts on a
    // per-request basis. Guard diagonals instead require one forced parity for
    // the post-FK whole-body mirror, so this centralized traversal preserves
    // the core pack-before-semantic-fallback ordering while suppressing any
    // per-contribution parity switch.
    let mut semantic = Some(if mirrored {
        requested_pose
            .mirrored_counterpart()
            .unwrap_or(requested_pose)
    } else {
        requested_pose
    });
    let mut semantic_seen = BTreeSet::new();
    while let Some(pose) = semantic {
        if !semantic_seen.insert(pose) {
            break;
        }
        let mut pack_id = Some(requested_pack.to_owned());
        let mut pack_seen = BTreeSet::new();
        while let Some(id) = pack_id {
            if !pack_seen.insert(id.clone()) {
                break;
            }
            let (canonical_id, pack) = catalog.packs.get_key_value(&id)?;
            if let Some(anchor) = pack.poses.get(&pose)
                && let Some(clip) = runtime
                    .clips
                    .get(&(canonical_id.clone(), anchor.motion.clone()))
            {
                return Some(ResolvedAnchor {
                    clip,
                    anchor,
                    pack_id: canonical_id,
                    semantic: pose,
                    mirrored,
                });
            }
            pack_id = pack.fallback.clone();
        }
        semantic = pose.fallback();
    }
    None
}

fn append_weighted_anchor(
    weighted: &mut Vec<WeightedClip>,
    resolved: &ResolvedAnchor,
    frame: u16,
    weight: f32,
    layer: ClipLayer,
) {
    let clip = resolved.clip.at_anchor_layer(frame, layer);
    append_weighted_clip(
        weighted,
        &clip,
        resolved.mirrored,
        frame_seconds(frame),
        weight,
    );
}

fn append_weighted_clip(
    weighted: &mut Vec<WeightedClip>,
    clip: &LoadedClip,
    mirrored: bool,
    time_seconds: f32,
    weight: f32,
) {
    if weight <= f32::EPSILON {
        return;
    }
    if let Some(existing) = weighted.iter_mut().find(|existing| {
        existing.clip.node == clip.node && (existing.time_seconds - time_seconds).abs() < 0.0001
    }) {
        existing.weight += weight;
        if mirrored {
            existing.mirrored_weight += weight;
        }
    } else {
        weighted.push(WeightedClip {
            clip: clip.clone(),
            weight,
            time_seconds,
            mirrored_weight: if mirrored { weight } else { 0.0 },
        });
    }
}

fn append_resolved_sample(
    weighted: &mut Vec<WeightedClip>,
    runtime: &AnimationRuntime,
    catalog: &AnimationPackCatalog,
    pack: &str,
    sample: PoseSample,
    forced_mirror_parity: Option<bool>,
) {
    append_resolved_sample_layer(
        weighted,
        runtime,
        catalog,
        pack,
        sample,
        forced_mirror_parity,
        ClipLayer::Whole,
    );
}

fn append_resolved_sample_layer(
    weighted: &mut Vec<WeightedClip>,
    runtime: &AnimationRuntime,
    catalog: &AnimationPackCatalog,
    pack: &str,
    sample: PoseSample,
    forced_mirror_parity: Option<bool>,
    layer: ClipLayer,
) {
    let start = match forced_mirror_parity {
        Some(mirrored) => resolve_anchor_with_parity(runtime, catalog, pack, sample.pose, mirrored),
        None => resolve_anchor(runtime, catalog, pack, sample.pose),
    };
    let Some(start) = start.and_then(|resolved| {
        select_gait_endpoint_parity(runtime, resolved, sample.mirror_lower_body)
    }) else {
        return;
    };
    match sample.sampling {
        PoseSampling::Anchor => {
            append_weighted_anchor(weighted, &start, start.anchor.frame, sample.weight, layer)
        }
        PoseSampling::Cycle { phase } => {
            append_weighted_clip(
                weighted,
                start.clip,
                start.mirrored,
                start.clip.duration_seconds * phase.rem_euclid(1.0),
                sample.weight,
            );
        }
        PoseSampling::Span { end, progress } => {
            let end_pose = end;
            let progress = progress.clamp(0.0, 1.0);
            let end = match forced_mirror_parity {
                Some(mirrored) => {
                    resolve_anchor_with_parity(runtime, catalog, pack, end_pose, mirrored)
                }
                None => resolve_anchor(runtime, catalog, pack, end_pose),
            };
            let Some(end) = end else {
                append_weighted_anchor(weighted, &start, start.anchor.frame, sample.weight, layer);
                return;
            };
            if start.pack_id == end.pack_id && start.anchor.motion == end.anchor.motion {
                append_weighted_anchor(
                    weighted,
                    &start,
                    start.anchor.frame,
                    sample.weight * (1.0 - progress),
                    layer,
                );
                append_weighted_anchor(
                    weighted,
                    &end,
                    end.anchor.frame,
                    sample.weight * progress,
                    layer,
                );
            } else if let Some(reference) = catalog.packs[end.pack_id]
                .references
                .get(&end.anchor.motion)
                .and_then(|references| {
                    references
                        .iter()
                        .filter(|reference| reference.pose == sample.pose)
                        .min_by_key(|reference| reference.frame.abs_diff(end.anchor.frame))
                })
            {
                append_weighted_anchor(
                    weighted,
                    &end,
                    reference.frame,
                    sample.weight * (1.0 - progress),
                    layer,
                );
                append_weighted_anchor(
                    weighted,
                    &end,
                    end.anchor.frame,
                    sample.weight * progress,
                    layer,
                );
            } else if let Some(reference) = catalog.packs[start.pack_id]
                .references
                .get(&start.anchor.motion)
                .and_then(|references| {
                    references
                        .iter()
                        .filter(|reference| reference.pose == end_pose)
                        .min_by_key(|reference| reference.frame.abs_diff(start.anchor.frame))
                })
            {
                append_weighted_anchor(
                    weighted,
                    &start,
                    start.anchor.frame,
                    sample.weight * (1.0 - progress),
                    layer,
                );
                append_weighted_anchor(
                    weighted,
                    &start,
                    reference.frame,
                    sample.weight * progress,
                    layer,
                );
            } else {
                append_weighted_anchor(
                    weighted,
                    &start,
                    start.anchor.frame,
                    sample.weight * (1.0 - progress),
                    layer,
                );
                append_weighted_anchor(
                    weighted,
                    &end,
                    end.anchor.frame,
                    sample.weight * progress,
                    layer,
                );
            }
        }
    }
}

fn sync_animation_graphs(
    mut commands: Commands,
    runtime: Res<AnimationRuntime>,
    mut players: Query<
        (Entity, &mut AnimationPlayer, &mut AnimationGraphRevision),
        With<AnimationPlayerOwner>,
    >,
) {
    let Some(graph) = runtime.graph.as_ref() else {
        return;
    };
    for (entity, mut player, mut revision) in &mut players {
        if revision.0 == runtime.revision {
            continue;
        }
        player.stop_all();
        revision.0 = runtime.revision;
        commands
            .entity(entity)
            .insert(AnimationGraphHandle(graph.clone()));
    }
}

fn drive_fk_players(
    backend: Res<AnimationBackend>,
    owners: Query<&AnimationPlayback>,
    mut players: Query<(&AnimationPlayerOwner, &mut AnimationPlayer)>,
) {
    for (owner, mut player) in &mut players {
        if *backend == AnimationBackend::PoseBuffer {
            player.stop_all();
            continue;
        }
        let Ok(playback) = owners.get(owner.0) else {
            continue;
        };
        if playback.use_authored_bind_pose {
            player.stop_all();
            continue;
        }
        // The graph produces weighted semantic samples. Anchors seek to their
        // authored frame; locomotion cycles seek continuously through the
        // complete motion at the shared authoritative phase.
        // Reusing the previous graph's active-node state lets dependency blend
        // poses feed a small amount of their prior output into a repeated
        // render of the same logical tick. Rebuild this tiny paused set so one
        // tick is idempotent across gameplay/side/front evaluations.
        player.stop_all();
        for weighted in &playback.clips {
            player
                .play(weighted.clip.node)
                .set_weight(weighted.weight)
                .pause()
                .seek_to(weighted.time_seconds);
        }
    }
}

fn update_rig_visibility(
    playbacks: Query<&AnimationPlayback>,
    mut fallbacks: Query<(&FallbackAnimationRig, &mut Visibility)>,
    mut authored: Query<(&AnimationRigScene, &mut Visibility), Without<FallbackAnimationRig>>,
) {
    let authored_owners = authored
        .iter_mut()
        .map(|(rig, _)| rig.0)
        .collect::<BTreeSet<_>>();
    for (owner, mut visibility) in &mut fallbacks {
        *visibility = if authored_owners.contains(&owner.0) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    for (owner, mut visibility) in &mut authored {
        *visibility = if playbacks.get(owner.0).is_ok() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Spawns a deliberately obvious T-pose mannequin. It remains visible until
/// the independently loaded authored base rig is available, even if no motion
/// clip has been exported yet.
pub fn spawn_fallback_t_pose(
    parent: &mut ChildSpawnerCommands,
    owner: Entity,
    color: Color,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let material = materials.add(StandardMaterial {
        base_color: color,
        metallic: 0.0,
        perceptual_roughness: 1.0,
        ..default()
    });
    let torso = meshes.add(Capsule3d::new(0.24, 0.5));
    let head = meshes.add(Sphere::new(0.22));
    let arm = meshes.add(Cuboid::new(0.72, 0.16, 0.18));
    let leg = meshes.add(Capsule3d::new(0.12, 0.72));
    parent
        .spawn((
            Name::new("Fallback bind-pose T rig"),
            FallbackAnimationRig(owner),
            Transform::from_xyz(0.0, PLAYER_VISUAL_Y_OFFSET, 0.0),
            Visibility::Inherited,
        ))
        .with_children(|rig| {
            rig.spawn((
                Name::new("hips/spine/chest"),
                Mesh3d(torso),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(0.0, 0.35, 0.0),
            ));
            rig.spawn((
                Name::new("head"),
                Mesh3d(head),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(0.0, 1.02, 0.0),
            ));
            for (name, x) in [
                ("upper_arm.L/forearm.L/hand.L", -0.56),
                ("upper_arm.R/forearm.R/hand.R", 0.56),
            ] {
                rig.spawn((
                    Name::new(name),
                    Mesh3d(arm.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(x, 0.68, 0.0),
                ));
            }
            for (name, x) in [
                ("thigh.L/shin.L/foot.L", -0.16),
                ("thigh.R/shin.R/foot.R", 0.16),
            ] {
                rig.spawn((
                    Name::new(name),
                    Mesh3d(leg.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(x, -0.46, 0.0),
                ));
            }
        });
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn sparse_catalog_declares_only_current_airborne_and_attack_assets() {
        let catalog = AnimationPackCatalog::biped_root().unwrap();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        assert!(root.motions.contains_key("airborne_center"));
        assert!(root.motions.contains_key("airborne_travel"));
        assert!(root.motions.contains_key("attack_thrust_lead_left_contact"));
        assert!(!root.motions.keys().any(|name| name.starts_with("jump_")));
        assert!(
            !root
                .motions
                .keys()
                .any(|name| name.contains("follow_through"))
        );
        assert!(!root.motions.keys().any(|name| name.contains("_commit")));
    }

    #[test]
    fn missing_runtime_art_preserves_graceful_semantic_fallback() {
        let catalog = AnimationPackCatalog::biped_root().unwrap();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        assert_eq!(
            root.poses[&SemanticPose::AttackThrustLeadLeftContact].motion,
            "attack_thrust_lead_left_contact"
        );
        assert!(
            !root
                .poses
                .contains_key(&SemanticPose::AttackThrustLeadRightContact)
        );
    }
}

#[cfg(test)]
mod tests;
