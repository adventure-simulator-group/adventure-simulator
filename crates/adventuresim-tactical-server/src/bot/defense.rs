use super::*;

const FRONTAL_FLANKING_MAX: f32 = 0.01;
/// Must land shortly before the attacker's weapon windup (see
/// `PlayerEquipment::weapon_windup_secs`, 300ms by default - the delay
/// between an attack's `Start` and its resolution), not just somewhere
/// under it. `resolve_defender_response` scores a committed
/// reaction by freshness at the moment of impact
/// (`input_reflex = 1 - elapsed_since_commit / MAX_REFLEX_WINDOW`, 500ms),
/// and `Dodge`/`Parry`'s effectiveness scales directly with that (`factor()`
/// in `adventuresim-core::combat`) - a reaction that commits early in the
/// window is technically "in time" but has mostly gone stale by the time
/// the hit actually resolves, and barely reduces the attack roll. Landing
/// in the last ~50ms before resolution keeps the reflex close to fresh
/// (`input_reflex` near 1.0) while leaving margin for the windup race's own
/// jitter.
const REACTION_DELAY_SECS: std::ops::Range<f32> = 0.20..0.27;

/// Per-bot chance (each out of 1.0) that a reflex defense (see
/// `try_start_reaction`) resolves to a parry or dodge. Inserted by a reactive
/// defense behavior package with balance-tuned defaults; BRP tests
/// mutate it to force deterministic parry/dodge outcomes (see
/// `DefenseChances` in `scripts/adventuresim_brp_lib.py`, regenerated via
/// `just generate-brp-types`).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct DefenseChances {
    pub parry_chance: f64,
    pub dodge_chance: f64,
}

impl Default for DefenseChances {
    fn default() -> Self {
        Self {
            parry_chance: 0.2,
            dodge_chance: 0.2,
        }
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

/// Predicts whether the nearest opposing AI facing a client attacker notices
/// the untargeted client windup and decides to dodge or parry it.
///
/// A bot has no real reflexes: its package controls whether facing is required
/// and how often it reads the attack correctly. A decision to react is
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
            Option<&ReactiveDefenseAi>,
            Option<&DefenseChances>,
        ),
        With<MissionEnemy>,
    >,
) {
    if !matches!(**event, MeleeActionRequest::Start { .. }) {
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
        bot,
        attacker_look,
        bot_look,
        *defense,
        chances.copied().unwrap_or_default(),
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
) {
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, event.target])
    else {
        return;
    };
    if let Ok((_, state, defense, chances)) = q_ai.get(event.target)
        && !state.is_incapacitated()
    {
        try_start_reaction(
            &mut cmd,
            event.target,
            attacker_look,
            defender_look,
            *defense,
            chances.copied().unwrap_or_default(),
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
            target,
            attacker_look,
            defender_look,
            *defense,
            chances.copied().unwrap_or_default(),
        );
    }
}

pub(super) fn try_start_reaction(
    cmd: &mut Commands,
    defender: Entity,
    attacker_look: &CharacterLook,
    defender_look: &CharacterLook,
    defense: ReactiveDefenseAi,
    chances: DefenseChances,
) {
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    if defense.requires_facing && flanking_from_dir((a1, a2), (d1, d2)) > FRONTAL_FLANKING_MAX {
        return;
    }
    let Some(choice) = roll_defend_choice(chances) else {
        return;
    };
    cmd.entity(defender).insert(PendingBotReaction {
        timer: Timer::from_seconds(rand::random_range(REACTION_DELAY_SECS), TimerMode::Once),
        choice,
    });
}

pub(super) fn roll_defend_choice(chances: DefenseChances) -> Option<DefendRequest> {
    let roll: f64 = rand::random();
    if roll < chances.parry_chance {
        Some(DefendRequest::Parry)
    } else if roll < chances.parry_chance + chances.dodge_chance {
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

        let choice = reaction.choice;
        cmd.entity(bot).remove::<PendingBotReaction>();
        cmd.trigger(DefendIntent {
            defender: bot,
            choice,
        });
    }
}
