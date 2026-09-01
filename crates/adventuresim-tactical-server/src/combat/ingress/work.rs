use super::*;

pub(super) fn fatigue_adjusted_attack_recovery(
    attacker: Entity,
    hand: AttackHand,
    recovery: CombatDuration,
    combat_states: &Query<&mut TacticalCombatState>,
    viewer: &TacticalPlayerViewer<'_, '_>,
) -> CombatDuration {
    let Ok(state) = combat_states.get(attacker) else {
        return recovery;
    };
    let Ok(view) = viewer.get_for_attack(attacker, hand) else {
        return recovery;
    };
    let performance = combat_fatigue_performance(
        state.oxygen_debt_joules,
        state.local_action_fatigue,
        view.raw_single_body_part_attr(SimpleAttribute::Endurance),
    );
    CombatDuration::from_secs_f32(fatigue_adjusted_recovery_seconds(
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
        view.raw_single_body_part_attr(SimpleAttribute::Endurance),
    );
    let state = &mut *state;
    apply_combat_workload(
        &mut state.oxygen_debt_joules,
        &mut state.local_action_fatigue,
        workload,
        view.raw_single_body_part_attr(SimpleAttribute::Endurance),
    );
}
