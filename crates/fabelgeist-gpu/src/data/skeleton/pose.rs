use crate::data::gpu::resource::GpuResource;
use crate::data::matrix::Mat4;
use crate::data::skeleton::Skeleton;
use crate::data::transform::Transform;
use crate::data::vector::{Vec3, Vec4};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};

pub use fabelgeist_animation::skeleton::build_skinning_matrices;

#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub joint_matrices: GpuResource, // Buffer containing Mat4 array
    pub world_transforms: Vec<Mat4>, // Global transforms for each joint
    pub local_transforms: Vec<(Vec3, Vec4, Vec3)>, // TRS for each joint
}

impl Pose {
    /// Builds a pose from a skeleton's local transforms.
    ///
    /// This is the seam between animation and rendering: anything that can
    /// produce local transforms — a clip, a retargeter, a solver — reaches the
    /// GPU through here, and none of them need to know how skinning works.
    pub fn from_locals(
        context: &WgpuContext,
        skeleton: &Skeleton,
        locals: &[crate::data::animation::JointTransform],
    ) -> Result<Self> {
        if locals.len() != skeleton.joints.len() {
            return Err(anyhow!("Pose joint count mismatch with skeleton"));
        }

        let world_transforms: Vec<Mat4> = crate::data::animation::model_pose(skeleton, locals)
            .into_iter()
            .map(|transform| transform.to_mat4())
            .collect();
        let local_transforms = locals.iter().map(|local| local.to_trs()).collect();
        let joint_matrices = build_skinning_matrices(skeleton, &world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage().with_label("Pose Matrices"),
        )?;

        Ok(Pose {
            joint_matrices: GpuResource::Buffer(buffer),
            world_transforms,
            local_transforms,
        })
    }

    /// Samples an engine-native clip onto a skeleton.
    ///
    /// `binding` comes from [`Animation::bind`](crate::data::animation::Animation::bind)
    /// and should be kept across frames; it is what makes per-frame sampling
    /// free of name lookups.
    pub fn from_clip(
        context: &WgpuContext,
        skeleton: &Skeleton,
        clip: &crate::data::animation::Animation,
        binding: &crate::data::animation::ClipBinding,
        time: f32,
    ) -> Result<Self> {
        let locals = clip.sample(binding, clip.loop_time(time));
        Self::from_locals(context, skeleton, &locals)
    }

    pub fn blend(
        context: &WgpuContext,
        skeleton: &Skeleton,
        pose_a: &Pose,
        pose_b: &Pose,
        factor: f32,
    ) -> Result<Self> {
        let count = skeleton.joints.len();
        if pose_a.local_transforms.len() != count || pose_b.local_transforms.len() != count {
            return Err(anyhow!("Pose joint count mismatch with skeleton"));
        }

        let mut local_transforms = Vec::with_capacity(count);
        let mut world_transforms = vec![Mat4::identity(); count];

        for i in 0..count {
            let (t0, r0, s0) = pose_a.local_transforms[i];
            let (t1, r1, s1) = pose_b.local_transforms[i];

            let t = t0.lerp(t1, factor);
            let r = r0.slerp(r1, factor);
            let s = s0.lerp(s1, factor);

            local_transforms.push((t, r, s));

            let joint = &skeleton.joints[i];
            let local_matrix = Mat4::from_trs(t, r, s);
            let global_matrix = if let Some(parent_idx) = joint.parent_index {
                world_transforms[parent_idx] * local_matrix
            } else {
                local_matrix
            };
            world_transforms[i] = global_matrix;
        }

        let joint_matrices = build_skinning_matrices(skeleton, &world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage()
                .with_label("Blended Pose Matrices"),
        )?;

        Ok(Pose {
            joint_matrices: GpuResource::Buffer(buffer),
            world_transforms,
            local_transforms,
        })
    }

    pub fn identity(context: &WgpuContext, skeleton: &Skeleton) -> Result<Self> {
        let count = skeleton.joints.len();
        let mut local_transforms = Vec::with_capacity(count);
        let mut world_transforms = vec![Mat4::identity(); count];

        for i in 0..count {
            let joint = &skeleton.joints[i];
            let (t, r, s) = joint.local_transform.to_trs();
            local_transforms.push((t, r, s));

            let local_matrix = Mat4::from_trs(t, r, s);
            let global_matrix = if let Some(parent_idx) = joint.parent_index {
                world_transforms[parent_idx] * local_matrix
            } else {
                local_matrix
            };
            world_transforms[i] = global_matrix;
        }

        let joint_matrices = build_skinning_matrices(skeleton, &world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage()
                .with_label("Identity Pose Matrices"),
        )?;

        Ok(Pose {
            joint_matrices: GpuResource::Buffer(buffer),
            world_transforms,
            local_transforms,
        })
    }

