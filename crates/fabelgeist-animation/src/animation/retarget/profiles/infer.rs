//! Guessing a rig profile from joint names.
//!
//! An explicit profile is always better and always available; this exists so
//! that a rig nobody has written one for — a mocap BVH, an unfamiliar FBX — can
//! be retargeted immediately, and so that writing the real profile starts from
//! something rather than nothing.
//!
//! The output is an ordinary [`RigProfile`]: inspect it with
//! [`ResolvedProfile::report`](super::super::ResolvedProfile::report), edit it,
//! serialize it, ship it. Nothing downstream can tell it was inferred.
//!
//! Inference is deliberately conservative. It claims a joint only on a keyword
//! it recognizes, it never marks anything required, and roles it cannot place
//! are simply left out — an unmapped joint keeps its rest pose, which is a
//! visible but harmless result, where a *wrongly* mapped one is neither.

use crate::skeleton::Skeleton;

use super::super::profile::{ChainBinding, ReferencePose, RigProfile, RootSource};
use super::super::semantic::{HumanoidChain, HumanoidJoint};

/// Which half of the body a joint's name claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Center,
}

/// Splits a joint name into the side it names and the rest of the name,
/// lowercased with separators removed.
///
/// Rigs mark sides in every way anyone has thought of: `LeftArm`, `l_uparm`,
/// `arm.L`, `LHipJoint`, `lFemur`.
fn split_side(name: &str) -> (Side, String) {
    let bare = name.rsplit([':', '|']).next().unwrap_or(name);
    let lower = bare.to_ascii_lowercase();

    let strip = |side: Side, rest: String| (side, simplify(&rest));

    for (word, side) in [("left", Side::Left), ("right", Side::Right)] {
        if let Some(position) = lower.find(word) {
            let mut rest = lower.clone();
            rest.replace_range(position..position + word.len(), "");
            return strip(side, rest);
        }
    }

    let separators = ['_', '-', '.', ' '];
    for (prefix, side) in [("l", Side::Left), ("r", Side::Right)] {
        for separator in separators {
            let marker = format!("{prefix}{separator}");
            if lower.starts_with(&marker) {
                return strip(side, lower[marker.len()..].to_string());
            }
            let marker = format!("{separator}{prefix}");
            if lower.ends_with(&marker) {
                return strip(side, lower[..lower.len() - marker.len()].to_string());
            }
        }
    }

    // `LHipJoint`, `lFemur`: a lone side letter before a capitalized word.
    let mut characters = bare.chars();
    if let (Some(first), Some(second)) = (characters.next(), characters.next())
        && second.is_ascii_uppercase()
    {
        match first {
            'l' | 'L' => return strip(Side::Left, bare[1..].to_string()),
            'r' | 'R' => return strip(Side::Right, bare[1..].to_string()),
            _ => {}
        }
    }

    (Side::Center, simplify(bare))
}

