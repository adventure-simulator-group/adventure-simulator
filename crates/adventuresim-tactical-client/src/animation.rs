use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    str::FromStr,
};

use adventuresim_tactical_core::prelude::*;
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
    BoneRole, HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidBone, HumanoidIkTargets,
    LocomotionBodyResponseState, LocomotionHeightState, ProceduralAnimationClock,
    ProceduralIkState, RaisedFootworkState, locomotion_support_weights,
};
const HUMANOID_UNARMED_PACK: &str = "humanoid_unarmed";
const BIPED_BASE_GLB: &str = "animations/biped/unarmed/base.glb";
const ANIMATION_FPS: f32 = 30.0;
const PRESENTATION_CROSSFADE_SECONDS: f32 = 0.18;
// Player transforms sit at the center of the 1.9 m server collider, while
// authored rigs use a floor-level origin. Keep visual feet on the collider's
// lower face so the first-person camera lands at the authored head.
const PLAYER_VISUAL_Y_OFFSET: f32 = -0.95;

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

pub struct TacticalAnimationPlugin;

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

const MAX_COALESCED_PRESENTATION_EVENTS: u64 = 8;

fn bounded_forward_sequence_delta(previous: u64, current: u64) -> Option<u64> {
    current
        .checked_sub(previous)
        .filter(|delta| *delta <= MAX_COALESCED_PRESENTATION_EVENTS)
}

fn latest_coalesced_landing(previous: u64, current: u64) -> Option<u64> {
    bounded_forward_sequence_delta(previous, current)
        .filter(|delta| *delta > 0)
        .map(|_| current)
}

