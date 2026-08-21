//! A canonical humanoid vocabulary.
//!
//! These names are not a skeleton — no rig is required to have them, and they
//! never replace a skeleton's own joints. They exist so two rigs that share no
//! naming can still be talked about in the same terms: a profile says which of
//! *its* joints plays each role, and the retargeter then works purely in
//! resolved joint indices.

use serde::{Deserialize, Serialize};

macro_rules! humanoid_joints {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// A role in a humanoid body.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum HumanoidJoint {
            $($variant),*
        }

        impl HumanoidJoint {
            /// Every role, in a stable order.
            pub const ALL: &'static [HumanoidJoint] = &[$(HumanoidJoint::$variant),*];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(HumanoidJoint::$variant => $name),*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(HumanoidJoint::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

humanoid_joints! {
    Pelvis => "Pelvis",
    SpineLower => "SpineLower",
    SpineMid => "SpineMid",
    Chest => "Chest",
    UpperChest => "UpperChest",
    Neck => "Neck",
    Head => "Head",

    ClavicleLeft => "ClavicleLeft",
    UpperArmLeft => "UpperArmLeft",
    LowerArmLeft => "LowerArmLeft",
    HandLeft => "HandLeft",

    ClavicleRight => "ClavicleRight",
    UpperArmRight => "UpperArmRight",
    LowerArmRight => "LowerArmRight",
    HandRight => "HandRight",

    UpperLegLeft => "UpperLegLeft",
    LowerLegLeft => "LowerLegLeft",
    FootLeft => "FootLeft",
    ToeLeft => "ToeLeft",

    UpperLegRight => "UpperLegRight",
    LowerLegRight => "LowerLegRight",
    FootRight => "FootRight",
    ToeRight => "ToeRight",

    ThumbProximalLeft => "ThumbProximalLeft",
    ThumbIntermediateLeft => "ThumbIntermediateLeft",
    ThumbDistalLeft => "ThumbDistalLeft",
    IndexProximalLeft => "IndexProximalLeft",
    IndexIntermediateLeft => "IndexIntermediateLeft",
    IndexDistalLeft => "IndexDistalLeft",
    MiddleProximalLeft => "MiddleProximalLeft",
    MiddleIntermediateLeft => "MiddleIntermediateLeft",
    MiddleDistalLeft => "MiddleDistalLeft",
    RingProximalLeft => "RingProximalLeft",
    RingIntermediateLeft => "RingIntermediateLeft",
    RingDistalLeft => "RingDistalLeft",
    LittleProximalLeft => "LittleProximalLeft",
    LittleIntermediateLeft => "LittleIntermediateLeft",
    LittleDistalLeft => "LittleDistalLeft",

    ThumbProximalRight => "ThumbProximalRight",
    ThumbIntermediateRight => "ThumbIntermediateRight",
    ThumbDistalRight => "ThumbDistalRight",
    IndexProximalRight => "IndexProximalRight",
    IndexIntermediateRight => "IndexIntermediateRight",
    IndexDistalRight => "IndexDistalRight",
    MiddleProximalRight => "MiddleProximalRight",
    MiddleIntermediateRight => "MiddleIntermediateRight",
    MiddleDistalRight => "MiddleDistalRight",
    RingProximalRight => "RingProximalRight",
    RingIntermediateRight => "RingIntermediateRight",
    RingDistalRight => "RingDistalRight",
    LittleProximalRight => "LittleProximalRight",
    LittleIntermediateRight => "LittleIntermediateRight",
    LittleDistalRight => "LittleDistalRight",
}

impl std::fmt::Display for HumanoidJoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for HumanoidJoint {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::from_name(name).ok_or_else(|| format!("unknown humanoid joint {name:?}"))
    }
}

