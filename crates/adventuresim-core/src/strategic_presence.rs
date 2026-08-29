//! Typed strategic presence and co-presence rules.
//!
//! Domain owners supply validated facts; this module prevents those facts from
//! being joined through feature-specific strings or upgraded from a coarse
//! settlement to an exact venue without explicit evidence.

use crate::{
    strategic_place::{PlaceIdentityError, StrategicPlaceId},
    strategic_time::MINUTES_PER_DAY,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResidencePresenceRole {
    OwnerOccupant,
    HouseholdOccupant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceBasis {
    SettlementMembership,
    ValidatedVenueSelection,
    ScheduledResident,
    ResidenceOccupancy(ResidencePresenceRole),
    CaseSiteOccupancy,
    CaseContextMembership,
}

/// One character's presence projected at an explicit observer frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategicPresence {
    character_id: u64,
    place: StrategicPlaceId,
    frontier: PresenceFrontier,
    basis: PresenceBasis,
}

/// The observer whose personal chronology authorizes a presence projection.
/// Target characters' clocks are intentionally neither read nor compared for
/// pairwise-soft interactions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresenceFrontier {
    pub observer_character_id: u64,
    pub personal_minute: u64,
}

impl StrategicPresence {
    pub fn settlement_membership(
        character_id: u64,
        settlement_id: impl Into<String>,
        frontier: PresenceFrontier,
    ) -> Result<Self, PresenceError> {
        Ok(Self {
            character_id,
            place: StrategicPlaceId::settlement(settlement_id)?,
            frontier,
            basis: PresenceBasis::SettlementMembership,
        })
    }

    /// Refines coarse settlement membership after the server validates an
    /// instantaneous venue selection against current settlement navigation.
    pub fn validated_venue_selection(
        settlement_presence: &Self,
        place: StrategicPlaceId,
    ) -> Result<Self, PresenceError> {
        let StrategicPlaceId::Settlement {
            settlement_id: coarse_settlement,
        } = &settlement_presence.place
        else {
            return Err(PresenceError::CoarseSettlementRequired);
        };
        if settlement_presence.basis != PresenceBasis::SettlementMembership {
            return Err(PresenceError::CoarseSettlementRequired);
        }
        if !matches!(
            &place,
            StrategicPlaceId::SettlementVenue { .. } | StrategicPlaceId::ChapterVenue { .. }
        ) || place.settlement_id() != Some(coarse_settlement.as_str())
        {
            return Err(PresenceError::PlaceMismatch);
        }
        Ok(Self {
            character_id: settlement_presence.character_id,
            place,
            frontier: settlement_presence.frontier,
            basis: PresenceBasis::ValidatedVenueSelection,
        })
    }

    pub fn scheduled_resident(
        character_id: u64,
        place: StrategicPlaceId,
        frontier: PresenceFrontier,
        schedule: DailyPresenceWindow,
        alive: bool,
        context_suppressed: bool,
        health_suppressed: bool,
    ) -> Result<ScheduledStrategicPresence, PresenceError> {
        if !matches!(
            &place,
            StrategicPlaceId::SettlementVenue { .. } | StrategicPlaceId::ChapterVenue { .. }
        ) {
            return Err(PresenceError::ExactVenueRequired);
        }
        if !alive {
            return Err(PresenceError::Unavailable);
        }
        let remaining_minutes = schedule.remaining_minutes(
            frontier.personal_minute,
            context_suppressed,
            health_suppressed,
        )?;
        Ok(ScheduledStrategicPresence {
            presence: Self {
                character_id,
                place,
                frontier,
                basis: PresenceBasis::ScheduledResident,
            },
            remaining_minutes,
        })
    }

    /// Exact residence presence requires an effective occupancy edge. Legal
    /// ownership alone deliberately has no constructor.
    pub fn residence_occupancy(
        character_id: u64,
        place: StrategicPlaceId,
        owner_character_id: u64,
        admitted_minute: u64,
        frontier: PresenceFrontier,
        holding_active: bool,
    ) -> Result<Self, PresenceError> {
        if !matches!(&place, StrategicPlaceId::Residence { .. }) {
            return Err(PresenceError::ResidenceRequired);
        }
        if admitted_minute > frontier.personal_minute {
            return Err(PresenceError::FutureEvidence);
        }
        if !holding_active {
            return Err(PresenceError::Unavailable);
        }
        let role = if character_id == owner_character_id {
            ResidencePresenceRole::OwnerOccupant
        } else {
            ResidencePresenceRole::HouseholdOccupant
        };
        Ok(Self {
            character_id,
            place,
            frontier,
            basis: PresenceBasis::ResidenceOccupancy(role),
        })
    }

