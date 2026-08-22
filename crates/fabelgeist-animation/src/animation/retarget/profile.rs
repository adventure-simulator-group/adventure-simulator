//! Declarative description of how a rig maps onto the humanoid vocabulary.
//!
//! Everything source-specific lives here. A [`RigProfile`] is data — it
//! serializes, it can ship next to an asset, and it can be written by hand for
//! a rig nobody anticipated. The retargeter never reads it directly; it reads
//! the [`ResolvedRig`](super::ResolvedRig) that a profile produces against a
//! concrete skeleton.

use fabelgeist_math::vector::Vec4;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::semantic::{HumanoidChain, HumanoidJoint};

/// Which of a rig's joints plays a humanoid role.
///
/// Several names may be listed: rigs vary between exports (`mixamorig:Hips`
/// versus `mixamorig1:Hips`), and the first name that the skeleton actually
/// has wins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointBinding {
    pub names: Vec<String>,
    /// Retargeting fails if a required joint is absent from the skeleton.
    #[serde(default)]
    pub required: bool,
    /// Extra rotation applied on top of the rest-pose difference the
    /// retargeter derives, for rigs whose rest pose is not a usable reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<Vec4>,
    /// Overrides the profile-wide translation policy for this joint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<TranslationPolicy>,
    /// The axis this joint bends about, in its own frame, for a joint the rig
    /// treats as a hinge.
    ///
    /// A rig usually knows this exactly — MHR's model definition says
    /// `l_lowarm.rz = l_elbow_bend`, so the axis is local Z — and it is the
    /// one thing bone directions cannot recover: straightening a limb fixes
    /// where the bones point but leaves the roll about them free, and roll is
    /// what aims a hinge. Declared here, a
    /// [`TPose`](ReferencePose::TPose) reference can solve that roll instead
    /// of inheriting whatever the rig was bound with.
    ///
    /// Signed as the axis of positive flexion, matching
    /// [`HumanoidJoint::t_pose_hinge`](super::semantic::HumanoidJoint::t_pose_hinge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinge: Option<fabelgeist_math::vector::Vec3>,
}

impl JointBinding {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            names: vec![name.into()],
            required: false,
            correction: None,
            translation: None,
            hinge: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_alias(mut self, name: impl Into<String>) -> Self {
        self.names.push(name.into());
        self
    }

    pub fn with_correction(mut self, correction: Vec4) -> Self {
        self.correction = Some(correction);
        self
    }

    pub fn with_translation(mut self, policy: TranslationPolicy) -> Self {
        self.translation = Some(policy);
        self
    }

    /// Declares the axis this joint bends about, in its own frame.
    pub fn with_hinge(mut self, hinge: fabelgeist_math::vector::Vec3) -> Self {
        self.hinge = Some(hinge);
        self
    }
}

/// A rig's own joints for one humanoid chain, root-most first.
///
/// Declaring a chain lets a rig expose more joints than the vocabulary names —
/// four spine joints where the vocabulary has three — and lets the retargeter
/// spread motion along the chain instead of dropping the extras.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainBinding {
    pub joints: Vec<String>,
}

impl ChainBinding {
    pub fn new<I, S>(joints: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            joints: joints.into_iter().map(Into::into).collect(),
        }
    }
}

/// Where a rig keeps locomotion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RootSource {
    /// The rig has no separate locomotion joint; the pelvis carries it.
    Pelvis,
    /// A dedicated joint above the pelvis (`Root`, `Armature`, `Reference`…).
    Joint(String),
    /// The rig is authored in place and has no locomotion at all.
    None,
}

impl Default for RootSource {
    fn default() -> Self {
        RootSource::Pelvis
    }
}

/// The posture a rig's motion is measured against.
///
/// Retargeting transfers motion *away from a reference*, and that only means
/// the same thing on two rigs if the reference is the same posture on both. A
/// bind pose is not: Mixamo binds straight-armed in a T-pose, MHR binds in an
/// A-pose with the elbow already bent 35 degrees. Measuring both against their
/// own bind poses asks the target's elbow to bend sideways by the difference,
/// which a hinge cannot do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReferencePose {
    /// The rig's bind pose, as authored. Right when both rigs bind alike.
    Bind,
    /// Straighten the rig into the canonical T-pose first, from its own
    /// geometry. Costs no authoring and is a no-op on a rig already T-posed,
    /// so it is the safe choice when the two rigs bind differently.
    TPose,
    /// Explicit local rotations from bind, per role, for a rig whose posture
    /// cannot be worked out from bone directions alone.
    Pose(IndexMap<HumanoidJoint, Vec4>),
}