impl HumanoidJoint {
    /// The role at the far end of this joint's bone.
    ///
    /// A joint on its own has no direction; the bone from it to the next joint
    /// does. That direction is what tells two rigs apart when one is T-posed
    /// and the other A-posed, so it is what the retargeter aligns.
    pub fn child(self) -> Option<HumanoidJoint> {
        use HumanoidJoint::*;
        Some(match self {
            Pelvis => SpineLower,
            SpineLower => SpineMid,
            SpineMid => Chest,
            Chest => UpperChest,
            UpperChest => Neck,
            Neck => Head,

            ClavicleLeft => UpperArmLeft,
            UpperArmLeft => LowerArmLeft,
            LowerArmLeft => HandLeft,
            ClavicleRight => UpperArmRight,
            UpperArmRight => LowerArmRight,
            LowerArmRight => HandRight,

            UpperLegLeft => LowerLegLeft,
            LowerLegLeft => FootLeft,
            FootLeft => ToeLeft,
            UpperLegRight => LowerLegRight,
            LowerLegRight => FootRight,
            FootRight => ToeRight,

            ThumbProximalLeft => ThumbIntermediateLeft,
            ThumbIntermediateLeft => ThumbDistalLeft,
            IndexProximalLeft => IndexIntermediateLeft,
            IndexIntermediateLeft => IndexDistalLeft,
            MiddleProximalLeft => MiddleIntermediateLeft,
            MiddleIntermediateLeft => MiddleDistalLeft,
            RingProximalLeft => RingIntermediateLeft,
            RingIntermediateLeft => RingDistalLeft,
            LittleProximalLeft => LittleIntermediateLeft,
            LittleIntermediateLeft => LittleDistalLeft,

            ThumbProximalRight => ThumbIntermediateRight,
            ThumbIntermediateRight => ThumbDistalRight,
            IndexProximalRight => IndexIntermediateRight,
            IndexIntermediateRight => IndexDistalRight,
            MiddleProximalRight => MiddleIntermediateRight,
            MiddleIntermediateRight => MiddleDistalRight,
            RingProximalRight => RingIntermediateRight,
            RingIntermediateRight => RingDistalRight,
            LittleProximalRight => LittleIntermediateRight,
            LittleIntermediateRight => LittleDistalRight,

            // Tips: the bone that ends here is the one to measure.
            Head | HandLeft | HandRight | ToeLeft | ToeRight => return None,
            ThumbDistalLeft | IndexDistalLeft | MiddleDistalLeft => return None,
            RingDistalLeft | LittleDistalLeft => return None,
            ThumbDistalRight | IndexDistalRight | MiddleDistalRight => return None,
            RingDistalRight | LittleDistalRight => return None,
        })
    }

    /// Where this joint's bone points in a canonical T-pose.
    ///
    /// The vocabulary of postures, so two rigs can be compared in the same one.
    /// `None` means "leave it as the rig authored it": feet and fingers are
    /// deliberately absent, because their bind orientation is usually right and
    /// straightening them does more harm than good.
    pub fn t_pose_direction(self) -> Option<fabelgeist_math::vector::Vec3> {
        use HumanoidJoint::*;
        let up = fabelgeist_math::vector::Vec3::new(0.0, 1.0, 0.0);
        let left = fabelgeist_math::vector::Vec3::new(1.0, 0.0, 0.0);
        let right = fabelgeist_math::vector::Vec3::new(-1.0, 0.0, 0.0);
        Some(match self {
            SpineLower | SpineMid | Chest | UpperChest | Neck => up,
            ClavicleLeft | UpperArmLeft | LowerArmLeft => left,
            ClavicleRight | UpperArmRight | LowerArmRight => right,
            // Legs and the pelvis are left alone. Rigs already agree on them
            // to within a few degrees, and turning the pelvis would move every
            // limb's reference away from the geometry the rig was built with.
            _ => return None,
        })
    }

    /// The role whose bone this joint hangs off, the inverse of
    /// [`HumanoidJoint::child`].
    ///
    /// Derived rather than tabulated, so the two can never disagree.
    pub fn parent(self) -> Option<HumanoidJoint> {
        HumanoidJoint::ALL
            .iter()
            .copied()
            .find(|role| role.child() == Some(self))
    }

