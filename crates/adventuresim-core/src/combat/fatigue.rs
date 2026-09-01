#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatActionWork {
    Attack,
    WeaponDefense,
    ExplosiveDodge,
}

/// Physical demand produced by one combat action.
///
/// `metabolic_joules` is whole-body chemical energy spent. `local_fatigue`
/// is a dimensionless fraction of the recruited muscle group's repeat-work
/// capacity; it affects cadence and output before oxygen debt contributes to
/// medical incapacitation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CombatActionWorkload {
    pub metabolic_joules: f32,
    pub local_fatigue: f32,
    pub active_seconds: f32,
}

#[must_use]
pub fn combat_action_workload(
    work: CombatActionWork,
    action_duration_seconds: f32,
    weapon_mass_kg: f32,
    weapon_inertia_kg_m2: f32,
    carried_mass_kg: f32,
    body_mass_kg: f32,
    endurance: f32,
) -> CombatActionWorkload {
    const MUSCLE_MECHANICAL_EFFICIENCY: f32 = 0.25;
    const LOCAL_REPEAT_WORK_JOULES_PER_STRENGTH: f32 = 4_000.0;
    const ATTACK_EFFECTIVE_TRAVEL_METRES: f32 = 3.0;
    const DEFENSE_EFFECTIVE_TRAVEL_METRES: f32 = 1.5;
    const WEAPON_ROTATION_RADIANS: f32 = std::f32::consts::PI;
    const EXPLOSIVE_DODGE_SPEED_METRES_PER_SECOND: f32 = 3.5;

    let duration = action_duration_seconds.clamp(0.1, 0.35);
    let weapon_energy = |travel_metres: f32| {
        let linear_speed = travel_metres / duration;
        let angular_speed = WEAPON_ROTATION_RADIANS / duration;
        0.5 * weapon_mass_kg.max(0.0) * linear_speed * linear_speed
            + 0.5 * weapon_inertia_kg_m2.max(0.0) * angular_speed * angular_speed
    };
    let mechanical_joules = match work {
        CombatActionWork::Attack => weapon_energy(ATTACK_EFFECTIVE_TRAVEL_METRES),
        CombatActionWork::WeaponDefense => weapon_energy(DEFENSE_EFFECTIVE_TRAVEL_METRES),
        CombatActionWork::ExplosiveDodge => {
            let moved_mass = body_mass_kg.max(1.0) + carried_mass_kg.max(0.0);
            0.5 * moved_mass * EXPLOSIVE_DODGE_SPEED_METRES_PER_SECOND.powi(2)
        }
    };
    let metabolic_joules = mechanical_joules / MUSCLE_MECHANICAL_EFFICIENCY;
    let recruitment = match work {
        CombatActionWork::Attack => 1.0,
        CombatActionWork::WeaponDefense => 0.8,
        CombatActionWork::ExplosiveDodge => 0.65,
    };
    CombatActionWorkload {
        metabolic_joules,
        local_fatigue: mechanical_joules * recruitment
            / (LOCAL_REPEAT_WORK_JOULES_PER_STRENGTH * endurance.max(0.5)),
        active_seconds: action_duration_seconds.max(0.0),
    }
}

#[must_use]
pub fn carried_load_metabolic_power_watts(moving: bool, carried_mass_kg: f32) -> f32 {
    if !moving {
        return 0.0;
    }
    const LEVEL_CARRY_METABOLIC_WATTS_PER_KILOGRAM: f32 = 2.0;
    carried_mass_kg.max(0.0) * LEVEL_CARRY_METABOLIC_WATTS_PER_KILOGRAM
}

#[must_use]
pub fn combat_movement_oxygen_debt_watts(
    effort_speed_metres_per_second: f32,
    sustainable_speed_metres_per_second: f32,
    carried_mass_kg: f32,
    endurance: f32,
) -> f32 {
    let sustainable = sustainable_speed_metres_per_second.max(0.1);
    let ratio = (effort_speed_metres_per_second.max(0.0) / sustainable).max(0.0);
    let required = combat_aerobic_power_watts(endurance) * ratio * ratio
        + carried_load_metabolic_power_watts(ratio > 0.0, carried_mass_kg);
    (required - combat_aerobic_power_watts(endurance)).max(0.0)
}

#[must_use]
pub fn combat_aerobic_power_watts(endurance: f32) -> f32 {
    const AEROBIC_POWER_WATTS_PER_ENDURANCE: f32 = 70.0;
    AEROBIC_POWER_WATTS_PER_ENDURANCE * endurance.max(0.5)
}

pub fn apply_combat_workload(
    oxygen_debt_joules: &mut f32,
    local_action_fatigue: &mut f32,
    workload: CombatActionWorkload,
    endurance: f32,
) {
    let aerobic_joules = combat_aerobic_power_watts(endurance) * workload.active_seconds;
    *oxygen_debt_joules += (workload.metabolic_joules - aerobic_joules).max(0.0);
    *local_action_fatigue = (*local_action_fatigue + workload.local_fatigue).clamp(0.0, 1.5);
}

