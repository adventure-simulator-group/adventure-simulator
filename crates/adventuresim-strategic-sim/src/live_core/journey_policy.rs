//! Journey observation, camp state, elapsed time, and inventory projection.

use super::*;

pub(super) const MAX_PUBLIC_JOURNEY_DIAGNOSTIC_MINUTES: u64 = u32::MAX as u64;
pub(super) const MAX_PUBLIC_JOURNEY_DIAGNOSTIC_INTERVALS: usize = MAX_CAMPS_PER_LEG as usize;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicActiveCampObservation {
    pub(super) completed_elapsed_minutes: u64,
    pub(super) total_elapsed_minutes: u64,
    pub(super) active_interval_start: u64,
    pub(super) active_interval_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicPostEncounterJourneyState {
    pub(super) unresolved_encounter: bool,
    pub(super) active_destination: bool,
    pub(super) journey_count: usize,
    pub(super) destination_matches: bool,
    pub(super) active_interval_count: usize,
    pub(super) actionable_actor: bool,
    pub(super) unsafe_member_count: usize,
    pub(super) evacuation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PostEncounterJourneyAction {
    ReclassifyPublicState,
    HoldNoActionableActor,
    HoldForRecovery,
    HandleActiveCamp,
    ContinueTravel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicRoutePoint {
    pub(super) latitude: LatitudeMicrodegrees,
    pub(super) longitude: LongitudeMicrodegrees,
    pub(super) elevation_m: i16,
}

impl PublicRoutePoint {
    pub(super) fn from_degrees(latitude: f64, longitude: f64, elevation_m: i16) -> Option<Self> {
        Some(Self {
            latitude: LatitudeMicrodegrees::from_degrees(latitude)?,
            longitude: LongitudeMicrodegrees::from_degrees(longitude)?,
            elevation_m,
        })
    }

    pub(super) fn from_e7(latitude: i32, longitude: i32, elevation_m: i16) -> Option<Self> {
        Some(Self {
            latitude: LatitudeE7::new(latitude)?.to_microdegrees(),
            longitude: LongitudeE7::new(longitude)?.to_microdegrees(),
            elevation_m,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicJourneyCampState {
    BetweenCamps,
    ActiveCamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PostRestProgress {
    Exact { actual_rest_minutes: u64 },
    InterruptedShort { actual_rest_minutes: u64 },
    TerminalBoundary { actual_rest_minutes: u64 },
}

impl PostRestProgress {
    pub(super) fn actual_rest_minutes(self) -> u64 {
        match self {
            Self::Exact {
                actual_rest_minutes,
            }
            | Self::InterruptedShort {
                actual_rest_minutes,
            }
            | Self::TerminalBoundary {
                actual_rest_minutes,
            } => actual_rest_minutes,
        }
    }
}

pub(super) fn classify_post_rest_progress(
    before_completed_elapsed: u64,
    requested_rest_minutes: u64,
    after_completed_elapsed: u64,
    after_total_elapsed: u64,
    interrupted: bool,
    terminal_state_change: bool,
) -> Result<PostRestProgress, &'static str> {
    if after_completed_elapsed > after_total_elapsed {
        return Err("post_rest_completed_after_total");
    }
    let actual_rest_minutes = after_completed_elapsed
        .checked_sub(before_completed_elapsed)
        .ok_or("post_rest_progress_regressed")?;
    if actual_rest_minutes > requested_rest_minutes {
        return Err("post_rest_overshot_request");
    }
    if terminal_state_change {
        return Ok(PostRestProgress::TerminalBoundary {
            actual_rest_minutes,
        });
    }
    if actual_rest_minutes == 0 {
        return Err("post_rest_zero_progress");
    }
    if actual_rest_minutes < requested_rest_minutes {
        return if interrupted {
            Ok(PostRestProgress::InterruptedShort {
                actual_rest_minutes,
            })
        } else {
            Err("post_rest_short_without_interruption")
        };
    }
    Ok(PostRestProgress::Exact {
        actual_rest_minutes,
    })
}

pub(super) fn public_alive_to_dead_ids(before: &[(u64, bool)], after: &[(u64, bool)]) -> Vec<u64> {
    let mut deaths = before
        .iter()
        .filter(|(_, alive)| *alive)
        .filter_map(|(character_id, _)| {
            after
                .iter()
                .find(|(after_id, _)| after_id == character_id)
                .is_some_and(|(_, alive)| !*alive)
                .then_some(*character_id)
        })
        .collect::<Vec<_>>();
    deaths.sort_unstable();
    deaths
}

pub(super) fn public_terminal_rest_elapsed(
    terminal_ids: &[u64],
    before: &[(u64, u64)],
    after: &[(u64, u64)],
) -> Option<u64> {
    terminal_ids
        .iter()
        .map(|character_id| {
            let before_elapsed = before
                .iter()
                .find(|(before_id, _)| before_id == character_id)?
                .1;
            let after_elapsed = after
                .iter()
                .find(|(after_id, _)| after_id == character_id)?
                .1;
            after_elapsed.checked_sub(before_elapsed)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()
}

pub(super) fn classify_public_journey_camp_state(
    active_interval_count: usize,
) -> Result<PublicJourneyCampState, &'static str> {
    match active_interval_count {
        0 => Ok(PublicJourneyCampState::BetweenCamps),
        1 => Ok(PublicJourneyCampState::ActiveCamp),
        _ => Err("overlapping_active_public_camps"),
    }
}

pub(super) fn classify_post_encounter_journey(
    state: PublicPostEncounterJourneyState,
) -> Result<PostEncounterJourneyAction, &'static str> {
    if state.unresolved_encounter || !state.active_destination {
        return Ok(PostEncounterJourneyAction::ReclassifyPublicState);
    }
    if state.journey_count != 1 || !state.destination_matches {
        return Err("post_encounter_journey_projection_mismatch");
    }
    if !state.actionable_actor {
        return Ok(PostEncounterJourneyAction::HoldNoActionableActor);
    }
    if state.unsafe_member_count > 0 && !state.evacuation {
        return Ok(PostEncounterJourneyAction::HoldForRecovery);
    }
    match state.active_interval_count {
        0 => Ok(PostEncounterJourneyAction::ContinueTravel),
        1 => Ok(PostEncounterJourneyAction::HandleActiveCamp),
        _ => Err("post_encounter_overlapping_active_camps"),
    }
}

pub(super) fn simulation_elapsed_minutes(starting_minute: u64, current_minute: u64) -> u64 {
    current_minute.saturating_sub(starting_minute)
}

pub(super) fn public_effective_inventory_quantity(
    quantity: u32,
    fraction_micros: Option<u32>,
) -> f32 {
    fraction_micros.map_or(quantity as f32, |value| {
        adventuresim_core::inventory_measurement::ConsumableFractionMicros::try_new(value)
            .expect("public consumable fraction must not exceed one whole")
            .as_unit_f32()
    })
}
