//! Canonical authored and runtime combat styles.

use serde::{Deserialize, Serialize};

/// The two mechanically distinct melee paths exposed by direct controls.
/// `Swing` covers cuts, chops, and swung impact/pick attacks; `Stab` covers
/// punches and point-first thrusts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    all(feature = "spacetimedb", runtime_catalog),
    derive(spacetimedb::SpacetimeType)
)]
#[serde(rename_all = "snake_case")]
pub enum MeleeAttackStyle {
    #[default]
    Swing,
    Stab,
}
