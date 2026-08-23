//! Tests for the generic retargeter, written against rigs that exist only
//! here. Nothing in this file mentions a real-world rig: if the algorithm
//! needs to know about one, it is not generic.

use crate::animation::retarget::profile::*;
use crate::animation::retarget::semantic::{HumanoidChain, HumanoidJoint};
use crate::animation::retarget::{Retargeter, retarget};
use crate::animation::{Animation, Curve, JointTrack, JointTransform, model_pose, rest_pose};
use crate::skeleton::{Joint, Skeleton};
use fabelgeist_math::matrix::Mat4;
use fabelgeist_math::transform::Transform;
use fabelgeist_math::vector::{Vec3, Vec4};

/// Builds a skeleton from `(name, parent, position, euler rotation)` rows.
fn skeleton(rows: &[(&str, Option<&str>, Vec3, Vec3)]) -> Skeleton {
    let joints = rows
        .iter()
        .enumerate()
        .map(|(index, (name, parent, position, rotation))| {
            let parent_index = parent.map(|parent| {
                rows.iter()
                    .position(|row| row.0 == parent)
                    .expect("parent must be declared before its children")
            });
            Joint::new(
                (*name).to_string(),
                index,
                parent_index,
                Mat4::identity(),
                Transform::new(*position, *rotation, Vec3::ones()),
                Some(index),
            )
        })
        .collect();
    Skeleton::new(joints)
}

/// A humanoid test rig. `size` scales every bone, so two rigs of different
/// size are otherwise identical; `arm` is the shoulders' rest rotation, which
/// is what makes one rig a T-pose and another an A-pose.
fn humanoid(prefix: &str, size: f32, arm: Vec3) -> Skeleton {
    let name = |part: &str| format!("{prefix}{part}");
    let position = |x: f32, y: f32, z: f32| Vec3::new(x * size, y * size, z * size);
    let none = Vec3::new(0.0, 0.0, 0.0);

    let rows: Vec<(String, Option<String>, Vec3, Vec3)> = vec![
        (name("hips"), None, position(0.0, 1.0, 0.0), none),
        (
            name("spine"),
            Some(name("hips")),
            position(0.0, 0.15, 0.0),
            none,
        ),
        (
            name("chest"),
            Some(name("spine")),
            position(0.0, 0.15, 0.0),
            none,
        ),
        (
            name("neck"),
            Some(name("chest")),
            position(0.0, 0.2, 0.0),
            none,
        ),
        (
            name("head"),
            Some(name("neck")),
            position(0.0, 0.1, 0.0),
            none,
        ),
        (
            name("shoulder_l"),
            Some(name("chest")),
            position(0.05, 0.1, 0.0),
            arm,
        ),
        (
            name("upperarm_l"),
            Some(name("shoulder_l")),
            position(0.1, 0.0, 0.0),
            none,
        ),
        (
            name("lowerarm_l"),
            Some(name("upperarm_l")),
            position(0.25, 0.0, 0.0),
            none,
        ),
        (
            name("hand_l"),
            Some(name("lowerarm_l")),
            position(0.25, 0.0, 0.0),
            none,
        ),
        (
            name("shoulder_r"),
            Some(name("chest")),
            position(-0.05, 0.1, 0.0),
            -arm,
        ),
        (
            name("upperarm_r"),
            Some(name("shoulder_r")),
            position(-0.1, 0.0, 0.0),
            none,
        ),
        (
            name("lowerarm_r"),
            Some(name("upperarm_r")),
            position(-0.25, 0.0, 0.0),
            none,
        ),
        (
            name("hand_r"),
            Some(name("lowerarm_r")),
            position(-0.25, 0.0, 0.0),
            none,
        ),
        (
            name("upperleg_l"),
            Some(name("hips")),
            position(0.1, -0.05, 0.0),
            none,
        ),
        (
            name("lowerleg_l"),
            Some(name("upperleg_l")),
            position(0.0, -0.45, 0.0),
            none,
        ),
        (
            name("foot_l"),
            Some(name("lowerleg_l")),
            position(0.0, -0.45, 0.0),
            none,
        ),
        (
            name("upperleg_r"),
            Some(name("hips")),
            position(-0.1, -0.05, 0.0),
            none,
        ),
        (
            name("lowerleg_r"),
            Some(name("upperleg_r")),
            position(0.0, -0.45, 0.0),
            none,
        ),
        (
            name("foot_r"),
            Some(name("lowerleg_r")),
            position(0.0, -0.45, 0.0),
            none,
        ),
    ];

    let rows: Vec<(&str, Option<&str>, Vec3, Vec3)> = rows
        .iter()
        .map(|(name, parent, position, rotation)| {
            (name.as_str(), parent.as_deref(), *position, *rotation)
        })
        .collect();
    skeleton(&rows)
}