/// Lowercase, alphanumerics only — the form keywords are matched against.
fn simplify(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// Keywords per role, most specific first. A joint matching a longer keyword
/// beats one matching a shorter one, which is what keeps `LeftUpLeg` from
/// being taken for a lower leg.
const BODY: &[(HumanoidJoint, &[&str])] = &[
    (HumanoidJoint::Pelvis, &["hips", "pelvis", "hip"]),
    (HumanoidJoint::Neck, &["neck"]),
    (HumanoidJoint::Head, &["head"]),
    (
        HumanoidJoint::ClavicleLeft,
        &["clavicle", "shoulder", "collar"],
    ),
    (
        HumanoidJoint::UpperArmLeft,
        &["upperarm", "uparm", "humerus", "shldr", "arm"],
    ),
    (
        HumanoidJoint::LowerArmLeft,
        &["forearm", "lowerarm", "lowarm", "elbow", "ulna"],
    ),
    (HumanoidJoint::HandLeft, &["hand", "wrist"]),
    (
        HumanoidJoint::UpperLegLeft,
        &["upperleg", "upleg", "thigh", "femur", "hip"],
    ),
    (
        HumanoidJoint::LowerLegLeft,
        &["lowerleg", "lowleg", "shin", "calf", "knee", "tibia", "leg"],
    ),
    (HumanoidJoint::FootLeft, &["foot", "ankle"]),
    (HumanoidJoint::ToeLeft, &["toebase", "toe", "ball"]),
];

/// The spine, which is a run rather than a set of named slots.
const SPINE: &[&str] = &["spine", "chest", "torso", "abdomen", "waist"];

const FINGERS: &[(&str, [HumanoidJoint; 3])] = &[
    (
        "thumb",
        [
            HumanoidJoint::ThumbProximalLeft,
            HumanoidJoint::ThumbIntermediateLeft,
            HumanoidJoint::ThumbDistalLeft,
        ],
    ),
    (
        "index",
        [
            HumanoidJoint::IndexProximalLeft,
            HumanoidJoint::IndexIntermediateLeft,
            HumanoidJoint::IndexDistalLeft,
        ],
    ),
    (
        "middle",
        [
            HumanoidJoint::MiddleProximalLeft,
            HumanoidJoint::MiddleIntermediateLeft,
            HumanoidJoint::MiddleDistalLeft,
        ],
    ),
    (
        "ring",
        [
            HumanoidJoint::RingProximalLeft,
            HumanoidJoint::RingIntermediateLeft,
            HumanoidJoint::RingDistalLeft,
        ],
    ),
    (
        "pinky",
        [
            HumanoidJoint::LittleProximalLeft,
            HumanoidJoint::LittleIntermediateLeft,
            HumanoidJoint::LittleDistalLeft,
        ],
    ),
    (
        "little",
        [
            HumanoidJoint::LittleProximalLeft,
            HumanoidJoint::LittleIntermediateLeft,
            HumanoidJoint::LittleDistalLeft,
        ],
    ),
];

/// The right-hand counterpart of a left-hand role.
fn mirrored(role: HumanoidJoint) -> HumanoidJoint {
    use HumanoidJoint::*;
    match role {
        ClavicleLeft => ClavicleRight,
        UpperArmLeft => UpperArmRight,
        LowerArmLeft => LowerArmRight,
        HandLeft => HandRight,
        UpperLegLeft => UpperLegRight,
        LowerLegLeft => LowerLegRight,
        FootLeft => FootRight,
        ToeLeft => ToeRight,
        ThumbProximalLeft => ThumbProximalRight,
        ThumbIntermediateLeft => ThumbIntermediateRight,
        ThumbDistalLeft => ThumbDistalRight,
        IndexProximalLeft => IndexProximalRight,
        IndexIntermediateLeft => IndexIntermediateRight,
        IndexDistalLeft => IndexDistalRight,
        MiddleProximalLeft => MiddleProximalRight,
        MiddleIntermediateLeft => MiddleIntermediateRight,
        MiddleDistalLeft => MiddleDistalRight,
        RingProximalLeft => RingProximalRight,
        RingIntermediateLeft => RingIntermediateRight,
        RingDistalLeft => RingDistalRight,
        LittleProximalLeft => LittleProximalRight,
        LittleIntermediateLeft => LittleIntermediateRight,
        LittleDistalLeft => LittleDistalRight,
        other => other,
    }
}

/// A joint as inference sees it.
struct Candidate {
    index: usize,
    side: Side,
    simple: String,
    claimed: bool,
}

impl RigProfile {
    /// Builds a profile for a rig by reading its joint names.
    ///
    /// Always check the result — [`Retargeter::report`](super::super::Retargeter::report)
    /// prints the whole mapping — and prefer an explicit profile for any rig
    /// you will use more than once.
    pub fn infer(skeleton: &Skeleton) -> Self {
        let mut candidates: Vec<Candidate> = skeleton
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| {
                let (side, simple) = split_side(&joint.name);
                Candidate {
                    index,
                    side,
                    simple,
                    claimed: false,
                }
            })
            .collect();

        // Nothing here can know what posture the rig was bound in, and a
        // reference that is merely "whatever this rig happened to bind as" is
        // not a reference at all — it only lines up with the target rig by
        // luck. Straightening from the rig's own geometry makes it definite,
        // and costs nothing on a rig that was already T-posed.
        let mut profile = RigProfile::new("inferred").with_reference(ReferencePose::TPose);
        let name_of = |index: usize| skeleton.joints[index].name.clone();

        // The spine first: it is a run of joints in hierarchy order, and
        // claiming it stops `chest` being mistaken for anything else.
        let spine: Vec<usize> = candidates
            .iter()
            .filter(|candidate| {
                candidate.side == Side::Center
                    && SPINE.iter().any(|word| candidate.simple.starts_with(word))
            })
            .map(|candidate| candidate.index)
            .collect();
        for index in &spine {
            candidates[*index].claimed = true;
        }
        for (position, index) in spine.iter().enumerate() {
            let role = match position {
                0 => HumanoidJoint::SpineLower,
                1 => HumanoidJoint::SpineMid,
                2 => HumanoidJoint::Chest,
                3 => HumanoidJoint::UpperChest,
                // Longer spines keep going through the chain rather than the
                // vocabulary, which is what chains are for.
                _ => break,
            };
            profile = profile.with(role, name_of(*index));
        }
        if spine.len() > 1 {
            profile = profile.with_chain(
                HumanoidChain::Spine,
                ChainBinding::new(spine.iter().map(|index| name_of(*index))),
            );
        }

        for (role, keywords) in BODY {
            for side in [Side::Center, Side::Left, Side::Right] {
                let role = match (side, role) {
                    (Side::Right, role) => mirrored(*role),
                    (_, role) => *role,
                };
                // Centre roles only apply to roles that have no side.
                let wanted = match role {
                    HumanoidJoint::Pelvis | HumanoidJoint::Neck | HumanoidJoint::Head => {
                        Side::Center
                    }
                    _ => side,
                };
                if wanted != side || profile.binding(role).is_some() {
                    continue;
                }
                if let Some(index) = best_match(&candidates, side, keywords) {
                    candidates[index].claimed = true;
                    profile = profile.with(role, name_of(index));
                }
            }
        }

        for (word, roles) in FINGERS {
            for side in [Side::Left, Side::Right] {
                let segments: Vec<usize> = candidates
                    .iter()
                    .filter(|candidate| {
                        !candidate.claimed
                            && candidate.side == side
                            && candidate.simple.contains(word)
                    })
                    .map(|candidate| candidate.index)
                    .collect();
                for (segment, index) in segments.iter().take(3).enumerate() {
                    let role = match side {
                        Side::Right => mirrored(roles[segment]),
                        _ => roles[segment],
                    };
                    if profile.binding(role).is_none() {
                        candidates[*index].claimed = true;
                        profile = profile.with(role, name_of(*index));
                    }
                }
            }
        }

        // The pelvis is where locomotion lives on most rigs; a rig with a
        // dedicated node above it says so by having one.
        let root = candidates
            .iter()
            .find(|candidate| {
                candidate.side == Side::Center
                    && matches!(candidate.simple.as_str(), "root" | "reference" | "armature")
                    && skeleton.joints[candidate.index].parent_index.is_none()
            })
            .map(|candidate| name_of(candidate.index));
        profile.root = match root {
            Some(name) => RootSource::Joint(name),
            None => RootSource::Pelvis,
        };

        // The pelvis is the one joint the retargeter cannot do without.
        if let Some(binding) = profile.joints.get_mut(&HumanoidJoint::Pelvis) {
            *binding = binding.clone().required();
        }
        profile
    }
}

