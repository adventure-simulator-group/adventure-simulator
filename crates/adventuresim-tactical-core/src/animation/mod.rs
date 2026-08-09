//! Semantic animation state shared by the tactical authority and presentation client.
//!
//! The server synchronizes intent and timing through [`SkeletonState`]. Authored
//! clips and evaluated bone transforms remain client-only presentation.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    str::FromStr,
};

use adventuresim_core::equipment::MeleeAttackStyle;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

mod evaluation;
mod locomotion;
mod packs;
mod semantic;
mod state;

pub use evaluation::*;
pub use locomotion::*;
pub use packs::*;
pub use semantic::*;
pub use state::*;

#[cfg(test)]
use {evaluation::gait_pair, state::initial_guard_swing_foot};

include!("tests.rs");
