use super::CombatFatigueParameters;

impl CombatFatigueParameters {
    fn capacity(self, endurance: f32) -> f32 {
        self.capacity_joules_per_endurance * endurance.max(self.minimum_endurance)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatActionWork {
    Attack,
    WeaponDefense,
    ExplosiveDodge,
}

/// Energy charged once when an action begins; no hidden muscle-fatigue state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CombatActionWorkload {
    pub metabolic_joules: f32,
}

#[must_use]
pub fn combat_action_workload(
    work: CombatActionWork,
    action_duration_seconds: f32,
    weapon_mass_kg: f32,
    weapon_inertia_kg_m2: f32,
    carried_mass_kg: f32,
    body_mass_kg: f32,
    parameters: CombatFatigueParameters,
) -> CombatActionWorkload {
    let duration = action_duration_seconds.clamp(
        parameters.minimum_action_seconds,
        parameters.maximum_action_seconds,
    );
    let weapon_energy = |travel_metres: f32| {
        let linear_speed = travel_metres / duration;
        let angular_speed = std::f32::consts::PI / duration;
        0.5 * weapon_mass_kg.max(0.0) * linear_speed * linear_speed
            + 0.5 * weapon_inertia_kg_m2.max(0.0) * angular_speed * angular_speed
    };
    let mechanical_joules = match work {
        CombatActionWork::Attack => weapon_energy(parameters.attack_travel_metres),
        CombatActionWork::WeaponDefense => weapon_energy(parameters.defense_travel_metres),
        CombatActionWork::ExplosiveDodge => {
            let moved_mass = body_mass_kg.max(0.0) + carried_mass_kg.max(0.0);
            0.5 * moved_mass * parameters.dodge_speed_metres_per_second.powi(2)
        }
    };
    CombatActionWorkload {
        metabolic_joules: mechanical_joules / parameters.mechanical_efficiency,
    }
}

/// Unsustainable movement adds to the same fatigue as weapon work.
/// Sustainable unloaded jogging holds fatigue steady; stationary rest recovers.
#[must_use]
pub fn combat_movement_fatigue_per_second(
    effort_speed_metres_per_second: f32,
    sustainable_speed_metres_per_second: f32,
    carried_mass_kg: f32,
    endurance: f32,
    parameters: CombatFatigueParameters,
) -> f32 {
    let sustainable = sustainable_speed_metres_per_second.max(f32::EPSILON);
    let ratio = effort_speed_metres_per_second.max(0.0) / sustainable;
    let aerobic =
        parameters.aerobic_watts_per_endurance * endurance.max(parameters.minimum_endurance);
    let carry = if ratio > 0.0 {
        carried_mass_kg.max(0.0) * parameters.carry_watts_per_kg
    } else {
        0.0
    };
    (aerobic * ratio * ratio + carry - aerobic).max(0.0) / parameters.capacity(endurance)
}

pub fn apply_combat_workload(
    fatigue: &mut f32,
    workload: CombatActionWorkload,
    endurance: f32,
    parameters: CombatFatigueParameters,
) {
    *fatigue =
        (*fatigue + workload.metabolic_joules / parameters.capacity(endurance)).clamp(0.0, 1.0);
}

pub fn recover_combat_fatigue(
    fatigue: &mut f32,
    rest_seconds: f32,
    endurance: f32,
    parameters: CombatFatigueParameters,
) {
    let recovered = parameters.recovery_watts_per_endurance
        * endurance.max(parameters.minimum_endurance)
        * rest_seconds.max(0.0)
        / parameters.capacity(endurance);
    *fatigue = (*fatigue - recovered).clamp(0.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{
        EMBEDDED_COMBAT_RESOLUTION_PARAMETERS, combat_incapacitation_performance,
        incapacitation_adjusted_recovery_seconds,
    };
    const PARAMETERS: CombatFatigueParameters = EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.fatigue;

    #[test]
    fn authored_capacity_controls_the_only_fatigue_pool() {
        let mut larger_capacity = PARAMETERS;
        larger_capacity.capacity_joules_per_endurance *= 2.0;
        let work = CombatActionWorkload {
            metabolic_joules: 1000.0,
        };
        let (mut ordinary, mut larger) = (0.0, 0.0);
        apply_combat_workload(&mut ordinary, work, 3.0, PARAMETERS);
        apply_combat_workload(&mut larger, work, 3.0, larger_capacity);
        assert_eq!(ordinary, larger * 2.0);
        larger_capacity.capacity_joules_per_endurance = 0.0;
        assert!(larger_capacity.validate().is_err());
        larger_capacity = PARAMETERS;
        larger_capacity.mechanical_efficiency = f32::NAN;
        assert!(larger_capacity.validate().is_err());
    }

    #[test]
    fn every_action_adds_visible_fatigue_and_rest_restores_performance() {
        for work in [
            CombatActionWork::Attack,
            CombatActionWork::WeaponDefense,
            CombatActionWork::ExplosiveDodge,
        ] {
            let workload = combat_action_workload(work, 0.75, 1.4, 0.18, 16.0, 78.0, PARAMETERS);
            let mut fatigue = 0.2;
            apply_combat_workload(&mut fatigue, workload, 3.0, PARAMETERS);
            assert!(fatigue > 0.2);
            assert_eq!(combat_incapacitation_performance(fatigue), 1.0 - fatigue);
            let before_rest = fatigue;
            recover_combat_fatigue(&mut fatigue, 2.0, 3.0, PARAMETERS);
            assert!(fatigue < before_rest);
            assert!(
                incapacitation_adjusted_recovery_seconds(
                    0.45,
                    combat_incapacitation_performance(before_rest)
                ) > incapacitation_adjusted_recovery_seconds(
                    0.45,
                    combat_incapacitation_performance(fatigue)
                )
            );
        }
    }

    #[test]
    fn heavy_weapons_cost_more_and_endurance_reduces_visible_cost() {
        let light = combat_action_workload(
            CombatActionWork::Attack,
            0.75,
            1.0,
            0.1,
            8.0,
            75.0,
            PARAMETERS,
        );
        let heavy = combat_action_workload(
            CombatActionWork::Attack,
            0.75,
            4.0,
            1.0,
            25.0,
            75.0,
            PARAMETERS,
        );
        assert!(heavy.metabolic_joules > light.metabolic_joules);
        let (mut ordinary, mut fit) = (0.0, 0.0);
        for _ in 0..22 {
            apply_combat_workload(&mut ordinary, light, 3.0, PARAMETERS);
            apply_combat_workload(&mut fit, light, 4.0, PARAMETERS);
        }
        assert!(fit > 0.0 && fit < ordinary && ordinary < 1.0);
    }

    #[test]
    fn fatigue_is_bounded_and_has_no_invisible_onset_threshold() {
        let mut fatigue = 0.0;
        apply_combat_workload(
            &mut fatigue,
            CombatActionWorkload {
                metabolic_joules: 1.0,
            },
            3.0,
            PARAMETERS,
        );
        assert!(fatigue > 0.0 && combat_incapacitation_performance(fatigue) < 1.0);
        apply_combat_workload(
            &mut fatigue,
            CombatActionWorkload {
                metabolic_joules: f32::MAX,
            },
            3.0,
            PARAMETERS,
        );
        assert_eq!(fatigue, 1.0);
        assert_eq!(combat_incapacitation_performance(fatigue), 0.0);
        recover_combat_fatigue(&mut fatigue, 10000.0, 3.0, PARAMETERS);
        assert_eq!(fatigue, 0.0);
    }
}