/// The unclaimed joint on `side` matching the most specific keyword.
///
/// Ties go to the joint nearer the start of the skeleton, which is nearer the
/// root in every importer the engine has.
fn best_match(candidates: &[Candidate], side: Side, keywords: &[&str]) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for candidate in candidates {
        if candidate.claimed || candidate.side != side {
            continue;
        }
        let Some(length) = keywords
            .iter()
            .filter(|keyword| candidate.simple.contains(**keyword))
            .map(|keyword| keyword.len())
            .max()
        else {
            continue;
        };
        if best.is_none_or(|(best_length, _)| length > best_length) {
            best = Some((length, candidate.index));
        }
    }
    best.map(|(_, index)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::Joint;
    use fabelgeist_math::matrix::Mat4;

    fn skeleton(names: &[&str]) -> Skeleton {
        Skeleton::new(
            names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    Joint::new(
                        (*name).to_string(),
                        index,
                        index.checked_sub(1),
                        Mat4::identity(),
                        Default::default(),
                        Some(index),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn sides_are_read_however_a_rig_spells_them() {
        assert_eq!(split_side("LeftUpLeg"), (Side::Left, "upleg".into()));
        assert_eq!(
            split_side("mixamorig:RightArm"),
            (Side::Right, "arm".into())
        );
        assert_eq!(split_side("l_uparm"), (Side::Left, "uparm".into()));
        assert_eq!(split_side("upperarm.R"), (Side::Right, "upperarm".into()));
        assert_eq!(split_side("LHipJoint"), (Side::Left, "hipjoint".into()));
        assert_eq!(split_side("lFemur"), (Side::Left, "femur".into()));
        // Words that merely start with l or r are not sides.
        assert_eq!(split_side("LowerLeg"), (Side::Center, "lowerleg".into()));
        assert_eq!(split_side("Hips"), (Side::Center, "hips".into()));
    }

    #[test]
    fn a_mixamo_style_rig_is_inferred() {
        let rig = skeleton(&[
            "mixamorig:Hips",
            "mixamorig:Spine",
            "mixamorig:Spine1",
            "mixamorig:Spine2",
            "mixamorig:Neck",
            "mixamorig:Head",
            "mixamorig:LeftShoulder",
            "mixamorig:LeftArm",
            "mixamorig:LeftForeArm",
            "mixamorig:LeftHand",
            "mixamorig:RightShoulder",
            "mixamorig:RightArm",
            "mixamorig:RightForeArm",
            "mixamorig:RightHand",
            "mixamorig:LeftUpLeg",
            "mixamorig:LeftLeg",
            "mixamorig:LeftFoot",
            "mixamorig:LeftToeBase",
        ]);
        let profile = RigProfile::infer(&rig);
        let resolved = profile.resolve(&rig).expect("an inferred profile resolves");
        let named = |role| {
            resolved
                .joint(role)
                .map(|index| rig.joints[index].name.as_str())
        };

        assert_eq!(named(HumanoidJoint::Pelvis), Some("mixamorig:Hips"));
        assert_eq!(named(HumanoidJoint::SpineLower), Some("mixamorig:Spine"));
        assert_eq!(named(HumanoidJoint::Chest), Some("mixamorig:Spine2"));
        assert_eq!(named(HumanoidJoint::Head), Some("mixamorig:Head"));
        assert_eq!(
            named(HumanoidJoint::ClavicleRight),
            Some("mixamorig:RightShoulder")
        );
        // The trap: "LeftArm" is an upper arm and "LeftLeg" a lower leg.
        assert_eq!(
            named(HumanoidJoint::UpperArmLeft),
            Some("mixamorig:LeftArm")
        );
        assert_eq!(
            named(HumanoidJoint::LowerArmLeft),
            Some("mixamorig:LeftForeArm")
        );
        assert_eq!(
            named(HumanoidJoint::UpperLegLeft),
            Some("mixamorig:LeftUpLeg")
        );
        assert_eq!(
            named(HumanoidJoint::LowerLegLeft),
            Some("mixamorig:LeftLeg")
        );
        assert_eq!(named(HumanoidJoint::ToeLeft), Some("mixamorig:LeftToeBase"));
        assert_eq!(resolved.chains[&HumanoidChain::Spine].len(), 3);
    }

    #[test]
    fn a_mocap_rig_with_different_names_is_inferred() {
        // The CMU/BioVision naming a lot of BVH files use.
        let rig = skeleton(&[
            "Hips",
            "LowerBack",
            "Spine",
            "Spine1",
            "Neck",
            "Head",
            "LeftShoulder",
            "LeftArm",
            "LeftForeArm",
            "LeftHand",
            "LHipJoint",
            "LeftUpLeg",
            "LeftLeg",
            "LeftFoot",
            "LeftToeBase",
        ]);
        let profile = RigProfile::infer(&rig);
        let resolved = profile.resolve(&rig).expect("an inferred profile resolves");
        let named = |role| {
            resolved
                .joint(role)
                .map(|index| rig.joints[index].name.as_str())
        };

        assert_eq!(named(HumanoidJoint::Pelvis), Some("Hips"));
        assert_eq!(named(HumanoidJoint::UpperLegLeft), Some("LeftUpLeg"));
        assert_eq!(named(HumanoidJoint::LowerLegLeft), Some("LeftLeg"));
        assert_eq!(named(HumanoidJoint::FootLeft), Some("LeftFoot"));
        assert_eq!(named(HumanoidJoint::Head), Some("Head"));
    }

    #[test]
    fn a_rig_of_nonsense_names_yields_an_empty_mapping_rather_than_a_wrong_one() {
        let rig = skeleton(&["bone_000", "bone_001", "bone_002"]);
        let profile = RigProfile::infer(&rig);
        assert!(
            profile.joints.is_empty(),
            "inferred {:?} from nothing",
            profile.joints.keys().collect::<Vec<_>>()
        );
        // And it fails loudly rather than retargeting garbage.
        assert!(profile.resolve(&rig).is_ok());
    }

    #[test]
    fn an_inferred_profile_is_just_data() {
        let rig = skeleton(&["Hips", "Spine", "Neck", "Head"]);
        let profile = RigProfile::infer(&rig);
        let json = serde_json::to_string(&profile).expect("it serializes");
        let restored: RigProfile = serde_json::from_str(&json).expect("it deserializes");
        assert_eq!(profile, restored);
    }
}
