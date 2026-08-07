use super::*;

pub(crate) fn update_tactical_combat_state(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    limbs: Query<&Limbs>,
    mut states: Query<(
        Entity,
        &mut TacticalCombatState,
        Option<&mut CombatState>,
        Option<&mut input::AccumulatedInput>,
    )>,
) {
    for (entity, mut state, legacy_state, mut input) in &mut states {
        let was_incapacitated = state.is_incapacitated();
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let balance = view.skill_check(Skill::Balance, LimbWeights::both_legs());
        state.imbalance = recover_combat_imbalance(state.imbalance, balance, time.delta_secs());
        let Ok(limbs) = limbs.get(entity) else {
            continue;
        };
        let will = view.skill_check(Skill::Will, LimbWeights::all_equal());
        state.incapacitation = combat_incapacitation(
            state.starting_incapacitation,
            state.starting_blood_fraction,
            state.blood_loss_fraction,
            limbs.total_damage(),
            will,
            state.imbalance,
        );

        // `CombatState` mirrors this system's authoritative
        // `TacticalCombatState` output, split into the separate pain/blood/
        // imbalance terms the tactical client's incapacitation HUD renders.
        if let Some(mut legacy_state) = legacy_state {
            legacy_state.imbalance = state.imbalance;
            legacy_state.blood_loss_fraction = state.blood_loss_fraction;
            legacy_state.recompute(limbs.total_damage(), will);
        }
        if state.is_incapacitated() {
            if let Some(input) = input.as_deref_mut() {
                input.last_movement = None;
                input.jumped = None;
            }
            if !was_incapacitated {
                cmd.entity(entity).remove::<PendingDefenderResponse>();
                cmd.trigger(TacticalCombatantDefeated(entity));
            }
        }
    }
}
