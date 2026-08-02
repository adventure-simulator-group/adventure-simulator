use std::{num::NonZeroU8, time::Duration};

use adventuresim_stdb_client::TacticalMissionResolution;
use adventuresim_tactical_core::prelude::{CharacterId, Player, TacticalCombatState};
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{SendMode, ServerTriggerExt, ToClients},
    message::TacticalOutcomeResponse,
};
use bevy::prelude::*;

use super::{
    AdmissionResult, EnrollmentEffect, FrozenTerminal, MissionState, TerminalCombatSnapshot,
    terminal_resolution,
};
use crate::{
    bot::MissionEnemy,
    combat::{TacticalCombatSide, TacticalConsequenceAccumulator},
    player_projection::LoadingPlayer,
    stdb::{SpacetimeDb, TerminalSubmissionResult},
};

const TERMINAL_RETRY_BACKOFF: Duration = Duration::from_secs(1);
const TERMINAL_PRESENTATION_DELAY: Duration = Duration::from_secs(3);
const TERMINAL_ACK_TIMEOUT: Duration = Duration::from_secs(10);
const PARTY_RECONNECT_GRACE: Duration = Duration::from_secs(10);

fn commit_terminal_resolution(
    resolution: TacticalMissionResolution,
    now: Duration,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
) -> Result {
    let frozen = FrozenTerminal {
        resolution,
        receipt: super::receipt::tactical_consequence_receipt(&consequences),
    };
    let Some(frozen) = state.begin_terminal_submission(frozen, now, TERMINAL_ACK_TIMEOUT) else {
        return Ok(());
    };
    submit_frozen_terminal(frozen, now, conn, state)
}

fn submit_frozen_terminal(
    frozen: FrozenTerminal,
    now: Duration,
    conn: Res<SpacetimeDb>,
    mut state: ResMut<MissionState>,
) -> Result {
    if let Err(error) = conn.submit_terminal(frozen.resolution, frozen.receipt) {
        state.terminal_enqueue_failed(now, TERMINAL_RETRY_BACKOFF);
        warn!(
            "Terminal result enqueue failed; retrying in {}s: {error}",
            TERMINAL_RETRY_BACKOFF.as_secs()
        );
    } else {
        info!(resolution = ?frozen.resolution, "Terminal result queued; awaiting reducer acceptance");
    }
    Ok(())
}

pub(crate) fn process_terminal_submission_results(
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    mut state: ResMut<MissionState>,
    mut cmd: Commands,
) {
    for result in conn.take_terminal_results() {
        let rejection = match &result {
            TerminalSubmissionResult::Rejected(error) => Some(error.clone()),
            TerminalSubmissionResult::Accepted => None,
        };
        if let Some(outcome) = state.apply_terminal_submission_result(
            result,
            time.elapsed(),
            TERMINAL_RETRY_BACKOFF,
            TERMINAL_PRESENTATION_DELAY,
        ) {
            cmd.server_trigger(ToClients {
                mode: SendMode::CLIENTS_ONLY,
                message: TacticalOutcomeResponse { outcome },
            });
            info!(
                ?outcome,
                "Mission terminal result accepted; presenting outcome"
            );
        } else if let Some(error) = rejection {
            warn!(
                "Terminal reducer rejected submission; retrying in {}s: {error}",
                TERMINAL_RETRY_BACKOFF.as_secs()
            );
        }
    }
}

pub(crate) fn fail_stalled_terminal_submission(
    time: Res<Time>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.fail_if_terminal_ack_stalled(time.elapsed()) {
        error!(
            timeout_seconds = TERMINAL_ACK_TIMEOUT.as_secs(),
            "Terminal reducer acknowledgement timed out; exiting without presenting an outcome"
        );
        exit.write(AppExit::Error(NonZeroU8::new(1).expect("one is non-zero")));
    }
}

pub(crate) fn finish_terminal_presentation(
    time: Res<Time>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(outcome) = state.advance_terminal_presentation(time.delta()) {
        info!(?outcome, "Terminal presentation complete; shutting down");
        exit.write(AppExit::Success);
    }
}

pub(crate) fn check_terminal_combat_outcome(
    mut commands: Commands,
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
    enemies: Query<(), (With<MissionEnemy>, With<Player>)>,
    combatants: Query<
        (
            Entity,
            &TacticalCombatSide,
            &TacticalCombatState,
            &CharacterId,
        ),
        With<Player>,
    >,
    loading_players: Query<(), With<LoadingPlayer>>,
) -> Result {
    if let Some(frozen) = state.terminal_retry_due(time.elapsed(), TERMINAL_ACK_TIMEOUT) {
        return submit_frozen_terminal(frozen, time.elapsed(), conn, state);
    }
    if !state.terminal_is_running() {
        return Ok(());
    }
    let mut loaded_party = 0;
    let mut incapacitated_party = 0;
    for (entity, side, combat_state, player_id) in &combatants {
        if *side == TacticalCombatSide::Party {
            if state.observe_loaded_party_member(*player_id) == AdmissionResult::RejectedAfterSeal {
                error!(
                    character_id = player_id.0,
                    "Rejecting unseen Party character projected after enrollment sealed"
                );
                commands.entity(entity).despawn();
                continue;
            }
            loaded_party += 1;
            incapacitated_party += u32::from(combat_state.is_incapacitated());
        }
    }
    let has_loading_player = !loading_players.is_empty();
    if has_loading_player {
        state.begin_enrollment();
    }
    match state.advance_enrollment(
        loaded_party,
        has_loading_player,
        time.delta(),
        PARTY_RECONNECT_GRACE,
    ) {
        EnrollmentEffect::Sealed => info!(
            expected = state.expected_party_members().get(),
            "Party enrollment sealed"
        ),
        EnrollmentEffect::Abandoned => {
            return commit_terminal_resolution(
                TacticalMissionResolution::Failed,
                time.elapsed(),
                conn,
                consequences,
                state,
            );
        }
        EnrollmentEffect::None => {}
    }
    let snapshot = TerminalCombatSnapshot {
        required_enemies: state.required_enemy_defeats(),
        loaded_enemies: enemies.iter().count() as u32,
        defeated_enemies: state.enemies_defeated(),
        loaded_party,
        incapacitated_party,
        enrollment_sealed: state.enrollment_ready(has_loading_player),
    };
    let Some(resolution) = terminal_resolution(snapshot) else {
        return Ok(());
    };
    commit_terminal_resolution(resolution, time.elapsed(), conn, consequences, state)
}

pub(crate) fn check_mission_timeout(
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
) -> Result {
    if !state.tick_timeout(time.delta()) || !state.terminal_is_running() {
        return Ok(());
    }
    info!("Mission timeout, committing bounded failure fallback");
    commit_terminal_resolution(
        TacticalMissionResolution::Failed,
        time.elapsed(),
        conn,
        consequences,
        state,
    )
}
