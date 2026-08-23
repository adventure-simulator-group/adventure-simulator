//! Mixamo as a source rig.
//!
//! This is a data file, not a feature. It names Mixamo's joints and says which
//! humanoid role each one plays; the retargeter never sees any of these
//! strings. Supporting another rig means writing a sibling of this file, not
//! touching the algorithm.

use crate::animation::retarget::profile::{ChainBinding, ReferencePose, RigProfile, RootSource};
use crate::animation::retarget::semantic::{HumanoidChain, HumanoidJoint};
use crate::skeleton::mixamo::MixamoRig;

/// Mixamo prefixes every joint; exports occasionally use `mixamorig1:` and
/// some pipelines strip the namespace entirely. The resolver normalizes
/// namespaces away, so binding the canonical name covers all three.
fn joint(name: &str) -> String {
    format!("mixamorig:{name}")
}

impl MixamoRig {
    /// The Mixamo rig described in humanoid terms.
    ///
    /// Mixamo keeps locomotion on the hips — there is no separate root joint —
    /// which is exactly the sort of per-rig convention a profile exists to
    /// record.
    pub fn profile() -> RigProfile {
        use HumanoidJoint::*;

        // Mixamo binds in a T-pose, so straightening is very nearly a no-op —
        // but saying so is what lets a target rig that binds differently
        // measure its motion against the same posture.
        let mut profile = RigProfile::new("Mixamo")
            .with_root(RootSource::Pelvis)
            .with_reference(ReferencePose::TPose)
            .with_markers([joint("Hips"), joint("Spine"), joint("LeftUpLeg")]);

        for (role, name, required) in [
            (Pelvis, "Hips", true),
            (SpineLower, "Spine", true),
            (SpineMid, "Spine1", false),
            (Chest, "Spine2", false),
            (Neck, "Neck", false),
            (Head, "Head", false),
            (ClavicleLeft, "LeftShoulder", false),
            (UpperArmLeft, "LeftArm", true),
            (LowerArmLeft, "LeftForeArm", true),
            (HandLeft, "LeftHand", false),
            (ClavicleRight, "RightShoulder", false),
            (UpperArmRight, "RightArm", true),
            (LowerArmRight, "RightForeArm", true),
            (HandRight, "RightHand", false),
            (UpperLegLeft, "LeftUpLeg", true),
            (LowerLegLeft, "LeftLeg", true),
            (FootLeft, "LeftFoot", true),
            (ToeLeft, "LeftToeBase", false),
            (UpperLegRight, "RightUpLeg", true),
            (LowerLegRight, "RightLeg", true),
            (FootRight, "RightFoot", true),
            (ToeRight, "RightToeBase", false),
        ] {
            profile = if required {
                profile.with_required(role, joint(name))
            } else {
                profile.with(role, joint(name))
            };
        }

        // Fingers are optional everywhere: a rig without them still retargets
        // its body, and a body-only clip is a perfectly good clip. Mixamo
        // calls the little finger "Pinky".
        for side in ["Left", "Right"] {
            for finger in ["Thumb", "Index", "Middle", "Ring", "Pinky"] {
                for (segment, role) in finger_roles(side, finger).into_iter().enumerate() {
                    profile =
                        profile.with(role, joint(&format!("{side}Hand{finger}{}", segment + 1)));
                }
            }
        }

        // Mixamo's spine is three joints; rigs it is retargeted onto rarely
        // agree. Declaring the chain lets motion be spread over however many
        // the target has.
        profile.with_chain(
            HumanoidChain::Spine,
            ChainBinding::new([joint("Spine"), joint("Spine1"), joint("Spine2")]),
        )
    }
}

/// The three humanoid segments of one finger.
fn finger_roles(side: &str, finger: &str) -> [HumanoidJoint; 3] {
    use HumanoidJoint::*;
    match (side, finger) {
        ("Left", "Thumb") => [ThumbProximalLeft, ThumbIntermediateLeft, ThumbDistalLeft],
        ("Left", "Index") => [IndexProximalLeft, IndexIntermediateLeft, IndexDistalLeft],
        ("Left", "Middle") => [MiddleProximalLeft, MiddleIntermediateLeft, MiddleDistalLeft],
        ("Left", "Ring") => [RingProximalLeft, RingIntermediateLeft, RingDistalLeft],
        ("Left", _) => [LittleProximalLeft, LittleIntermediateLeft, LittleDistalLeft],
        (_, "Thumb") => [ThumbProximalRight, ThumbIntermediateRight, ThumbDistalRight],
        (_, "Index") => [IndexProximalRight, IndexIntermediateRight, IndexDistalRight],
        (_, "Middle") => [
            MiddleProximalRight,
            MiddleIntermediateRight,
            MiddleDistalRight,
        ],
        (_, "Ring") => [RingProximalRight, RingIntermediateRight, RingDistalRight],
        (_, _) => [
            LittleProximalRight,
            LittleIntermediateRight,
            LittleDistalRight,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_resolves_against_the_mixamo_skeleton() {
        let skeleton = MixamoRig::skeleton();
        let resolved = MixamoRig::profile()
            .resolve(&skeleton)
            .expect("the Mixamo profile must resolve against the Mixamo rig");

        assert_eq!(
            resolved.joint(HumanoidJoint::Pelvis),
            skeleton.find_joint_by_name("mixamorig:Hips")
        );
        assert_eq!(
            resolved.joint(HumanoidJoint::LowerArmRight),
            skeleton.find_joint_by_name("mixamorig:RightForeArm")
        );
        assert_eq!(
            resolved.joint(HumanoidJoint::IndexDistalLeft),
            skeleton.find_joint_by_name("mixamorig:LeftHandIndex3")
        );
        assert!(
            resolved.missing.is_empty(),
            "unmapped: {:?}",
            resolved.missing
        );
        assert_eq!(resolved.chains[&HumanoidChain::Spine].len(), 3);
    }

    #[test]
    fn detection_is_by_marker_joints() {
        assert!(MixamoRig::profile().matches(&MixamoRig::skeleton()));
    }
}
