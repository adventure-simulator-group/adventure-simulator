//! Engine-native skeletal animation.
//!
//! An [`Animation`] is a self-contained, serializable set of keyframed
//! joint tracks addressed by joint name. It is the format every importer bakes
//! into and the only format the retargeter and the runtime sampler speak, so a
//! clip that came from glTF, from a retarget pass, or from disk are all the
//! same thing by the time anything plays them.
//!
//! Locomotion can be held separately from the pose in [`RootMotion`], which
//! keeps "where the character went" out of the joint hierarchy.

use crate::skeleton::Skeleton;
use fabelgeist_math::matrix::Mat4;
use fabelgeist_math::transform::Transform;
use fabelgeist_math::vector::{Vec3, Vec4};
use serde::{Deserialize, Serialize};

pub mod retarget;

/// A joint's local transform, with the rotation kept as a quaternion.
///
/// [`Transform`] stores Euler angles, which is convenient to author but lossy
/// to compose; animation and retargeting work in quaternions throughout and
/// only convert at the edges.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointTransform {
    pub translation: Vec3,
    /// Unit quaternion, `(x, y, z, w)`.
    pub rotation: Vec4,
    pub scale: Vec3,
}

impl Default for JointTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl JointTransform {
    pub fn identity() -> Self {
        Self {
            translation: Vec3::new(0.0, 0.0, 0.0),
            rotation: Vec4::quat_identity(),
            scale: Vec3::ones(),
        }
    }