impl Default for ReferencePose {
    fn default() -> Self {
        ReferencePose::Bind
    }
}

/// How one rig names the humanoid body.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RigProfile {
    pub name: String,
    pub joints: IndexMap<HumanoidJoint, JointBinding>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub chains: IndexMap<HumanoidChain, ChainBinding>,
    #[serde(default)]
    pub root: RootSource,
    /// The posture this rig's motion is measured against.
    #[serde(default)]
    pub reference: ReferencePose,
    /// Rotation taking this rig's space to engine space, for assets an
    /// importer could not normalize. Identity for anything well-behaved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<Vec4>,
    /// Joint names whose presence identifies this rig, for optional detection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
}

impl RigProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn with_joint(mut self, joint: HumanoidJoint, binding: JointBinding) -> Self {
        self.joints.insert(joint, binding);
        self
    }

    /// Binds a role to a single joint name, the common case.
    pub fn with(mut self, joint: HumanoidJoint, name: impl Into<String>) -> Self {
        self.joints.insert(joint, JointBinding::new(name));
        self
    }

    /// Binds a role to a joint that must exist.
    pub fn with_required(mut self, joint: HumanoidJoint, name: impl Into<String>) -> Self {
        self.joints
            .insert(joint, JointBinding::new(name).required());
        self
    }

    pub fn with_chain(mut self, chain: HumanoidChain, binding: ChainBinding) -> Self {
        self.chains.insert(chain, binding);
        self
    }

    pub fn with_root(mut self, root: RootSource) -> Self {
        self.root = root;
        self
    }

    pub fn with_reference(mut self, reference: ReferencePose) -> Self {
        self.reference = reference;
        self
    }

    pub fn with_markers<I, S>(mut self, markers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.markers = markers.into_iter().map(Into::into).collect();
        self
    }

    pub fn binding(&self, joint: HumanoidJoint) -> Option<&JointBinding> {
        self.joints.get(&joint)
    }
}

/// What to do with a joint's translation channel.
///
/// Rotations transfer between bodies of any proportion; translations do not.
/// Copying every translation track is how retargeted animation ends up with
/// dislocated limbs, so translation is opt-in per joint or per class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranslationPolicy {
    /// Keep the target's rest translation. The default for limbs.
    Ignore,
    /// Take the source translation as-is, in engine units.
    Copy,
    /// Take the source translation scaled by the rig size ratio.
    Scaled,
    /// Scaled translation on the locomotion joint only.
    RootOnly,
    /// Scaled translation on the pelvis only. The usual choice.
    PelvisOnly,
}

impl Default for TranslationPolicy {
    fn default() -> Self {
        TranslationPolicy::PelvisOnly
    }
}

/// The body measurement used to compare two rigs' sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleMeasure {
    /// Rest distance from pelvis to head. Robust and available on any humanoid.
    PelvisToHead,
    /// Rest distance from pelvis down to the foot.
    LegLength,
    /// Vertical extent of the whole rest skeleton, the last resort.
    SkeletonHeight,
}

impl Default for ScaleMeasure {
    fn default() -> Self {
        ScaleMeasure::PelvisToHead
    }
}

/// How to reconcile two rigs' unit scales and sizes.
///
/// Derived from the rest poses rather than from the source file's scene scale,
/// which is unreliable across exporters and says nothing about body size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ScalePolicy {
    /// Treat both rigs as already sharing units and size.
    None,
    /// A ratio supplied by the profile, target units per source unit.
    Fixed(f32),
    /// Measure both rest poses and divide.
    Auto(ScaleMeasure),
}

impl Default for ScalePolicy {
    fn default() -> Self {
        ScalePolicy::Auto(ScaleMeasure::PelvisToHead)
    }
}

/// Which components of locomotion to separate from the pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootMotionChannels {
    /// Displacement across the ground plane.
    pub horizontal: bool,
    /// Displacement along the up axis. Usually left in the pose so crouches
    /// and steps survive.
    pub vertical: bool,
    /// Turning around the up axis.
    pub yaw: bool,
}