/// The humanoid rig described in humanoid terms.
fn humanoid_profile(prefix: &str) -> RigProfile {
    use HumanoidJoint::*;
    let name = |part: &str| format!("{prefix}{part}");
    RigProfile::new(format!("test:{prefix}"))
        .with_required(Pelvis, name("hips"))
        .with(SpineLower, name("spine"))
        .with(Chest, name("chest"))
        .with(Neck, name("neck"))
        .with(Head, name("head"))
        .with(ClavicleLeft, name("shoulder_l"))
        .with_required(UpperArmLeft, name("upperarm_l"))
        .with(LowerArmLeft, name("lowerarm_l"))
        .with(HandLeft, name("hand_l"))
        .with(ClavicleRight, name("shoulder_r"))
        .with_required(UpperArmRight, name("upperarm_r"))
        .with(LowerArmRight, name("lowerarm_r"))
        .with(HandRight, name("hand_r"))
        .with(UpperLegLeft, name("upperleg_l"))
        .with(LowerLegLeft, name("lowerleg_l"))
        .with(FootLeft, name("foot_l"))
        .with(UpperLegRight, name("upperleg_r"))
        .with(LowerLegRight, name("lowerleg_r"))
        .with(FootRight, name("foot_r"))
}

fn settings() -> RetargetSettings {
    RetargetSettings::default().with_root_motion(RootMotionPolicy::Keep)
}

fn quat(axis: Vec3, degrees: f32) -> Vec4 {
    Vec4::from_axis_angle(axis, degrees.to_radians())
}

#[track_caller]
fn assert_quat_eq(actual: Vec4, expected: Vec4, what: &str) {
    // A quaternion and its negation are the same rotation.
    let alignment = actual.dot(expected).abs();
    assert!(
        (alignment - 1.0).abs() < 1.0e-4,
        "{what}: {actual} is not the rotation {expected}"
    );
}

#[track_caller]
fn assert_vec_eq(actual: Vec3, expected: Vec3, tolerance: f32, what: &str) {
    assert!(
        (actual - expected).length() < tolerance,
        "{what}: {actual} is not {expected}"
    );
}

/// Poses a source rig by overriding some joints' local rotations.
fn pose(source: &Skeleton, overrides: &[(&str, Vec4)]) -> Vec<JointTransform> {
    let mut locals = rest_pose(source);
    for (name, rotation) in overrides {
        let index = source
            .find_joint_by_name(name)
            .unwrap_or_else(|| panic!("no joint named {name}"));
        locals[index].rotation = *rotation;
    }
    locals
}

/// A joint's motion away from its rest pose, in model space. This is the
/// quantity retargeting is supposed to preserve.
fn model_delta(skeleton: &Skeleton, locals: &[JointTransform], joint: &str) -> Vec4 {
    let index = skeleton.find_joint_by_name(joint).expect("joint exists");
    let rest = model_pose(skeleton, &rest_pose(skeleton))[index].rotation;
    model_pose(skeleton, locals)[index]
        .rotation
        .mul_quat(rest.conjugate())
        .normalize()
}

