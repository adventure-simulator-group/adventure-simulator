//! Generic skeletal animation retargeting.
//!
//! Three concepts, deliberately kept apart:
//!
//! * a **source** skeleton and clip, in engine-native form, whatever produced
//!   them;
//! * a [`RetargetProfile`], which is data describing how two rigs correspond;
//! * the [`Retargeter`], which knows only about resolved joint indices.
//!
//! ```no_run
//! # use fabelgeist_animation::animation::{Animation, retarget};
//! # use fabelgeist_animation::skeleton::Skeleton;
//! # fn example(source: &Skeleton, clip: &Animation, target: &Skeleton) -> anyhow::Result<()> {
//! let profile = retarget::RetargetProfile::new(
//!     fabelgeist_animation::skeleton::mixamo::MixamoRig::profile(),
//!     retarget::RigProfile::new("my rig"),
//! );
//! let retargeted = retarget::retarget(source, clip, target, &profile)?;
//! # let _ = retargeted;
//! # Ok(())
//! # }
//! ```
//!
//! Adding a new animation source means importing it into an engine
//! [`Animation`](crate::animation::Animation) and writing a
//! profile. It does not mean touching anything in this module.

pub mod profile;
pub mod profiles;
pub mod resolve;
pub mod retargeter;
pub mod semantic;

pub use profile::{
    Axis, ChainBinding, JointBinding, RetargetProfile, RetargetSettings, RigProfile,
    RootMotionChannels, RootMotionPolicy, RootSource, ScaleMeasure, ScalePolicy, TranslationPolicy,
};
pub use profiles::detect;
pub use resolve::{ResolvedProfile, ResolvedRig};
pub use retargeter::{Retargeter, retarget};
pub use semantic::{HumanoidChain, HumanoidJoint};

#[cfg(test)]
mod tests;
