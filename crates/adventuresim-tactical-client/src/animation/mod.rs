#![expect(
    unused_imports,
    reason = "the gameplay client, animation viewer, and reflection linker consume different parts of this shared facade"
)]

use std::collections::{BTreeMap, BTreeSet, HashSet};

use adventuresim_tactical_core::animation::AttackCurve;
use adventuresim_tactical_core::prelude::*;
#[cfg(not(target_family = "wasm"))]
use adventuresim_tactical_netcode::message::PlayerInputRequest;
use adventuresim_tactical_netcode::message::SuccessfulAttackResponse;
use bevy::{
    animation::AnimationTargetId,
    asset::{AssetId, LoadState},
    ecs::hierarchy::ChildSpawnerCommands,
    gltf::Gltf,
    prelude::*,
};

pub(crate) fn capture_animation_target_id(target: AnimationTargetId) -> String {
    format!("AnimationTargetId({})", target.0)
}

pub(crate) fn capture_entity_id(entity: Entity) -> String {
    if entity == Entity::PLACEHOLDER {
        "PLACEHOLDER".to_owned()
    } else {
        format!("{}v{}", entity.index_u32(), entity.generation().to_bits())
    }
}

#[cfg(test)]
mod capture_id_tests {
    use super::*;

    #[test]
    fn capture_ids_have_explicit_stable_encodings() {
        assert_eq!(capture_entity_id(Entity::PLACEHOLDER), "PLACEHOLDER");
        assert_eq!(capture_entity_id(Entity::from_raw_u32(1).unwrap()), "1v0");
        let target = AnimationTargetId::from_name(&Name::new("capture-bone"));
        assert_eq!(
            capture_animation_target_id(target),
            "AnimationTargetId(4fa8b1ad-0d70-5ad3-9c90-7f17995275f4)"
        );
    }
}

pub(crate) mod jitter;
pub(crate) mod pose_buffer;
mod procedural;

pub(crate) use procedural::{
    ArmIkState, BoneRole, HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidBone,
    HumanoidIkTargets, HumanoidRig, LegIkDiagnostics, LegIkState, LocomotionBodyResponseState,
    LocomotionHeightState, MhrBone, ProceduralAnimationClock, RaisedFootworkState,
    authored_bind_global, locomotion_support_weights, measured_ankle_sole_offset_metres,
    sole_contact_tolerance_metres,
};
const HUMANOID_UNARMED_PACK: &str = "humanoid_unarmed";
const HUMANOID_2H_CLOSE_PACK: &str = "humanoid_2h_close";
const BIPED_BASE_GLB: &str = "animations/biped/unarmed/base.glb";
const BIPED_GRIP_HILT_GLB: &str = "animations/biped/grip_hilt.glb";
const BIPED_GRIP_POLEARM_GLB: &str = "animations/biped/grip_polearm.glb";
fn animation_frames_per_second() -> f32 {
    runtime_animation_config().playback.frames_per_second
}
// Player transforms sit at the center of the 1.9 m server collider, while
// authored rigs use a floor-level origin. Keep visual feet on the collider's
// lower face so the first-person camera lands at the authored head.
fn player_visual_y_offset_metres() -> f32 {
    runtime_animation_config()
        .playback
        .player_visual_y_offset_metres
}

mod diagnostics;
use diagnostics::log_animation_diagnostics;
mod continuation;
use continuation::{ContinuationSpan, append_continuation_span};
#[cfg(not(target_family = "wasm"))]
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

/// Per-step travel and irreducible stance slip measured from the loaded
/// authored foot curves. Until a motion and rig are both available,
/// presentation retains the shared simulation profile instead of guessing
/// from clip duration.
#[derive(Debug, Clone, Copy)]
struct AuthoredStrideMeasurement {
    step_distance: f32,
    maximum_stance_slip: f32,
}

/// Periodic authored sample phase indexed by a uniformly advancing physical
/// gait phase. Values are deliberately left unwrapped so interpolation remains
/// continuous across the cycle seam; callers wrap only the final result.
#[derive(Debug, Clone)]
struct AuthoredPhaseCurve {
    authored_phases: Vec<f32>,
}