#[test]
fn identical_rigs_reproduce_the_source_motion() {
    let rig = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("a:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&rig, &rig, &profile).expect("profile resolves");

    let source = pose(
        &rig,
        &[
            ("a:upperarm_l", quat(Vec3::new(0.0, 0.0, 1.0), 40.0)),
            ("a:lowerarm_l", quat(Vec3::new(0.0, 1.0, 0.0), -30.0)),
            ("a:spine", quat(Vec3::new(1.0, 0.0, 0.0), 15.0)),
        ],
    );
    let result = retargeter.pose(&source);

    assert!((retargeter.scale() - 1.0).abs() < 1.0e-5);
    for (index, joint) in rig.joints.iter().enumerate() {
        assert_quat_eq(
            result[index].rotation,
            source[index].rotation,
            &format!("joint {}", joint.name),
        );
        assert_vec_eq(
            result[index].translation,
            source[index].translation,
            1.0e-4,
            &format!("joint {}", joint.name),
        );
    }
}

#[test]
fn a_source_at_rest_leaves_the_target_at_rest() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.7, Vec3::new(0.0, 0.0, -45.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let result = retargeter.pose(&rest_pose(&source));
    for (index, joint) in target.joints.iter().enumerate() {
        let rest = JointTransform::from_transform(&joint.local_transform);
        assert_quat_eq(result[index].rotation, rest.rotation, &joint.name);
        assert_vec_eq(
            result[index].translation,
            rest.translation,
            1.0e-4,
            &joint.name,
        );
    }
}

#[test]
fn different_rest_orientations_are_corrected() {
    // A T-posed source and an A-posed target: copying local rotations across
    // would put the target's arm 45 degrees off.
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, -45.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let lift = quat(Vec3::new(1.0, 0.0, 0.0), 35.0);
    let source_pose = pose(&source, &[("a:upperarm_l", lift)]);
    let result = retargeter.pose(&source_pose);

    let source_delta = model_delta(&source, &source_pose, "a:upperarm_l");
    let target_delta = model_delta(&target, &result, "b:upperarm_l");
    assert_quat_eq(target_delta, source_delta, "upper arm motion");

    // And it is genuinely a correction, not a copy of the local rotation.
    let source_local = source_pose[source.find_joint_by_name("a:upperarm_l").unwrap()].rotation;
    let target_local = result[target.find_joint_by_name("b:upperarm_l").unwrap()].rotation;
    assert!(
        target_local.dot(source_local).abs() < 0.999,
        "the target's local rotation should differ from the source's"
    );
}

#[test]
fn missing_required_joints_fail_with_a_useful_message() {
    let rig = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = humanoid_profile("a:").with_required(HumanoidJoint::Head, "a:cranium");
    let error = profile
        .resolve(&rig)
        .expect_err("a required joint the rig lacks must fail");
    let message = format!("{error}");
    assert!(message.contains("Head"), "{message}");
    assert!(message.contains("a:cranium"), "{message}");
}

#[test]
fn missing_optional_joints_are_reported_and_retargeting_continues() {
    let rig = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    // The rig has no toes and no fingers; neither is required.
    let profile = humanoid_profile("a:")
        .with(HumanoidJoint::ToeLeft, "a:toe_l")
        .with(HumanoidJoint::IndexProximalLeft, "a:index_l");
    let resolved = profile
        .resolve(&rig)
        .expect("optional joints may be absent");
    assert!(resolved.missing.contains(&HumanoidJoint::ToeLeft));
    assert!(resolved.missing.contains(&HumanoidJoint::IndexProximalLeft));
    assert!(resolved.has(HumanoidJoint::FootLeft));
}

#[test]
fn unmapped_source_bones_are_ignored_and_extra_target_bones_keep_their_rest_pose() {
    let mut source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let hips = source.find_joint_by_name("a:hips").unwrap();
    source.joints.push(Joint::new(
        "a:prop".to_string(),
        source.joints.len(),
        Some(hips),
        Mat4::identity(),
        Transform::from_position(Vec3::new(0.3, 0.0, 0.0)),
        Some(source.joints.len()),
    ));

    let mut target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let chest = target.find_joint_by_name("b:chest").unwrap();
    let tail_rest = Transform::new(
        Vec3::new(0.0, 0.05, -0.2),
        Vec3::new(0.0, 30.0, 0.0),
        Vec3::ones(),
    );
    target.joints.push(Joint::new(
        "b:tail".to_string(),
        target.joints.len(),
        Some(chest),
        Mat4::identity(),
        tail_rest,
        Some(target.joints.len()),
    ));

    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let source_pose = pose(
        &source,
        &[
            ("a:prop", quat(Vec3::new(0.0, 1.0, 0.0), 90.0)),
            ("a:spine", quat(Vec3::new(1.0, 0.0, 0.0), 20.0)),
        ],
    );
    let result = retargeter.pose(&source_pose);

    let tail = target.find_joint_by_name("b:tail").unwrap();
    let rest = JointTransform::from_transform(&tail_rest);
    assert_quat_eq(result[tail].rotation, rest.rotation, "extra target bone");
    assert_vec_eq(
        result[tail].translation,
        rest.translation,
        1.0e-5,
        "extra target bone",
    );

    let clip = retargeter.clip(&clip_from_pose(&source, &source_pose, 1.0));
    assert!(
        clip.track("b:tail").is_none(),
        "an unmapped target joint should not get a track"
    );
    assert!(
        clip.tracks.iter().all(|track| track.joint != "a:prop"),
        "an unmapped source joint should not leak into the output"
    );
}

/// Wraps a single pose into a two-key clip, so pose-level expectations can be
/// checked through the clip path as well.
fn clip_from_pose(skeleton: &Skeleton, locals: &[JointTransform], duration: f32) -> Animation {
    let mut clip = Animation::new("test");
    clip.duration = duration;
    for (index, local) in locals.iter().enumerate() {
        clip.tracks.push(JointTrack {
            joint: skeleton.joints[index].name.clone(),
            rotation: Some(Curve::new(
                vec![0.0, duration],
                vec![local.rotation, local.rotation],
            )),
            translation: Some(Curve::new(
                vec![0.0, duration],
                vec![local.translation, local.translation],
            )),
            scale: None,
        });
    }
    clip
}

#[test]
fn clip_duration_and_key_timing_survive_retargeting() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.4, Vec3::new(0.0, 0.0, -20.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());

    let mut clip = Animation::new("walk");
    clip.tracks.push(JointTrack {
        joint: "a:upperarm_l".into(),
        rotation: Some(Curve::new(
            vec![0.0, 0.25, 0.75, 1.5],
            vec![
                Vec4::quat_identity(),
                quat(Vec3::new(0.0, 0.0, 1.0), 20.0),
                quat(Vec3::new(0.0, 0.0, 1.0), -20.0),
                Vec4::quat_identity(),
            ],
        )),
        ..Default::default()
    });
    clip.tracks.push(JointTrack {
        joint: "a:hips".into(),
        translation: Some(Curve::new(
            vec![0.0, 1.0],
            vec![Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.1, 0.0)],
        )),
        ..Default::default()
    });
    clip.recompute_duration();

    let retargeted = retarget(&source, &clip, &target, &profile).expect("retargeting succeeds");

    assert_eq!(retargeted.name, "walk");
    assert!((retargeted.duration - clip.duration).abs() < 1.0e-6);
    assert_eq!(retargeted.key_times(), clip.key_times());
    let arm = retargeted
        .track("b:upperarm_l")
        .expect("the arm is animated");
    assert_eq!(
        arm.rotation.as_ref().expect("rotation track").times,
        vec![0.0, 0.25, 0.75, 1.0, 1.5]
    );
}

