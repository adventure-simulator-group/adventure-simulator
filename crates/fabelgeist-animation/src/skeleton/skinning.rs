//! Turning a posed skeleton into skinning matrices.
//!
//! This is the CPU half of skinning: it produces the joint matrices a shader
//! multiplies vertices by, without knowing what a shader is. `fabelgeist-gpu` wraps
//! the result in a buffer-backed `Pose`.

use crate::skeleton::Skeleton;
use anyhow::{Result, anyhow};
use fabelgeist_math::matrix::Mat4;

/// Builds matrices that deform mesh-local vertices into mesh-local vertices.
///
/// Joint transforms are stored relative to the skeleton root, while imported
/// inverse bind matrices include that root transform. Applying the root here
/// keeps it out of `Mesh::transform`, which is reserved for rendering/placement.
pub fn build_skinning_matrices(
    skeleton: &Skeleton,
    world_transforms: &[Mat4],
) -> Result<Vec<Mat4>> {
    if world_transforms.len() != skeleton.joints.len() {
        return Err(anyhow!("Pose joint count mismatch with skeleton"));
    }

    let skin_joint_count = skeleton
        .joints
        .iter()
        .filter_map(|joint| joint.joint_index)
        .max()
        .map_or(0, |index| index + 1);
    let skeleton_root = skeleton.transform.to_mat4();
    let mut joint_matrices = vec![Mat4::identity(); skin_joint_count];

    for (joint, world_transform) in skeleton.joints.iter().zip(world_transforms) {
        if let Some(index) = joint.joint_index {
            joint_matrices[index] = skeleton_root * *world_transform * joint.inverse_bind_matrix;
        }
    }

    Ok(joint_matrices)
}