impl Default for RootMotionChannels {
    fn default() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            yaw: true,
        }
    }
}

/// What becomes of the source rig's locomotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootMotionPolicy {
    /// Leave locomotion in the pose, as pelvis translation.
    Keep,
    /// Move it into the clip's root motion track.
    Extract(RootMotionChannels),
    /// Remove it entirely, producing an in-place clip.
    Strip(RootMotionChannels),
}

impl Default for RootMotionPolicy {
    fn default() -> Self {
        RootMotionPolicy::Extract(RootMotionChannels::default())
    }
}

impl RootMotionPolicy {
    pub fn channels(self) -> Option<RootMotionChannels> {
        match self {
            RootMotionPolicy::Keep => None,
            RootMotionPolicy::Extract(channels) | RootMotionPolicy::Strip(channels) => {
                Some(channels)
            }
        }
    }

    pub fn keeps_track(self) -> bool {
        matches!(self, RootMotionPolicy::Extract(_))
    }
}

/// A world axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Default for Axis {
    fn default() -> Self {
        Axis::Y
    }
}

impl Axis {
    pub fn vector(self) -> fabelgeist_math::vector::Vec3 {
        match self {
            Axis::X => fabelgeist_math::vector::Vec3::new(1.0, 0.0, 0.0),
            Axis::Y => fabelgeist_math::vector::Vec3::new(0.0, 1.0, 0.0),
            Axis::Z => fabelgeist_math::vector::Vec3::new(0.0, 0.0, 1.0),
        }
    }
}

/// Policy that is about the transfer itself rather than about either rig.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetargetSettings {
    /// Applies to every joint without its own binding-level override.
    #[serde(default)]
    pub translation: TranslationPolicy,
    /// Per-role overrides, which beat both the default and the binding.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub joint_translation: IndexMap<HumanoidJoint, TranslationPolicy>,
    #[serde(default)]
    pub scale: ScalePolicy,
    #[serde(default)]
    pub root_motion: RootMotionPolicy,
    /// The up axis of engine space, used to split locomotion.
    #[serde(default)]
    pub up: Axis,
    /// Whether a source joint whose role the target rig lacks is an error.
    /// Off by default: extra source bones are normal and ignoring them is safe.
    #[serde(default)]
    pub strict: bool,
}

impl Default for RetargetSettings {
    fn default() -> Self {
        Self {
            translation: TranslationPolicy::default(),
            joint_translation: IndexMap::new(),
            scale: ScalePolicy::default(),
            root_motion: RootMotionPolicy::default(),
            up: Axis::default(),
            strict: false,
        }
    }
}

impl RetargetSettings {
    pub fn with_translation(mut self, policy: TranslationPolicy) -> Self {
        self.translation = policy;
        self
    }

    pub fn with_joint_translation(
        mut self,
        joint: HumanoidJoint,
        policy: TranslationPolicy,
    ) -> Self {
        self.joint_translation.insert(joint, policy);
        self
    }

    pub fn with_scale(mut self, policy: ScalePolicy) -> Self {
        self.scale = policy;
        self
    }

    pub fn with_root_motion(mut self, policy: RootMotionPolicy) -> Self {
        self.root_motion = policy;
        self
    }
}

/// A complete recipe: which rig the animation came from, which rig it is going
/// to, and how to treat what does not line up.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RetargetProfile {
    pub name: String,
    pub source: RigProfile,
    pub target: RigProfile,
    #[serde(default)]
    pub settings: RetargetSettings,
}

impl RetargetProfile {
    pub fn new(source: RigProfile, target: RigProfile) -> Self {
        Self {
            name: format!("{} -> {}", source.name, target.name),
            source,
            target,
            settings: RetargetSettings::default(),
        }
    }

    pub fn with_settings(mut self, settings: RetargetSettings) -> Self {
        self.settings = settings;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_conservative_choice() {
        let settings = RetargetSettings::default();
        assert_eq!(settings.translation, TranslationPolicy::PelvisOnly);
        assert!(matches!(settings.scale, ScalePolicy::Auto(_)));
        assert!(settings.root_motion.keeps_track());
    }
}
