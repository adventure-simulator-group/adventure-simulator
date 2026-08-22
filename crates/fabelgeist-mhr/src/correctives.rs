//! The non-linear pose-corrective network.
//!
//! Joint rotations are turned into a 6D rotation feature per joint, pushed
//! through a sparse 750 -> 3000 layer, a ReLU, and a dense 3000 -> `3 * V`
//! layer, giving a per-vertex offset applied before skinning.

use std::path::Path;

use anyhow::{Context, Result, bail};
use burn::tensor::{Device, Tensor, TensorData, activation};
use fabelgeist_numpy_storage::Npz;

/// The first two joints do not define a local pose, so they carry no feature.
pub const SKIPPED_JOINTS: usize = 2;
/// Hidden units per posed joint.
const HIDDEN_PER_JOINT: usize = 24;
/// Feature channels per posed joint (a 6D rotation).
const FEATURES_PER_JOINT: usize = 6;

/// Names of the arrays MHR stores the network in.
const BASIS_ARRAY: &str = "corrective_blendshapes";
const SPARSE_INDICES_ARRAY: &str = "0.sparse_indices";
const SPARSE_WEIGHT_ARRAY: &str = "0.sparse_weight";

pub struct PoseCorrectives {
    /// Sparse activation layer, densified and transposed: `[posed * 6, hidden]`.
    activation: Tensor<2>,
    /// Corrective basis: `[hidden, vertices * 3]`.
    basis: Tensor<2>,
    posed_joints: usize,
    num_vertices: usize,
}

impl PoseCorrectives {
    /// Loads the corrective network for one level of detail, or `None` when the
    /// LOD ships no corrective basis.
    ///
    /// `activation_path` holds the sparse mask and weights shared by every LOD;
    /// `basis_path` holds this LOD's corrective basis.
    pub fn load(
        activation_path: &Path,
        basis_path: &Path,
        num_joints: usize,
        num_vertices: usize,
        device: &Device,
    ) -> Result<Option<Self>> {
        let archive = Npz::open(basis_path)?;
        let activation = Npz::open(activation_path)?;
        Self::from_archives(activation, archive, num_joints, num_vertices, device)
    }

    /// Loads corrective archives already held in memory (for web/streamed assets).
    pub fn from_bytes(
        activation: Vec<u8>,
        basis: Vec<u8>,
        num_joints: usize,
        num_vertices: usize,
        device: &Device,
    ) -> Result<Option<Self>> {
        Self::from_archives(
            Npz::from_bytes(activation)?,
            Npz::from_bytes(basis)?,
            num_joints,
            num_vertices,
            device,
        )
    }

    fn from_archives(
        activation: Npz,
        archive: Npz,
        num_joints: usize,
        num_vertices: usize,
        device: &Device,
    ) -> Result<Option<Self>> {
        if !archive.contains(BASIS_ARRAY) {
            return Ok(None);
        }
        let basis = archive
            .array(BASIS_ARRAY)
            .context("reading the corrective basis")?;
        let (components, basis_vertices) = match basis.shape[..] {
            [components, vertices, 3] => (components, vertices),
            _ => bail!(
                "{BASIS_ARRAY} has shape {:?}, expected [components, vertices, 3]",
                basis.shape
            ),
        };
        if basis_vertices != num_vertices {
            bail!(
                "corrective basis covers {basis_vertices} vertices but the mesh has {num_vertices}"
            );
        }

        let posed_joints = num_joints - SKIPPED_JOINTS;
        let hidden = posed_joints * HIDDEN_PER_JOINT;
        if components != hidden {
            bail!("corrective basis has {components} components, expected {hidden}");
        }

        let indices = activation
            .array(SPARSE_INDICES_ARRAY)
            .context("reading the sparse activation indices")?
            .to_i64();
        let values = activation
            .array(SPARSE_WEIGHT_ARRAY)
            .context("reading the sparse activation weights")?
            .to_f32();
        if indices.len() != 2 * values.len() {
            bail!(
                "sparse activation has {} indices for {} weights",
                indices.len(),
                values.len()
            );
        }

        // Densify straight into the transposed layout the matmul wants. The
        // layer is 4% dense, so a dense matmul beats a sparse gather on GPU.
        let inputs = posed_joints * FEATURES_PER_JOINT;
        let mut dense = vec![0.0f32; inputs * hidden];
        for (slot, value) in values.iter().enumerate() {
            let row = indices[slot] as usize;
            let column = indices[values.len() + slot] as usize;
            if row >= hidden || column >= inputs {
                bail!("sparse activation index ({row}, {column}) is out of bounds");
            }
            dense[column * hidden + row] = *value;
        }

        Ok(Some(Self {
            activation: Tensor::from_data(TensorData::new(dense, [inputs, hidden]), device),
            basis: Tensor::from_data(
                TensorData::new(basis.to_f32(), [components, num_vertices * 3]),
                device,
            ),
            posed_joints,
            num_vertices,
        }))
    }

    pub fn num_vertices(&self) -> usize {
        self.num_vertices
    }

    /// Per-vertex corrective offsets `[batch, vertices, 3]`, from joint
    /// parameters shaped `[batch, joints, 7]`.
    pub fn forward(&self, joint_parameters: Tensor<3>) -> Tensor<3> {
        let batch = joint_parameters.dims()[0];
        let features = self.pose_features(joint_parameters);
        let hidden = activation::relu(features.matmul(self.activation.clone()));
        hidden
            .matmul(self.basis.clone())
            .reshape([batch, self.num_vertices, 3])
    }

    /// The 6D rotation feature per posed joint, flattened to `[batch, posed * 6]`.
    ///
    /// A 6D feature is the first two columns of the joint's rotation matrix,
    /// with 1 subtracted from the two diagonal entries so a rest pose is zero.
    fn pose_features(&self, joint_parameters: Tensor<3>) -> Tensor<2> {
        let batch = joint_parameters.dims()[0];
        let euler = joint_parameters
            .narrow(1, SKIPPED_JOINTS, self.posed_joints)
            .narrow(2, 3, 3);

        let (sx, cx) = axis_sin_cos(&euler, 0);
        let (sy, cy) = axis_sin_cos(&euler, 1);
        let (sz, cz) = axis_sin_cos(&euler, 2);

        let feature = Tensor::cat(
            vec![
                (cy.clone() * cz.clone()).sub_scalar(1.0),
                cy.clone() * sz.clone(),
                sy.clone().neg(),
                cx.clone().neg() * sz.clone() + sx.clone() * sy.clone() * cz.clone(),
                (cx * cz + sx.clone() * sy * sz).sub_scalar(1.0),
                sx * cy,
            ],
            2,
        );
        feature.reshape([batch, self.posed_joints * FEATURES_PER_JOINT])
    }
}

fn axis_sin_cos(euler: &Tensor<3>, axis: usize) -> (Tensor<3>, Tensor<3>) {
    let angle = euler.clone().narrow(2, axis, 1);
    (angle.clone().sin(), angle.cos())
}