#[test]
fn output_quaternions_stay_normalized() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 2.3, Vec3::new(0.0, 0.0, -45.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let source_pose = pose(
        &source,
        &[
            ("a:hips", quat(Vec3::new(0.2, 1.0, 0.3), 65.0)),
            ("a:spine", quat(Vec3::new(1.0, 0.0, 0.4), 25.0)),
            ("a:upperarm_r", quat(Vec3::new(0.0, 0.3, 1.0), -80.0)),
            ("a:lowerleg_l", quat(Vec3::new(1.0, 0.0, 0.0), 55.0)),
        ],
    );
    for local in retargeter.pose(&source_pose) {
        let length = local.rotation.dot(local.rotation).sqrt();
        assert!(
            (length - 1.0).abs() < 1.0e-5,
            "quaternion length {length} is not 1"
        );
    }
}

#[test]
fn scale_normalization_follows_the_rigs_proportions() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 2.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    // The rigs differ only in size, so pelvis-to-head gives exactly 2.
    assert!(
        (retargeter.scale() - 2.0).abs() < 1.0e-4,
        "{}",
        retargeter.scale()
    );

    // A pelvis lift of 0.1 source units becomes 0.2 target units.
    let mut source_pose = rest_pose(&source);
    let hips = source.find_joint_by_name("a:hips").unwrap();
    source_pose[hips].translation = source_pose[hips].translation + Vec3::new(0.0, 0.1, 0.0);

    let result = retargeter.pose(&source_pose);
    let target_hips = target.find_joint_by_name("b:hips").unwrap();
    let rest = rest_pose(&target)[target_hips].translation;
    assert_vec_eq(
        result[target_hips].translation - rest,
        Vec3::new(0.0, 0.2, 0.0),
        1.0e-4,
        "scaled pelvis translation",
    );
}