    pub fn from_transform(transform: &Transform) -> Self {
        let (translation, rotation, scale) = transform.to_trs();
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Converts back to the Euler-based [`Transform`] the skeleton stores.
    pub fn to_transform(self) -> Transform {
        Transform::from_mat4(self.to_mat4())
    }

    /// Decomposes an affine matrix, taking the rotation out as a quaternion.
    ///
    /// Unlike going through [`Transform`], this never passes through Euler
    /// angles, so it is exact at any orientation. Importers need that: formats
    /// like FBX compose rotations with pivots and offsets, and the product is
    /// only available as a matrix.
    ///
    /// A negative determinant (a mirrored transform) is folded into the X
    /// scale, since a quaternion cannot represent a reflection.
    pub fn from_mat4(matrix: Mat4) -> Self {
        let columns = matrix.columns;
        let axis =
            |index: usize| Vec3::new(columns[index][0], columns[index][1], columns[index][2]);
        let (x, y, z) = (axis(0), axis(1), axis(2));

        let mut scale = Vec3::new(x.length(), y.length(), z.length());
        if x.cross(y).dot(z) < 0.0 {
            scale.x = -scale.x;
        }

        let normalize = |vector: Vec3, length: f32| {
            if length.abs() > 1.0e-8 {
                vector * (1.0 / length)
            } else {
                Vec3::new(0.0, 0.0, 0.0)
            }
        };
        let x = normalize(x, scale.x);
        let y = normalize(y, scale.y);
        let z = normalize(z, scale.z);

        // Shepperd's method: pick the largest diagonal term so the square root
        // is never taken of something near zero.
        let (m00, m10, m20) = (x.x, x.y, x.z);
        let (m01, m11, m21) = (y.x, y.y, y.z);
        let (m02, m12, m22) = (z.x, z.y, z.z);
        let trace = m00 + m11 + m22;
        let rotation = if trace > 0.0 {
            let s = (trace + 1.0).max(0.0).sqrt() * 2.0;
            Vec4::new((m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s)
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).max(0.0).sqrt() * 2.0;
            Vec4::new(0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s)
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).max(0.0).sqrt() * 2.0;
            Vec4::new((m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s)
        } else {
            let s = (1.0 + m22 - m00 - m11).max(0.0).sqrt() * 2.0;
            Vec4::new((m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s)
        };

        Self {
            translation: Vec3::new(columns[3][0], columns[3][1], columns[3][2]),
            rotation: rotation.normalize(),
            scale,
        }
    }

    pub fn to_mat4(self) -> Mat4 {
        Mat4::from_trs(self.translation, self.rotation, self.scale)
    }

    pub fn to_trs(self) -> (Vec3, Vec4, Vec3) {
        (self.translation, self.rotation, self.scale)
    }

    /// `self * child`, i.e. the child expressed in this transform's parent space.
    ///
    /// Exact for uniform scale, which is what skeletons use; non-uniform scale
    /// under a rotation is not representable as a TRS triple by anyone.
    pub fn compose(self, child: Self) -> Self {
        Self {
            translation: self.translation
                + self.rotation.rotate_vec3(self.scale * child.translation),
            rotation: self.rotation.mul_quat(child.rotation).normalize(),
            scale: self.scale * child.scale,
        }
    }

    pub fn inverse(self) -> Self {
        let rotation = self.rotation.conjugate();
        let scale = Vec3::new(
            invert(self.scale.x),
            invert(self.scale.y),
            invert(self.scale.z),
        );
        Self {
            translation: rotation.rotate_vec3(-self.translation) * scale,
            rotation,
            scale,
        }
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.translation + self.rotation.rotate_vec3(self.scale * point)
    }

    /// Re-normalizes the rotation, which repeated composition slowly erodes.
    pub fn normalized(mut self) -> Self {
        self.rotation = self.rotation.normalize();
        self
    }
}

fn invert(value: f32) -> f32 {
    if value.abs() > f32::EPSILON {
        1.0 / value
    } else {
        1.0
    }
}

/// A skeleton's local transforms in joint order, the currency of posing.
pub type LocalPose = Vec<JointTransform>;

/// The rest (bind) pose a skeleton declares through its joint hierarchy.
pub fn rest_pose(skeleton: &Skeleton) -> LocalPose {
    skeleton
        .joints
        .iter()
        .map(|joint| JointTransform::from_transform(&joint.local_transform))
        .collect()
}

/// Accumulates local transforms into model space, parents before children.
///
/// Joints are stored parent-first by every importer in the engine; a joint
/// whose parent appears later is treated as a root rather than silently
/// producing garbage.
pub fn model_pose(skeleton: &Skeleton, locals: &[JointTransform]) -> LocalPose {
    let mut model: LocalPose = Vec::with_capacity(locals.len());
    for (index, local) in locals.iter().enumerate() {
        let transform = match skeleton.joints[index].parent_index {
            Some(parent) if parent < index => model[parent].compose(*local),
            _ => *local,
        };
        model.push(transform);
    }
    model
}

/// Values that can be interpolated between keyframes.
pub trait Interpolate: Copy {
    fn interpolate(self, other: Self, factor: f32) -> Self;
}

impl Interpolate for Vec3 {
    fn interpolate(self, other: Self, factor: f32) -> Self {
        self.lerp(other, factor)
    }
}

impl Interpolate for Vec4 {
    fn interpolate(self, other: Self, factor: f32) -> Self {
        self.slerp(other, factor)
    }
}

/// A keyframed channel: times and values, one value per time, linearly
/// interpolated (spherically, for quaternions).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve<T> {
    pub times: Vec<f32>,
    pub values: Vec<T>,
}

impl<T> Default for Curve<T> {
    fn default() -> Self {
        Self {
            times: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl<T: Interpolate> Curve<T> {
    pub fn new(times: Vec<f32>, values: Vec<T>) -> Self {
        Self { times, values }
    }

    /// A curve holding a single value for all time.
    pub fn constant(value: T) -> Self {
        Self {
            times: vec![0.0],
            values: vec![value],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.times.is_empty() || self.values.is_empty()
    }

    pub fn duration(&self) -> f32 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// Samples the curve, clamping outside the keyed range.
    pub fn sample(&self, time: f32) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let last = self.times.len().min(self.values.len()) - 1;
        if time <= self.times[0] {
            return Some(self.values[0]);
        }
        if time >= self.times[last] {
            return Some(self.values[last]);
        }
        let upper = self.times.partition_point(|&t| t < time).min(last);
        let lower = upper.saturating_sub(1);
        let span = self.times[upper] - self.times[lower];
        let factor = if span > f32::EPSILON {
            (time - self.times[lower]) / span
        } else {
            0.0
        };
        Some(self.values[lower].interpolate(self.values[upper], factor))
    }
}

/// Every channel animated for one joint. Absent channels fall back to the
/// skeleton's rest pose, so a rotation-only clip leaves proportions alone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JointTrack {
    /// Name of the joint in the skeleton this clip targets.
    pub joint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<Curve<Vec3>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Curve<Vec4>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Curve<Vec3>>,
}

impl JointTrack {
    pub fn new(joint: impl Into<String>) -> Self {
        Self {
            joint: joint.into(),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.translation.is_none() && self.rotation.is_none() && self.scale.is_none()
    }

    pub fn duration(&self) -> f32 {
        let mut duration: f32 = 0.0;
        if let Some(curve) = &self.translation {
            duration = duration.max(curve.duration());
        }
        if let Some(curve) = &self.rotation {
            duration = duration.max(curve.duration());
        }
        if let Some(curve) = &self.scale {
            duration = duration.max(curve.duration());
        }
        duration
    }

    /// Overlays this track's channels onto a joint's rest transform.
    pub fn sample_onto(&self, rest: JointTransform, time: f32) -> JointTransform {
        JointTransform {
            translation: self
                .translation
                .as_ref()
                .and_then(|curve| curve.sample(time))
                .unwrap_or(rest.translation),
            rotation: self
                .rotation
                .as_ref()
                .and_then(|curve| curve.sample(time))
                .unwrap_or(rest.rotation),
            scale: self
                .scale
                .as_ref()
                .and_then(|curve| curve.sample(time))
                .unwrap_or(rest.scale),
        }
    }
}

/// Locomotion held outside the joint hierarchy.
///
/// Displacement is in the clip's own space and relative to the start of the
/// clip, so a walk cycle can be applied to whatever the character's current
/// world transform happens to be.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RootMotion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<Curve<Vec3>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Curve<Vec4>>,
}

impl RootMotion {
    pub fn is_empty(&self) -> bool {
        self.translation.is_none() && self.rotation.is_none()
    }

    pub fn sample(&self, time: f32) -> JointTransform {
        JointTransform {
            translation: self
                .translation
                .as_ref()
                .and_then(|curve| curve.sample(time))
                .unwrap_or(Vec3::new(0.0, 0.0, 0.0)),
            rotation: self
                .rotation
                .as_ref()
                .and_then(|curve| curve.sample(time))
                .unwrap_or(Vec4::quat_identity()),
            scale: Vec3::ones(),
        }
    }
}

/// A skeletal animation, independent of any character instance.
///
/// Tracks name joints rather than indexing them, so one clip plays on every
/// skeleton that shares the naming — which is the point of retargeting onto a
/// canonical rig.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub duration: f32,
    pub tracks: Vec<JointTrack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_motion: Option<RootMotion>,
}

impl Animation {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn track(&self, joint: &str) -> Option<&JointTrack> {
        self.tracks.iter().find(|track| track.joint == joint)
    }

