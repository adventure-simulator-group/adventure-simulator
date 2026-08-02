use std::{collections::HashSet, num::NonZeroU32, time::Duration};

use adventuresim_tactical_core::prelude::CharacterId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnrollmentEffect {
    None,
    Sealed,
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdmissionResult {
    Admitted,
    RejectedAfterSeal,
}

#[derive(Debug)]
pub(crate) enum PartyEnrollment {
    Awaiting {
        expected: NonZeroU32,
    },
    Enrolling {
        expected: NonZeroU32,
        seen: HashSet<CharacterId>,
        empty_for: Duration,
    },
    Sealed {
        expected: NonZeroU32,
        seen: HashSet<CharacterId>,
        empty_for: Duration,
    },
}

impl PartyEnrollment {
    pub(crate) fn new(expected: NonZeroU32) -> Self {
        Self::Awaiting { expected }
    }

    pub(crate) fn begin(&mut self) {
        if let Self::Awaiting { expected } = *self {
            *self = Self::Enrolling {
                expected,
                seen: HashSet::new(),
                empty_for: Duration::ZERO,
            };
        }
    }

    pub(crate) fn observe_loaded(&mut self, character: CharacterId) -> AdmissionResult {
        self.begin();
        match self {
            Self::Enrolling { seen, .. } => {
                seen.insert(character);
                AdmissionResult::Admitted
            }
            Self::Sealed { seen, .. } if seen.contains(&character) => AdmissionResult::Admitted,
            Self::Sealed { .. } => AdmissionResult::RejectedAfterSeal,
            Self::Awaiting { .. } => unreachable!("begin transitions awaiting enrollment"),
        }
    }

    pub(crate) fn allows_join(&self, character: CharacterId) -> bool {
        match self {
            Self::Awaiting { .. } | Self::Enrolling { .. } => true,
            Self::Sealed { seen, .. } => seen.contains(&character),
        }
    }

    pub(crate) fn advance(
        &mut self,
        loaded_party: u32,
        has_loading_player: bool,
        delta: Duration,
        reconnect_grace: Duration,
    ) -> EnrollmentEffect {
        let mut effect = EnrollmentEffect::None;
        if let Self::Enrolling {
            expected,
            seen,
            empty_for,
        } = self
            && seen.len() >= expected.get() as usize
            && !has_loading_player
        {
            let expected = *expected;
            let seen = std::mem::take(seen);
            let empty_for = *empty_for;
            *self = Self::Sealed {
                expected,
                seen,
                empty_for,
            };
            effect = EnrollmentEffect::Sealed;
        }

        let empty_for = match self {
            Self::Awaiting { .. } => return effect,
            Self::Enrolling { empty_for, .. } | Self::Sealed { empty_for, .. } => empty_for,
        };
        if loaded_party == 0 && !has_loading_player {
            *empty_for = empty_for.saturating_add(delta);
        } else {
            *empty_for = Duration::ZERO;
        }
        if *empty_for >= reconnect_grace {
            EnrollmentEffect::Abandoned
        } else {
            effect
        }
    }

    pub(crate) fn ready_for_outcome(&self, has_loading_player: bool) -> bool {
        matches!(self, Self::Sealed { .. }) && !has_loading_player
    }

    pub(crate) fn expected(&self) -> NonZeroU32 {
        match self {
            Self::Awaiting { expected }
            | Self::Enrolling { expected, .. }
            | Self::Sealed { expected, .. } => *expected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(10);

    #[test]
    fn enrollment_seals_only_after_expected_roster_finishes_loading() {
        let mut enrollment = PartyEnrollment::new(NonZeroU32::new(2).unwrap());
        enrollment.begin();
        enrollment.observe_loaded(CharacterId(1));
        assert_eq!(
            enrollment.advance(1, false, Duration::ZERO, GRACE),
            EnrollmentEffect::None
        );
        enrollment.observe_loaded(CharacterId(2));
        assert_eq!(
            enrollment.advance(2, true, Duration::ZERO, GRACE),
            EnrollmentEffect::None
        );
        assert_eq!(
            enrollment.advance(2, false, Duration::ZERO, GRACE),
            EnrollmentEffect::Sealed
        );
        assert!(enrollment.ready_for_outcome(false));
    }

    #[test]
    fn begun_partial_enrollment_uses_bounded_abandonment_grace() {
        let mut enrollment = PartyEnrollment::new(NonZeroU32::new(2).unwrap());
        enrollment.observe_loaded(CharacterId(1));
        assert_eq!(
            enrollment.advance(0, false, GRACE - Duration::from_millis(1), GRACE),
            EnrollmentEffect::None
        );
        assert_eq!(
            enrollment.advance(0, false, Duration::from_millis(1), GRACE),
            EnrollmentEffect::Abandoned
        );
    }

    #[test]
    fn sealed_roster_allows_known_reconnect_and_rejects_unseen_member() {
        let mut enrollment = PartyEnrollment::new(NonZeroU32::new(1).unwrap());
        assert_eq!(
            enrollment.observe_loaded(CharacterId(1)),
            AdmissionResult::Admitted
        );
        assert_eq!(
            enrollment.advance(1, false, Duration::ZERO, GRACE),
            EnrollmentEffect::Sealed
        );
        assert!(enrollment.allows_join(CharacterId(1)));
        assert!(!enrollment.allows_join(CharacterId(2)));
        assert_eq!(
            enrollment.observe_loaded(CharacterId(1)),
            AdmissionResult::Admitted
        );
        assert_eq!(
            enrollment.observe_loaded(CharacterId(2)),
            AdmissionResult::RejectedAfterSeal
        );
    }
}