#[test]
fn a_fixed_scale_overrides_the_measurement() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 2.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings().with_scale(ScalePolicy::Fixed(0.5)));
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");
    assert!((retargeter.scale() - 0.5).abs() < 1.0e-5);
}

#[test]
fn limb_translations_are_ignored_so_proportions_survive() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 2.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    // A source clip that stretches the forearm must not stretch the target.
    let mut source_pose = rest_pose(&source);
    let forearm = source.find_joint_by_name("a:lowerarm_l").unwrap();
    source_pose[forearm].translation = source_pose[forearm].translation * 3.0;

    let result = retargeter.pose(&source_pose);
    let target_forearm = target.find_joint_by_name("b:lowerarm_l").unwrap();
    assert_vec_eq(
        result[target_forearm].translation,
        rest_pose(&target)[target_forearm].translation,
        1.0e-5,
        "forearm translation",
    );
}

#[test]
fn root_motion_is_separated_from_the_pose_and_scaled() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 2.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(
            RetargetSettings::default().with_root_motion(RootMotionPolicy::Extract(
                RootMotionChannels {
                    horizontal: true,
                    vertical: false,
                    yaw: true,
                },
            )),
        );
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let hips = source.find_joint_by_name("a:hips").unwrap();
    let mut source_pose = rest_pose(&source);
    source_pose[hips].translation = source_pose[hips].translation + Vec3::new(1.0, 0.2, 0.0);
    source_pose[hips].rotation = quat(Vec3::new(0.0, 1.0, 0.0), 90.0);

    let (locals, locomotion) = retargeter.pose_with_root(&source_pose);

    // Horizontal travel and yaw left the pose ...
    assert_vec_eq(
        locomotion.translation,
        Vec3::new(2.0, 0.0, 0.0),
        1.0e-4,
        "extracted travel, in target units",
    );
    assert_quat_eq(
        locomotion.rotation,
        quat(Vec3::new(0.0, 1.0, 0.0), 90.0),
        "extracted yaw",
    );

    // ... while the vertical component stayed in it, scaled with the rig.
    let target_hips = target.find_joint_by_name("b:hips").unwrap();
    let rest = rest_pose(&target)[target_hips].translation;
    assert_vec_eq(
        locals[target_hips].translation - rest,
        Vec3::new(0.0, 0.4, 0.0),
        1.0e-4,
        "vertical motion stays in the pose",
    );
    assert_quat_eq(
        locals[target_hips].rotation,
        Vec4::quat_identity(),
        "the pelvis faces its rest direction once yaw is extracted",
    );
}

#[test]
fn an_in_place_clip_gets_no_root_motion_track() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"));

    let source_pose = pose(
        &source,
        &[("a:spine", quat(Vec3::new(1.0, 0.0, 0.0), 10.0))],
    );
    let clip = retarget(
        &source,
        &clip_from_pose(&source, &source_pose, 1.0),
        &target,
        &profile,
    )
    .expect("retargeting succeeds");
    assert!(clip.root_motion.is_none());
}