    /// Sets `duration` from the tracks, for clips assembled track by track.
    pub fn recompute_duration(&mut self) {
        self.duration = self
            .tracks
            .iter()
            .map(JointTrack::duration)
            .fold(0.0f32, f32::max);
    }

    /// Every distinct keyframe time in the clip, sorted.
    ///
    /// Retargeting re-keys at exactly these times so the output preserves the
    /// input's frame timing instead of resampling onto some fixed rate.
    pub fn key_times(&self) -> Vec<f32> {
        let mut times: Vec<f32> = Vec::new();
        let mut push = |curve_times: &[f32]| times.extend_from_slice(curve_times);
        for track in &self.tracks {
            if let Some(curve) = &track.translation {
                push(&curve.times);
            }
            if let Some(curve) = &track.rotation {
                push(&curve.times);
            }
            if let Some(curve) = &track.scale {
                push(&curve.times);
            }
        }
        if let Some(root) = &self.root_motion {
            if let Some(curve) = &root.translation {
                times.extend_from_slice(&curve.times);
            }
            if let Some(curve) = &root.rotation {
                times.extend_from_slice(&curve.times);
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        times.dedup_by(|a, b| (*a - *b).abs() <= 1.0e-6);
        if times.is_empty() {
            times.push(0.0);
        }
        times
    }

    /// Resolves track indices against a skeleton once, so per-frame sampling
    /// costs no lookups.
    pub fn bind(&self, skeleton: &Skeleton) -> ClipBinding {
        let tracks = skeleton
            .joints
            .iter()
            .map(|joint| {
                self.tracks
                    .iter()
                    .position(|track| track.joint == joint.name)
            })
            .collect();
        ClipBinding {
            tracks,
            rest: rest_pose(skeleton),
        }
    }

    /// Samples the clip into local joint transforms; unanimated joints keep
    /// their rest transform.
    pub fn sample(&self, binding: &ClipBinding, time: f32) -> LocalPose {
        binding
            .rest
            .iter()
            .zip(&binding.tracks)
            .map(|(rest, track)| match track {
                Some(index) => self.tracks[*index].sample_onto(*rest, time),
                None => *rest,
            })
            .collect()
    }

    /// Wraps `time` into the clip's duration, for looping playback.
    pub fn loop_time(&self, time: f32) -> f32 {
        if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        }
    }

    /// Joints named by the clip that the skeleton does not have.
    pub fn unbound_tracks(&self, skeleton: &Skeleton) -> Vec<&str> {
        self.tracks
            .iter()
            .filter(|track| skeleton.find_joint_by_name(&track.joint).is_none())
            .map(|track| track.joint.as_str())
            .collect()
    }
}

/// A clip's tracks resolved against a specific skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipBinding {
    /// Track index per skeleton joint, `None` where the joint is unanimated.
    pub tracks: Vec<Option<usize>>,
    pub rest: LocalPose,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_clamp_outside_their_keyed_range() {
        let curve = Curve::new(vec![1.0, 2.0], vec![Vec3::new(0.0, 0.0, 0.0), Vec3::ones()]);
        assert_eq!(curve.sample(0.0), Some(Vec3::new(0.0, 0.0, 0.0)));
        assert_eq!(curve.sample(3.0), Some(Vec3::ones()));
        assert_eq!(curve.sample(1.5), Some(Vec3::from_scalar(0.5)));
    }