    /// Physical party/Character occupancy at an exact case site. Observer
    /// discovery remains the adapter's responsibility.
    pub fn case_site_occupancy(
        character_id: u64,
        place: StrategicPlaceId,
        frontier: PresenceFrontier,
        occupied: bool,
        alive_at_frontier: bool,
    ) -> Result<Self, PresenceError> {
        if !matches!(&place, StrategicPlaceId::CaseSite { .. }) {
            return Err(PresenceError::CaseSiteRequired);
        }
        if !occupied || !alive_at_frontier {
            return Err(PresenceError::Unavailable);
        }
        Ok(Self {
            character_id,
            place,
            frontier,
            basis: PresenceBasis::CaseSiteOccupancy,
        })
    }

    /// Presence granted by one live contextual membership at a case site.
    /// The expected revision prevents a stale projected context from joining.
    #[expect(
        clippy::too_many_arguments,
        reason = "presence validation compares each authority fact explicitly"
    )]
    pub fn case_context_membership(
        character_id: u64,
        place: StrategicPlaceId,
        frontier: PresenceFrontier,
        active: bool,
        expected_context_id: &str,
        actual_context_id: &str,
        expected_revision: u32,
        actual_revision: u32,
        alive_at_frontier: bool,
    ) -> Result<Self, PresenceError> {
        if !matches!(&place, StrategicPlaceId::CaseSite { .. }) {
            return Err(PresenceError::CaseSiteRequired);
        }
        if !active
            || expected_context_id.is_empty()
            || expected_context_id != actual_context_id
            || expected_revision != actual_revision
            || !alive_at_frontier
        {
            return Err(PresenceError::Unavailable);
        }
        Ok(Self {
            character_id,
            place,
            frontier,
            basis: PresenceBasis::CaseContextMembership,
        })
    }

    pub fn character_id(&self) -> u64 {
        self.character_id
    }

    pub fn place(&self) -> &StrategicPlaceId {
        &self.place
    }

    pub fn frontier(&self) -> PresenceFrontier {
        self.frontier
    }

    pub fn basis(&self) -> PresenceBasis {
        self.basis
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledStrategicPresence {
    presence: StrategicPresence,
    remaining_minutes: u64,
}

impl ScheduledStrategicPresence {
    pub fn presence(&self) -> &StrategicPresence {
        &self.presence
    }

    pub fn remaining_minutes(&self) -> u64 {
        self.remaining_minutes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyPresenceWindow {
    pub start_minute: u16,
    pub end_minute: u16,
}

/// Historical suppression contributed by one outbreak infection course.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresenceSuppression {
    pub context_suppressed: bool,
    pub health_suppressed: bool,
}

/// Computes patient suppression solely from facts effective at the observer's
/// minute. A later recovery, death, or source remediation cannot rewrite an
/// earlier projection.
pub fn outbreak_patient_suppression_at(
    contracted_minute: u64,
    recovery_minute: u64,
    remediation_minute: Option<u64>,
    observer_minute: u64,
    alive_at_observer: bool,
) -> Result<PresenceSuppression, PresenceError> {
    if recovery_minute < contracted_minute {
        return Err(PresenceError::InvalidChronology);
    }
    if !alive_at_observer {
        return Ok(PresenceSuppression {
            context_suppressed: false,
            health_suppressed: true,
        });
    }
    let infection_active =
        contracted_minute <= observer_minute && observer_minute < recovery_minute;
    Ok(PresenceSuppression {
        context_suppressed: infection_active
            && remediation_minute.is_none_or(|minute| minute > observer_minute),
        health_suppressed: infection_active,
    })
}

impl DailyPresenceWindow {
    pub fn minutes_until_start(self, personal_minute: u64) -> Result<u32, PresenceError> {
        if u64::from(self.start_minute) > MINUTES_PER_DAY
            || u64::from(self.end_minute) > MINUTES_PER_DAY
            || self.start_minute == self.end_minute
        {
            return Err(PresenceError::InvalidSchedule);
        }
        if self
            .remaining_minutes(personal_minute, false, false)
            .is_ok()
        {
            return Ok(0);
        }
        let current = personal_minute % MINUTES_PER_DAY;
        let start = u64::from(self.start_minute);
        let wait = (start + MINUTES_PER_DAY - current) % MINUTES_PER_DAY;
        Ok(if wait == 0 { MINUTES_PER_DAY } else { wait } as u32)
    }

    pub fn remaining_minutes(
        self,
        personal_minute: u64,
        context_suppressed: bool,
        health_suppressed: bool,
    ) -> Result<u64, PresenceError> {
        if u64::from(self.start_minute) > MINUTES_PER_DAY
            || u64::from(self.end_minute) > MINUTES_PER_DAY
        {
            return Err(PresenceError::InvalidSchedule);
        }
        if context_suppressed || health_suppressed {
            return Err(PresenceError::Suppressed);
        }
        let minute = personal_minute % MINUTES_PER_DAY;
        let start = u64::from(self.start_minute);
        let end = u64::from(self.end_minute);
        if start == end {
            return Err(PresenceError::OutsideSchedule);
        }
        let remaining = if start < end {
            if start <= minute && minute < end {
                Some(end - minute)
            } else {
                None
            }
        } else if minute >= start {
            Some((MINUTES_PER_DAY - minute) + end)
        } else if minute < end {
            Some(end - minute)
        } else {
            None
        };
        remaining.ok_or(PresenceError::OutsideSchedule)
    }
}

/// Co-presence is equality of canonical place at the same projected personal
/// observer frontier. A settlement shell never equals an exact venue, and
/// facts projected for different observers/frontiers never silently join.
pub fn are_co_present(left: &StrategicPresence, right: &StrategicPresence) -> bool {
    left.frontier == right.frontier && left.place == right.place
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceError {
    InvalidPlaceIdentity,
    CoarseSettlementRequired,
    ExactVenueRequired,
    ResidenceRequired,
    CaseSiteRequired,
    PlaceMismatch,
    InvalidSchedule,
    OutsideSchedule,
    Suppressed,
    FutureEvidence,
    InvalidChronology,
    Unavailable,
}

impl From<PlaceIdentityError> for PresenceError {
    fn from(_: PlaceIdentityError) -> Self {
        Self::InvalidPlaceIdentity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategic_place::SettlementVenueKind;

    fn frontier(observer_character_id: u64, personal_minute: u64) -> PresenceFrontier {
        PresenceFrontier {
            observer_character_id,
            personal_minute,
        }
    }

    #[test]
    fn settlement_membership_does_not_equal_exact_venue_presence() {
        let coarse =
            StrategicPresence::settlement_membership(1, "lubeck", frontier(1, 720)).unwrap();
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let exact = StrategicPresence::validated_venue_selection(&coarse, inn).unwrap();

        assert!(!are_co_present(&coarse, &exact));
        assert_eq!(exact.basis(), PresenceBasis::ValidatedVenueSelection);
    }

    #[test]
    fn chapter_representative_can_share_an_effective_service_venue() {
        let observer_frontier = frontier(1, 720);
        let coarse =
            StrategicPresence::settlement_membership(1, "lubeck", observer_frontier).unwrap();
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let actor = StrategicPresence::validated_venue_selection(&coarse, inn.clone()).unwrap();
        let representative = StrategicPresence::scheduled_resident(
            2,
            inn,
            observer_frontier,
            DailyPresenceWindow {
                start_minute: 0,
                end_minute: 1_440,
            },
            true,
            false,
            false,
        )
        .unwrap();

        assert!(are_co_present(&actor, representative.presence()));
    }

    #[test]
    fn residence_presence_distinguishes_owner_occupant_from_household_occupant() {
        let home =
            StrategicPlaceId::residence("lubeck", "residence-holding:1:lubeck:cheap:0").unwrap();
        let observer_frontier = frontier(1, 200);
        let owner = StrategicPresence::residence_occupancy(
            1,
            home.clone(),
            1,
            100,
            observer_frontier,
            true,
        )
        .unwrap();
        let guest =
            StrategicPresence::residence_occupancy(2, home, 1, 150, observer_frontier, true)
                .unwrap();

        assert_eq!(
            owner.basis(),
            PresenceBasis::ResidenceOccupancy(ResidencePresenceRole::OwnerOccupant)
        );
        assert_eq!(
            guest.basis(),
            PresenceBasis::ResidenceOccupancy(ResidencePresenceRole::HouseholdOccupant)
        );
        assert!(are_co_present(&owner, &guest));
    }

    #[test]
    fn schedule_suppression_and_future_occupancy_fail_closed() {
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let window = DailyPresenceWindow {
            start_minute: 480,
            end_minute: 1_020,
        };
        assert_eq!(
            StrategicPresence::scheduled_resident(
                2,
                inn,
                frontier(1, 720),
                window,
                true,
                true,
                false
            ),
            Err(PresenceError::Suppressed)
        );
        assert_eq!(
            window.remaining_minutes(1_100, false, false),
            Err(PresenceError::OutsideSchedule)
        );

        let home = StrategicPlaceId::residence("lubeck", "holding-1").unwrap();
        assert_eq!(
            StrategicPresence::residence_occupancy(2, home, 1, 800, frontier(1, 720), true),
            Err(PresenceError::FutureEvidence)
        );
    }

    #[test]
    fn daily_presence_wait_uses_the_same_wrapped_schedule_as_remaining_time() {
        let daytime = DailyPresenceWindow {
            start_minute: 240,
            end_minute: 960,
        };
        assert_eq!(daytime.minutes_until_start(77), Ok(163));
        assert_eq!(daytime.minutes_until_start(300), Ok(0));

        let overnight = DailyPresenceWindow {
            start_minute: 1_200,
            end_minute: 120,
        };
        assert_eq!(overnight.minutes_until_start(60), Ok(0));
        assert_eq!(overnight.minutes_until_start(600), Ok(600));
    }

    #[test]
    fn co_presence_rejects_different_places_and_personal_frontiers() {
        let lubeck =
            StrategicPresence::settlement_membership(1, "lubeck", frontier(1, 720)).unwrap();
        let hamburg =
            StrategicPresence::settlement_membership(2, "hamburg", frontier(1, 720)).unwrap();
        let future =
            StrategicPresence::settlement_membership(2, "lubeck", frontier(1, 721)).unwrap();
        let other_observer =
            StrategicPresence::settlement_membership(2, "lubeck", frontier(2, 720)).unwrap();

        assert!(!are_co_present(&lubeck, &hamburg));
        assert!(!are_co_present(&lubeck, &future));
        assert!(!are_co_present(&lubeck, &other_observer));
    }

    #[test]
    fn outbreak_suppression_is_projected_at_the_observer_frontier() {
        assert_eq!(
            outbreak_patient_suppression_at(100, 300, Some(250), 200, true).unwrap(),
            PresenceSuppression {
                context_suppressed: true,
                health_suppressed: true,
            }
        );
        assert_eq!(
            outbreak_patient_suppression_at(100, 300, Some(250), 260, true).unwrap(),
            PresenceSuppression {
                context_suppressed: false,
                health_suppressed: true,
            }
        );
        assert_eq!(
            outbreak_patient_suppression_at(100, 300, Some(250), 300, true).unwrap(),
            PresenceSuppression {
                context_suppressed: false,
                health_suppressed: false,
            }
        );
        assert_eq!(
            outbreak_patient_suppression_at(100, 300, None, 200, false).unwrap(),
            PresenceSuppression {
                context_suppressed: false,
                health_suppressed: true,
            }
        );
    }

    #[test]
    fn case_occupant_and_current_context_actor_are_co_present() {
        let frontier = frontier(1, 500);
        let site = StrategicPlaceId::case_site("outbreak:site:well").unwrap();
        let actor =
            StrategicPresence::case_site_occupancy(1, site.clone(), frontier, true, true).unwrap();
        let patient = StrategicPresence::case_context_membership(
            2,
            site,
            frontier,
            true,
            "membership:2",
            "membership:2",
            4,
            4,
            true,
        )
        .unwrap();
        assert!(are_co_present(&actor, &patient));
    }

    #[test]
    fn case_context_rejects_mismatch_staleness_and_malformed_identity() {
        let frontier = frontier(1, 500);
        let first = StrategicPlaceId::case_site("case:one").unwrap();
        let second = StrategicPlaceId::case_site("case:two").unwrap();
        let actor =
            StrategicPresence::case_site_occupancy(1, first.clone(), frontier, true, true).unwrap();
        let elsewhere = StrategicPresence::case_context_membership(
            2,
            second,
            frontier,
            true,
            "membership:2",
            "membership:2",
            1,
            1,
            true,
        )
        .unwrap();
        assert!(!are_co_present(&actor, &elsewhere));
        assert_eq!(
            StrategicPresence::case_context_membership(
                2,
                first.clone(),
                frontier,
                true,
                "membership:2",
                "membership:2",
                1,
                2,
                true,
            ),
            Err(PresenceError::Unavailable)
        );
        assert_eq!(
            StrategicPresence::case_context_membership(
                2,
                first,
                frontier,
                true,
                "membership:stale",
                "membership:2",
                2,
                2,
                true,
            ),
            Err(PresenceError::Unavailable)
        );
        assert!(StrategicPlaceId::case_site("bad site id").is_err());
    }
}
