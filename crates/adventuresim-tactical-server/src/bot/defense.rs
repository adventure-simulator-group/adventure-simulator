use super::*;

const PARRY_CHANCE: f64 = 0.2;
const DODGE_CHANCE: f64 = 0.2;
const FRONTAL_FLANKING_MAX: f32 = 0.01;
const REACTION_DELAY_SECS: std::ops::Range<f32> = 0.05..0.6;

#[derive(Component)]
pub(super) struct CountedEnemyDefeat;

#[derive(Component)]
pub(super) struct PendingBotReaction {
    timer: Timer,
    choice: DefendRequest,
}

pub(super) fn on_tactical_combatant_defeated(
    defeated: On<TacticalCombatantDefeated>,
    enemies: Query<(), (With<MissionEnemy>, Without<CountedEnemyDefeat>)>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
) {
    let entity = defeated.0;
    if enemies.get(entity).is_err() {
        return;
    }
    commands.entity(entity).insert(CountedEnemyDefeat);
    state.record_enemy_defeat();
}

/// Predicts whether the nearest opposing AI facing a client attacker notices
/// the untargeted client windup and decides to dodge or parry it.
///
/// A bot has no real reflexes: it only ever gets a chance to react when it is
/// facing its attacker (`flanking <= FRONTAL_FLANKING_MAX`), and even then it
/// correctly reads the attack only some of the time. A decision to react is
/// committed only after a random delay (see [`REACTION_DELAY_SECS`]).
pub(super) fn on_attack_started(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    q_character: Query<(&CharacterLook, &Transform, &TacticalCombatSide)>,
    q_bots: Query<
        (
            Entity,
            &CharacterLook,
            &Transform,
            &TacticalCombatSide,
            &TacticalCombatState,
        ),
        With<OffensiveCombatAi>,
    >,
) {
    if !matches!(**event, MeleeActionRequest::Start) {
        return;
    }
    let Some(attacker) = event.client_id.entity() else {
        return;
    };
    let Ok((attacker_look, attacker_transform, attacker_side)) = q_character.get(attacker) else {
        return;
    };
    let nearest = q_bots
        .iter()
        .filter(|(_, _, _, side, state)| **side != *attacker_side && !state.is_incapacitated())
        .min_by(|(a, _, a_transform, _, _), (b, _, b_transform, _, _)| {
            compare_target(attacker_transform, a_transform, *a, b_transform, *b)
        });
    let Some((bot, bot_look, _, _, _)) = nearest else {
        return;
    };
    try_start_reaction(&mut cmd, bot, attacker_look, bot_look);
}

pub(super) fn on_targeted_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<(&CharacterLook, &TacticalCombatState), With<OffensiveCombatAi>>,
) {
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, event.target])
    else {
        return;
    };
    if q_ai
        .get(event.target)
        .is_ok_and(|(_, state)| !state.is_incapacitated())
    {
        try_start_reaction(&mut cmd, event.target, attacker_look, defender_look);
    }
}

pub(super) fn on_targeted_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<(&CharacterLook, &TacticalCombatState), With<OffensiveCombatAi>>,
) {
    let Some(target) = event.target else {
        return;
    };
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, target]) else {
        return;
    };
    if q_ai
        .get(target)
        .is_ok_and(|(_, state)| !state.is_incapacitated())
    {
        try_start_reaction(&mut cmd, target, attacker_look, defender_look);
    }
}

pub(super) fn try_start_reaction(
    cmd: &mut Commands,
    defender: Entity,
    attacker_look: &CharacterLook,
    defender_look: &CharacterLook,
) {
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    if flanking_from_dir((a1, a2), (d1, d2)) > FRONTAL_FLANKING_MAX {
        return;
    }
    let Some(choice) = roll_defend_choice() else {
        return;
    };
    cmd.entity(defender).insert(PendingBotReaction {
        timer: Timer::from_seconds(rand::random_range(REACTION_DELAY_SECS), TimerMode::Once),
        choice,
    });
}

pub(super) fn roll_defend_choice() -> Option<DefendRequest> {
    let roll: f64 = rand::random();
    if roll < PARRY_CHANCE {
        Some(DefendRequest::Parry)
    } else if roll < PARRY_CHANCE + DODGE_CHANCE {
        Some(DefendRequest::Dodge)
    } else {
        None
    }
}

pub(super) fn tick_bot_reactions(
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut q_reacting: Query<(Entity, &mut PendingBotReaction, &TacticalCombatState)>,
) {
    for (bot, mut reaction, state) in &mut q_reacting {
        if state.is_incapacitated() {
            cmd.entity(bot).remove::<PendingBotReaction>();
            continue;
        }
        reaction.timer.tick(time.delta());
        if !reaction.timer.is_finished() {
            continue;
        }

        cmd.entity(bot)
            .remove::<PendingBotReaction>()
            .insert(PendingDefenderResponse {
                choice: reaction.choice,
                set_at: CombatInstant::from_elapsed(&time),
            });
    }
}