pub fn recover_combat_fatigue(
    oxygen_debt_joules: &mut f32,
    local_action_fatigue: &mut f32,
    rest_seconds: f32,
    endurance: f32,
) {
    const OXYGEN_DEBT_RECOVERY_WATTS_PER_ENDURANCE: f32 = 55.0;
    const LOCAL_FATIGUE_RECOVERY_PER_SECOND_AT_ENDURANCE_THREE: f32 = 0.035;
    let rest = rest_seconds.max(0.0);
    *oxygen_debt_joules = (*oxygen_debt_joules
        - OXYGEN_DEBT_RECOVERY_WATTS_PER_ENDURANCE * endurance.max(0.5) * rest)
        .max(0.0);
    *local_action_fatigue = (*local_action_fatigue
        - LOCAL_FATIGUE_RECOVERY_PER_SECOND_AT_ENDURANCE_THREE * (endurance.max(0.5) / 3.0) * rest)
        .max(0.0);
}

#[must_use]
pub fn oxygen_debt_incapacitation(oxygen_debt_joules: f32, endurance: f32) -> f32 {
    const COLLAPSE_ONSET_JOULES_PER_ENDURANCE: f32 = 30_000.0;
    const COLLAPSE_RANGE_JOULES_PER_ENDURANCE: f32 = 25_000.0;
    let reserve = endurance.max(0.5);
    ((oxygen_debt_joules.max(0.0) - COLLAPSE_ONSET_JOULES_PER_ENDURANCE * reserve)
        / (COLLAPSE_RANGE_JOULES_PER_ENDURANCE * reserve))
        .clamp(0.0, 1.0)
}

#[must_use]
pub fn combat_fatigue_performance(
    oxygen_debt_joules: f32,
    local_action_fatigue: f32,
    endurance: f32,
) -> f32 {
    const WINDED_JOULES_PER_ENDURANCE: f32 = 8_000.0;
    let winded = oxygen_debt_joules.max(0.0) / (WINDED_JOULES_PER_ENDURANCE * endurance.max(0.5));
    (1.0 / (1.0 + 0.8 * local_action_fatigue.max(0.0) + 0.25 * winded)).clamp(0.35, 1.0)
}

#[must_use]
pub fn fatigue_adjusted_recovery_seconds(base_recovery_seconds: f32, performance: f32) -> f32 {
    base_recovery_seconds.max(0.0) / performance.clamp(0.35, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_short_bout_winds_without_systemic_collapse_and_rest_recovers() {
        let light =
            combat_action_workload(CombatActionWork::Attack, 0.75, 1.0, 0.1, 8.0, 75.0, 3.0);
        let heavy =
            combat_action_workload(CombatActionWork::Attack, 0.75, 4.0, 1.0, 25.0, 75.0, 3.0);
        let dodge = combat_action_workload(
            CombatActionWork::ExplosiveDodge,
            0.5,
            0.0,
            0.0,
            25.0,
            75.0,
            3.0,
        );
        let parry = combat_action_workload(
            CombatActionWork::WeaponDefense,
            0.5,
            1.0,
            0.1,
            8.0,
            75.0,
            3.0,
        );
        assert!(heavy.metabolic_joules > light.metabolic_joules);
        assert!(dodge.metabolic_joules > light.metabolic_joules);

        let mut oxygen_debt = 0.0;
        let mut local_fatigue = 0.0;
        for workload in std::iter::repeat_n(light, 15)
            .chain(std::iter::repeat_n(parry, 15))
            .chain(std::iter::once(dodge))
        {
            apply_combat_workload(&mut oxygen_debt, &mut local_fatigue, workload, 3.0);
        }
        assert!(local_fatigue > 0.0 && local_fatigue < 1.0);
        assert!(combat_fatigue_performance(oxygen_debt, local_fatigue, 3.0) < 1.0);
        assert_eq!(oxygen_debt_incapacitation(oxygen_debt, 3.0), 0.0);

        let before_rest = (oxygen_debt, local_fatigue);
        recover_combat_fatigue(&mut oxygen_debt, &mut local_fatigue, 2.0, 3.0);
        assert!(oxygen_debt < before_rest.0);
        assert!(local_fatigue < before_rest.1);
    }

    #[test]
    fn endurance_four_does_not_collapse_after_twenty_two_attacks() {
        let attack =
            combat_action_workload(CombatActionWork::Attack, 0.75, 1.4, 0.18, 16.0, 78.0, 4.0);
        let dodge = combat_action_workload(
            CombatActionWork::ExplosiveDodge,
            0.5,
            0.0,
            0.0,
            16.0,
            78.0,
            4.0,
        );
        let mut oxygen_debt = 0.0;
        let mut local_fatigue = 0.0;
        for workload in std::iter::repeat_n(attack, 22).chain(std::iter::repeat_n(dodge, 2)) {
            apply_combat_workload(&mut oxygen_debt, &mut local_fatigue, workload, 4.0);
        }
        assert_eq!(oxygen_debt_incapacitation(oxygen_debt, 4.0), 0.0);
        assert!(combat_fatigue_performance(oxygen_debt, local_fatigue, 4.0) < 0.9);
    }

    #[test]
    fn fatigue_lengthens_action_recovery_before_incapacitation() {
        let fresh = combat_fatigue_performance(0.0, 0.0, 3.0);
        let fatigued = combat_fatigue_performance(12_000.0, 0.45, 3.0);
        assert!(fatigued < fresh);
        assert!(
            fatigue_adjusted_recovery_seconds(0.45, fatigued)
                > fatigue_adjusted_recovery_seconds(0.45, fresh)
        );
        assert_eq!(oxygen_debt_incapacitation(12_000.0, 3.0), 0.0);
    }
}
