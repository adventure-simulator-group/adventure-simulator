use super::*;

pub(super) fn incapacitation_adjusted_attack_recovery(
    attacker: Entity,
    recovery: CombatDuration,
    combat_states: &Query<&mut TacticalCombatState>,
) -> CombatDuration {
    let Ok(state) = combat_states.get(attacker) else {
        return recovery;
    };
    let performance = combat_incapacitation_performance(state.incapacitation);
    CombatDuration::from_secs_f32(incapacitation_adjusted_recovery_seconds(
        recovery.as_secs_f32(),
        performance,
    ))
}

pub(super) fn charge_started_attack_work(
    attacker: Entity,
    hand: AttackHand,
    contact_windup: CombatDuration,
    recovery: CombatDuration,
    viewer: &TacticalPlayerViewer<'_, '_>,
    combat_states: &mut Query<&mut TacticalCombatState>,
    config: &TacticalCombatConfig,
) {
    let Ok(view) = viewer.get_for_attack(attacker, hand) else {
        return;
    };
    let Ok(mut state) = combat_states.get_mut(attacker) else {
        return;
    };
    let workload = combat_action_workload(
        CombatActionWork::Attack,
        contact_windup.as_secs_f32() + recovery.as_secs_f32(),
        view.weapon_weight(),
        view.weapon_moment_of_inertia(),
        view.inventory_weight(),
        view.body_weight(),
        config.resolution.fatigue,
    );
    let state = &mut *state;
    state.charge_work(
        workload,
        view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        config.resolution.fatigue,
    );
}
