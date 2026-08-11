use std::time::Duration;

use adventuresim_stdb_client::{TacticalConsequenceReceipt, TacticalMissionResolution};
use adventuresim_tactical_netcode::message::TacticalOutcome;

use crate::stdb::TerminalSubmissionResult;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrozenTerminal {
    pub(crate) resolution: TacticalMissionResolution,
    pub(crate) receipt: TacticalConsequenceReceipt,
}

#[derive(Clone, Debug, PartialEq, Default)]
enum TerminalLifecycle {
    #[default]
    Running,
    RetryScheduled {
        frozen: FrozenTerminal,
        not_before: Duration,
    },
    Submitting {
        frozen: FrozenTerminal,
        ack_deadline: Duration,
    },
    TransportFailed,
    Presenting {
        outcome: TacticalOutcome,
        remaining: Duration,
    },
    Finished {
        outcome: TacticalOutcome,
    },
}

impl TerminalLifecycle {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub(crate) fn begin(
        &mut self,
        frozen: FrozenTerminal,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        if !self.is_running() {
            return None;
        }
        let submission = frozen.clone();
        *self = Self::Submitting {
            frozen,
            ack_deadline: now.saturating_add(ack_timeout),
        };
        Some(submission)
    }

    pub(crate) fn retry_due(
        &mut self,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        let Self::RetryScheduled { frozen, not_before } = self else {
            return None;
        };
        if now < *not_before {
            return None;
        }
        let frozen = frozen.clone();
        let submission = frozen.clone();
        *self = Self::Submitting {
            frozen,
            ack_deadline: now.saturating_add(ack_timeout),
        };
        Some(submission)
    }

    pub(crate) fn enqueue_failed(&mut self, now: Duration, retry_backoff: Duration) {
        let Self::Submitting { frozen, .. } = self else {
            return;
        };
        let frozen = frozen.clone();
        *self = Self::RetryScheduled {
            frozen,
            not_before: now.saturating_add(retry_backoff),
        };
    }

    pub(crate) fn apply_submission_result(
        &mut self,
        result: TerminalSubmissionResult,
        now: Duration,
        retry_backoff: Duration,
        presentation_delay: Duration,
    ) -> Option<TacticalOutcome> {
        let Self::Submitting { frozen, .. } = self else {
            return None;
        };
        match result {
            TerminalSubmissionResult::Accepted => {
                let outcome = presentation_outcome(frozen.resolution);
                *self = Self::Presenting {
                    outcome,
                    remaining: presentation_delay,
                };
                Some(outcome)
            }
            TerminalSubmissionResult::Rejected(_) => {
                let frozen = frozen.clone();
                *self = Self::RetryScheduled {
                    frozen,
                    not_before: now.saturating_add(retry_backoff),
                };
                None
            }
        }
    }

    pub(crate) fn fail_if_ack_stalled(&mut self, now: Duration) -> bool {
        let stalled = matches!(
            self,
            Self::Submitting { ack_deadline, .. } if now >= *ack_deadline
        );
        if stalled {
            *self = Self::TransportFailed;
        }
        stalled
    }

    pub(crate) fn advance_presentation(&mut self, delta: Duration) -> Option<TacticalOutcome> {
        let Self::Presenting { outcome, remaining } = self else {
            return None;
        };
        *remaining = remaining.saturating_sub(delta);
        if !remaining.is_zero() {
            return None;
        }
        let outcome = *outcome;
        *self = Self::Finished { outcome };
        Some(outcome)
    }
}

/// Crate-visible terminal facade. The lifecycle variants remain private so
/// presentation and commitment cannot be fabricated by another server module.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct TerminalState(TerminalLifecycle);

impl TerminalState {
    pub(crate) fn is_running(&self) -> bool {
        self.0.is_running()
    }

    pub(crate) fn begin(
        &mut self,
        frozen: FrozenTerminal,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        self.0.begin(frozen, now, ack_timeout)
    }

    pub(crate) fn retry_due(
        &mut self,
        now: Duration,
        ack_timeout: Duration,
    ) -> Option<FrozenTerminal> {
        self.0.retry_due(now, ack_timeout)
    }

