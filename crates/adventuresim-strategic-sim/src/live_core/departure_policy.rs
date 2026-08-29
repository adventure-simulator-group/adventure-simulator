//! Departure readiness, safe-wait selection, and provisioning deferrals.

use super::*;

pub(super) const TRAVEL_PROVISION_RESERVE_DAYS: f32 = 1.0;
pub(super) const MAX_TRAVEL_PROVISION_UNITS_PER_ITEM: u32 = 512;
/// Public fail-safe bound for a disclosed one-way distance. The ordinary
/// daylight projection covers schedule downtime; four times that projection
/// covers fatigue-expanded outbound travel plus the return leg. The separate
/// reserve day remains available for delays and encounters.
pub(super) const JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR: u64 = 4;
/// Keep a material movement margin rather than departing at the point where
/// the authoritative linear encumbrance rule reaches zero.
pub(super) const MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS: u32 = 2_000;
pub(super) const MAX_DEPARTURE_WETNESS_BPS: u16 = 8_000;
pub(super) const MAX_DEPARTURE_ABS_THERMAL_STRAIN: u32 = 2_500;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DepartureDeferralReason {
    AmmunitionProviderProjectionUnavailable,
    AmmunitionUnaffordable,
    AmmunitionWouldOverload,
    EquipmentNotReady,
    PartyLoadUnsafe,
    PartyTentQuoteUnavailable,
    PartyTentUnaffordable,
    PartyTentWouldOverload,
    RouteActionNotSurvivable,
    RouteActionSiteMismatch,
    RouteFatigueRecoveryRequired,
    RouteThermalRisk,
    RouteThermalUnsafeAllPublicWindows,
    RouteWeatherProjectionUnavailable,
    SafePublicRouteWindow,
    WaitTowardSafePublicRouteWindow,
    SurvivalProjectionUnavailable,
    SurvivalReadinessRequiresSettlement,
    ThermalRecoveryRequired,
}

impl DepartureDeferralReason {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::AmmunitionProviderProjectionUnavailable => {
                "ammunition_provider_projection_unavailable"
            }
            Self::AmmunitionUnaffordable => "ammunition_unaffordable",
            Self::AmmunitionWouldOverload => "ammunition_would_overload",
            Self::EquipmentNotReady => "equipment_not_ready",
            Self::PartyLoadUnsafe => "party_load_unsafe",
            Self::PartyTentQuoteUnavailable => "party_tent_quote_unavailable",
            Self::PartyTentUnaffordable => "party_tent_unaffordable",
            Self::PartyTentWouldOverload => "party_tent_would_overload",
            Self::RouteActionNotSurvivable => "route_action_not_survivable",
            Self::RouteActionSiteMismatch => "route_action_site_mismatch",
            Self::RouteFatigueRecoveryRequired => "route_fatigue_recovery_required",
            Self::RouteThermalRisk => "route_thermal_risk",
            Self::RouteThermalUnsafeAllPublicWindows => "route_thermal_unsafe_all_public_windows",
            Self::RouteWeatherProjectionUnavailable => "route_weather_projection_unavailable",
            Self::SafePublicRouteWindow => "safe_public_route_window",
            Self::WaitTowardSafePublicRouteWindow => "wait_toward_safe_public_route_window",
            Self::SurvivalProjectionUnavailable => "survival_projection_unavailable",
            Self::SurvivalReadinessRequiresSettlement => "survival_readiness_requires_settlement",
            Self::ThermalRecoveryRequired => "thermal_recovery_required",
        }
    }
}

impl std::fmt::Display for DepartureDeferralReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DepartureReadiness {
    Ready,
    ReadyWithItinerary {
        walking_minutes_per_day: u16,
        travel_at_night: bool,
        case_site_recovery_minutes: u64,
    },
    WaitForSafeDeparture {
        reason: DepartureDeferralReason,
        wait_minutes: u64,
        walking_minutes_per_day: u16,
        travel_at_night: bool,
        case_site_recovery_minutes: u64,
    },
    Deferred(DepartureDeferralReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SettlementDepartureWait<'a> {
    pub(super) character_id: u64,
    pub(super) agent: u32,
    pub(super) case_id: &'a str,
    pub(super) reason: DepartureDeferralReason,
    pub(super) wait_minutes: u64,
    pub(super) walking_minutes_per_day: u16,
    pub(super) travel_at_night: bool,
}

pub(super) fn safe_departure_wait_minutes(
    immediate_safe: bool,
    delayed_safe: bool,
    wait_minutes: Option<u64>,
) -> Option<u64> {
    (!immediate_safe && delayed_safe)
        .then_some(wait_minutes?)
        .filter(|minutes| (60..=MINUTES_PER_DAY).contains(minutes))
}

pub(super) const MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS: u64 = 7;
pub(super) const MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES: u64 =
    MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS * MINUTES_PER_DAY;

pub(super) fn generated_safe_departure_waits(
    starting_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Vec<u64> {
    let mut waits = (60..=MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES)
        .step_by(60)
        .filter(|wait| {
            adventuresim_core::strategic_time::is_walking_time(
                starting_minute.saturating_add(*wait),
                walking_minutes,
                travel_at_night,
            )
        })
        .collect::<Vec<_>>();
    waits.extend(generated_daily_walking_start_waits(
        starting_minute,
        walking_minutes,
        travel_at_night,
    ));
    waits.sort_unstable();
    waits.dedup();
    waits
}

pub(super) fn generated_daily_walking_start_waits(
    starting_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Vec<u64> {
    (0..=MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS)
        .filter_map(|day_offset| {
            let day_wait = day_offset * MINUTES_PER_DAY;
            adventuresim_core::strategic_time::minutes_until_next_walking_start(
                starting_minute.saturating_add(day_wait),
                walking_minutes,
                travel_at_night,
            )
            .and_then(|wait| forecast_safe_departure_wait_minutes(day_wait.saturating_add(wait)))
        })
        .collect()
}

pub(super) fn forecast_safe_departure_wait_minutes(next_walking_start: u64) -> Option<u64> {
    (next_walking_start <= MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES)
        .then_some(next_walking_start.max(60))
}

pub(super) fn representable_safe_departure_wait_minutes(next_walking_start: u64) -> Option<u64> {
    (next_walking_start <= MINUTES_PER_DAY).then_some(next_walking_start.max(60))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TravelProvisionDeferralReason {
    ContributionRevalidationFailed,
    EssentialsUnaffordable,
    EssentialsUnavailable,
    FinanceBackoff,
    PayerProviderProjectionUnavailable,
    RequiresSettlement,
}

impl TravelProvisionDeferralReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ContributionRevalidationFailed => "journey_contribution_revalidation_failed",
            Self::EssentialsUnaffordable => "journey_essentials_unaffordable",
            Self::EssentialsUnavailable => "journey_essentials_unavailable",
            Self::FinanceBackoff => "journey_finance_backoff",
            Self::PayerProviderProjectionUnavailable => {
                "journey_payer_provider_projection_unavailable"
            }
            Self::RequiresSettlement => "provisioning_requires_settlement",
        }
    }
}

impl std::fmt::Display for TravelProvisionDeferralReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TravelProvisionDecision {
    Ready,
    Deferred(TravelProvisionDeferralReason),
}
