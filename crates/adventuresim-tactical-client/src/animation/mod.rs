use std::collections::{BTreeMap, BTreeSet, HashSet};

use adventuresim_tactical_core::prelude::*;
#[cfg(not(target_family = "wasm"))]
use adventuresim_tactical_netcode::message::PlayerInputRequest;
use adventuresim_tactical_netcode::message::SuccessfulAttackResponse;
use bevy::{
    animation::AnimationTargetId, asset::LoadState, ecs::hierarchy::ChildSpawnerCommands,
    gltf::Gltf, prelude::*,
};

pub(crate) mod jitter;
pub(crate) mod pose_buffer;
mod procedural;

#[allow(unused_imports)]
pub(crate) use procedural::{
    ArmIkState, BoneRole, HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidBone,
    HumanoidIkTargets, HumanoidRig, LegIkDiagnostics, LegIkState, LocomotionBodyResponseState,
    LocomotionHeightState, MEASURED_ANKLE_SOLE_OFFSET_METRES, ProceduralAnimationClock,
    RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES, authored_bind_global,
    locomotion_support_weights,
};
const HUMANOID_UNARMED_PACK: &str = "humanoid_unarmed";
const BIPED_BASE_GLB: &str = "animations/biped/unarmed/base.glb";
const ANIMATION_FPS: f32 = 30.0;
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
pub(crate) mod semantic_route;

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
    duration_seconds: f32,
    layer: ClipLayer,
}

impl LoadedClip {
    fn at_anchor(&self, frame: u16) -> Self {
        self.at_anchor_layer(frame, ClipLayer::Whole)
    }

    fn at_anchor_layer(&self, _frame: u16, layer: ClipLayer) -> Self {
        let mut clip = self.clone();
        clip.layer = layer;
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
    canonical_targets: HashSet<AnimationTargetId>,
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
}

impl AnimationPlayback {
    #[allow(dead_code)] // Used by the standalone animation-viewer binary.
    pub(super) fn authored_pose_is_ready(&self) -> bool {
        !self.use_authored_bind_pose && !self.clips.is_empty()
    }

    pub(super) fn presentation_is_settled(&self) -> bool {
        self.authored_pose_is_ready()
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

#[derive(Component)]
pub struct FallbackAnimationRig(pub Entity);

#[derive(Component)]
pub(super) struct AnimationRigScene(pub Entity);

#[derive(Component)]
struct AnimationRigAttached;

#[derive(Component)]
struct RigAnimationTargetsBound;

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct AuthoredBindTransform {
    pub(crate) owner: Entity,
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
    players: Query<
        (
            Entity,
            &PresentedSkeleton,
            &semantic_route::SemanticRouteTrace,
            Option<&mut AnimationPlayback>,
        ),
        With<Player>,
    >,
) {
    for (entity, skeleton, route_trace, playback) in players {
        // The preceding chained system directly routes authoritative
        // presentation state into deterministic semantic samples.
        let evaluation = route_trace.evaluation.clone();
        let samples = if evaluation.action.is_empty() {
            &evaluation.base
        } else {
            &evaluation.action
        };
        let mut weighted = Vec::<WeightedClip>::new();
        let base_layer = if !evaluation.lower_body.is_empty() {
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
                base_layer,
            );
        }
        for sample in &evaluation.lower_body {
            append_resolved_sample_layer(
                &mut weighted,
                &runtime,
                &catalog,
                &skeleton.animation_pack,
                *sample,
                ClipLayer::Lower,
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
            foot_ik_weights: semantic_foot_ik_weights(&evaluation),
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
            playback.weapon_guard = skeleton.weapon_guard();
            playback.ordinary_locomotion_active = ordinary_locomotion_active;
            apply_playback_pose(&mut playback, target);
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

fn apply_playback_pose(playback: &mut AnimationPlayback, pose: PlaybackPose) {
    playback.clips = pose.clips;
    playback.use_authored_bind_pose = pose.use_authored_bind_pose;
    playback.whole_body_mirror = pose.whole_body_mirror;
    playback.foot_ik_weights = pose.foot_ik_weights;
}

/// IK ownership follows the direct semantic locomotion samples.
/// The curve shapes are animation metadata: walk keeps one supported foot,
/// run has genuine intervals with neither foot loaded, and idle loads both.
fn semantic_foot_ik_weights(evaluation: &AnimationEvaluation) -> Vec2 {
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
        existing.clip.handle.id() == clip.handle.id()
            && existing.clip.layer == clip.layer
            && (existing.time_seconds - time_seconds).abs() < 0.0001
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
) {
    append_resolved_sample_layer(weighted, runtime, catalog, pack, sample, ClipLayer::Whole);
}

fn append_resolved_sample_layer(
    weighted: &mut Vec<WeightedClip>,
    runtime: &AnimationRuntime,
    catalog: &AnimationPackCatalog,
    pack: &str,
    sample: PoseSample,
    layer: ClipLayer,
) {
    let start = resolve_anchor(runtime, catalog, pack, sample.pose);
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
            let end = resolve_anchor(runtime, catalog, pack, end_pose);
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
                ("l_uparm/l_lowarm/l_wrist", -0.56),
                ("r_uparm/r_lowarm/r_wrist", 0.56),
            ] {
                rig.spawn((
                    Name::new(name),
                    Mesh3d(arm.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(x, 0.68, 0.0),
                ));
            }
            for (name, x) in [
                ("l_upleg/l_lowleg/l_foot", -0.16),
                ("r_upleg/r_lowleg/r_foot", 0.16),
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
    fn sparse_catalog_declares_airborne_and_three_attack_assets() {
        let catalog = AnimationPackCatalog::biped_root().unwrap();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        assert!(root.motions.contains_key("airborne_center"));
        assert!(root.motions.contains_key("airborne_travel"));
        assert!(root.motions.contains_key("swing"));
        assert!(root.motions.contains_key("swing_follow"));
        assert!(root.motions.contains_key("thrust"));
        assert!(!root.motions.keys().any(|name| name.starts_with("jump_")));
    }

    #[test]
    fn attack_semantics_use_the_canonical_motion_names() {
        let catalog = AnimationPackCatalog::biped_root().unwrap();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        assert_eq!(root.poses[&SemanticPose::AttackThrust].motion, "thrust");
        assert_eq!(root.poses[&SemanticPose::AttackSwing].motion, "swing");
        assert_eq!(
            root.poses[&SemanticPose::AttackSwingFollow].motion,
            "swing_follow"
        );
    }
}

#[cfg(test)]
mod tests;