    /// Which way this joint's hinge points in a canonical T-pose, for the
    /// joints that are hinges.
    ///
    /// Bone directions pin a limb's shape but leave the roll about each bone
    /// free, and roll is exactly what aims a hinge: an arm can be straight out
    /// along X with its elbow set to bend forwards, upwards or anywhere
    /// between. This is the missing half of the posture — the direction a
    /// straight arm's elbow bends *about* when the palm faces down, which is
    /// the T-pose every humanoid capture is authored against.
    ///
    /// Signed as the axis of *positive* flexion, so a rig declaring its own
    /// hinge is aimed the right way round rather than rolled half a turn.
    ///
    /// Legs are absent for the same reason they have no
    /// [`t_pose_direction`](HumanoidJoint::t_pose_direction): they are never
    /// straightened, so there is no canonical bone direction to roll about.
    pub fn t_pose_hinge(self) -> Option<fabelgeist_math::vector::Vec3> {
        use HumanoidJoint::*;
        // Palms down, so a left elbow flexing forwards turns about -Y and a
        // right elbow, mirrored, about +Y.
        Some(match self {
            LowerArmLeft => fabelgeist_math::vector::Vec3::new(0.0, -1.0, 0.0),
            LowerArmRight => fabelgeist_math::vector::Vec3::new(0.0, 1.0, 0.0),
            _ => return None,
        })
    }

    /// Whether this joint is a contact or aim target that a later IK or
    /// contact-correction pass would want to pin.
    pub fn is_end_effector(self) -> bool {
        matches!(
            self,
            HumanoidJoint::HandLeft
                | HumanoidJoint::HandRight
                | HumanoidJoint::FootLeft
                | HumanoidJoint::FootRight
                | HumanoidJoint::Head
        )
    }

    /// Whether the joint belongs to a hand, so a profile can leave every
    /// finger unmapped without that counting as an incomplete body mapping.
    pub fn is_finger(self) -> bool {
        HumanoidChain::FINGERS
            .iter()
            .any(|chain| chain.joints().contains(&self))
    }
}

/// An ordered run of humanoid joints, root-most first.
///
/// Chains are what make hierarchy mismatches tractable: a source rig with
/// three spine joints and a target with four are still the same *chain*, and
/// the retargeter can distribute motion along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HumanoidChain {
    Spine,
    Neck,
    ArmLeft,
    ArmRight,
    LegLeft,
    LegRight,
    ThumbLeft,
    IndexLeft,
    MiddleLeft,
    RingLeft,
    LittleLeft,
    ThumbRight,
    IndexRight,
    MiddleRight,
    RingRight,
    LittleRight,
}

