//! Presentation clock shared by procedural owners and deterministic tools.

use bevy::prelude::*;

/// Optional deterministic clock for tools that render the same simulation
/// tick more than once. Gameplay leaves the override unset and advances from
/// Bevy's render delta.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct ProceduralAnimationClock {
    pub(crate) fixed_tick: Option<(u64, f32)>,
}

impl ProceduralAnimationClock {
    pub(crate) fn fixed_step(&self) -> Option<(u64, f32)> {
        self.fixed_tick
    }
}
