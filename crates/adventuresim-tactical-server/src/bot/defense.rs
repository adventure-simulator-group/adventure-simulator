use super::*;

type DefensiveBotQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static CharacterLook,
        &'static Transform,
        &'static TacticalCombatSide,
        &'static TacticalCombatState,
        Option<&'static ReactiveDefenseAi>,
        Option<&'static DefenseChances>,
    ),
    With<MissionEnemy>,
>;

/// Per-bot chance (out of 1.0) that a reflex defense (see
/// `try_start_reaction`) resolves to a dodge. Inserted by a reactive
/// defense behavior package with balance-tuned defaults; BRP tests
/// mutate it to force deterministic dodge outcomes (see
/// `DefenseChances` in `scripts/adventuresim_brp_lib.py`, regenerated via
/// `just generate-brp-types`).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct DefenseChances {
    pub dodge_chance: f64,
}

impl Default for DefenseChances {
    fn default() -> Self {
        Self { dodge_chance: 0.2 }
    }
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
pub struct ReactiveDefenseAi {
    pub requires_facing: bool,
}

impl Default for ReactiveDefenseAi {
    fn default() -> Self {
        Self {
            requires_facing: true,
        }
    }
}

#[derive(Component)]
pub(super) struct PendingBotReaction {
    timer: Timer,
    choice: DefendRequest,
}

struct BotReactionAttempt<'a> {
    defender: Entity,
    attacker_look: &'a CharacterLook,
    defender_look: &'a CharacterLook,
    defense: ReactiveDefenseAi,
    chances: DefenseChances,
    windup_seconds: f32,
}

/// Predicts whether the nearest opposing AI facing a client attacker notices
/// the untargeted client windup and decides to dodge it.
///
/// A bot has no real reflexes: its package controls whether facing is required
/// and how often it reads the attack correctly. A decision to react is
/// committed only after a random delay.
pub(super) fn on_attack_started(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    q_character: Query<(&CharacterLook, &Transform, &TacticalCombatSide)>,
    viewer: TacticalPlayerViewer,
    q_bots: DefensiveBotQuery<'_, '_>,
    combat_config: Res<TacticalCombatConfig>,
) {
    let MeleeActionRequest {
        strike_family,
        hand,
        target,
    } = **event;
    if target.is_some() {
        // Targeted player starts enter the shared server start intent and are
        // handled by `on_targeted_attack_started` alongside AI attacks.
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
        .filter(|(_, _, _, side, state, _, _)| {
            **side != *attacker_side && !state.is_incapacitated()
        })
        .min_by(
            |(a, _, a_transform, _, _, _, _), (b, _, b_transform, _, _, _, _)| {
                compare_target(attacker_transform, a_transform, *a, b_transform, *b)
            },
        );
    let Some((bot, bot_look, _, _, _, Some(defense), chances)) = nearest else {
        return;
    };
    try_start_reaction(
        &mut cmd,
        BotReactionAttempt {
            defender: bot,
            attacker_look,
            defender_look: bot_look,
            defense: *defense,
            chances: chances.copied().unwrap_or_default(),
            windup_seconds: viewer
                .get_for_attack(attacker, hand)
                .map(|view| attack_preparation_secs(&view, strike_family.melee_style()))
                .unwrap_or(0.3),
        },
        &combat_config.ai.ordinary.defense,
    );
}

pub(super) fn on_targeted_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<(
        &CharacterLook,
        &TacticalCombatState,
        &ReactiveDefenseAi,
        Option<&DefenseChances>,
    )>,
    combat_config: Res<TacticalCombatConfig>,
) {
    let Some(target) = event.target else {
        return;
    };
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, target]) else {
        return;
    };
    if let Ok((_, state, defense, chances)) = q_ai.get(target)
        && !state.is_incapacitated()
    {
        try_start_reaction(
            &mut cmd,
            BotReactionAttempt {
                defender: target,
                attacker_look,
                defender_look,
                defense: *defense,
                chances: chances.copied().unwrap_or_default(),
                windup_seconds: event.windup.as_secs_f32(),
            },
            &combat_config.ai.ordinary.defense,
        );
    }
}

pub(super) fn on_targeted_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<(
        &CharacterLook,
        &TacticalCombatState,
        &ReactiveDefenseAi,
        Option<&DefenseChances>,
    )>,
    combat_config: Res<TacticalCombatConfig>,
) {
    let Some(target) = event.target else {
        return;
    };
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, target]) else {
        return;
    };
    if let Ok((_, state, defense, chances)) = q_ai.get(target)
        && !state.is_incapacitated()
    {
        try_start_reaction(
            &mut cmd,
            BotReactionAttempt {
                defender: target,
                attacker_look,
                defender_look,
                defense: *defense,
                chances: chances.copied().unwrap_or_default(),
                windup_seconds: event.animation_windup.as_secs_f32(),
            },
            &combat_config.ai.ordinary.defense,
        );
    }
}

fn try_start_reaction(
    cmd: &mut Commands,
    attempt: BotReactionAttempt<'_>,
    config: &AiDefenseConfig,
) {
    let (a2, a1) = attempt.attacker_look.yaw.sin_cos();
    let (d2, d1) = attempt.defender_look.yaw.sin_cos();
    if attempt.defense.requires_facing
        && flanking_from_dir((a1, a2), (d1, d2)) > config.frontal_flanking_max
    {
        return;
    }
    let Some(choice) = roll_defend_choice(attempt.chances) else {
        return;
    };
    let delay = if attempt.chances.dodge_chance >= 1.0 {
        // The authored test dodger is deliberately anticipatory. Leave enough
        // of even a fast fist windup for the quickstep's launch phase.
        (attempt.windup_seconds - 0.16).clamp(0.02, 0.12)
    } else {
        rand::random_range(config.reaction_delay_min_seconds..config.reaction_delay_max_seconds)
    };
    cmd.entity(attempt.defender).insert(PendingBotReaction {
        timer: Timer::from_seconds(delay, TimerMode::Once),
        choice,
    });
}

pub(super) fn roll_defend_choice(chances: DefenseChances) -> Option<DefendRequest> {
    let roll: f64 = rand::random();
    if roll < chances.dodge_chance {
        Some(DefendRequest::Dodge {
            direction: if rand::random() { Vec2::X } else { Vec2::NEG_X },
        })
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

        let choice = reaction.choice;
        cmd.entity(bot).remove::<PendingBotReaction>();
        cmd.trigger(DefendIntent {
            defender: bot,
            choice,
        });
    }
}
