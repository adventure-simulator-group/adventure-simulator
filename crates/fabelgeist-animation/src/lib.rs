//! Skeletons, keyframed clips, and retargeting between rigs.
//!
//! This is the engine's animation model, and it is deliberately independent of
//! how anything is drawn: a [`Skeleton`](skeleton::Skeleton) is joint names,
//! parents and bind matrices, an [`Animation`](animation::Animation) is
//! keyframed tracks addressed by joint name, and
//! [`build_skinning_matrices`](skeleton::build_skinning_matrices) turns a posed
//! skeleton into the matrices a skinning shader wants.
//!
//! Uploading those matrices to a GPU is somebody else's job — `fabelgeist-gpu` owns
//! the buffer-backed `Pose` built on top of this.

pub mod animation;
pub mod skeleton;

pub use animation::{Animation, JointTransform, LocalPose, model_pose, rest_pose};
pub use skeleton::{Joint, JointInfo, Skeleton, build_skinning_matrices};
