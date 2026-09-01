use adventuresim_core::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Live, server-authoritative combat effects. This component is replicated for
/// presentation but remains transient and is never written to SpacetimeDB.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
pub struct TacticalCombatState {
    pub starting_incapacitation: f32,
    pub starting_blood_fraction: f32,
    pub starting_fear: f32,
    pub starting_fatigue: f32,
    pub starting_hunger: f32,
    pub starting_thirst: f32,
    pub starting_thermal: f32,
    pub blood_loss_fraction: f32,
    pub acute_trauma: f32,
    /// Recoverable whole-body oxygen deficit accumulated during burst work.
    pub oxygen_debt_joules: f32,
    /// Fatigue in the muscle groups repeatedly recruited for attacks and
    /// defenses. This degrades cadence and output but is not incapacitation.
    pub local_action_fatigue: f32,
    pub imbalance: f32,
    pub incapacitation: f32,
}

impl Default for TacticalCombatState {
    fn default() -> Self {
        Self {
            starting_incapacitation: 0.0,
            starting_blood_fraction: 1.0,
            starting_fear: 0.0,
            starting_fatigue: 0.0,
            starting_hunger: 0.0,
            starting_thirst: 0.0,
            starting_thermal: 0.0,
            blood_loss_fraction: 0.0,
            acute_trauma: 0.0,
            oxygen_debt_joules: 0.0,
            local_action_fatigue: 0.0,
            imbalance: 0.0,
            incapacitation: 0.0,
        }
    }
}

impl TacticalCombatState {
    /// Returns the source values represented by the tactical incapacitation
    /// wheel. Pain and blood loss are recomputed live in combat; the remaining
    /// strategic sources retain their enrollment-time breakdown.
    pub fn incapacitation_sources(
        &self,
        total_limb_damage: f32,
        will_check: f32,
        endurance: f32,
    ) -> TacticalIncapacitationSources {
        let remaining_blood =
            (self.starting_blood_fraction - self.blood_loss_fraction).clamp(0.0, 1.0);
        TacticalIncapacitationSources {
            pain: pain_incapacitation(total_limb_damage, will_check),
            blood_loss: blood_loss_incapacitation(remaining_blood, 1.0),
            fear: self.starting_fear.max(0.0),
            fatigue: self.starting_fatigue.max(0.0),
            hunger: self.starting_hunger.max(0.0),
            thirst: self.starting_thirst.max(0.0),
            thermal: self.starting_thermal.max(0.0),
            oxygen_debt: oxygen_debt_incapacitation(self.oxygen_debt_joules, endurance),
            imbalance: self.imbalance.max(0.0),
        }
    }

    /// Derives readiness from the one replicated incapacitation value.
    ///
    /// Readiness is intentionally not stored separately: clients, authority
    /// checks, AI, and mission resolution therefore cannot observe divergent
    /// boolean/component copies of the same state.
    pub fn incapacitation_status(&self) -> IncapacitationStatus {
        match self.incapacitation {
            total if total >= 1.0 => IncapacitationStatus::Incapacitated,
            total if total > 0.5 => IncapacitationStatus::Staggered,
            _ => IncapacitationStatus::Ready,
        }
    }

    pub fn is_incapacitated(&self) -> bool {
        self.incapacitation_status() == IncapacitationStatus::Incapacitated
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TacticalIncapacitationSources {
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub thermal: f32,
    pub oxygen_debt: f32,
    pub imbalance: f32,
}

impl TacticalIncapacitationSources {
    pub fn total(self) -> f32 {
        self.pain
            + self.blood_loss
            + self.fear
            + self.fatigue
            + self.hunger
            + self.thirst
            + self.thermal
            + self.oxygen_debt
            + self.imbalance
    }
}
