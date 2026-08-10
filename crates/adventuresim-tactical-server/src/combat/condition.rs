use super::*;

pub(crate) fn update_tactical_combat_state(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    limbs: Query<&Limbs>,
    mut states: Query<(
        Entity,
        &mut TacticalCombatState,
        Option<&mut input::AccumulatedInput>,
        Option<&LinearVelocity>,
    )>,
) {
    for (entity, mut state, mut input, velocity) in &mut states {
        let was_incapacitated = state.is_incapacitated();
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let endurance = view.raw_single_body_part_attr(SimpleAttribute::Endurance);
        let planar_speed =
            velocity.map_or(0.0, |velocity| Vec2::new(velocity.x, velocity.z).length());
        state.exhaustion = (state.exhaustion
            + tactical_exhaustion_change_per_second(planar_speed, endurance) * time.delta_secs())
        .max(0.0);
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
        ) + state.exhaustion;
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
