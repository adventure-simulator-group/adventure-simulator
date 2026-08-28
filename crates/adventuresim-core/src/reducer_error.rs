//! Stable machine-readable reducer failure codes.
//!
//! Reducer transports still carry a string envelope, but behavior may inspect
//! only the code prefix. The detail that follows is presentation and logging
//! prose and may change freely.

use std::{fmt, str::FromStr};

const REDUCER_ERROR_PREFIX: &str = "[reducer-error:";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReducerErrorCode {
    ContractIssuerUnavailable,
    InvestigationActionStale,
    InvestigationActionUnavailable,
    InvestigationNightWindow,
    InvestigationRouteInvalid,
    JourneyDaylightWindowRequired,
    MerchantProviderUnavailable,
    VictimCohortStateChanged,
}

impl ReducerErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContractIssuerUnavailable => "contract_issuer_unavailable",
            Self::InvestigationActionStale => "investigation_action_stale",
            Self::InvestigationActionUnavailable => "investigation_action_unavailable",
            Self::InvestigationNightWindow => "investigation_night_window",
            Self::InvestigationRouteInvalid => "investigation_route_invalid",
            Self::JourneyDaylightWindowRequired => "journey_daylight_window_required",
            Self::MerchantProviderUnavailable => "merchant_provider_unavailable",
            Self::VictimCohortStateChanged => "victim_cohort_state_changed",
        }
    }
}

impl fmt::Display for ReducerErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ReducerErrorCode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "contract_issuer_unavailable" => Ok(Self::ContractIssuerUnavailable),
            "investigation_action_stale" => Ok(Self::InvestigationActionStale),
            "investigation_action_unavailable" => Ok(Self::InvestigationActionUnavailable),
            "investigation_night_window" => Ok(Self::InvestigationNightWindow),
            "investigation_route_invalid" => Ok(Self::InvestigationRouteInvalid),
            "journey_daylight_window_required" => Ok(Self::JourneyDaylightWindowRequired),
            "merchant_provider_unavailable" => Ok(Self::MerchantProviderUnavailable),
            "victim_cohort_state_changed" => Ok(Self::VictimCohortStateChanged),
            _ => Err(()),
        }
    }
}

pub fn coded_reducer_error(code: ReducerErrorCode, detail: &str) -> String {
    format!("{REDUCER_ERROR_PREFIX}{code}] {detail}")
}

pub fn parse_reducer_error(value: &str) -> Option<ReducerErrorCode> {
    let start = value.find(REDUCER_ERROR_PREFIX)? + REDUCER_ERROR_PREFIX.len();
    value[start..].split_once(']')?.0.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_without_inspecting_detail_prose() {
        let error = coded_reducer_error(
            ReducerErrorCode::MerchantProviderUnavailable,
            "This wording can change",
        );
        assert_eq!(
            parse_reducer_error(&format!("purchase failed: {error}")),
            Some(ReducerErrorCode::MerchantProviderUnavailable)
        );
        assert_eq!(parse_reducer_error("This wording can change"), None);
    }
}