#[test]
fn a_longer_target_chain_receives_the_whole_source_bend() {
    // Two spine joints on the source, four on the target: the vocabulary
    // cannot pair these up, so the chain declaration has to.
    let source = skeleton(&[
        (
            "s:hips",
            None,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:spine",
            Some("s:hips"),
            Vec3::new(0.0, 0.2, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:chest",
            Some("s:spine"),
            Vec3::new(0.0, 0.2, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:head",
            Some("s:chest"),
            Vec3::new(0.0, 0.3, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
    ]);
    let target = skeleton(&[
        (
            "t:pelvis",
            None,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:spine0",
            Some("t:pelvis"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:spine1",
            Some("t:spine0"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:spine2",
            Some("t:spine1"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:spine3",
            Some("t:spine2"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:head",
            Some("t:spine3"),
            Vec3::new(0.0, 0.3, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
    ]);

    let source_profile = RigProfile::new("short spine")
        .with_required(HumanoidJoint::Pelvis, "s:hips")
        .with(HumanoidJoint::SpineLower, "s:spine")
        .with(HumanoidJoint::Chest, "s:chest")
        .with(HumanoidJoint::Head, "s:head")
        .with_chain(
            HumanoidChain::Spine,
            ChainBinding::new(["s:spine", "s:chest"]),
        );
    let target_profile = RigProfile::new("long spine")
        .with_required(HumanoidJoint::Pelvis, "t:pelvis")
        .with(HumanoidJoint::SpineLower, "t:spine0")
        .with(HumanoidJoint::SpineMid, "t:spine1")
        .with(HumanoidJoint::Chest, "t:spine2")
        .with(HumanoidJoint::Head, "t:head")
        .with_chain(
            HumanoidChain::Spine,
            ChainBinding::new(["t:spine0", "t:spine1", "t:spine2", "t:spine3"]),
        );

    let profile = RetargetProfile::new(source_profile, target_profile).with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let bend = quat(Vec3::new(1.0, 0.0, 0.0), 20.0);
    let source_pose = pose(&source, &[("s:spine", bend), ("s:chest", bend)]);
    let result = retargeter.pose(&source_pose);

    // Every target spine joint took a share ...
    for joint in ["t:spine0", "t:spine1", "t:spine2", "t:spine3"] {
        let index = target.find_joint_by_name(joint).unwrap();
        assert!(
            result[index].rotation.w.abs() < 0.9999,
            "{joint} should carry part of the bend"
        );
    }
    // ... and the top of the chain ends up where the source's does.
    assert_quat_eq(
        model_delta(&target, &result, "t:spine3"),
        model_delta(&source, &source_pose, "s:chest"),
        "accumulated spine bend",
    );
}

#[test]
fn a_shorter_target_chain_still_receives_the_full_bend() {
    let source = skeleton(&[
        (
            "s:hips",
            None,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:spine0",
            Some("s:hips"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:spine1",
            Some("s:spine0"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "s:spine2",
            Some("s:spine1"),
            Vec3::new(0.0, 0.1, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
    ]);
    let target = skeleton(&[
        (
            "t:hips",
            None,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:spine",
            Some("t:hips"),
            Vec3::new(0.0, 0.15, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
        (
            "t:chest",
            Some("t:spine"),
            Vec3::new(0.0, 0.15, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
        ),
    ]);

    let profile = RetargetProfile::new(
        RigProfile::new("four")
            .with_required(HumanoidJoint::Pelvis, "s:hips")
            .with_chain(
                HumanoidChain::Spine,
                ChainBinding::new(["s:spine0", "s:spine1", "s:spine2"]),
            ),
        RigProfile::new("two")
            .with_required(HumanoidJoint::Pelvis, "t:hips")
            .with_chain(
                HumanoidChain::Spine,
                ChainBinding::new(["t:spine", "t:chest"]),
            ),
    )
    .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let bend = quat(Vec3::new(0.0, 0.0, 1.0), 15.0);
    let source_pose = pose(
        &source,
        &[("s:spine0", bend), ("s:spine1", bend), ("s:spine2", bend)],
    );
    let result = retargeter.pose(&source_pose);

    assert_quat_eq(
        model_delta(&target, &result, "t:chest"),
        model_delta(&source, &source_pose, "s:spine2"),
        "accumulated bend into a shorter chain",
    );
}

#[test]
fn profiles_round_trip_through_serialization() {
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(
            settings()
                .with_translation(TranslationPolicy::PelvisOnly)
                .with_joint_translation(HumanoidJoint::HandLeft, TranslationPolicy::Scaled)
                .with_scale(ScalePolicy::Auto(ScaleMeasure::LegLength)),
        );

    let json = serde_json::to_string_pretty(&profile).expect("a profile serializes");
    let restored: RetargetProfile = serde_json::from_str(&json).expect("a profile deserializes");
    assert_eq!(profile, restored);
}

#[test]
fn clips_round_trip_through_serialization() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.6, Vec3::new(0.0, 0.0, -30.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"));

    let source_pose = pose(
        &source,
        &[("a:upperarm_l", quat(Vec3::new(0.0, 0.0, 1.0), 30.0))],
    );
    let clip = retarget(
        &source,
        &clip_from_pose(&source, &source_pose, 2.0),
        &target,
        &profile,
    )
    .expect("retargeting succeeds");

    let json = serde_json::to_string(&clip).expect("a clip serializes");
    let restored: Animation = serde_json::from_str(&json).expect("a clip deserializes");
    assert_eq!(clip, restored);

    // And it still plays: sampling the restored clip reproduces the pose.
    let binding = restored.bind(&target);
    let sampled = restored.sample(&binding, 1.0);
    let direct = Retargeter::new(&source, &target, &profile)
        .expect("profile resolves")
        .pose(&source_pose);
    for (index, joint) in target.joints.iter().enumerate() {
        assert_quat_eq(sampled[index].rotation, direct[index].rotation, &joint.name);
    }
}

#[test]
fn the_report_names_both_sides_of_every_mapping() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"));
    let report = Retargeter::new(&source, &target, &profile)
        .expect("profile resolves")
        .report();

    assert!(report.contains("Pelvis:"), "{report}");
    assert!(report.contains("source = a:hips"), "{report}");
    assert!(report.contains("target = b:hips"), "{report}");
    assert!(report.contains("end effectors"), "{report}");
}

#[test]
fn a_rigs_armature_rotation_is_part_of_its_frame() {
    // The same rig twice, differing only in the armature transform that
    // relates its joints to the scene -- which is exactly what a
    // Blender-exported glTF does, keeping its joints Z-up and putting the
    // conversion on the armature.
    let mut source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    source.transform = Transform::from_rotation(Vec3::new(90.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, 0.0));

    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let retargeter = Retargeter::new(&source, &target, &profile).expect("profile resolves");

    let lift = quat(Vec3::new(1.0, 0.0, 0.0), 40.0);
    let source_pose = pose(&source, &[("a:upperarm_l", lift)]);
    let result = retargeter.pose(&source_pose);

    // The source's motion, expressed in the scene rather than in its own
    // joint space, is what should arrive on the target.
    let basis = JointTransform::from_transform(&source.transform).rotation;
    let expected = basis
        .mul_quat(model_delta(&source, &source_pose, "a:upperarm_l"))
        .mul_quat(basis.conjugate());
    assert_quat_eq(
        model_delta(&target, &result, "b:upperarm_l"),
        expected,
        "motion across a rotated armature",
    );
}

#[test]
fn root_motion_is_measured_from_the_clips_own_first_frame() {
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let profile = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"));
    let hips = source.find_joint_by_name("a:hips").unwrap();
    let rest = rest_pose(&source)[hips].translation;

    // A clip authored away from its rig's bind pose, but not travelling.
    // The offset is an authoring artifact, not locomotion.
    let mut parked = rest_pose(&source);
    parked[hips].translation = rest + Vec3::new(3.0, 0.0, -7.0);
    let clip = retarget(
        &source,
        &clip_from_pose(&source, &parked, 1.0),
        &target,
        &profile,
    )
    .expect("retargeting succeeds");
    assert!(
        clip.root_motion.is_none(),
        "a parked clip should read as in place, not as a seven metre teleport"
    );

    // A clip that does travel starts its track at zero and reports the delta.
    let mut walk = Animation::new("walk");
    walk.tracks.push(JointTrack {
        joint: "a:hips".into(),
        translation: Some(Curve::new(
            vec![0.0, 1.0],
            vec![
                rest + Vec3::new(3.0, 0.0, 0.0),
                rest + Vec3::new(5.0, 0.0, 0.0),
            ],
        )),
        ..Default::default()
    });
    walk.recompute_duration();

    let clip = retarget(&source, &walk, &target, &profile).expect("retargeting succeeds");
    let motion = clip.root_motion.as_ref().expect("the clip travels");
    let travel = motion.translation.as_ref().expect("a travel curve");
    assert_vec_eq(
        travel.values[0],
        Vec3::new(0.0, 0.0, 0.0),
        1.0e-4,
        "first key",
    );
    assert_vec_eq(
        travel.values[1],
        Vec3::new(2.0, 0.0, 0.0),
        1.0e-4,
        "the delta",
    );
}

#[test]
fn a_t_pose_reference_measures_motion_from_a_t_pose_not_the_bind_pose() {
    // A T-posed source and an A-posed target. The source at its bind pose *is*
    // a T-pose, so a target that declares the T-pose as its reference should
    // land in a T-pose too -- arms out, not down where it binds.
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(0.0, 0.0, -45.0));

    let arm_direction = |skeleton: &Skeleton, locals: &[JointTransform], prefix: &str| {
        let model = model_pose(skeleton, locals);
        let upper = model[skeleton
            .find_joint_by_name(&format!("{prefix}upperarm_l"))
            .unwrap()]
        .translation;
        let lower = model[skeleton
            .find_joint_by_name(&format!("{prefix}lowerarm_l"))
            .unwrap()]
        .translation;
        (lower - upper).normalize()
    };

    let referenced = RetargetProfile::new(
        humanoid_profile("a:"),
        humanoid_profile("b:").with_reference(ReferencePose::TPose),
    )
    .with_settings(settings());
    let posed = Retargeter::new(&source, &target, &referenced)
        .expect("profile resolves")
        .pose(&rest_pose(&source));
    let bone = arm_direction(&target, &posed, "b:");
    assert!(
        bone.x > 0.99,
        "a T-pose reference should straighten the arm, got {bone}"
    );

    // The default is the rig's own bind pose, which leaves it where it binds.
    let bound = RetargetProfile::new(humanoid_profile("a:"), humanoid_profile("b:"))
        .with_settings(settings());
    let posed = Retargeter::new(&source, &target, &bound)
        .expect("profile resolves")
        .pose(&rest_pose(&source));
    let bone = arm_direction(&target, &posed, "b:");
    assert!(
        bone.x < 0.8 && bone.y < -0.5,
        "the bind reference should leave the arm down, got {bone}"
    );
}

#[test]
fn a_reference_pose_round_trips_through_serialization() {
    let profile = humanoid_profile("a:").with_reference(ReferencePose::TPose);
    let json = serde_json::to_string(&profile).expect("it serializes");
    let restored: RigProfile = serde_json::from_str(&json).expect("it deserializes");
    assert_eq!(restored.reference, ReferencePose::TPose);
    assert_eq!(profile, restored);

    // An absent reference reads as the bind pose, so old profiles keep working.
    let legacy: RigProfile =
        serde_json::from_str(r#"{"name":"old","joints":{}}"#).expect("a profile without one");
    assert_eq!(legacy.reference, ReferencePose::Bind);
}

#[test]
fn a_declared_hinge_is_rolled_onto_the_t_poses_own_hinge() {
    // Both rigs T-posed in *direction*, but the target's arm is rolled 60
    // degrees about its own bone. Straightening cannot see that: the bone
    // points the same way either way. Only the hinge declaration can.
    let source = humanoid("a:", 1.0, Vec3::new(0.0, 0.0, 0.0));
    let target = humanoid("b:", 1.0, Vec3::new(60.0, 0.0, -45.0));
    let hinge = Vec3::new(0.0, 0.0, 1.0);

    // The source flexes its elbow the way a T-posed body does, about -Y.
    let mut flexed = rest_pose(&source);
    flexed[source.find_joint_by_name("a:lowerarm_l").unwrap()].rotation =
        quat(Vec3::new(0.0, -1.0, 0.0), 70.0);

    let elbow = target.find_joint_by_name("b:lowerarm_l").unwrap();
    let target_rest = rest_pose(&target);
    let off_hinge = |profile: RigProfile| {
        let profile = RetargetProfile::new(
            humanoid_profile("a:").with_reference(ReferencePose::TPose),
            profile.with_reference(ReferencePose::TPose),
        )
        .with_settings(settings());
        let posed = Retargeter::new(&source, &target, &profile)
            .expect("profile resolves")
            .pose(&flexed);
        let animated = target_rest[elbow]
            .rotation
            .conjugate()
            .mul_quat(posed[elbow].rotation)
            .normalize();
        let swing = animated
            .mul_quat(animated.twist_about(hinge).conjugate())
            .normalize();
        (2.0 * swing.w.abs().clamp(0.0, 1.0).acos()).to_degrees()
    };

    // Straightened but not aimed, the roll arrives as bend across the hinge --
    // motion a single-channel elbow would have to throw away.
    let unaimed = off_hinge(humanoid_profile("b:"));
    assert!(
        unaimed > 30.0,
        "the target's roll should show up as off-hinge motion, got {unaimed:.1} degrees"
    );

    // Declaring the hinge lets the reference roll the arm until the two agree.
    let aimed = off_hinge(humanoid_profile("b:").with_joint(
        HumanoidJoint::LowerArmLeft,
        JointBinding::new("b:lowerarm_l").with_hinge(hinge),
    ));
    assert!(
        aimed < 1.0,
        "a declared hinge should leave the bend on the hinge, got {aimed:.1} degrees"
    );
}
