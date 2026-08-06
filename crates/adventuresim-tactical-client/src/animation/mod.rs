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

mod procedural;

#[allow(unused_imports)]
pub(crate) use procedural::{
    ArmIkState, BoneRole, HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidBone,
    HumanoidIkTargets, LegIkState, LocomotionBodyResponseState, LocomotionHeightState,
    MEASURED_ANKLE_SOLE_OFFSET_METRES, ProceduralAnimationClock, RaisedFootworkState,
    locomotion_support_weights,
};
const HUMANOID_UNARMED_PACK: &str = "humanoid_unarmed";
const BIPED_BASE_GLB: &str = "animations/biped/unarmed/base.glb";
const ANIMATION_FPS: f32 = 30.0;
const PRESENTATION_CROSSFADE_SECONDS: f32 = 0.18;
// Player transforms sit at the center of the 1.9 m server collider, while
// authored rigs use a floor-level origin. Keep visual feet on the collider's
// lower face so the first-person camera lands at the authored head.
const PLAYER_VISUAL_Y_OFFSET: f32 = -0.95;

mod diagnostics;
use diagnostics::log_animation_diagnostics;
#[cfg(not(target_family = "wasm"))]
#[allow(unused_imports)] // Gameplay diagnostics consume these; the viewer target does not.
pub(crate) use diagnostics::{
    AnimationDiagnosticLog, DiagnosticInputStatus, RenderScheduleTelemetry,
};

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

mod catalog;
pub use catalog::AnimationPackCatalog;
use catalog::*;

#[derive(Debug, Clone)]
struct LoadedClip {
    /// Dedicated graph node for continuous timeline playback. A Bevy graph
    /// node has one seek position, so sparse pose anchors use separate nodes
    /// below when two frames from the same source clip must be blended.
    node: AnimationNodeIndex,
    anchor_nodes: BTreeMap<u16, AnimationNodeIndex>,
}

impl LoadedClip {
    fn at_anchor(&self, frame: u16) -> Self {
        let mut clip = self.clone();
        clip.node = self.anchor_nodes.get(&frame).copied().unwrap_or(self.node);
        clip
    }
}

#[derive(Resource, Default)]
struct AnimationRuntime {
    requested_base: Option<Handle<Gltf>>,
    base_processed: bool,
    base_failed: bool,
    base_scene: Option<Handle<Scene>>,
    requested_motions: BTreeMap<(String, String), Handle<Gltf>>,
    processed_motions: BTreeSet<(String, String)>,
    unavailable_motions: BTreeSet<(String, String)>,
    clip_handles: BTreeMap<(String, String), Handle<AnimationClip>>,
    clips: BTreeMap<(String, String), LoadedClip>,
    library: AnimationPackLibrary,
    graph: Option<Handle<AnimationGraph>>,
    revision: u64,
    canonical_targets: HashSet<AnimationTargetId>,
}

#[derive(Component, Debug)]
pub(super) struct AnimationPlayback {
    clips: Vec<WeightedClip>,
    use_authored_bind_pose: bool,
    pub(super) whole_body_mirror: f32,
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
}

impl Default for AnimationPlayback {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            use_authored_bind_pose: true,
            whole_body_mirror: 0.0,
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
}

#[derive(Debug, Clone)]
struct PlaybackTransition {
    from: PlaybackPose,
    elapsed_seconds: f32,
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
    catalog: Res<AnimationPackCatalog>,
    runtime: Res<AnimationRuntime>,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    players: Query<(Entity, &PresentedSkeleton, Option<&mut AnimationPlayback>), With<Player>>,
) {
    for (entity, skeleton, playback) in players {
        let evaluation = AnimationEvaluation::from_skeleton(skeleton);
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
        for sample in samples {
            append_resolved_sample(
                &mut weighted,
                &runtime,
                &catalog,
                &skeleton.animation_pack,
                *sample,
                coherent_guard_parity,
            );
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
            clips: weighted,
        };
        let ordinary_locomotion_active = skeleton.is_grounded()
            && skeleton.action_kind() == SkeletonAction::None
            && skeleton.weapon_guard() == WeaponGuardState::Lowered
            && skeleton.animation_speed() > 0.05;
        if let Some(mut playback) = playback {
            update_presentation_crossfade(
                &mut playback,
                target,
                skeleton.weapon_guard(),
                ordinary_locomotion_active,
                &procedural_clock,
                time.delta_secs(),
            );
        } else {
            commands.entity(entity).insert(AnimationPlayback {
                clips: target.clips,
                use_authored_bind_pose: target.use_authored_bind_pose,
                whole_body_mirror: target.whole_body_mirror,
                weapon_guard: skeleton.weapon_guard(),
                ordinary_locomotion_active,
                presentation_transition: None,
                evaluation_tick: procedural_clock.fixed_step().map(|(tick, _)| tick),
            });
        }
    }
}

fn update_presentation_crossfade(
    playback: &mut AnimationPlayback,
    target: PlaybackPose,
    weapon_guard: WeaponGuardState,
    ordinary_locomotion_active: bool,
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
    let locomotion_released =
        playback.ordinary_locomotion_active && !ordinary_locomotion_active && !guard_changed;
    let transition_started = guard_changed || locomotion_released;
    if transition_started {
        playback.presentation_transition = Some(PlaybackTransition {
            from: playback_pose(playback),
            elapsed_seconds: 0.0,
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
        let progress =
            (transition.elapsed_seconds / PRESENTATION_CROSSFADE_SECONDS).clamp(0.0, 1.0);
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
    }
}

fn apply_playback_pose(playback: &mut AnimationPlayback, pose: PlaybackPose) {
    playback.clips = pose.clips;
    playback.use_authored_bind_pose = pose.use_authored_bind_pose;
    playback.whole_body_mirror = pose.whole_body_mirror;
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
    }
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
) {
    let clip = resolved.clip.at_anchor(frame);
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
            append_weighted_anchor(weighted, &start, start.anchor.frame, sample.weight)
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
                append_weighted_anchor(weighted, &start, start.anchor.frame, sample.weight);
                return;
            };
            if start.pack_id == end.pack_id && start.anchor.motion == end.anchor.motion {
                append_weighted_anchor(
                    weighted,
                    &start,
                    start.anchor.frame,
                    sample.weight * (1.0 - progress),
                );
                append_weighted_anchor(weighted, &end, end.anchor.frame, sample.weight * progress);
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
                );
                append_weighted_anchor(weighted, &end, end.anchor.frame, sample.weight * progress);
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
                );
                append_weighted_anchor(weighted, &start, reference.frame, sample.weight * progress);
            } else {
                append_weighted_anchor(
                    weighted,
                    &start,
                    start.anchor.frame,
                    sample.weight * (1.0 - progress),
                );
                append_weighted_anchor(weighted, &end, end.anchor.frame, sample.weight * progress);
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
    owners: Query<&AnimationPlayback>,
    mut players: Query<(&AnimationPlayerOwner, &mut AnimationPlayer)>,
) {
    for (owner, mut player) in &mut players {
        let Ok(playback) = owners.get(owner.0) else {
            continue;
        };
        if playback.use_authored_bind_pose {
            player.stop_all();
            continue;
        }
        for (_, active) in player.playing_animations_mut() {
            active.set_weight(0.0);
        }
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