impl AuthoredPhaseCurve {
    fn sample(&self, physical_phase: f32) -> f32 {
        if self.authored_phases.len() < 2 {
            return physical_phase.rem_euclid(1.0);
        }
        let phase = physical_phase.rem_euclid(1.0);
        let coordinate = phase * (self.authored_phases.len() - 1) as f32;
        let lower = coordinate.floor() as usize;
        let upper = (lower + 1).min(self.authored_phases.len() - 1);
        self.authored_phases[lower]
            .lerp(self.authored_phases[upper], coordinate - lower as f32)
            .rem_euclid(1.0)
    }
}

impl AuthoredStrideMeasurement {
    fn lerp(self, other: Self, amount: f32) -> Self {
        Self {
            step_distance: self.step_distance.lerp(other.step_distance, amount),
            maximum_stance_slip: self
                .maximum_stance_slip
                .lerp(other.maximum_stance_slip, amount),
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub(super) struct AuthoredLocomotionStrides {
    walk: Option<AuthoredStrideMeasurement>,
    run: Option<AuthoredStrideMeasurement>,
    strafe: Option<AuthoredStrideMeasurement>,
    skip: Option<AuthoredStrideMeasurement>,
    phase_curves: BTreeMap<String, AuthoredPhaseCurve>,
    measured_clips: BTreeMap<String, AssetId<AnimationClip>>,
}

impl AuthoredLocomotionStrides {
    fn clear_motion(&mut self, motion: &str) {
        self.phase_curves.remove(motion);
        match motion {
            "walk" => self.walk = None,
            "run" => self.run = None,
            "strafe" => self.strafe = None,
            "skip" => self.skip = None,
            _ => {}
        }
    }

    fn sample_phase(&self, pose: SemanticPose, physical_phase: f32) -> f32 {
        let motion = match pose {
            SemanticPose::WalkContact => "walk",
            SemanticPose::RunContact => "run",
            _ => return physical_phase.rem_euclid(1.0),
        };
        self.phase_curves.get(motion).map_or_else(
            || physical_phase.rem_euclid(1.0),
            |curve| curve.sample(physical_phase),
        )
    }

    fn ordinary(&self, speed: f32) -> Option<AuthoredStrideMeasurement> {
        let walk = self.walk?;
        let blend = ((speed - walk_locomotion_profile().reference_speed)
            / (run_locomotion_profile().reference_speed
                - walk_locomotion_profile().reference_speed))
            .clamp(0.0, 1.0);
        if blend <= f32::EPSILON {
            Some(walk)
        } else {
            Some(walk.lerp(self.run?, blend))
        }
    }

    fn combat(&self, direction: Vec2) -> Option<AuthoredStrideMeasurement> {
        let strafe_weight = direction.x.abs();
        let skip_weight = direction.y.abs();
        let total = strafe_weight + skip_weight;
        if total <= f32::EPSILON {
            return None;
        }
        let mut stride = AuthoredStrideMeasurement {
            step_distance: 0.0,
            maximum_stance_slip: 0.0,
        };
        if strafe_weight > f32::EPSILON {
            let strafe = self.strafe?;
            stride.step_distance += strafe.step_distance * strafe_weight;
            stride.maximum_stance_slip += strafe.maximum_stance_slip * strafe_weight;
        }
        if skip_weight > f32::EPSILON {
            let skip = self.skip?;
            stride.step_distance += skip.step_distance * skip_weight;
            stride.maximum_stance_slip += skip.maximum_stance_slip * skip_weight;
        }
        stride.step_distance /= total;
        stride.maximum_stance_slip /= total;
        Some(stride)
    }
}

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
    /// Combat upper-body motion owns the pelvis rotation while the paired
    /// locomotion layer owns its translation.
    CombatUpper,
    CombatLower,
    /// Static equipment grip owns only the subtree below the right wrist.
    MainHand,
    /// Static equipment grip owns only the subtree below the left wrist.
    Offhand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WeaponGrip {
    Hilt,
    Polearm,
}

impl WeaponGrip {
    const ALL: [Self; 2] = [Self::Hilt, Self::Polearm];

    const fn path(self) -> &'static str {
        match self {
            Self::Hilt => BIPED_GRIP_HILT_GLB,
            Self::Polearm => BIPED_GRIP_POLEARM_GLB,
        }
    }
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
    requested_grips: BTreeMap<WeaponGrip, Handle<Gltf>>,
    processed_grips: BTreeSet<WeaponGrip>,
    unavailable_grips: BTreeSet<WeaponGrip>,
    grips: BTreeMap<WeaponGrip, LoadedClip>,
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
    extrapolated_spans: Vec<ExtrapolatedSpan>,
    continuation_spans: Vec<ContinuationSpan>,
    use_authored_bind_pose: bool,
    pub(super) whole_body_mirror: f32,
    pub(super) foot_ik_weights: Vec2,
    weapon_guard: WeaponGuardState,
    ordinary_locomotion_active: bool,
    direct_sampling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoseSamplingCadence {
    Buffered,
    Direct,
}

impl AnimationPlayback {
    pub(super) fn authored_pose_is_ready(&self) -> bool {
        !self.use_authored_bind_pose
            && (!self.clips.is_empty()
                || !self.extrapolated_spans.is_empty()
                || !self.continuation_spans.is_empty())
    }

    pub(super) fn presentation_is_settled(&self) -> bool {
        self.authored_pose_is_ready()
    }

    fn sampling_cadence(&self) -> PoseSamplingCadence {
        if !self.direct_sampling
            && self.extrapolated_spans.is_empty()
            && self.continuation_spans.is_empty()
        {
            PoseSamplingCadence::Buffered
        } else {
            // Every action segment follows the authoritative action phase.
            // Switching only its simple spans back to the 30 Hz clip grid
            // aliases fast weapon motion and creates a cadence discontinuity.
            PoseSamplingCadence::Direct
        }
    }
}

impl Default for AnimationPlayback {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            extrapolated_spans: Vec::new(),
            continuation_spans: Vec::new(),
            use_authored_bind_pose: true,
            whole_body_mirror: 0.0,
            foot_ik_weights: Vec2::ZERO,
            weapon_guard: WeaponGuardState::Lowered,
            ordinary_locomotion_active: false,
            direct_sampling: false,
        }
    }
}

#[derive(Debug, Clone)]
struct WeightedClip {
    clip: LoadedClip,
    weight: f32,
    time_seconds: f32,
    mirrored_weight: f32,
    /// Unwarped physical phase for ordinary sparse-pose locomotion. Its
    /// presence selects the Bevy-side semantic-anchor sampler while
    /// `time_seconds` retains the measured authored phase coordinate.
    locomotion_phase: Option<f32>,
}

#[derive(Debug, Clone)]
struct ExtrapolatedSpan {
    start: LoadedClip,
    start_time_seconds: f32,
    end: LoadedClip,
    end_time_seconds: f32,
    coordinate: f32,
    weight: f32,
    mirrored_weight: f32,
}

#[derive(Debug, Clone)]
struct PlaybackPose {
    clips: Vec<WeightedClip>,
    extrapolated_spans: Vec<ExtrapolatedSpan>,
    continuation_spans: Vec<ContinuationSpan>,
    use_authored_bind_pose: bool,
    whole_body_mirror: f32,
    foot_ik_weights: Vec2,
    direct_sampling: bool,
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
    pub(super) velocity_change: Vec3,
    pub(super) body_part: BodyPart,
}

mod full_ragdoll;
mod loading;
pub(crate) mod secondary_physics;
use loading::*;
#[expect(
    clippy::type_complexity,
    reason = "the Bevy query couples each presented skeleton with its route, inventory, and playback state"
)]
fn evaluate_skeletons(
    mut commands: Commands,
    catalog: Res<AnimationPackCatalog>,
    runtime: Res<AnimationRuntime>,
    locomotion_strides: Res<AuthoredLocomotionStrides>,
    players: Query<
        (
            Entity,
            &PresentedSkeleton,
            &semantic_route::SemanticRouteTrace,
            Option<&InventoryItems>,
            Option<&mut AnimationPlayback>,
        ),
        With<Player>,
    >,
    weapons: Query<(&WeaponItem, &ItemProperties)>,
    equip_slots: Query<&EquipSlot>,
) {
    for (entity, skeleton, route_trace, inventory, playback) in players {
        // The preceding chained system directly routes authoritative
        // presentation state into deterministic semantic samples.
        let evaluation = route_trace.evaluation.clone();
        let samples = if evaluation.action.is_empty() {
            &evaluation.base
        } else {
            &evaluation.action
        };
        let mut weighted = Vec::<WeightedClip>::new();
        let mut extrapolated_spans = Vec::<ExtrapolatedSpan>::new();
        let mut continuation_spans = Vec::<ContinuationSpan>::new();
        let split_combat_pelvis = combat_pelvis_is_component_blended(skeleton, &evaluation);
        let base_layer = if split_combat_pelvis {
            ClipLayer::CombatUpper
        } else if !evaluation.lower_body.is_empty() {
            ClipLayer::Upper
        } else {
            ClipLayer::Whole
        };
        let sample_resolver = PoseSampleResolver {
            runtime: &runtime,
            catalog: &catalog,
            pack: &route_trace.inputs.pack,
            locomotion_strides: &locomotion_strides,
        };
        for sample in samples {
            sample_resolver.append_layer(
                &mut weighted,
                &mut extrapolated_spans,
                &mut continuation_spans,
                *sample,
                base_layer,
            );
        }
        let lower_layer = if split_combat_pelvis {
            ClipLayer::CombatLower
        } else {
            ClipLayer::Lower
        };
        for sample in &evaluation.lower_body {
            sample_resolver.append_layer(
                &mut weighted,
                &mut extrapolated_spans,
                &mut continuation_spans,
                *sample,
                lower_layer,
            );
        }
        let whole_body_mirror = {
            let total = weighted.iter().map(|clip| clip.weight).sum::<f32>()
                + extrapolated_spans
                    .iter()
                    .map(|span| span.weight)
                    .sum::<f32>()
                + continuation_spans
                    .iter()
                    .map(|span| span.weight)
                    .sum::<f32>();
            if total > f32::EPSILON {
                ((weighted
                    .iter()
                    .map(|clip| clip.mirrored_weight)
                    .sum::<f32>()
                    + extrapolated_spans
                        .iter()
                        .map(|span| span.mirrored_weight)
                        .sum::<f32>()
                    + continuation_spans
                        .iter()
                        .map(|span| span.mirrored_weight)
                        .sum::<f32>())
                    / total)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        if let Some((weapon, properties)) = equipped_main_weapon(inventory, &weapons, &equip_slots)
            && let Some(clip) = runtime.grips.get(&weapon_grip(&weapon.skill_weights))
        {
            let uses_offhand =
                weapon_uses_offhand(&properties.id, offhand_is_empty(inventory, &equip_slots));
            let authored_pose_owns_hands = authored_pose_owns_hands(samples);
            for layer in weapon_grip_layers(authored_pose_owns_hands, uses_offhand)
                .into_iter()
                .flatten()
            {
                append_weighted_clip(
                    &mut weighted,
                    &clip.at_anchor_layer(0, layer),
                    false,
                    0.0,
                    1.0,
                    None,
                );
            }
        }
        let target = PlaybackPose {
            use_authored_bind_pose: weighted.is_empty()
                && extrapolated_spans.is_empty()
                && continuation_spans.is_empty(),
            whole_body_mirror,
            foot_ik_weights: semantic_foot_ik_weights(&evaluation),
            direct_sampling: !evaluation.action.is_empty(),
            clips: weighted,
            extrapolated_spans,
            continuation_spans,
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
                extrapolated_spans: target.extrapolated_spans,
                continuation_spans: target.continuation_spans,
                use_authored_bind_pose: target.use_authored_bind_pose,
                whole_body_mirror: target.whole_body_mirror,
                foot_ik_weights: target.foot_ik_weights,
                weapon_guard: skeleton.weapon_guard(),
                ordinary_locomotion_active,
                direct_sampling: target.direct_sampling,
            });
        }
    }
}

