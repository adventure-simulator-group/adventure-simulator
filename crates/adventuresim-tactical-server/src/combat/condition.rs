use super::*;

type CombatStateQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static mut TacticalCombatState,
        Option<&'static mut input::AccumulatedInput>,
        Option<&'static mut AuthoritativeMovementIntent>,
        Option<&'static MovementPace>,
        Option<&'static SkeletonState>,
    ),
>;

pub(crate) fn update_tactical_combat_state(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    limbs: Query<&Limbs>,
    mut states: CombatStateQuery<'_, '_>,
) {
    for (entity, mut state, mut input, mut movement_intent, pace, skeleton) in &mut states {
        let was_incapacitated = state.is_incapacitated();
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let endurance = view.raw_single_body_part_attr(SimpleAttribute::Endurance);
        let burden = view.body_weight() + view.inventory_weight();
        let sprint_speed = tactical_sprint_speed(
            view.raw_limb_attr(LimbAttribute::Strength, BodyPart::LeftLeg),
            view.raw_limb_attr(LimbAttribute::Strength, BodyPart::RightLeg),
            view.body_part_health(BodyPart::LeftLeg),
            view.body_part_health(BodyPart::RightLeg),
            burden,
        );
        let movement = movement_intent.as_deref().and_then(|intent| intent.0);
        let movement_exhaustion_change = tactical_movement_exhaustion_change_per_second(
            movement,
            pace.copied().unwrap_or_default(),
            skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard),
            skeleton.map_or(BodyState::default(), SkeletonState::body),
            endurance,
            sprint_speed,
        );
        state.exhaustion =
            (state.exhaustion + movement_exhaustion_change * time.delta_secs()).max(0.0);
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
            if let Some(movement_intent) = movement_intent.as_deref_mut() {
                movement_intent.0 = None;
            }
            if !was_incapacitated {
                cmd.entity(entity).remove::<PendingDefenderResponse>();
                cmd.trigger(TacticalCombatantDefeated(entity));
            }
        }
    }
}