impl HumanoidChain {
    pub const ALL: &'static [HumanoidChain] = &[
        HumanoidChain::Spine,
        HumanoidChain::Neck,
        HumanoidChain::ArmLeft,
        HumanoidChain::ArmRight,
        HumanoidChain::LegLeft,
        HumanoidChain::LegRight,
        HumanoidChain::ThumbLeft,
        HumanoidChain::IndexLeft,
        HumanoidChain::MiddleLeft,
        HumanoidChain::RingLeft,
        HumanoidChain::LittleLeft,
        HumanoidChain::ThumbRight,
        HumanoidChain::IndexRight,
        HumanoidChain::MiddleRight,
        HumanoidChain::RingRight,
        HumanoidChain::LittleRight,
    ];

    pub const FINGERS: &'static [HumanoidChain] = &[
        HumanoidChain::ThumbLeft,
        HumanoidChain::IndexLeft,
        HumanoidChain::MiddleLeft,
        HumanoidChain::RingLeft,
        HumanoidChain::LittleLeft,
        HumanoidChain::ThumbRight,
        HumanoidChain::IndexRight,
        HumanoidChain::MiddleRight,
        HumanoidChain::RingRight,
        HumanoidChain::LittleRight,
    ];

    /// The chain's joints in the canonical vocabulary. A rig that has more
    /// joints in a chain than this declares them by name in its profile.
    pub fn joints(self) -> &'static [HumanoidJoint] {
        use HumanoidJoint::*;
        match self {
            HumanoidChain::Spine => &[SpineLower, SpineMid, Chest, UpperChest],
            HumanoidChain::Neck => &[Neck, Head],
            HumanoidChain::ArmLeft => &[ClavicleLeft, UpperArmLeft, LowerArmLeft, HandLeft],
            HumanoidChain::ArmRight => &[ClavicleRight, UpperArmRight, LowerArmRight, HandRight],
            HumanoidChain::LegLeft => &[UpperLegLeft, LowerLegLeft, FootLeft, ToeLeft],
            HumanoidChain::LegRight => &[UpperLegRight, LowerLegRight, FootRight, ToeRight],
            HumanoidChain::ThumbLeft => {
                &[ThumbProximalLeft, ThumbIntermediateLeft, ThumbDistalLeft]
            }
            HumanoidChain::IndexLeft => {
                &[IndexProximalLeft, IndexIntermediateLeft, IndexDistalLeft]
            }
            HumanoidChain::MiddleLeft => {
                &[MiddleProximalLeft, MiddleIntermediateLeft, MiddleDistalLeft]
            }
            HumanoidChain::RingLeft => &[RingProximalLeft, RingIntermediateLeft, RingDistalLeft],
            HumanoidChain::LittleLeft => {
                &[LittleProximalLeft, LittleIntermediateLeft, LittleDistalLeft]
            }
            HumanoidChain::ThumbRight => {
                &[ThumbProximalRight, ThumbIntermediateRight, ThumbDistalRight]
            }
            HumanoidChain::IndexRight => {
                &[IndexProximalRight, IndexIntermediateRight, IndexDistalRight]
            }
            HumanoidChain::MiddleRight => &[
                MiddleProximalRight,
                MiddleIntermediateRight,
                MiddleDistalRight,
            ],
            HumanoidChain::RingRight => {
                &[RingProximalRight, RingIntermediateRight, RingDistalRight]
            }
            HumanoidChain::LittleRight => &[
                LittleProximalRight,
                LittleIntermediateRight,
                LittleDistalRight,
            ],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HumanoidChain::Spine => "Spine",
            HumanoidChain::Neck => "Neck",
            HumanoidChain::ArmLeft => "ArmLeft",
            HumanoidChain::ArmRight => "ArmRight",
            HumanoidChain::LegLeft => "LegLeft",
            HumanoidChain::LegRight => "LegRight",
            HumanoidChain::ThumbLeft => "ThumbLeft",
            HumanoidChain::IndexLeft => "IndexLeft",
            HumanoidChain::MiddleLeft => "MiddleLeft",
            HumanoidChain::RingLeft => "RingLeft",
            HumanoidChain::LittleLeft => "LittleLeft",
            HumanoidChain::ThumbRight => "ThumbRight",
            HumanoidChain::IndexRight => "IndexRight",
            HumanoidChain::MiddleRight => "MiddleRight",
            HumanoidChain::RingRight => "RingRight",
            HumanoidChain::LittleRight => "LittleRight",
        }
    }
}

impl std::fmt::Display for HumanoidChain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_names_round_trip() {
        for joint in HumanoidJoint::ALL {
            assert_eq!(HumanoidJoint::from_name(joint.as_str()), Some(*joint));
        }
    }

    #[test]
    fn every_chain_joint_is_in_the_vocabulary() {
        for chain in HumanoidChain::ALL {
            for joint in chain.joints() {
                assert!(
                    HumanoidJoint::ALL.contains(joint),
                    "{joint} is not declared"
                );
            }
        }
    }

    #[test]
    fn fingers_are_recognized_as_optional_detail() {
        assert!(HumanoidJoint::IndexDistalRight.is_finger());
        assert!(!HumanoidJoint::HandRight.is_finger());
    }
}
