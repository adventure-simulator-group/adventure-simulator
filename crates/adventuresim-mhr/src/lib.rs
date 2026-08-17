//! Native Burn implementation of MHR, the Momentum Human Rig.
//!
//! MHR is a parametric 3D body model with 45 identity coefficients, 204 pose
//! and scale parameters, 72 facial expression coefficients, and a non-linear
//! pose-corrective network, published across seven levels of detail.
//!
//! Everything is loaded straight from the released assets — the binary FBX
//! rig, the `.model` parameter transform, and the `.npz` correctives.
//! The forward pass is Burn tensors end to end.

pub use adventuresim_fbx as fbx;
pub mod character;
pub mod math;
pub mod model;
pub mod model_def;
pub mod pose_correctives;
pub mod skel_state;
pub mod storage;

pub use character::{BlendShapes, Character, Mesh, Skeleton, SkinWeights};
pub use model::{
    Mhr, MhrConfig, MhrOutput, MAX_LOD, NUM_BLEND_SHAPES, NUM_FACE_EXPRESSION_BLEND_SHAPES,
    NUM_IDENTITY_BLEND_SHAPES,
};
pub use model_def::{parse_model_definition, ParameterTransform};
pub use pose_correctives::PoseCorrectives;
