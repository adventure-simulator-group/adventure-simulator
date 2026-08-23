mod enrollment;
mod receipt;
mod systems;
mod terminal;

use std::{num::NonZeroU32, time::Duration};

use adventuresim_tactical_core::prelude::CharacterId;
use bevy::prelude::*;

use enrollment::PartyEnrollment;
pub(crate) use enrollment::{AdmissionResult, EnrollmentEffect};
pub(crate) use systems::{
    check_mission_timeout, check_terminal_combat_outcome, fail_stalled_terminal_submission,
    finish_terminal_presentation, process_terminal_submission_results,
};
pub(crate) use terminal::FrozenTerminal;
use terminal::TerminalState;

#[derive(Resource, Debug)]
pub(crate) struct MissionState {
    timeout: Option<Timer>,
    required_enemy_defeats: u32,
    enrollment: PartyEnrollment,
    terminal: TerminalState,
}

impl MissionState {
    pub(crate) fn new(
        timeout: Option<Timer>,
        required_enemy_defeats: u32,
        expected_party_members: NonZeroU32,
    ) -> Self {
        Self {
            timeout,
            required_enemy_defeats,
            enrollment: PartyEnrollment::new(expected_party_members),
            terminal: TerminalState::default(),
        }
    }

    pub(crate) fn required_enemy_defeats(&self) -> u32 {
        self.required_enemy_defeats
    }

    pub(crate) fn begin_enrollment(&mut self) {
        self.enrollment.begin();
    }

    pub(crate) fn observe_loaded_party_member(
        &mut self,
        character: CharacterId,
    ) -> AdmissionResult {
        self.enrollment.observe_loaded(character)
    }

    pub(crate) fn allows_party_join(&self, character: CharacterId) -> bool {
        self.enrollment.allows_join(character)
    }

    pub(crate) fn advance_enrollment(
        &mut self,
        loaded_party: u32,
        has_loading_player: bool,
        delta: Duration,
        reconnect_grace: Duration,
    ) -> EnrollmentEffect {
        self.enrollment
            .advance(loaded_party, has_loading_player, delta, reconnect_grace)
    }

    pub(crate) fn enrollment_ready(&self, has_loading_player: bool) -> bool {
        self.enrollment.ready_for_outcome(has_loading_player)
    }

    pub(crate) fn expected_party_members(&self) -> NonZeroU32 {
        self.enrollment.expected()
    }

    pub(crate) fn terminal_is_running(&self) -> bool {
        self.terminal.is_running()
    }

    pub(crate) fn begin_terminal_submission(
        &mut self,
        frozen: FrozenTerminal,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        self.terminal.begin(frozen, now, ack_timeout)
    }

    pub(crate) fn terminal_retry_due(
        &mut self,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        self.terminal.retry_due(now, ack_timeout)
    }

    pub(crate) fn terminal_enqueue_failed(&mut self, now: Duration, retry_backoff: Duration) {
        self.terminal.enqueue_failed(now, retry_backoff);
    }

    pub(crate) fn apply_terminal_submission_result(
        &mut self,
        result: crate::stdb::TerminalSubmissionResult,
        now: Duration,
        retry_backoff: Duration,
        presentation_delay: Duration,
    ) -> Option<adventuresim_tactical_netcode::message::TacticalOutcome> {
        self.terminal
            .apply_submission_result(result, now, retry_backoff, presentation_delay)
    }

    pub(crate) fn fail_if_terminal_ack_stalled(&mut self, now: Duration) -> bool {
        self.terminal.fail_if_ack_stalled(now)
    }

    pub(crate) fn advance_terminal_presentation(
        &mut self,
        delta: Duration,
    ) -> Option<adventuresim_tactical_netcode::message::TacticalOutcome> {
        self.terminal.advance_presentation(delta)
    }

    pub(crate) fn tick_timeout(&mut self, delta: Duration) -> bool {
        let Some(timeout) = &mut self.timeout else {
            return false;
        };
        timeout.tick(delta);
        timeout.is_finished()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalCombatSnapshot {
    pub required_enemies: u32,
    pub loaded_enemies: u32,
    pub incapacitated_enemies: u32,
    pub loaded_party: u32,
    pub incapacitated_party: u32,
    pub enrollment_sealed: bool,
}

pub(crate) fn terminal_resolution(
    snapshot: TerminalCombatSnapshot,
) -> Option<adventuresim_stdb_client::TacticalMissionResolution> {
    use adventuresim_stdb_client::TacticalMissionResolution;

    if snapshot.required_enemies == 0
        || snapshot.loaded_enemies < snapshot.required_enemies
        || !snapshot.enrollment_sealed
        || snapshot.loaded_party == 0
    {
        return None;
    }
    let enemies_defeated = snapshot.incapacitated_enemies >= snapshot.required_enemies;
    let party_defeated = snapshot.incapacitated_party >= snapshot.loaded_party;
    match (enemies_defeated, party_defeated) {
        (_, true) => Some(TacticalMissionResolution::Failed),
        (true, false) => Some(TacticalMissionResolution::Defeated),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use adventuresim_stdb_client::TacticalMissionResolution;

    use super::*;

    fn snapshot() -> TerminalCombatSnapshot {
        TerminalCombatSnapshot {
            required_enemies: 2,
            loaded_enemies: 2,
            incapacitated_enemies: 0,
            loaded_party: 2,
            incapacitated_party: 0,
            enrollment_sealed: true,
        }
    }

    #[test]
    fn terminal_resolution_waits_for_complete_enrollment_and_enemy_projection() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                loaded_enemies: 1,
                ..snapshot()
            }),
            None
        );
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                enrollment_sealed: false,
                ..snapshot()
            }),
            None
        );
    }

    #[test]
    fn simultaneous_defeat_deterministically_fails() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                incapacitated_enemies: 2,
                incapacitated_party: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Failed)
        );
    }

    #[test]
    fn victory_requires_enemy_defeat_with_an_active_party_member() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                incapacitated_enemies: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Defeated)
        );
    }

    #[test]
    fn enemy_incapacitation_must_be_simultaneous() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                incapacitated_enemies: 1,
                ..snapshot()
            }),
            None
        );
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                incapacitated_enemies: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Defeated)
        );
    }
}
