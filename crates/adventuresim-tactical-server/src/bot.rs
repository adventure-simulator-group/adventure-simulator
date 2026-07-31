use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    message::{AttackStartedRequest, DefendRequest},
};
use bevy::prelude::*;

use crate::{MissionState, combat::PendingDefenderResponse};

/// Chance that a bot notices an incoming attack in time to parry it.
const PARRY_CHANCE: f64 = 0.2;
/// Chance that a bot notices an incoming attack in time to dodge it.
const DODGE_CHANCE: f64 = 0.2;
/// Flanking values at or below this are considered "facing each other" (see
/// [`flanking_from_dir`]), which is the only case a bot can react at all.
const FRONTAL_FLANKING_MAX: f32 = 0.01;
/// Range (in seconds) a bot's reaction to a noticed attack is delayed by,
/// simulating varying skill/reflexes between bots. A bot that rolls a long
/// delay may end up committing its reaction only after the attack has
/// already been resolved, i.e. reacting too late to matter.
const REACTION_DELAY_SECS: std::ops::Range<f32> = 0.05..0.6;

/// Marks a server-controlled bot filling in for a temporary (non-connected)
/// mission character.
#[derive(Component)]
pub struct MissionEnemy;

#[derive(Component)]
pub struct CountedEnemyDeath;

/// Emitted only by the server's authoritative combat/death pipeline. Combat
/// damage is not implemented yet, so no client-controlled path can emit it and
/// incomplete missions fail closed at timeout.
#[derive(Event)]
pub struct AuthoritativeEnemyDeath(pub Entity);

/// A bot's yet-to-commit reaction to a noticed attack. Ticks down for
/// [`REACTION_DELAY_SECS`] before becoming a [`PendingDefenderResponse`],
/// simulating the bot's reflexes.
#[derive(Component)]
struct PendingBotReaction {
    timer: Timer,
    choice: DefendRequest,
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_authoritative_enemy_death)
            .add_observer(on_attack_started)
            .add_systems(Update, tick_bot_reactions);
    }
}

fn on_authoritative_enemy_death(
    death: On<AuthoritativeEnemyDeath>,
    enemies: Query<(), (With<MissionEnemy>, Without<CountedEnemyDeath>)>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
) {
    let entity = death.0;
    if enemies.get(entity).is_err() {
        return;
    }
    commands.entity(entity).insert(CountedEnemyDeath);
    state.enemies_killed = state.enemies_killed.saturating_add(1);
}

pub fn mission_objective_satisfied(required: u32, killed: u32) -> bool {
    killed >= required
}

/// Predicts, for every bot facing the attacker head-on, whether it notices
/// this attack starting and, primitively, decides to dodge or parry it.
///
/// A bot has no real reflexes: it only ever gets a chance to react when it is
/// facing its attacker (`flanking <= FRONTAL_FLANKING_MAX`), and even then it
/// correctly reads the attack only some of the time. A decision to react is
/// committed only after a random delay (see [`REACTION_DELAY_SECS`]).
fn on_attack_started(
    event: On<FromClient<AttackStartedRequest>>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_bots: Query<(Entity, &CharacterLook), With<MissionEnemy>>,
) {
    let Some(attacker) = event.client_id.entity() else {
        return;
    };
    let Ok(attacker_look) = q_character.get(attacker) else {
        return;
    };
    let (a2, a1) = attacker_look.yaw.sin_cos();

    for (bot, bot_look) in &q_bots {
        let (d2, d1) = bot_look.yaw.sin_cos();
        if flanking_from_dir((a1, a2), (d1, d2)) > FRONTAL_FLANKING_MAX {
            continue;
        }

        let Some(choice) = roll_defend_choice() else {
            continue;
        };

        cmd.entity(bot).insert(PendingBotReaction {
            timer: Timer::from_seconds(rand::random_range(REACTION_DELAY_SECS), TimerMode::Once),
            choice,
        });
    }
}

fn roll_defend_choice() -> Option<DefendRequest> {
    let roll: f64 = rand::random();
    if roll < PARRY_CHANCE {
        Some(DefendRequest::Parry)
    } else if roll < PARRY_CHANCE + DODGE_CHANCE {
        Some(DefendRequest::Dodge)
    } else {
        None
    }
}

fn tick_bot_reactions(
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut q_reacting: Query<(Entity, &mut PendingBotReaction)>,
) {
    for (bot, mut reaction) in &mut q_reacting {
        reaction.timer.tick(time.delta());
        if !reaction.timer.is_finished() {
            continue;
        }

        cmd.entity(bot)
            .remove::<PendingBotReaction>()
            .insert(PendingDefenderResponse {
                choice: reaction.choice,
                set_at: time.elapsed_secs(),
            });
    }
}

#[cfg(test)]
mod tests {
    use super::mission_objective_satisfied;

    #[test]
    fn mission_success_requires_the_full_authoritative_objective() {
        assert!(mission_objective_satisfied(0, 0));
        assert!(!mission_objective_satisfied(3, 0));
        assert!(!mission_objective_satisfied(3, 2));
        assert!(mission_objective_satisfied(3, 3));
        assert!(mission_objective_satisfied(3, 4));
    }
}
