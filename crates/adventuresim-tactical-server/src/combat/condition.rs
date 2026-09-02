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
        Option<&'static TacticalWounds>,
    ),
>;

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects the authoritative configuration and independent combat queries"
)]
pub(crate) fn update_tactical_combat_state(
    mut cmd: Commands,
    time: Res<Time<()>>,
    config: Res<TacticalCombatConfig>,
    viewer: TacticalPlayerViewer,
    limbs: Query<&Limbs>,
    metadata: Query<(&TacticalCombatSide, &CharacterId)>,
    mut consequences: Option<ResMut<TacticalConsequenceAccumulator>>,
    mut states: CombatStateQuery<'_, '_>,
) {
    for (entity, mut state, mut input, mut movement_intent, pace, skeleton, wounds) in &mut states {
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        update_fatigue(
            &view,
            &mut state,
            movement_intent.as_deref().and_then(|intent| intent.0),
            pace.copied().unwrap_or_default(),
            skeleton,
            time.delta_secs(),
            config.resolution.fatigue,
        );
        state.blood_loss_fraction = advance_combat_bleeding(
            state.blood_loss_fraction,
            wounds.map_or(&[], |wounds| wounds.0.as_slice()),
            time.delta_secs(),
        );
        if let Some(consequences) = consequences.as_deref_mut()
            && let Ok((TacticalCombatSide::Party, character_id)) = metadata.get(entity)
        {
            consequences
                .party
                .entry(*character_id)
                .or_default()
                .blood_loss_fraction = state.blood_loss_fraction;
        }
        state.imbalance = recover_combat_imbalance(state.imbalance, time.delta_secs());
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
        ) + state.acute_trauma
            + state.fatigue;
        if state.is_incapacitated() {
            if let Some(input) = input.as_deref_mut() {
                input.last_movement = None;
                input.jumped = None;
            }
            if let Some(movement_intent) = movement_intent.as_deref_mut() {
                movement_intent.0 = None;
            }
            cmd.entity(entity).remove::<PendingDefenderResponse>();
        }
    }
}

fn update_fatigue(
    view: &TacticalPlayerView<'_, '_, '_>,
    state: &mut TacticalCombatState,
    movement: Option<Vec2>,
    pace: MovementPace,
    skeleton: Option<&SkeletonState>,
    elapsed_seconds: f32,
    parameters: CombatFatigueParameters,
) {
    let endurance = view.raw_single_body_part_attr(SimpleAttribute::Endurance);
    let active_action = skeleton.is_some_and(|skeleton| {
        matches!(
            skeleton.action_kind(),
            SkeletonAction::Attack | SkeletonAction::Dodge | SkeletonAction::Block
        )
    });
    let moving = movement.is_some_and(|movement| movement.length_squared() > f32::EPSILON);
    let burden = view.body_weight() + view.inventory_weight();
    let sprint_speed = tactical_sprint_speed(
        view.raw_limb_attr(LimbAttribute::Strength, BodyPart::LeftLeg),
        view.raw_limb_attr(LimbAttribute::Strength, BodyPart::RightLeg),
        view.body_part_health(BodyPart::LeftLeg),
        view.body_part_health(BodyPart::RightLeg),
        burden,
    );
    let jog_speed = tactical_jog_speed(endurance);
    let effort_speed = tactical_movement_speed_for_pace(
        movement,
        pace,
        skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard),
        jog_speed,
        sprint_speed,
    );
    state.fatigue = (state.fatigue
        + combat_movement_fatigue_per_second(
            effort_speed,
            jog_speed,
            view.inventory_weight(),
            endurance,
            parameters,
        ) * elapsed_seconds)
        .clamp(0.0, 1.0);
    if !active_action && !moving {
        let state = &mut *state;
        recover_combat_fatigue(&mut state.fatigue, elapsed_seconds, endurance, parameters);
    }
}