fn combat_pelvis_is_component_blended(
    skeleton: &SkeletonState,
    evaluation: &AnimationEvaluation,
) -> bool {
    !evaluation.lower_body.is_empty()
        && skeleton.posture() == Posture::Upright
        && skeleton.weapon_guard() == WeaponGuardState::Raised
        && !skeleton.is_quickstep()
        && !skeleton.is_posture_transitioning()
}

fn weapon_grip(skill_weights: &[f32; 9]) -> WeaponGrip {
    if skill_weights[0] > f32::EPSILON {
        WeaponGrip::Polearm
    } else {
        WeaponGrip::Hilt
    }
}

fn held_item(
    inventory: Option<&InventoryItems>,
    equip_slots: &Query<&EquipSlot>,
    slot: EquipSlot,
) -> Option<Entity> {
    inventory?.iter().find(|&entity| {
        equip_slots
            .get(entity)
            .is_ok_and(|equipped| *equipped == slot)
    })
}

fn equipped_main_weapon<'a>(
    inventory: Option<&InventoryItems>,
    weapons: &'a Query<(&WeaponItem, &ItemProperties)>,
    equip_slots: &Query<&EquipSlot>,
) -> Option<(&'a WeaponItem, &'a ItemProperties)> {
    held_item(inventory, equip_slots, EquipSlot::HoldingRight)
        .and_then(|entity| weapons.get(entity).ok())
}

