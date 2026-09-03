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
    pub starting_hunger: f32,
    pub starting_thirst: f32,
    pub starting_thermal: f32,
    pub blood_loss_fraction: f32,
    pub acute_trauma: f32,
    /// General fatigue fraction represented by the black wheel segment.
    pub fatigue: f32,
    pub imbalance: f32,
    pub incapacitation: f32,
    /// Burden contribution, shown as translucent grey.
    pub encumbrance: f32,
    /// Presentation-only increase over the configured forecast horizon.
    pub projected_increase: TacticalIncapacitationSources,
}

impl Default for TacticalCombatState {
    fn default() -> Self {
        Self {
            starting_incapacitation: 0.0,
            starting_blood_fraction: 1.0,
            starting_fear: 0.0,
            starting_hunger: 0.0,
            starting_thirst: 0.0,
            starting_thermal: 0.0,
            blood_loss_fraction: 0.0,
            acute_trauma: 0.0,
            fatigue: 0.0,
            imbalance: 0.0,
            incapacitation: 0.0,
            encumbrance: 0.0,
            projected_increase: TacticalIncapacitationSources::default(),
        }
    }
}

impl TacticalCombatState {
    /// Charge work and update readiness atomically, including between server ticks.
    pub fn charge_work(
        &mut self,
        workload: CombatActionWorkload,
        endurance: f32,
        parameters: CombatFatigueParameters,
    ) {
        let before = self.fatigue;
        apply_combat_workload(&mut self.fatigue, workload, endurance, parameters);
        self.incapacitation = (self.incapacitation + self.fatigue - before).max(0.0);
    }

    /// Returns the source values represented by the tactical incapacitation
    /// wheel. Pain, blood loss, and fatigue are recomputed live in combat; other
    /// strategic sources retain their enrollment-time breakdown.
    pub fn incapacitation_sources(
        &self,
        total_limb_damage: f32,
        will_check: f32,
    ) -> TacticalIncapacitationSources {
        let remaining_blood =
            (self.starting_blood_fraction - self.blood_loss_fraction).clamp(0.0, 1.0);
        TacticalIncapacitationSources {
            pain: pain_incapacitation(total_limb_damage, will_check),
            acute_trauma: self.acute_trauma.max(0.0),
            blood_loss: blood_loss_incapacitation(remaining_blood, 1.0),
            fear: self.starting_fear.max(0.0),
            fatigue: self.fatigue.clamp(0.0, 1.0),
            hunger: self.starting_hunger.max(0.0),
            thirst: self.starting_thirst.max(0.0),
            thermal: self.starting_thermal.max(0.0),
            imbalance: self.imbalance.max(0.0),
            encumbrance: self.encumbrance.max(0.0),
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct TacticalIncapacitationSources {
    pub pain: f32,
    pub acute_trauma: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub thermal: f32,
    pub imbalance: f32,
    pub encumbrance: f32,
}

impl TacticalIncapacitationSources {
    pub fn total(self) -> f32 {
        self.pain
            + self.acute_trauma
            + self.blood_loss
            + self.fear
            + self.fatigue
            + self.hunger
            + self.thirst
            + self.thermal
            + self.imbalance
            + self.encumbrance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_is_visible_and_authoritative_before_the_next_tick() {
        let mut state = TacticalCombatState {
            fatigue: 0.2,
            acute_trauma: 0.1,
            incapacitation: 0.3,
            ..default()
        };
        let parameters = EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.fatigue;
        let work = combat_action_workload(
            CombatActionWork::Attack,
            0.75,
            1.4,
            0.18,
            16.0,
            78.0,
            parameters,
        );
        state.charge_work(work, 3.0, parameters);
        let sources = state.incapacitation_sources(0.0, 3.0);
        assert!(sources.fatigue > 0.2);
        assert!((sources.total() - state.incapacitation).abs() < f32::EPSILON);
        assert_eq!(
            combat_incapacitation_performance(state.incapacitation),
            1.0 - sources.total()
        );
        state.charge_work(
            CombatActionWorkload {
                metabolic_joules: f32::MAX,
            },
            3.0,
            parameters,
        );
        assert!(state.is_incapacitated());
        assert_eq!(state.incapacitation_sources(0.0, 3.0).fatigue, 1.0);
    }

    #[test]
    fn tactical_skill_checks_do_not_double_count_calorie_history() {
        let fresh = crate::player::Stats::default();
        let spent = crate::player::Stats {
            calories_used: 10000.0,
            ..default()
        };
        let attributes = PlayerAttributeValues {
            endurance: 3.0,
            left_arm_strength: 3.0,
            right_arm_strength: 3.0,
            left_leg_strength: 3.0,
            right_leg_strength: 3.0,
            left_arm_agility: 3.0,
            right_arm_agility: 3.0,
            ..default()
        };
        let body = CombatBody::default();
        let skills = CombatSkills {
            sword_hours: 12000.0,
            ..default()
        };
        let equipment = CombatEquipment::default();
        let check = |stats: &crate::player::Stats| {
            skills.skill_check_by_parts(
                Skill::Sword,
                &attributes,
                &body,
                stats,
                &equipment,
                LimbWeights::all_equal(),
            )
        };
        assert!(check(&fresh) > 0.0);
        assert_eq!(check(&fresh), check(&spent));
    }
}