fn coalesced_contacts(
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
            .init_resource::<AnimationRuntime>()
            .init_resource::<TerrainIkEnabled>()
            .init_resource::<ProceduralAnimationClock>()
            .add_message::<LocomotionPresentationEvent>()
            .add_systems(Startup, request_animation_packs)
            .add_observer(on_successful_attack)
            .add_systems(
                Update,
                (
                    collect_loaded_packs,
                    attach_loaded_rig_scenes,
                    establish_animation_targets,
                    identify_animation_players,
                    procedural::bind_humanoid_bones,
                    procedural::capture_humanoid_rig_axes,
                    capture_authored_bind_transforms,
                    evaluate_skeletons,
                    tick_impact_reactions,
                    sync_animation_graphs,
                    drive_fk_players,
                    update_rig_visibility,
                    emit_locomotion_presentation_events,
                    trace_locomotion_presentation_events,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                reset_authored_bind_before_fk.before(AnimationSystems),
            )
            .add_systems(
                PostUpdate,
                (
                    restore_authored_bind_pose,
                    procedural::apply_gait_mirroring,
                    procedural::stabilize_locomotion_torso,
                    procedural::apply_landing_leg_compression,
                    procedural::apply_locomotion_body_response,
                    procedural::apply_head_and_torso_look,
                    procedural::apply_impact_reaction,
                    procedural::apply_terrain_leg_ik,
                    procedural::apply_arm_and_weapon_constraints,
                )
                    .chain()
                    .after(AnimationSystems)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn trace_locomotion_presentation_events(mut messages: MessageReader<LocomotionPresentationEvent>) {
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

/// Runtime switch for terrain height, slope, and pelvis conformity. Ordinary
/// locomotion retains authored FK and leg motion while this is disabled; only
/// raised-guard flat footwork remains procedural. Debug builds expose F8 as an
/// explicit opt-in toggle.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainIkEnabled(pub bool);

impl Default for TerrainIkEnabled {
    fn default() -> Self {
        Self(false)
    }
}

/// Explicit code-owned file/frame catalog. Semantic ownership never depends on
/// glTF animation names or scene contents.
#[derive(Resource, Debug)]
pub struct AnimationPackCatalog {
    packs: BTreeMap<String, PackCatalog>,
}

impl Default for AnimationPackCatalog {
    fn default() -> Self {
        Self::biped_root().expect("built-in biped animation catalog must be valid")
    }
}

#[derive(Debug, Clone)]
struct PackCatalog {
    skeleton_family: String,
    fallback: Option<String>,
    motions: BTreeMap<String, MotionSource>,
    poses: BTreeMap<SemanticPose, PoseAnchor>,
    references: BTreeMap<String, Vec<ReferenceAnchor>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MotionSource {
    path: String,
    last_frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PoseAnchor {
    motion: String,
    frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceAnchor {
    pose: SemanticPose,
    frame: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CatalogError {
    DuplicatePose(SemanticPose),
    UnknownMotion(String),
}

struct PackBuilder {
    id: String,
    path_prefix: String,
    pack: PackCatalog,
}

impl PackBuilder {
    fn new(id: &str, skeleton_family: &str, fallback: Option<&str>, path_prefix: &str) -> Self {
        Self {
            id: id.to_owned(),
            path_prefix: path_prefix.trim_end_matches('/').to_owned(),
            pack: PackCatalog {
                skeleton_family: skeleton_family.to_owned(),
                fallback: fallback.map(str::to_owned),
                motions: BTreeMap::new(),
                poses: BTreeMap::new(),
                references: BTreeMap::new(),
            },
        }
    }

    fn motion(&mut self, id: &str, last_frame: u16) {
        self.pack.motions.insert(
            id.to_owned(),
            MotionSource {
                path: format!("{}/{id}.glb", self.path_prefix),
                last_frame,
            },
        );
    }

    fn pose(&mut self, motion: &str, frame: u16, pose: &str) -> Result<(), CatalogError> {
        if !self.pack.motions.contains_key(motion) {
            return Err(CatalogError::UnknownMotion(motion.to_owned()));
        }
        let pose = SemanticPose::from_str(pose)
            .map_err(|()| CatalogError::UnknownMotion(pose.to_owned()))?;
        if self
            .pack
            .poses
            .insert(
                pose,
                PoseAnchor {
                    motion: motion.to_owned(),
                    frame,
                },
            )
            .is_some()
        {
            return Err(CatalogError::DuplicatePose(pose));
        }
        Ok(())
    }

    fn reference(&mut self, motion: &str, frame: u16, pose: &str) -> Result<(), CatalogError> {
        if !self.pack.motions.contains_key(motion) {
            return Err(CatalogError::UnknownMotion(motion.to_owned()));
        }
        let pose = SemanticPose::from_str(pose)
            .map_err(|()| CatalogError::UnknownMotion(pose.to_owned()))?;
        self.pack
            .references
            .entry(motion.to_owned())
            .or_default()
            .push(ReferenceAnchor { pose, frame });
        Ok(())
    }

    fn finish(self) -> (String, PackCatalog) {
        (self.id, self.pack)
    }
}

impl AnimationPackCatalog {
    fn biped_root() -> Result<Self, CatalogError> {
        let mut builder = PackBuilder::new(
            HUMANOID_UNARMED_PACK,
            "humanoid",
            None,
            "animations/biped/unarmed",
        );
        for pose in [
            "idle_relaxed",
            "crouch_idle",
            "guard_lead_left",
            "guard_lead_right",
            "prone_idle",
            "supine_idle",
        ] {
            builder.motion(pose, 0);
            builder.pose(pose, 0, pose)?;
        }
        for motion in [
            "guard_walk_lead_left",
            "guard_walk_lead_right",
            "guard_strafe_lead_left_left",
            "guard_strafe_lead_left_right",
            "guard_strafe_lead_right_left",
            "guard_strafe_lead_right_right",
        ] {
            builder.motion(motion, 0);
            builder.pose(motion, 0, motion)?;
        }
        for (motion, last_frame, anchors) in [
            ("walk", 32, [(0, "walk_contact"), (8, "walk_passing")]),
            ("run", 20, [(0, "run_contact"), (5, "run_flight")]),
            (
                "crouch_walk",
                40,
                [(0, "crouch_walk_contact"), (10, "crouch_walk_passing")],
            ),
            (
                "prone_crawl",
                32,
                [(0, "prone_crawl_contact"), (8, "prone_crawl_passing")],
            ),
            (
                "prone_strafe",
                32,
                [(0, "prone_strafe_contact"), (8, "prone_strafe_passing")],
            ),
            (
                "supine_scamper",
                32,
                [(0, "supine_scamper_contact"), (8, "supine_scamper_passing")],
            ),
        ] {
            builder.motion(motion, last_frame);
            for (frame, pose) in anchors {
                builder.pose(motion, frame, pose)?;
            }
        }
        for (motion, pose) in [
            ("duck_lead_left_backward", "duck_lead_left_backward"),
            ("duck_lead_left_left", "duck_lead_left_left"),
            ("duck_lead_left_right", "duck_lead_left_right"),
        ] {
            builder.motion(motion, 0);
            builder.pose(motion, 0, pose)?;
        }
        builder.motion("duck_forward", 12);
        builder.pose("duck_forward", 6, "duck_forward")?;
        builder.reference("duck_forward", 0, "crouch_idle")?;
        builder.reference("duck_forward", 12, "crouch_idle")?;
        for direction in ["center", "forward", "backward", "left", "right"] {
            let motion = format!("jump_{direction}");
            builder.motion(&motion, 30);
            for (frame, phase) in [(6, "launch"), (15, "flight"), (24, "landing")] {
                builder.pose(&motion, frame, &format!("{motion}_{phase}"))?;
            }
        }
        for family in ["thrust", "slash"] {
            let contact_motion = format!("attack_{family}_lead_left_contact");
            builder.motion(&contact_motion, 0);
            for lead in ["left", "right"] {
                for footwork in ["stay", "switch"] {
                    let legacy_motion = format!("attack_{family}_lead_{lead}_{footwork}");
                    builder.motion(&legacy_motion, 20);
                    builder.pose(&legacy_motion, 4, &format!("{legacy_motion}_commit"))?;
                    if lead == "left" {
                        builder.pose(&contact_motion, 0, &format!("{legacy_motion}_contact"))?;
                    } else {
                        builder.pose(&legacy_motion, 8, &format!("{legacy_motion}_contact"))?;
                    }
                    builder.pose(
                        &legacy_motion,
                        13,
                        &format!("{legacy_motion}_follow_through"),
                    )?;
                    builder.reference(&legacy_motion, 0, &format!("guard_lead_{lead}"))?;
                    let end_lead = if footwork == "switch" {
                        if lead == "left" { "right" } else { "left" }
                    } else {
                        lead
                    };
                    builder.reference(&legacy_motion, 20, &format!("guard_lead_{end_lead}"))?;
                }
            }
        }
        for motion in [
            "block_cut_left_lead_left",
            "block_cut_left_lead_right",
            "block_cut_right_lead_left",
            "block_cut_right_lead_right",
            "block_thrust_lead_left",
            "block_thrust_lead_right",
        ] {
            builder.motion(motion, 14);
            builder.pose(motion, 6, motion)?;
            let lead = if motion.ends_with("lead_left") {
                "guard_lead_left"
            } else {
                "guard_lead_right"
            };
            builder.reference(motion, 0, lead)?;
            builder.reference(motion, 14, lead)?;
        }
        for (motion, last_frame, frame, pose) in [
            (
                "upright_prone_transition",
                24,
                12,
                "upright_prone_transition",
            ),
            ("dive", 18, 10, "dive_impact"),
            ("prone_supine_roll_left", 20, 10, "prone_supine_roll_left"),
            ("prone_supine_roll_right", 20, 10, "prone_supine_roll_right"),
        ] {
            builder.motion(motion, last_frame);
            builder.pose(motion, frame, pose)?;
            match motion {
                "upright_prone_transition" => {
                    builder.reference(motion, 0, "crouch_idle")?;
                    builder.reference(motion, 24, "prone_idle")?;
                }
                "dive" => {
                    builder.reference(motion, 0, "jump_forward_launch")?;
                    builder.reference(motion, 18, "prone_idle")?;
                }
                "prone_supine_roll_left" | "prone_supine_roll_right" => {
                    builder.reference(motion, 0, "prone_idle")?;
                    builder.reference(motion, 20, "supine_idle")?;
                }
                _ => {}
            }
        }
        let (id, pack) = builder.finish();
        Ok(Self {
            packs: BTreeMap::from([(id, pack)]),
        })
    }
}

#[derive(Debug, Clone)]
struct LoadedClip {
    node: AnimationNodeIndex,
    /// The code-owned catalog range is authoritative. Exporters may retain
    /// extra timeline keys, but locomotion must never sample beyond the
    /// documented closure frame.
    cycle_duration_seconds: f32,
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
    pub(super) lower_body_mirror: f32,
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
            lower_body_mirror: 0.0,
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
    lower_body_mirror: f32,
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

fn request_animation_packs(
    catalog: Res<AnimationPackCatalog>,
    asset_server: Res<AssetServer>,
    mut runtime: ResMut<AnimationRuntime>,
) {
    runtime.requested_base = Some(asset_server.load(animation_asset_path(BIPED_BASE_GLB)));
    for (pack_id, pack) in &catalog.packs {
        for (motion_id, source) in &pack.motions {
            runtime.requested_motions.insert(
                (pack_id.clone(), motion_id.clone()),
                asset_server.load(animation_asset_path(&source.path)),
            );
        }
    }
}

fn collect_loaded_packs(
    catalog: Res<AnimationPackCatalog>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut runtime: ResMut<AnimationRuntime>,
) {
    let mut changed = false;
    if !runtime.base_processed && !runtime.base_failed {
        let Some(handle) = runtime.requested_base.as_ref() else {
            return;
        };
        if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
            runtime.base_failed = true;
            warn!(path = BIPED_BASE_GLB, "Authored base rig is unavailable");
        } else if let Some(gltf) = gltfs.get(handle) {
            runtime.base_scene = gltf
                .default_scene
                .clone()
                .or_else(|| gltf.scenes.first().cloned());
            if runtime.base_scene.is_none() {
                runtime.base_failed = true;
                warn!(path = BIPED_BASE_GLB, "Authored base rig has no scene");
            } else {
                runtime.base_processed = true;
            }
        }
    }

    let requested = runtime
        .requested_motions
        .iter()
        .map(|(key, handle)| (key.clone(), handle.clone()))
        .collect::<Vec<_>>();
    for (key, handle) in requested {
        if runtime.processed_motions.contains(&key) || runtime.unavailable_motions.contains(&key) {
            continue;
        }
        if matches!(asset_server.load_state(handle.id()), LoadState::Failed(_)) {
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        let Some(gltf) = gltfs.get(&handle) else {
            continue;
        };
        let Some(clip_handle) = sole_animation(&gltf.animations) else {
            warn!(
                pack = key.0,
                motion = key.1,
                count = gltf.animations.len(),
                "Motion must contain exactly one animation"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        };
        let Some(clip) = clips.get(clip_handle) else {
            continue;
        };
        if runtime.canonical_targets.is_empty() {
            continue;
        }
        if !clip_targets_match_base(clip, &runtime.canonical_targets) {
            warn!(
                pack = key.0,
                motion = key.1,
                "Motion targets are incompatible with the base rig"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        let Some(source) = catalog
            .packs
            .get(&key.0)
            .and_then(|pack| pack.motions.get(&key.1))
        else {
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        };
        if !frame_fits_clip(source.last_frame, clip.duration()) {
            warn!(
                pack = key.0,
                motion = key.1,
                last_frame = source.last_frame,
                duration = clip.duration(),
                "Motion is shorter than its catalog frames"
            );
            runtime.unavailable_motions.insert(key.clone());
            changed = true;
            continue;
        }
        runtime
            .clip_handles
            .insert(key.clone(), clip_handle.clone());
        runtime.processed_motions.insert(key);
        changed = true;
    }

    if !changed {
        return;
    }
    runtime.library = AnimationPackLibrary::default();
    for (pack_id, pack) in &catalog.packs {
        let available = pack
            .poses
            .iter()
            .filter(|(_, anchor)| {
                runtime
                    .processed_motions
                    .contains(&(pack_id.clone(), anchor.motion.clone()))
            })
            .map(|(&pose, _)| pose)
            .collect();
        if let Err(error) = runtime.library.insert(AnimationPack {
            id: pack_id.clone(),
            skeleton_family: pack.skeleton_family.clone(),
            fallback: pack.fallback.clone(),
            clips: available,
        }) {
            error!(?error, pack = pack_id, "Rejected animation pack");
        }
    }
    let ordered = runtime
        .clip_handles
        .iter()
        .map(|(key, handle)| (key.clone(), handle.clone()))
        .collect::<Vec<_>>();
    let (graph, nodes) =
        AnimationGraph::from_clips(ordered.iter().map(|(_, handle)| handle.clone()));
    runtime.clips = ordered
        .into_iter()
        .zip(nodes)
        .map(|((key, handle), node)| {
            let source = &catalog.packs[&key.0].motions[&key.1];
            let exported_duration = clips
                .get(&handle)
                .map(AnimationClip::duration)
                .unwrap_or(0.0);
            let cycle_duration_seconds =
                authoritative_cycle_duration(source.last_frame, exported_duration)
                    .expect("processed motion already passed catalog-range validation");
            (
                key,
                LoadedClip {
                    node,
                    cycle_duration_seconds,
                },
            )
        })
        .collect();
    runtime.graph = Some(graphs.add(graph));
    runtime.revision = runtime.revision.wrapping_add(1);
}

fn sole_animation(animations: &[Handle<AnimationClip>]) -> Option<&Handle<AnimationClip>> {
    (animations.len() == 1).then(|| &animations[0])
}

fn frame_fits_clip(frame: u16, duration: f32) -> bool {
    frame_seconds(frame) <= duration + 0.5 / ANIMATION_FPS
}

fn authoritative_cycle_duration(last_frame: u16, exported_duration: f32) -> Option<f32> {
    frame_fits_clip(last_frame, exported_duration).then(|| frame_seconds(last_frame))
}

fn frame_seconds(frame: u16) -> f32 {
    frame as f32 / ANIMATION_FPS
}

fn clip_targets_match_base(
    clip: &AnimationClip,
    canonical_targets: &HashSet<AnimationTargetId>,
) -> bool {
    targets_match_base(clip.curves().keys(), canonical_targets)
}

fn targets_match_base<'a>(
    mut targets: impl Iterator<Item = &'a AnimationTargetId>,
    canonical_targets: &HashSet<AnimationTargetId>,
) -> bool {
    let Some(first) = targets.next() else {
        return false;
    };
    canonical_targets.contains(first) && targets.all(|target| canonical_targets.contains(target))
}

fn attach_loaded_rig_scenes(
    mut commands: Commands,
    runtime: Res<AnimationRuntime>,
    players: Query<(Entity, &SkeletonState, Has<AnimationRigAttached>), With<Player>>,
) {
    for (player, _skeleton, attached) in &players {
        if attached {
            continue;
        }
        let Some(scene) = runtime.base_scene.as_ref() else {
            continue;
        };
        commands.entity(player).with_children(|parent| {
            parent.spawn((
                Name::new("Authored animation rig"),
                AnimationRigScene(player),
                SceneRoot(scene.clone()),
                Transform::from_xyz(0.0, PLAYER_VISUAL_Y_OFFSET, 0.0),
                Visibility::Hidden,
            ));
        });
        commands
            .entity(player)
            .insert((AnimationPlayback::default(), AnimationRigAttached));
    }
}

/// A zero-animation base glTF receives none of Bevy's usual animation wiring.
/// Recreate the loader's stable name-path target IDs so independently exported
/// motion files can animate the canonical `Skeleton` hierarchy.
fn establish_animation_targets(
    mut commands: Commands,
    mut runtime: ResMut<AnimationRuntime>,
    roots: Query<(Entity, &AnimationRigScene), Without<RigAnimationTargetsBound>>,
    children: Query<&Children>,
    names: Query<&Name>,
    existing_players: Query<(), With<AnimationPlayer>>,
) {
    for (rig_root, owner) in &roots {
        let Some(skeleton_root) = find_named_descendant(rig_root, "Skeleton", &children, &names)
        else {
            continue;
        };
        let player = descendants_including(skeleton_root, &children)
            .into_iter()
            .find(|entity| existing_players.contains(*entity))
            .unwrap_or(skeleton_root);
        commands.entity(player).insert((
            AnimationPlayer::default(),
            AnimationPlayerOwner(owner.0),
            AnimationGraphRevision::default(),
        ));
        bind_animation_target_paths(
            &mut commands,
            skeleton_root,
            player,
            Vec::new(),
            &children,
            &names,
            &mut runtime.canonical_targets,
        );
        commands.entity(rig_root).insert(RigAnimationTargetsBound);
    }
}

fn descendants_including(root: Entity, children: &Query<&Children>) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        found.push(entity);
        if let Ok(entity_children) = children.get(entity) {
            pending.extend(entity_children.iter());
        }
    }
    found
}

fn find_named_descendant(
    root: Entity,
    target: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    descendants_including(root, children)
        .into_iter()
        .find(|entity| names.get(*entity).is_ok_and(|name| name.as_str() == target))
}

fn bind_animation_target_paths(
    commands: &mut Commands,
    entity: Entity,
    player: Entity,
    mut path: Vec<Name>,
    children: &Query<&Children>,
    names: &Query<&Name>,
    canonical_targets: &mut HashSet<AnimationTargetId>,
) {
    let Ok(name) = names.get(entity) else {
        return;
    };
    path.push(name.clone());
    let target = AnimationTargetId::from_names(path.iter());
    canonical_targets.insert(target);
    commands.entity(entity).insert((target, AnimatedBy(player)));
    if let Ok(entity_children) = children.get(entity) {
        for child in entity_children.iter() {
            bind_animation_target_paths(
                commands,
                child,
                player,
                path.clone(),
                children,
                names,
                canonical_targets,
            );
        }
    }
}

fn identify_animation_players(
    mut commands: Commands,
    added: Query<Entity, Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    roots: Query<&AnimationRigScene>,
) {
    for player in &added {
        let mut current = player;
        for _ in 0..64 {
            if let Ok(root) = roots.get(current) {
                commands.entity(player).insert((
                    AnimationPlayerOwner(root.0),
                    AnimationGraphRevision::default(),
                ));
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

fn evaluate_skeletons(
    mut commands: Commands,
    catalog: Res<AnimationPackCatalog>,
    runtime: Res<AnimationRuntime>,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    players: Query<(Entity, &SkeletonState, Option<&mut AnimationPlayback>), With<Player>>,
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
        let mut mirror_weight = 0.0;
        let mut resolved_weight = 0.0;
        for sample in samples {
            let before = weighted.iter().map(|clip| clip.weight).sum::<f32>();
            append_resolved_sample(
                &mut weighted,
                &runtime,
                &catalog,
                &skeleton.animation_pack,
                *sample,
                coherent_guard_parity,
            );
            let added = (weighted.iter().map(|clip| clip.weight).sum::<f32>() - before).max(0.0);
            resolved_weight += added;
            mirror_weight += added * sample.mirror_lower_body.clamp(0.0, 1.0);
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
            lower_body_mirror: if resolved_weight > f32::EPSILON {
                (mirror_weight / resolved_weight).clamp(0.0, 1.0)
            } else {
                0.0
            },
        };
        let ordinary_locomotion_active = skeleton.grounded
            && skeleton.action == SkeletonAction::None
            && skeleton.weapon_guard == WeaponGuardState::Lowered
            && skeleton.animation_speed() > 0.05;
        if let Some(mut playback) = playback {
            update_presentation_crossfade(
                &mut playback,
                target,
                skeleton.weapon_guard,
                ordinary_locomotion_active,
                &procedural_clock,
                time.delta_secs(),
            );
        } else {
            commands.entity(entity).insert(AnimationPlayback {
                clips: target.clips,
                use_authored_bind_pose: target.use_authored_bind_pose,
                lower_body_mirror: target.lower_body_mirror,
                whole_body_mirror: target.whole_body_mirror,
                weapon_guard: skeleton.weapon_guard,
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
        lower_body_mirror: playback.lower_body_mirror,
        whole_body_mirror: playback.whole_body_mirror,
    }
}

fn apply_playback_pose(playback: &mut AnimationPlayback, pose: PlaybackPose) {
    playback.clips = pose.clips;
    playback.use_authored_bind_pose = pose.use_authored_bind_pose;
    playback.lower_body_mirror = pose.lower_body_mirror;
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
        lower_body_mirror: from.lower_body_mirror.lerp(to.lower_body_mirror, progress),
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

fn append_weighted(
    weighted: &mut Vec<WeightedClip>,
    resolved: &ResolvedAnchor,
    time_seconds: f32,
    weight: f32,
) {
    if weight <= f32::EPSILON {
        return;
    }
    if let Some(existing) = weighted.iter_mut().find(|existing| {
        existing.clip.node == resolved.clip.node
            && (existing.time_seconds - time_seconds).abs() < 0.0001
    }) {
        existing.weight += weight;
        if resolved.mirrored {
            existing.mirrored_weight += weight;
        }
    } else {
        weighted.push(WeightedClip {
            clip: resolved.clip.clone(),
            weight,
            time_seconds,
            mirrored_weight: if resolved.mirrored { weight } else { 0.0 },
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
    let Some(start) = start else {
        return;
    };
    match sample.sampling {
        PoseSampling::Anchor => append_weighted(
            weighted,
            &start,
            frame_seconds(start.anchor.frame),
            sample.weight,
        ),
        PoseSampling::Cycle { progress } => append_weighted(
            weighted,
            &start,
            start.clip.cycle_duration_seconds * progress.rem_euclid(1.0),
            sample.weight,
        ),
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
                append_weighted(
                    weighted,
                    &start,
                    frame_seconds(start.anchor.frame),
                    sample.weight,
                );
                return;
            };
            if start.pack_id == end.pack_id && start.anchor.motion == end.anchor.motion {
                let frame = (start.anchor.frame as f32).lerp(end.anchor.frame as f32, progress);
                append_weighted(weighted, &start, frame / ANIMATION_FPS, sample.weight);
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
                let frame = (reference.frame as f32).lerp(end.anchor.frame as f32, progress);
                append_weighted(weighted, &end, frame / ANIMATION_FPS, sample.weight);
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
                let frame = (start.anchor.frame as f32).lerp(reference.frame as f32, progress);
                append_weighted(weighted, &start, frame / ANIMATION_FPS, sample.weight);
            } else {
                append_weighted(
                    weighted,
                    &start,
                    frame_seconds(start.anchor.frame),
                    sample.weight * (1.0 - progress),
                );
                append_weighted(
                    weighted,
                    &end,
                    frame_seconds(end.anchor.frame),
                    sample.weight * progress,
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
mod tests {
    use super::*;

    #[test]
    fn terrain_ik_defaults_off() {
        assert!(!TerrainIkEnabled::default().0);
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
        // Right-lead ducks are semantic mirror counterparts rather than
        // duplicate anchors in the same authored files.
        assert_eq!(root.poses.len() + 3, SemanticPose::HUMANOID_REQUIRED.len());
        assert_eq!(
            root.motions["walk"].path,
            "animations/biped/unarmed/walk.glb"
        );
        assert_eq!(
            root.poses[&SemanticPose::WalkPassing],
            PoseAnchor {
                motion: "walk".to_owned(),
                frame: 8,
            }
        );
        assert_eq!(
            root.poses[&SemanticPose::AttackThrustLeadLeftSwitchContact],
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
        builder.pose("one", 0, "idle_relaxed").unwrap();
        assert_eq!(
            builder.pose("two", 0, "idle_relaxed"),
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
        builder.pose("idle", 0, "idle_relaxed").unwrap();
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
            let cycle_duration_seconds =
                catalog.packs[HUMANOID_UNARMED_PACK].motions[&anchor.motion].last_frame as f32
                    / ANIMATION_FPS;
            let next_node = AnimationNodeIndex::new(runtime.clips.len());
            runtime
                .clips
                .entry((HUMANOID_UNARMED_PACK.to_owned(), anchor.motion.clone()))
                .or_insert(LoadedClip {
                    node: next_node,
                    cycle_duration_seconds,
                });
        }
        runtime
    }

    #[test]
    fn same_motion_span_uses_one_exact_time_sample() {
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
                mirror_lower_body: 0.0,
            },
            None,
        );
        assert_eq!(weighted.len(), 1);
        assert!((weighted[0].time_seconds - 4.0 / 30.0).abs() < 0.0001);
    }

    #[test]
    fn complete_cycle_uses_the_motion_frame_range() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([SemanticPose::WalkContact]);
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::WalkContact,
                sampling: PoseSampling::Cycle { progress: 0.5 },
                weight: 1.0,
                mirror_lower_body: 0.0,
            },
            None,
        );
        assert_eq!(weighted.len(), 1);
        assert!((weighted[0].time_seconds - 16.0 / ANIMATION_FPS).abs() < 0.0001);
    }

    #[test]
    fn complete_cycle_ignores_exported_keys_after_catalog_closure() {
        let catalog = AnimationPackCatalog::default();
        let mut runtime = runtime_with_available([SemanticPose::RunContact]);
        let exported_clip_duration = 64.0 / ANIMATION_FPS;
        runtime
            .clips
            .get_mut(&(HUMANOID_UNARMED_PACK.to_owned(), "run".to_owned()))
            .unwrap()
            .cycle_duration_seconds =
            authoritative_cycle_duration(20, exported_clip_duration).unwrap();
        let mut weighted = Vec::new();
        append_resolved_sample(
            &mut weighted,
            &runtime,
            &catalog,
            HUMANOID_UNARMED_PACK,
            PoseSample {
                pose: SemanticPose::RunContact,
                sampling: PoseSampling::Cycle { progress: 0.999 },
                weight: 1.0,
                mirror_lower_body: 0.0,
            },
            None,
        );
        assert!(
            weighted
                .iter()
                .all(|sample| sample.time_seconds <= 20.0 / ANIMATION_FPS)
        );
        assert!(
            weighted
                .iter()
                .all(|sample| sample.time_seconds < exported_clip_duration)
        );
    }

    #[test]
    fn reference_anchor_keeps_attack_entry_in_one_motion() {
        let catalog = AnimationPackCatalog::default();
        let runtime = runtime_with_available([
            SemanticPose::GuardLeadLeft,
            SemanticPose::AttackThrustLeadLeftStayCommit,
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
                    end: SemanticPose::AttackThrustLeadLeftStayCommit,
                    progress: 0.5,
                },
                weight: 1.0,
                mirror_lower_body: 0.0,
            },
            None,
        );
        assert_eq!(weighted.len(), 1);
        assert!((weighted[0].time_seconds - 2.0 / ANIMATION_FPS).abs() < 0.0001);
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
    fn authored_rig_attaches_to_a_player_with_skeleton_state() {
        let mut world = World::new();
        let mut runtime = AnimationRuntime::default();
        runtime.base_scene = Some(Handle::default());
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
                mirror_lower_body: 0.0,
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
                mirror_lower_body: 0.0,
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
            mirror_lower_body: 0.0,
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
                .node,
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_walk_lead_left".to_owned(),
            )]
                .node,
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
                mirror_lower_body: 0.0,
            },
            PoseSample {
                pose: SemanticPose::GuardLeadLeft,
                sampling: PoseSampling::Span {
                    end: SemanticPose::GuardStrafeLeadLeftLeft,
                    progress: 0.5,
                },
                weight: 0.5,
                mirror_lower_body: 0.0,
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
                .node,
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_walk_lead_right".to_owned(),
            )]
                .node,
            runtime.clips[&(
                HUMANOID_UNARMED_PACK.to_owned(),
                "guard_strafe_lead_right_right".to_owned(),
            )]
                .node,
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
            mirror_lower_body: 0.0,
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
                mirror_lower_body: 0.0,
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
            lower_body_mirror: mirror,
            whole_body_mirror: mirror,
        }
    }

    #[test]
    fn guard_crossfade_activates_at_the_current_effective_pose() {
        let mut playback = AnimationPlayback {
            lower_body_mirror: 0.8,
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
            &clock,
            0.0,
        );

        assert!(playback.presentation_transition.is_some());
        assert!((playback.lower_body_mirror - 0.8).abs() < 0.0001);
        assert!((playback.whole_body_mirror - 0.8).abs() < 0.0001);
    }

    #[test]
    fn guard_crossfade_completes_at_the_latest_target_pose() {
        let mut playback = AnimationPlayback {
            lower_body_mirror: 0.8,
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
            &clock,
            0.0,
        );
        clock.set_fixed_tick(11, PRESENTATION_CROSSFADE_SECONDS);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            &clock,
            0.0,
        );

        assert!(playback.presentation_transition.is_none());
        assert!((playback.lower_body_mirror - 0.2).abs() < 0.0001);
        assert!((playback.whole_body_mirror - 0.2).abs() < 0.0001);
    }

    #[test]
    fn hard_stop_retains_then_releases_the_effective_locomotion_pose() {
        let mut playback = AnimationPlayback {
            lower_body_mirror: 0.8,
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
            &clock,
            0.0,
        );
        assert!(playback.presentation_transition.is_some());
        assert!((playback.lower_body_mirror - 0.8).abs() < 0.0001);

        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.0),
            WeaponGuardState::Lowered,
            false,
            &clock,
            1.0,
        );
        assert!((playback.lower_body_mirror - 0.8).abs() < 0.0001);

        clock.set_fixed_tick(11, PRESENTATION_CROSSFADE_SECONDS);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.0),
            WeaponGuardState::Lowered,
            false,
            &clock,
            0.0,
        );
        assert!(playback.presentation_transition.is_none());
        assert!(playback.lower_body_mirror.abs() < 0.0001);
        assert!(playback.whole_body_mirror.abs() < 0.0001);
    }

    #[test]
    fn reversing_guard_mid_crossfade_has_no_presentation_jump() {
        let mut playback = AnimationPlayback {
            lower_body_mirror: 0.8,
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
            &clock,
            0.0,
        );
        clock.set_fixed_tick(11, 0.09);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.2),
            WeaponGuardState::Raised,
            false,
            &clock,
            0.0,
        );
        let before_reversal = playback.lower_body_mirror;

        clock.set_fixed_tick(12, 0.09);
        update_presentation_crossfade(
            &mut playback,
            mirror_test_pose(0.8),
            WeaponGuardState::Lowered,
            false,
            &clock,
            0.0,
        );

        let transition = playback.presentation_transition.as_ref().unwrap();
        assert!((playback.lower_body_mirror - before_reversal).abs() < 0.0001);
        assert!((transition.from.lower_body_mirror - before_reversal).abs() < 0.0001);
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
}