fn offhand_is_empty(inventory: Option<&InventoryItems>, equip_slots: &Query<&EquipSlot>) -> bool {
    held_item(inventory, equip_slots, EquipSlot::HoldingLeft).is_none()
}

fn authored_pose_owns_hands(samples: &[PoseSample]) -> bool {
    samples
        .iter()
        .any(|sample| sample.pose.is_main_hand_attack())
}

fn weapon_uses_offhand(item_id: &str, offhand_is_empty: bool) -> bool {
    offhand_is_empty
        && item_catalog::weapon_handling(item_id) == Some(item_catalog::WeaponHandling::TwoHanded)
}

fn weapon_grip_layers(
    authored_pose_owns_hands: bool,
    uses_offhand: bool,
) -> [Option<ClipLayer>; 2] {
    match (authored_pose_owns_hands, uses_offhand) {
        (true, true) => [None, None],
        (false, true) => [Some(ClipLayer::MainHand), Some(ClipLayer::Offhand)],
        _ => [Some(ClipLayer::MainHand), None],
    }
}

fn equipped_animation_pack(
    inventory: Option<&InventoryItems>,
    items: &Query<&ItemProperties, With<WeaponItem>>,
    equip_slots: &Query<&EquipSlot>,
) -> &'static str {
    let Some(properties) = held_item(inventory, equip_slots, EquipSlot::HoldingRight)
        .and_then(|entity| items.get(entity).ok())
    else {
        return HUMANOID_UNARMED_PACK;
    };
    animation_pack_for_weapon(
        &properties.id,
        weapon_uses_offhand(&properties.id, offhand_is_empty(inventory, equip_slots)),
    )
}