    pub(crate) fn enqueue_failed(&mut self, now: Duration, retry_backoff: Duration) {
        self.0.enqueue_failed(now, retry_backoff);
    }

    pub(crate) fn apply_submission_result(
        &mut self,
        result: TerminalSubmissionResult,
        now: Duration,
        retry_backoff: Duration,
        presentation_delay: Duration,
    ) -> Option<TacticalOutcome> {
        self.0
            .apply_submission_result(result, now, retry_backoff, presentation_delay)
    }

    pub(crate) fn fail_if_ack_stalled(&mut self, now: Duration) -> bool {
        self.0.fail_if_ack_stalled(now)
    }

    pub(crate) fn advance_presentation(&mut self, delta: Duration) -> Option<TacticalOutcome> {
        self.0.advance_presentation(delta)
    }
}

pub(crate) fn presentation_outcome(resolution: TacticalMissionResolution) -> TacticalOutcome {
    match resolution {
        TacticalMissionResolution::Defeated
        | TacticalMissionResolution::DrivenOff
        | TacticalMissionResolution::Captured => TacticalOutcome::Victory,
        TacticalMissionResolution::Failed | TacticalMissionResolution::CaptureTargetKilled => {
            TacticalOutcome::Defeat
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACK: Duration = Duration::from_secs(10);
    const RETRY: Duration = Duration::from_secs(1);
    const PRESENT: Duration = Duration::from_secs(3);

    fn frozen(resolution: TacticalMissionResolution) -> FrozenTerminal {
        FrozenTerminal {
            resolution,
            receipt: TacticalConsequenceReceipt {
                party: Vec::new(),
                equipment_contacts: Vec::new(),
            },
        }
    }

    #[test]
    fn rejection_retries_the_same_inseparable_frozen_payload() {
        let original = frozen(TacticalMissionResolution::Defeated);
        let mut state = TerminalLifecycle::Running;
        assert_eq!(
            state.begin(original.clone(), Duration::ZERO, ACK),
            Some(original.clone())
        );
        state.apply_submission_result(
            TerminalSubmissionResult::Rejected("no".into()),
            Duration::ZERO,
            RETRY,
            PRESENT,
        );
        assert!(
            state
                .retry_due(RETRY - Duration::from_millis(1), ACK)
                .is_none()
        );
        assert_eq!(state.retry_due(RETRY, ACK), Some(original));
    }

    #[test]
    fn accepted_submission_is_the_only_path_to_one_presentation() {
        let mut state = TerminalLifecycle::Running;
        state.begin(
            frozen(TacticalMissionResolution::Defeated),
            Duration::ZERO,
            ACK,
        );
        assert_eq!(
            state.apply_submission_result(
                TerminalSubmissionResult::Accepted,
                Duration::ZERO,
                RETRY,
                PRESENT,
            ),
            Some(TacticalOutcome::Victory)
        );
        assert!(
            state
                .advance_presentation(PRESENT - Duration::from_millis(1))
                .is_none()
        );
        assert_eq!(
            state.advance_presentation(Duration::from_millis(1)),
            Some(TacticalOutcome::Victory)
        );
        assert!(state.advance_presentation(PRESENT).is_none());
    }

    #[test]
    fn synchronous_enqueue_failure_observes_the_retry_boundary() {
        let original = frozen(TacticalMissionResolution::Failed);
        let mut state = TerminalLifecycle::Running;
        state.begin(original.clone(), Duration::ZERO, ACK);
        state.enqueue_failed(Duration::ZERO, RETRY);

        assert!(
            state
                .retry_due(RETRY - Duration::from_millis(1), ACK)
                .is_none()
        );
        assert_eq!(state.retry_due(RETRY, ACK), Some(original));
    }

    #[test]
    fn ambiguous_ack_fails_closed_without_presentation_or_retry() {
        let mut state = TerminalLifecycle::Running;
        state.begin(
            frozen(TacticalMissionResolution::Failed),
            Duration::ZERO,
            ACK,
        );
        assert!(!state.fail_if_ack_stalled(ACK - Duration::from_millis(1)));
        assert!(state.fail_if_ack_stalled(ACK));
        assert!(matches!(state, TerminalLifecycle::TransportFailed));
        assert!(state.retry_due(Duration::MAX, ACK).is_none());
    }
}
