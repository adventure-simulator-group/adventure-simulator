//! Native Burn implementation of MHR, the Momentum Human Rig.
//!
//! MHR is a parametric 3D body model with 45 identity coefficients, 204 pose
//! and scale parameters, 72 facial expression coefficients, and a non-linear
//! pose-corrective network, published across seven levels of detail.
//!
//! Everything is loaded straight from the released assets — the binary FBX
//! rig, the `.model` parameter transform, and the `.npz` correctives — with no
//! Python, no FBX SDK, and no momentum runtime. The forward pass is Burn
//! tensors end to end.
//!
//! ```no_run
//! # fn main() -> anyhow::Result<()> {
//! use burn::tensor::{Device, Tensor};
//! use fabelgeist_mhr::{Mhr, MhrConfig, NUM_IDENTITY_BLEND_SHAPES};
//!
//! let device = Device::default();
//! let model = Mhr::from_files("D:/AI/Models/mhr", MhrConfig::default(), &device)?;
//!
//! let identity = Tensor::zeros([1, NUM_IDENTITY_BLEND_SHAPES], &device);
//! let pose = model.zero_parameters(1);
//! let output = model.forward(identity, pose, None)?;
//! # let _ = output;
//! # Ok(())
//! # }
//! ```
//!
//! Reference implementation: <https://github.com/facebookresearch/MHR>.

pub mod character;
pub mod correctives;
pub mod math;
pub mod model;
pub mod model_def;
pub mod skel_state;

pub use character::{BlendShapes, Character, Mesh, Skeleton, SkinWeights};
pub use correctives::PoseCorrectives;
pub use model::{
    MAX_LOD, Mhr, MhrConfig, MhrOutput, NUM_BLEND_SHAPES, NUM_FACE_EXPRESSION_BLEND_SHAPES,
    NUM_IDENTITY_BLEND_SHAPES,
};
pub use model_def::{ParameterTransform, parse_model_definition};