fn animation_pack_for_weapon(item_id: &str, uses_offhand: bool) -> &'static str {
    if item_catalog::weapon_handling(item_id) == Some(item_catalog::WeaponHandling::TwoHanded)
        && !uses_offhand
    {
        return HUMANOID_UNARMED_PACK;
    }
    if let Some(pack) = item_catalog::weapon_animation_pack(item_id) {
        return pack;
    }
    match item_catalog::weapon_handling(item_id) {
        Some(item_catalog::WeaponHandling::TwoHanded) => HUMANOID_2H_CLOSE_PACK,
        _ => HUMANOID_UNARMED_PACK,
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
    playback.extrapolated_spans = pose.extrapolated_spans;
    playback.continuation_spans = pose.continuation_spans;
    playback.use_authored_bind_pose = pose.use_authored_bind_pose;
    playback.whole_body_mirror = pose.whole_body_mirror;
    playback.foot_ik_weights = pose.foot_ik_weights;
    playback.direct_sampling = pose.direct_sampling;
}

/// IK ownership follows the direct semantic locomotion samples.
/// The curve shapes are animation metadata: walk keeps one supported foot,
/// run has genuine intervals with neither foot loaded, and idle loads both.
fn semantic_foot_ik_weights(evaluation: &AnimationEvaluation) -> Vec2 {
    let samples = if !evaluation.lower_body.is_empty() {
        &evaluation.lower_body
    } else if evaluation.action.is_empty() {
        &evaluation.base
    } else {
        return Vec2::ZERO;
    };
    let mut result = Vec2::ZERO;
    let mut total = 0.0;
    for sample in samples {
        let mut weights = match (sample.pose, sample.sampling) {
            (SemanticPose::IdleRelaxed, _) => Vec2::ONE,
            (SemanticPose::CombatStance, _) => Vec2::ONE,
            (
                SemanticPose::StrafeCycle | SemanticPose::SkipCycle,
                PoseSampling::Cycle { phase },
            ) => combat_cycle_ik_weights(phase),
            (
                SemanticPose::QuickstepForwardTakeoff
                | SemanticPose::QuickstepRightTakeoff
                | SemanticPose::QuickstepLeftTakeoff
                | SemanticPose::QuickstepBackTakeoff,
                PoseSampling::Timeline { .. },
            ) => Vec2::ZERO,
            (SemanticPose::WalkContact, PoseSampling::Cycle { phase }) => {
                let (left, right) = gait_support_weights(walk_locomotion_profile(), phase);
                Vec2::new(left, right)
            }
            (SemanticPose::RunContact, PoseSampling::Cycle { phase }) => {
                // Simulation support spans the complete stance interval. IK
                // is animation metadata and should only become strong near
                // the authored contact itself, after the foot has descended.
                let contact_profile = LocomotionProfile {
                    support_phase_radius: 0.11,
                    ..run_locomotion_profile()
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

/// Select the single authored combat contact. Frames 0/12 are swing midpoints
/// and frames 6/18 switch contact. The pose buffer interpolates continuously
/// into each terrain-conformed target; support identity itself must not overlap
/// because an outgoing foot is free as soon as the incoming foot contacts.
fn combat_cycle_ik_weights(phase: f32) -> Vec2 {
    let phase = phase.rem_euclid(1.0);
    if (0.25..0.75).contains(&phase) {
        Vec2::X
    } else {
        Vec2::Y
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the Bevy query selects newly transformed nodes that do not yet own a captured bind transform"
)]
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
    if event.impact_velocity_change.length_squared() > f32::EPSILON {
        commands
            .entity(event.impact_recipient)
            .insert(ImpactReaction {
                remaining: 0.22,
                velocity_change: event.impact_velocity_change,
                body_part: event.body_part,
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
    timeline_reversed: bool,
}

fn resolve_anchor<'a>(
    runtime: &'a AnimationRuntime,
    catalog: &'a AnimationPackCatalog,
    requested_pack: &str,
    pose: SemanticPose,
) -> Option<ResolvedAnchor<'a>> {
    let ordinary = resolve_library_anchor(runtime, catalog, requested_pack, pose);
    if ordinary
        .as_ref()
        .is_some_and(|resolved| resolved.semantic == pose)
    {
        return ordinary;
    }

    // A directional quickstep may omit its opposite file. In that case its
    // takeoff resolves to the authored opposite contact and its contact to the
    // opposite takeoff, which samples that one clip backward. Exact authored
    // direction clips always win above.
    if let Some(reversed_pose) = reversed_quickstep_pose(pose)
        && let Some(reversed) =
            resolve_library_anchor(runtime, catalog, requested_pack, reversed_pose)
        && reversed.semantic == reversed_pose
    {
        return Some(ResolvedAnchor {
            timeline_reversed: true,
            ..reversed
        });
    }

    ordinary
}

fn resolve_library_anchor<'a>(
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
        timeline_reversed: false,
    })
}

fn reversed_quickstep_pose(pose: SemanticPose) -> Option<SemanticPose> {
    use SemanticPose::*;
    Some(match pose {
        QuickstepForwardTakeoff => QuickstepBackTakeoff,
        QuickstepForwardContact => QuickstepBackContact,
        QuickstepBackTakeoff => QuickstepForwardTakeoff,
        QuickstepBackContact => QuickstepForwardContact,
        QuickstepRightTakeoff => QuickstepLeftTakeoff,
        QuickstepRightContact => QuickstepLeftContact,
        QuickstepLeftTakeoff => QuickstepRightTakeoff,
        QuickstepLeftContact => QuickstepRightContact,
        _ => return None,
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
        None,
    );
}

fn append_weighted_clip(
    weighted: &mut Vec<WeightedClip>,
    clip: &LoadedClip,
    mirrored: bool,
    time_seconds: f32,
    weight: f32,
    locomotion_phase: Option<f32>,
) {
    if weight <= f32::EPSILON {
        return;
    }
    if let Some(existing) = weighted.iter_mut().find(|existing| {
        existing.clip.handle.id() == clip.handle.id()
            && existing.clip.layer == clip.layer
            && (existing.time_seconds - time_seconds).abs() < 0.0001
            && existing.locomotion_phase == locomotion_phase
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
            locomotion_phase,
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
    let mut extrapolated_spans = Vec::new();
    let mut continuation_spans = Vec::new();
    let locomotion_strides = AuthoredLocomotionStrides::default();
    PoseSampleResolver {
        runtime,
        catalog,
        pack,
        locomotion_strides: &locomotion_strides,
    }
    .append_layer(
        weighted,
        &mut extrapolated_spans,
        &mut continuation_spans,
        sample,
        ClipLayer::Whole,
    );
}

#[derive(Clone, Copy)]
struct PoseSampleResolver<'a> {
    runtime: &'a AnimationRuntime,
    catalog: &'a AnimationPackCatalog,
    pack: &'a str,
    locomotion_strides: &'a AuthoredLocomotionStrides,
}

impl PoseSampleResolver<'_> {
    fn append_layer(
        &self,
        weighted: &mut Vec<WeightedClip>,
        extrapolated_spans: &mut Vec<ExtrapolatedSpan>,
        continuation_spans: &mut Vec<ContinuationSpan>,
        sample: PoseSample,
        layer: ClipLayer,
    ) {
        let Self {
            runtime,
            catalog,
            pack,
            locomotion_strides,
        } = *self;
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
                let sample_phase = locomotion_strides.sample_phase(sample.pose, phase);
                let clip = start.clip.at_anchor_layer(start.anchor.frame, layer);
                append_weighted_clip(
                    weighted,
                    &clip,
                    start.mirrored,
                    start.clip.duration_seconds * sample_phase,
                    sample.weight,
                    matches!(
                        sample.pose,
                        SemanticPose::WalkContact | SemanticPose::RunContact
                    )
                    .then_some(phase.rem_euclid(1.0)),
                );
            }
            PoseSampling::Timeline { progress } => {
                let progress = progress.clamp(0.0, 1.0);
                let timeline = if start.timeline_reversed {
                    1.0 - progress
                } else {
                    progress
                };
                let clip = start.clip.at_anchor_layer(start.anchor.frame, layer);
                append_weighted_clip(
                    weighted,
                    &clip,
                    start.mirrored,
                    start.clip.duration_seconds * timeline,
                    sample.weight,
                    None,
                );
            }
            PoseSampling::Span { end, progress } => {
                let end_pose = end;
                let progress = progress.clamp(0.0, 1.0);
                let end = resolve_anchor(runtime, catalog, pack, end_pose);
                let Some(end) = end else {
                    append_weighted_anchor(
                        weighted,
                        &start,
                        start.anchor.frame,
                        sample.weight,
                        layer,
                    );
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
            PoseSampling::CurveSpan { end, coordinate } => {
                let end_pose = end;
                let coordinate = coordinate.clamp(
                    -AttackCurve::maximum_drawback(),
                    1.0 + AttackCurve::maximum_overshoot(),
                );
                let Some(end) = resolve_anchor(runtime, catalog, pack, end_pose) else {
                    append_weighted_anchor(
                        weighted,
                        &start,
                        start.anchor.frame,
                        sample.weight,
                        layer,
                    );
                    return;
                };
                let (span_start, start_frame, span_end, end_frame) = if start.pack_id == end.pack_id
                    && start.anchor.motion == end.anchor.motion
                {
                    (
                        start.clone(),
                        start.anchor.frame,
                        end.clone(),
                        end.anchor.frame,
                    )
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
                    (end.clone(), reference.frame, end.clone(), end.anchor.frame)
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
                    (
                        start.clone(),
                        start.anchor.frame,
                        start.clone(),
                        reference.frame,
                    )
                } else {
                    (
                        start.clone(),
                        start.anchor.frame,
                        end.clone(),
                        end.anchor.frame,
                    )
                };
                extrapolated_spans.push(ExtrapolatedSpan {
                    start: span_start.clip.at_anchor_layer(start_frame, layer),
                    start_time_seconds: frame_seconds(start_frame),
                    end: span_end.clip.at_anchor_layer(end_frame, layer),
                    end_time_seconds: frame_seconds(end_frame),
                    coordinate,
                    weight: sample.weight,
                    mirrored_weight: if span_start.mirrored {
                        sample.weight
                    } else {
                        0.0
                    },
                });
            }
            sampling @ PoseSampling::ContinuationSpan { .. } => append_continuation_span(
                self,
                weighted,
                continuation_spans,
                &start,
                sample,
                sampling,
                layer,
            ),
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
            Transform::from_xyz(0.0, player_visual_y_offset_metres(), 0.0),
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
        assert!(root.motions.contains_key("thrust"));
        assert!(root.motions.contains_key("offhand"));
        assert!(!root.motions.keys().any(|name| name.starts_with("jump_")));
    }

    #[test]
    fn attack_semantics_use_the_canonical_motion_names() {
        let catalog = AnimationPackCatalog::biped_root().unwrap();
        let root = &catalog.packs[HUMANOID_UNARMED_PACK];
        assert_eq!(root.poses[&SemanticPose::AttackThrust].motion, "thrust");
        assert_eq!(root.poses[&SemanticPose::AttackSwing].motion, "swing");
        assert_eq!(root.poses[&SemanticPose::ContinueSwing].motion, "swing");
        assert_eq!(root.poses[&SemanticPose::AttackOffhand].motion, "offhand");
    }

    #[test]
    fn combat_cycle_ik_switches_exclusive_support_at_authored_contacts() {
        assert_eq!(combat_cycle_ik_weights(0.0), Vec2::Y);
        assert_eq!(combat_cycle_ik_weights(0.125), Vec2::Y);
        assert_eq!(combat_cycle_ik_weights(0.25), Vec2::X);
        assert_eq!(combat_cycle_ik_weights(0.375), Vec2::X);
        assert_eq!(combat_cycle_ik_weights(0.5), Vec2::X);
        assert_eq!(combat_cycle_ik_weights(0.625), Vec2::X);
        assert_eq!(combat_cycle_ik_weights(0.75), Vec2::Y);
        assert_eq!(combat_cycle_ik_weights(0.875), Vec2::Y);
    }

    #[test]
    fn planted_and_moving_combat_both_component_blend_the_pelvis() {
        let mut planted = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        planted.begin_attack(AttackSpec::default(), 0, 100).unwrap();
        let planted_evaluation = AnimationEvaluation::from_skeleton(&planted);
        assert!(!planted_evaluation.action.is_empty());
        assert!(!planted_evaluation.lower_body.is_empty());
        assert!(combat_pelvis_is_component_blended(
            &planted,
            &planted_evaluation
        ));

        let moving = planted
            .clone()
            .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::X, 1.0));
        let moving_evaluation = AnimationEvaluation::from_skeleton(&moving);
        assert!(!moving_evaluation.action.is_empty());
        assert!(combat_pelvis_is_component_blended(
            &moving,
            &moving_evaluation
        ));

        let mut quickstep = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        quickstep
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();
        let quickstep_evaluation = AnimationEvaluation::from_skeleton(&quickstep);
        assert!(!combat_pelvis_is_component_blended(
            &quickstep,
            &quickstep_evaluation
        ));
    }

    #[test]
    fn authored_quickstep_timeline_never_requests_leg_ik() {
        let mut state = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        state
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();
        // The first half is the interpolation from the planted stance into
        // authored takeoff. Once frame 0 owns the lower body, every authored
        // quickstep pose remains completely free of leg IK.
        for tick in [100, 150, 199] {
            state.advance_action(tick);
            let evaluation = AnimationEvaluation::from_skeleton(&state);
            assert_eq!(semantic_foot_ik_weights(&evaluation), Vec2::ZERO, "{tick}");
        }
    }
}

#[cfg(test)]
mod tests;