    #[test]
    fn composing_a_transform_with_its_inverse_is_the_identity() {
        let transform = JointTransform {
            translation: Vec3::new(1.0, -2.0, 3.0),
            rotation: Vec4::from_axis_angle(Vec3::new(1.0, 2.0, 3.0), 0.7),
            scale: Vec3::from_scalar(2.0),
        };
        let identity = transform.compose(transform.inverse());
        assert!(identity.translation.length() < 1.0e-5);
        assert!((identity.rotation.w.abs() - 1.0).abs() < 1.0e-5);
        assert!((identity.scale.x - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn matrix_decomposition_round_trips_at_any_orientation() {
        // Including the quarter turns where Euler extraction is degenerate.
        for angle in [0.0, 0.5, 90.0f32.to_radians(), -90.0f32.to_radians(), 2.9] {
            for axis in [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.3, -0.7, 1.0),
            ] {
                let original = JointTransform {
                    translation: Vec3::new(3.0, -1.0, 0.5),
                    rotation: Vec4::from_axis_angle(axis, angle),
                    scale: Vec3::new(2.0, 2.0, 2.0),
                };
                let decomposed = JointTransform::from_mat4(original.to_mat4());
                assert!(
                    (decomposed.rotation.dot(original.rotation).abs() - 1.0).abs() < 1.0e-5,
                    "axis {axis}, angle {angle}: {} is not {}",
                    decomposed.rotation,
                    original.rotation
                );
                assert!((decomposed.translation - original.translation).length() < 1.0e-5);
                assert!((decomposed.scale.x - 2.0).abs() < 1.0e-5);
            }
        }
    }

    #[test]
    fn key_times_merge_every_channel() {
        let mut clip = Animation::new("test");
        clip.tracks.push(JointTrack {
            joint: "a".into(),
            rotation: Some(Curve::new(
                vec![0.0, 0.5],
                vec![Vec4::quat_identity(), Vec4::quat_identity()],
            )),
            ..Default::default()
        });
        clip.tracks.push(JointTrack {
            joint: "b".into(),
            translation: Some(Curve::new(vec![0.5, 1.0], vec![Vec3::ones(), Vec3::ones()])),
            ..Default::default()
        });
        clip.recompute_duration();
        assert_eq!(clip.key_times(), vec![0.0, 0.5, 1.0]);
        assert_eq!(clip.duration, 1.0);
    }
}