    pub fn add(
        context: &WgpuContext,
        skeleton: &Skeleton,
        pose_a: &Pose,
        pose_b: &Pose,
    ) -> Result<Self> {
        let count = skeleton.joints.len();
        if pose_a.local_transforms.len() != count || pose_b.local_transforms.len() != count {
            return Err(anyhow!("Pose joint count mismatch with skeleton"));
        }

        let mut local_transforms = Vec::with_capacity(count);
        let mut world_transforms = vec![Mat4::identity(); count];

        for i in 0..count {
            let (t0, r0, s0) = pose_a.local_transforms[i];
            let (t1, r1, s1) = pose_b.local_transforms[i];

            let t = t0 + t1;
            let r = r0.mul_quat(r1);
            let s = s0 * s1;

            local_transforms.push((t, r, s));

            let joint = &skeleton.joints[i];
            let local_matrix = Mat4::from_trs(t, r, s);
            let global_matrix = if let Some(parent_idx) = joint.parent_index {
                world_transforms[parent_idx] * local_matrix
            } else {
                local_matrix
            };
            world_transforms[i] = global_matrix;
        }

        let joint_matrices = build_skinning_matrices(skeleton, &world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage().with_label("Added Pose Matrices"),
        )?;

        Ok(Pose {
            joint_matrices: GpuResource::Buffer(buffer),
            world_transforms,
            local_transforms,
        })
    }

    pub fn zero(context: &WgpuContext, skeleton: &Skeleton) -> Result<Self> {
        let count = skeleton.joints.len();
        let mut local_transforms = Vec::with_capacity(count);
        let mut world_transforms = vec![Mat4::identity(); count];

        for i in 0..count {
            let t = Vec3::new(0.0, 0.0, 0.0);
            let r = Vec4::new(0.0, 0.0, 0.0, 1.0);
            let s = Vec3::new(1.0, 1.0, 1.0);
            local_transforms.push((t, r, s));

            let local_matrix = Mat4::from_trs(t, r, s);
            let joint = &skeleton.joints[i];
            let global_matrix = if let Some(parent_idx) = joint.parent_index {
                world_transforms[parent_idx] * local_matrix
            } else {
                local_matrix
            };
            world_transforms[i] = global_matrix;
        }

        let joint_matrices = build_skinning_matrices(skeleton, &world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage().with_label("Zero Pose Matrices"),
        )?;

        Ok(Pose {
            joint_matrices: GpuResource::Buffer(buffer),
            world_transforms,
            local_transforms,
        })
    }

    pub fn set_joint(
        &mut self,
        context: &WgpuContext,
        skeleton: &Skeleton,
        joint_index: usize,
        transform: Transform,
    ) -> Result<()> {
        if joint_index >= self.local_transforms.len() {
            return Err(anyhow!("Joint index out of bounds"));
        }

        self.local_transforms[joint_index] = transform.to_trs();
        self.recompute_matrices(context, skeleton)
    }

    pub fn get_joint(&self, joint_index: usize) -> Result<Transform> {
        if joint_index >= self.local_transforms.len() {
            return Err(anyhow!("Joint index out of bounds"));
        }

        let (t, r, s) = self.local_transforms[joint_index];
        let local_matrix = Mat4::from_trs(t, r, s);
        Ok(Transform::from_mat4(local_matrix))
    }

    pub fn recompute_matrices(&mut self, context: &WgpuContext, skeleton: &Skeleton) -> Result<()> {
        let count = skeleton.joints.len();
        self.world_transforms = vec![Mat4::identity(); count];

        for i in 0..count {
            let (t, r, s) = self.local_transforms[i];
            let joint = &skeleton.joints[i];
            let local_matrix = Mat4::from_trs(t, r, s);
            let global_matrix = if let Some(parent_idx) = joint.parent_index {
                self.world_transforms[parent_idx] * local_matrix
            } else {
                local_matrix
            };
            self.world_transforms[i] = global_matrix;
        }

        let joint_matrices = build_skinning_matrices(skeleton, &self.world_transforms)?;

        let buffer = crate::data::gpu::buffer::Buffer::from_slice(
            context,
            &joint_matrices,
            crate::data::gpu::buffer::BufferDefinition::storage().with_label("Pose Matrices"),
        )?;

        self.joint_matrices = GpuResource::Buffer(buffer);
        Ok(())
    }

    pub fn rotate(
        &mut self,
        context: &WgpuContext,
        skeleton: &Skeleton,
        joint_index: usize,
        rotation: crate::data::vector::Vec3,
    ) -> Result<()> {
        let transform = self.get_joint(joint_index)?;
        let rotation_transform = Transform::from_rotation(rotation);
        let new_transform = transform.compose(&rotation_transform);
        self.set_joint(context, skeleton, joint_index, new_transform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::skeleton::Joint;

    #[test]
    fn skinning_matrices_cancel_the_skeleton_root_at_bind_pose() {
        let skeleton_root = Transform::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(90.0, 0.0, 0.0),
            Vec3::new(0.01, 0.01, 0.01),
        );
        let joint_world = Transform::from_position(Vec3::new(0.0, 100.0, 0.0)).to_mat4();
        let inverse_bind = (skeleton_root.to_mat4() * joint_world)
            .inverse()
            .expect("bind transform must be invertible");
        let mut skeleton = Skeleton::new(vec![Joint::new(
            "root".to_string(),
            0,
            None,
            inverse_bind,
            Transform::from_mat4(joint_world),
            Some(0),
        )]);
        skeleton.transform = skeleton_root;

        let matrices = build_skinning_matrices(&skeleton, &[joint_world])
            .expect("bind-pose skinning matrices should build");

        for column in 0..4 {
            for row in 0..4 {
                let expected = Mat4::identity().columns[column][row];
                assert!(
                    (matrices[0].columns[column][row] - expected).abs() < 1.0e-4,
                    "matrix differs at column {column}, row {row}"
                );
            }
        }
    }
}
