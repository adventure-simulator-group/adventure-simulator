//! Generated-case selection, action scoring, and investigation replanning.

use super::*;

pub(super) const MAX_GENERATED_CASE_STEPS_PER_CYCLE: u32 = 16;
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SelectedCaseSitePlan {
    pub(super) walking_minutes_per_day: u16,
    pub(super) travel_at_night: bool,
    pub(super) departure_wait_minutes: u64,
    pub(super) outbound: adventuresim_core::strategic_time::ItineraryForecast,
    pub(super) returned: adventuresim_core::strategic_time::ItineraryForecast,
    pub(super) minimum_insulation_bps: u16,
    pub(super) case_site_recovery_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OnSiteActionDecision {
    Ready,
    RestThenRetry(u64),
    ReturnNow,
    Hold,
}

pub(super) fn classify_on_site_action_decision(
    action_return_safe: bool,
    rest_action_return_safe: bool,
    recovery_minutes: u64,
    return_now_safe: bool,
) -> OnSiteActionDecision {
    if action_return_safe {
        OnSiteActionDecision::Ready
    } else if rest_action_return_safe && (1..=MINUTES_PER_DAY).contains(&recovery_minutes) {
        OnSiteActionDecision::RestThenRetry(recovery_minutes)
    } else if return_now_safe {
        OnSiteActionDecision::ReturnNow
    } else {
        OnSiteActionDecision::Hold
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedAdvanceResult {
    Progressed,
    RecoveryCommitted,
    NoProgress,
}

pub(super) fn classify_generated_advance(
    public_progressed: bool,
    elapsed_advanced: bool,
) -> GeneratedAdvanceResult {
    if public_progressed {
        GeneratedAdvanceResult::Progressed
    } else if elapsed_advanced {
        GeneratedAdvanceResult::RecoveryCommitted
    } else {
        GeneratedAdvanceResult::NoProgress
    }
}

pub(super) fn calories_after_strenuous_action(calories_used: f32, action_minutes: u64) -> f32 {
    calories_used
        + action_minutes as f32 / MINUTES_PER_DAY as f32
            * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
}

pub(super) fn projected_action_ready(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> bool {
    projected_action_status(
        nonfatigue_incapacitation,
        calories_after_action,
        fatigue_capacity,
    ) == adventuresim_core::morale::IncapacitationStatus::Ready
}

pub(super) fn projected_action_survivable(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> bool {
    projected_action_status(
        nonfatigue_incapacitation,
        calories_after_action,
        fatigue_capacity,
    ) != adventuresim_core::morale::IncapacitationStatus::Incapacitated
}

pub(super) fn projected_itinerary_survivable(
    nonfatigue_incapacitation: f32,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    member_index: usize,
    fatigue_capacity: f32,
) -> bool {
    itinerary
        .member_maximum_fatigue
        .get(member_index)
        .is_some_and(|fatigue| {
            projected_action_survivable(
                nonfatigue_incapacitation,
                fatigue * fatigue_capacity,
                fatigue_capacity,
            )
        })
}

pub(super) fn projected_action_status(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> adventuresim_core::morale::IncapacitationStatus {
    adventuresim_core::morale::StrategicIncapacitation {
        pain: nonfatigue_incapacitation.max(0.0),
        fatigue: adventuresim_core::morale::fatigue_incapacitation(
            calories_after_action / fatigue_capacity.max(0.01),
        ),
        ..Default::default()
    }
    .status()
}

pub(super) fn round_trip_walking_window_minutes(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
) -> Option<u16> {
    let required = movement_minutes
        .checked_mul(2)?
        .checked_add(action_minutes)?;
    let required_breakpoint = required.checked_add(59)?.checked_div(60)?.checked_mul(60)?;
    u16::try_from(required_breakpoint.max(u64::from(current_walking_minutes)))
        .ok()
        .filter(|minutes| u64::from(*minutes) <= MINUTES_PER_DAY)
}

pub(super) fn generated_action_walking_windows(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
) -> Vec<u16> {
    let mut windows = Vec::new();
    let mut push = |minutes: u64| {
        if let Ok(minutes) = u16::try_from(minutes)
            && (adventuresim_core::strategic_time::MIN_WALKING_MINUTES_PER_DAY
                ..=adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY)
                .contains(&minutes)
            && !windows.contains(&minutes)
        {
            windows.push(minutes);
        }
    };
    push(u64::from(current_walking_minutes));
    if let Some(widened) =
        round_trip_walking_window_minutes(current_walking_minutes, movement_minutes, action_minutes)
    {
        push(u64::from(widened));
    }
    let exact_action_breakpoint = movement_minutes.saturating_add(action_minutes);
    push(exact_action_breakpoint);
    push(movement_minutes);
    let descent_start = exact_action_breakpoint.min(u64::from(
        adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY,
    ));
    for minutes in (u64::from(adventuresim_core::strategic_time::MIN_WALKING_MINUTES_PER_DAY)
        ..descent_start)
        .rev()
    {
        push(minutes);
    }
    windows
}

pub(super) fn select_generated_case_site_plan<T>(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
    current_travel_at_night: bool,
    starting_minute: u64,
    mut evaluate: impl FnMut(u16, bool, u64) -> Option<T>,
) -> Option<T> {
    let windows =
        generated_action_walking_windows(current_walking_minutes, movement_minutes, action_minutes);
    for travel_at_night in [current_travel_at_night, !current_travel_at_night] {
        for (window_index, &walking_minutes) in windows.iter().enumerate() {
            if adventuresim_core::strategic_time::is_walking_time(
                starting_minute,
                walking_minutes,
                travel_at_night,
            ) && let Some(plan) = evaluate(walking_minutes, travel_at_night, 0)
            {
                return Some(plan);
            }
            let candidate_waits = if window_index < 4 {
                generated_safe_departure_waits(starting_minute, walking_minutes, travel_at_night)
            } else {
                generated_daily_walking_start_waits(
                    starting_minute,
                    walking_minutes,
                    travel_at_night,
                )
            };
            for wait_minutes in candidate_waits {
                if let Some(plan) = evaluate(walking_minutes, travel_at_night, wait_minutes) {
                    return Some(plan);
                }
            }
        }
    }
    None
}

pub(super) fn joint_case_site_plan_failure_reason(
    complete_candidate_count: u32,
    thermally_safe_complete_candidate_count: u32,
    candidate_projection_unavailable: bool,
    candidate_fatigue_unsafe: bool,
    candidate_site_mismatch: bool,
) -> DepartureDeferralReason {
    if complete_candidate_count > 0 && thermally_safe_complete_candidate_count == 0 {
        DepartureDeferralReason::RouteThermalUnsafeAllPublicWindows
    } else if thermally_safe_complete_candidate_count > 0 && candidate_fatigue_unsafe {
        DepartureDeferralReason::RouteFatigueRecoveryRequired
    } else if candidate_site_mismatch {
        DepartureDeferralReason::RouteActionSiteMismatch
    } else if complete_candidate_count > 0 || !candidate_projection_unavailable {
        DepartureDeferralReason::RouteActionNotSurvivable
    } else {
        DepartureDeferralReason::RouteWeatherProjectionUnavailable
    }
}

pub(super) fn generated_action_score(
    profile: &AgentProfile,
    action: &BackendInvestigationAction,
) -> (u8, u32, u16, u32, u32) {
    let progress = match projected_investigation_action_state(&action.availability) {
        ProjectedInvestigationActionState::Available => 3,
        ProjectedInvestigationActionState::Travel => 2,
        ProjectedInvestigationActionState::Wait(_) => 1,
        ProjectedInvestigationActionState::Blocked => 0,
    };
    let wait_minutes = match &action.availability {
        InvestigationActionAvailability::Available => 0,
        InvestigationActionAvailability::Unavailable(unavailable) => unavailable.wait_minutes,
    };
    (
        progress,
        generated_method_skill_fit(profile, &action.method),
        10_000_u16.saturating_sub(action.uncertainty_bps),
        u32::MAX.saturating_sub(action.duration_max_minutes),
        u32::MAX.saturating_sub(wait_minutes),
    )
}

pub(super) fn sort_generated_actions(
    profile: &AgentProfile,
    actions: &mut [BackendInvestigationAction],
) {
    actions.sort_by(|left, right| {
        generated_action_score(profile, right)
            .cmp(&generated_action_score(profile, left))
            .then_with(|| left.action_id.cmp(&right.action_id))
    });
}

pub(super) fn role_rank(role: BuildRole) -> u8 {
    match role {
        BuildRole::FrontLine => 0,
        BuildRole::Skirmisher => 1,
        BuildRole::Ranged => 2,
        BuildRole::Healer => 3,
        BuildRole::Devout => 4,
        BuildRole::Civilian => 5,
    }
}

pub(crate) fn balanced_party_groups(
    profiles: &[AgentProfile],
    party_size: usize,
) -> Vec<Vec<usize>> {
    let group_count = profiles.len().div_ceil(party_size);
    if group_count == 0 {
        return Vec::new();
    }
    let mut targets = vec![profiles.len() / group_count; group_count];
    for target in targets.iter_mut().take(profiles.len() % group_count) {
        *target += 1;
    }
    let mut order = (0..profiles.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        (
            role_rank(profiles[index].build.role),
            profiles[index].agent_id,
        )
    });
    let mut groups = vec![Vec::new(); group_count];
    let mut cursor = 0;
    for index in order {
        let group = (0..group_count)
            .map(|offset| (cursor + offset) % group_count)
            .find(|&group| groups[group].len() < targets[group])
            .expect("party target capacity covers every profile");
        groups[group].push(index);
        cursor = (group + 1) % group_count;
    }
    for group in &mut groups {
        group.sort_by_key(|&index| {
            (
                profiles[index].build.activity_only,
                profiles[index].agent_id,
            )
        });
    }
    groups
}

pub(super) fn projected_case_row_matches(
    owner_character_id: u64,
    selected_case_id: &str,
    row_owner_character_id: u64,
    row_public_case_id: &str,
) -> bool {
    row_owner_character_id == owner_character_id && row_public_case_id == selected_case_id
}

pub(super) fn occupied_case_pin_matches(
    owner_character_id: u64,
    selected_case_id: &str,
    occupied_site_id: &CaseSiteId,
    pin_owner_character_id: u64,
    pin_public_case_id: &str,
    pin_site_id: &CaseSiteId,
) -> bool {
    projected_case_row_matches(
        owner_character_id,
        selected_case_id,
        pin_owner_character_id,
        pin_public_case_id,
    ) && pin_site_id == occupied_site_id
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectedInvestigationActionState {
    Available,
    Travel,
    Wait(u32),
    Blocked,
}

pub(super) fn projected_investigation_action_state(
    availability: &InvestigationActionAvailability,
) -> ProjectedInvestigationActionState {
    match availability {
        InvestigationActionAvailability::Available => ProjectedInvestigationActionState::Available,
        InvestigationActionAvailability::Unavailable(unavailable)
            if unavailable.can_travel_to_required_site =>
        {
            ProjectedInvestigationActionState::Travel
        }
        InvestigationActionAvailability::Unavailable(unavailable) => {
            projected_investigation_wait_minutes(unavailable.reason, unavailable.wait_minutes)
                .map_or(ProjectedInvestigationActionState::Blocked, |minutes| {
                    ProjectedInvestigationActionState::Wait(minutes)
                })
        }
    }
}

pub(super) fn projected_investigation_wait_minutes(
    reason: InvestigationActionUnavailableReason,
    wait_minutes: u32,
) -> Option<u32> {
    match reason {
        InvestigationActionUnavailableReason::NightWindow
        | InvestigationActionUnavailableReason::ContactScheduleWindow => (1
            ..=MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES)
            .contains(&wait_minutes)
            .then_some(wait_minutes),
        InvestigationActionUnavailableReason::PartyNotReady
        | InvestigationActionUnavailableReason::TravelRequired
        | InvestigationActionUnavailableReason::TargetChanged
        | InvestigationActionUnavailableReason::ContactNotPresent
        | InvestigationActionUnavailableReason::CharacterUnavailable
        | InvestigationActionUnavailableReason::PartyRequired => None,
    }
}

pub(super) fn current_contact_schedule_wait_minutes(
    action: &BackendInvestigationAction,
    presences: impl IntoIterator<Item = SettlementResidentPresence>,
    actor_minute: u64,
) -> Option<u32> {
    let contact_character_id = action.contact_character_id?;
    let presence = presences
        .into_iter()
        .find(|presence| presence.character_id == contact_character_id)?;
    if presence.context_suppressed || presence.health_suppressed {
        return None;
    }
    DailyPresenceWindow {
        start_minute: presence.start_minute,
        end_minute: presence.end_minute,
    }
    .minutes_until_start(actor_minute)
    .ok()
}

pub(super) fn investigation_unavailable_reason_key(
    reason: InvestigationActionUnavailableReason,
) -> &'static str {
    match reason {
        InvestigationActionUnavailableReason::PartyNotReady => "party_not_ready",
        InvestigationActionUnavailableReason::TravelRequired => "travel_required",
        InvestigationActionUnavailableReason::NightWindow => "night_window",
        InvestigationActionUnavailableReason::TargetChanged => "target_changed",
        InvestigationActionUnavailableReason::ContactScheduleWindow => "contact_schedule_window",
        InvestigationActionUnavailableReason::ContactNotPresent => "contact_not_present",
        InvestigationActionUnavailableReason::CharacterUnavailable => "character_unavailable",
        InvestigationActionUnavailableReason::PartyRequired => "party_required",
    }
}

pub(super) fn dialogue_contact_presence_changed(error: &CoreLoopError) -> bool {
    error.reducer_code() == Some(ReducerErrorCode::DialogueContactUnavailable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InvestigationActionReplanReason {
    Unavailable,
    Stale,
}

impl InvestigationActionReplanReason {
    pub(super) const fn stable_id(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
        }
    }
}

pub(super) fn investigation_action_replan_reason(
    error: &CoreLoopError,
) -> Option<InvestigationActionReplanReason> {
    match error.reducer_code()? {
        ReducerErrorCode::InvestigationActionUnavailable => {
            Some(InvestigationActionReplanReason::Unavailable)
        }
        ReducerErrorCode::InvestigationActionStale => Some(InvestigationActionReplanReason::Stale),
        _ => None,
    }
}

pub(super) fn projected_case_site_journey_minutes(
    distance_m: u64,
    walking_minutes_per_day: u16,
) -> Option<u64> {
    if distance_m == 0
        || walking_minutes_per_day == 0
        || u64::from(walking_minutes_per_day) > MINUTES_PER_DAY
    {
        return None;
    }
    let movement_minutes = case_site_movement_minutes(distance_m)?;
    let walking_minutes = u64::from(walking_minutes_per_day);
    let completed_walking_days = movement_minutes.saturating_sub(1) / walking_minutes;
    Some(
        movement_minutes
            .saturating_add(
                completed_walking_days
                    .saturating_mul(MINUTES_PER_DAY.saturating_sub(walking_minutes)),
            )
            .saturating_mul(JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR),
    )
}

pub(super) fn projected_camp_rest_minutes(
    completed_elapsed_minutes: u64,
    total_elapsed_minutes: u64,
    intervals: &[JourneyCampInterval],
) -> Option<(u64, u64)> {
    if completed_elapsed_minutes >= total_elapsed_minutes {
        return None;
    }
    let mut active = intervals.iter().filter_map(|camp| {
        let camp_start = camp.elapsed_start_minute.max(completed_elapsed_minutes);
        let camp_end = camp
            .elapsed_start_minute
            .saturating_add(camp.elapsed_minutes)
            .min(total_elapsed_minutes);
        (camp.elapsed_start_minute <= completed_elapsed_minutes && camp_end > camp_start)
            .then(|| (camp_start, camp_end - camp_start))
    });
    let result = active.next()?;
    active.next().is_none().then_some(result)
}

pub(super) fn projected_active_camp_interval_count(
    completed_elapsed_minutes: u64,
    total_elapsed_minutes: u64,
    intervals: &[JourneyCampInterval],
) -> usize {
    if completed_elapsed_minutes >= total_elapsed_minutes {
        return 0;
    }
    intervals
        .iter()
        .filter(|camp| {
            let camp_start = camp.elapsed_start_minute.max(completed_elapsed_minutes);
            let camp_end = camp
                .elapsed_start_minute
                .saturating_add(camp.elapsed_minutes)
                .min(total_elapsed_minutes);
            camp.elapsed_start_minute <= completed_elapsed_minutes && camp_end > camp_start
        })
        .count()
}

pub(super) fn bounded_public_journey_diagnostic(value: u64) -> u64 {
    value.min(MAX_PUBLIC_JOURNEY_DIAGNOSTIC_MINUTES)
}

pub(super) fn bounded_public_forecast_count(value: usize) -> usize {
    value.min(MAX_PUBLIC_JOURNEY_DIAGNOSTIC_INTERVALS)
}
