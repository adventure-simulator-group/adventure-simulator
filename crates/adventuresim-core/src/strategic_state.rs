//! Parsed domain states for flattened strategic persistence rows.
//!
//! SpacetimeDB rows remain flat for queryability. Callers should construct these
//! types at the storage boundary so contradictory option/status combinations do
//! not escape into reducer logic.

use std::fmt;

use crate::strategic_place::CaseSiteId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractState {
    Offered,
    Accepted {
        party_id: String,
        accepted_at: u64,
    },
    ReadyToReport {
        party_id: String,
        accepted_at: u64,
    },
    Paid {
        party_id: String,
        accepted_at: u64,
        paid_at: u64,
    },
    Withdrawn {
        prior_acceptance: Option<ContractAcceptance>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractAcceptance {
    pub party_id: String,
    pub accepted_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatContractStatus {
    Offered,
    Accepted,
    ReadyToReport,
    Paid,
    Withdrawn,
}

impl ContractState {
    pub fn parse(
        status: FlatContractStatus,
        party_id: Option<String>,
        accepted_at: Option<u64>,
        paid_at: Option<u64>,
    ) -> Result<Self, StateParseError> {
        match (status, party_id, accepted_at, paid_at) {
            (FlatContractStatus::Offered, None, None, None) => Ok(Self::Offered),
            (FlatContractStatus::Accepted, Some(party_id), Some(accepted_at), None) => {
                Ok(Self::Accepted {
                    party_id,
                    accepted_at,
                })
            }
            (FlatContractStatus::ReadyToReport, Some(party_id), Some(accepted_at), None) => {
                Ok(Self::ReadyToReport {
                    party_id,
                    accepted_at,
                })
            }
            (FlatContractStatus::Paid, Some(party_id), Some(accepted_at), Some(paid_at))
                if paid_at >= accepted_at =>
            {
                Ok(Self::Paid {
                    party_id,
                    accepted_at,
                    paid_at,
                })
            }
            (FlatContractStatus::Withdrawn, None, None, None) => Ok(Self::Withdrawn {
                prior_acceptance: None,
            }),
            (FlatContractStatus::Withdrawn, Some(party_id), Some(accepted_at), None) => {
                Ok(Self::Withdrawn {
                    prior_acceptance: Some(ContractAcceptance {
                        party_id,
                        accepted_at,
                    }),
                })
            }
            _ => Err(StateParseError(
                "contract status and lifecycle fields disagree",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionBinding {
    pub case_site_id: CaseSiteId,
    pub hostile_group_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostileResolution {
    Defeated,
    DrivenOff,
    Surrendered,
    Captured {
        subject_id: String,
        custody_version: CustodyVersion,
    },
    CaptureTargetKilled {
        subject_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustodyVersion(u32);

impl CustodyVersion {
    pub fn new(version: u32) -> Self {
        Self(version)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissionAttemptState {
    Bound {
        binding: Option<MissionBinding>,
    },
    Committed {
        binding: Option<MissionBinding>,
        resolution: HostileResolution,
    },
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatMissionStatus {
    Bound,
    Committed,
    Failed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatResolution {
    Defeated,
    DrivenOff,
    Surrendered,
    Captured,
    CaptureTargetKilled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatMissionState {
    pub status: FlatMissionStatus,
    pub case_site_id: Option<CaseSiteId>,
    pub hostile_group_id: Option<String>,
    pub resolution: Option<FlatResolution>,
    pub subject_id: Option<String>,
    pub custody_version: Option<u32>,
}

impl MissionAttemptState {
    pub fn parse(flat: FlatMissionState) -> Result<Self, StateParseError> {
        let FlatMissionState {
            status,
            case_site_id,
            hostile_group_id,
            resolution,
            subject_id,
            custody_version,
        } = flat;
        let binding = match (case_site_id, hostile_group_id) {
            (None, None) => None,
            (Some(case_site_id), Some(hostile_group_id)) => Some(MissionBinding {
                case_site_id,
                hostile_group_id,
            }),
            _ => {
                return Err(StateParseError(
                    "mission binding must contain both identifiers",
                ));
            }
        };
        match (status, resolution, subject_id, custody_version) {
            (FlatMissionStatus::Bound, None, None, None) => Ok(Self::Bound { binding }),
            (FlatMissionStatus::Committed, Some(FlatResolution::Defeated), None, None) => {
                Ok(Self::Committed {
                    binding,
                    resolution: HostileResolution::Defeated,
                })
            }
            (FlatMissionStatus::Committed, Some(FlatResolution::DrivenOff), None, None) => {
                Ok(Self::Committed {
                    binding,
                    resolution: HostileResolution::DrivenOff,
                })
            }
            (FlatMissionStatus::Committed, Some(FlatResolution::Surrendered), None, None) => {
                Ok(Self::Committed {
                    binding,
                    resolution: HostileResolution::Surrendered,
                })
            }
            (
                FlatMissionStatus::Committed,
                Some(FlatResolution::Captured),
                Some(subject_id),
                Some(version),
            ) => Ok(Self::Committed {
                binding,
                resolution: HostileResolution::Captured {
                    subject_id,
                    custody_version: CustodyVersion::new(version),
                },
            }),
            (
                FlatMissionStatus::Committed,
                Some(FlatResolution::CaptureTargetKilled),
                Some(subject_id),
                None,
            ) => Ok(Self::Committed {
                binding,
                resolution: HostileResolution::CaptureTargetKilled { subject_id },
            }),
            (FlatMissionStatus::Failed, None, None, None) => Ok(Self::Failed),
            (FlatMissionStatus::Cancelled, None, None, None) => Ok(Self::Cancelled),
            _ => Err(StateParseError(
                "mission status and resolution fields disagree",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateParseError(pub &'static str);
impl fmt::Display for StateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for StateParseError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCommitmentStatus {
    Reserved,
    Fulfilled,
    Cancelled,
    Expired,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCommitmentReason {
    WeddingCompleted,
    ParticipantDead,
    ParticipantUnderage,
    ResidenceUnavailable,
    CeremonyLocationUnavailable,
    CancelledByParticipant,
    ReservationExpired,
    MarriageEnded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentState {
    Reserved {
        effective_minute: u64,
    },
    Fulfilled {
        resolved_minute: u64,
    },
    Cancelled {
        resolved_minute: u64,
        reason: FlatCommitmentReason,
    },
    Expired {
        resolved_minute: u64,
    },
    Ended {
        resolved_minute: u64,
    },
}

impl CommitmentState {
    pub fn parse(
        status: FlatCommitmentStatus,
        effective_minute: u64,
        resolved_minute: Option<u64>,
        reason: Option<FlatCommitmentReason>,
    ) -> Result<Self, StateParseError> {
        match (status, resolved_minute, reason) {
            (FlatCommitmentStatus::Reserved, None, None) => Ok(Self::Reserved { effective_minute }),
            (
                FlatCommitmentStatus::Fulfilled,
                Some(resolved_minute),
                Some(FlatCommitmentReason::WeddingCompleted),
            ) => Ok(Self::Fulfilled { resolved_minute }),
            (
                FlatCommitmentStatus::Cancelled,
                Some(resolved_minute),
                Some(
                    reason @ (FlatCommitmentReason::ParticipantDead
                    | FlatCommitmentReason::ParticipantUnderage
                    | FlatCommitmentReason::ResidenceUnavailable
                    | FlatCommitmentReason::CeremonyLocationUnavailable
                    | FlatCommitmentReason::CancelledByParticipant),
                ),
            ) => Ok(Self::Cancelled {
                resolved_minute,
                reason,
            }),
            (
                FlatCommitmentStatus::Expired,
                Some(resolved_minute),
                Some(FlatCommitmentReason::ReservationExpired),
            ) => Ok(Self::Expired { resolved_minute }),
            (
                FlatCommitmentStatus::Ended,
                Some(resolved_minute),
                Some(FlatCommitmentReason::MarriageEnded),
            ) => Ok(Self::Ended { resolved_minute }),
            _ => Err(StateParseError(
                "commitment status and terminal fields disagree",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCourtshipKind {
    Formal,
    Informal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCourtshipStatus {
    Active,
    Exposed,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCourtshipSecrecyReason {
    FatherDisapproval,
    FormalRouteUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatCourtshipTerminalReason {
    EngagementScheduled,
    EndedByParticipant,
    PartnerUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CourtshipRoute {
    Formal {
        approved_father_id: u64,
        planned_dowry: u32,
    },
    Informal {
        secrecy_reason: FlatCourtshipSecrecyReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtshipState {
    Active,
    Exposed,
    Ended {
        resolved_minute: u64,
        reason: FlatCourtshipTerminalReason,
    },
}

pub fn parse_courtship(
    kind: FlatCourtshipKind,
    secrecy_reason: Option<FlatCourtshipSecrecyReason>,
    approved_father_id: Option<u64>,
    planned_dowry: u32,
    status: FlatCourtshipStatus,
    resolved_minute: Option<u64>,
    terminal_reason: Option<FlatCourtshipTerminalReason>,
) -> Result<(CourtshipRoute, CourtshipState), StateParseError> {
    let route = match (kind, secrecy_reason, approved_father_id) {
        (FlatCourtshipKind::Formal, None, Some(approved_father_id)) => CourtshipRoute::Formal {
            approved_father_id,
            planned_dowry,
        },
        (FlatCourtshipKind::Informal, Some(secrecy_reason), None) if planned_dowry == 0 => {
            CourtshipRoute::Informal { secrecy_reason }
        }
        _ => return Err(StateParseError("courtship kind and route fields disagree")),
    };
    let state = match (status, resolved_minute, terminal_reason) {
        (FlatCourtshipStatus::Active, None, None) => CourtshipState::Active,
        (FlatCourtshipStatus::Exposed, None, None) => CourtshipState::Exposed,
        (FlatCourtshipStatus::Ended, Some(resolved_minute), Some(reason)) => {
            CourtshipState::Ended {
                resolved_minute,
                reason,
            }
        }
        _ => {
            return Err(StateParseError(
                "courtship status and terminal fields disagree",
            ));
        }
    };
    Ok((route, state))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatMarriageStatus {
    Active,
    Widowed,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarriageState {
    Active,
    Widowed { resolved_minute: u64 },
    Ended { resolved_minute: u64 },
}

impl MarriageState {
    pub fn parse(
        status: FlatMarriageStatus,
        resolved_minute: Option<u64>,
    ) -> Result<Self, StateParseError> {
        match (status, resolved_minute) {
            (FlatMarriageStatus::Active, None) => Ok(Self::Active),
            (FlatMarriageStatus::Widowed, Some(resolved_minute)) => {
                Ok(Self::Widowed { resolved_minute })
            }
            (FlatMarriageStatus::Ended, Some(resolved_minute)) => {
                Ok(Self::Ended { resolved_minute })
            }
            _ => Err(StateParseError(
                "marriage status and terminal minute disagree",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatPregnancyStatus {
    Active,
    Born,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PregnancyState {
    Active,
    Born { child_id: u64, resolved_minute: u64 },
    Ended { resolved_minute: u64 },
}

impl PregnancyState {
    pub fn parse(
        status: FlatPregnancyStatus,
        child_id: Option<u64>,
        resolved_minute: Option<u64>,
    ) -> Result<Self, StateParseError> {
        match (status, child_id, resolved_minute) {
            (FlatPregnancyStatus::Active, None, None) => Ok(Self::Active),
            (FlatPregnancyStatus::Born, Some(child_id), Some(resolved_minute)) => Ok(Self::Born {
                child_id,
                resolved_minute,
            }),
            (FlatPregnancyStatus::Ended, None, Some(resolved_minute)) => {
                Ok(Self::Ended { resolved_minute })
            }
            _ => Err(StateParseError(
                "pregnancy status and outcome fields disagree",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract_rejects_partial_acceptance_and_backwards_payment() {
        assert!(
            ContractState::parse(FlatContractStatus::Accepted, Some("p".into()), None, None)
                .is_err()
        );
        assert!(
            ContractState::parse(
                FlatContractStatus::Paid,
                Some("p".into()),
                Some(10),
                Some(9)
            )
            .is_err()
        );
    }
    #[test]
    fn mission_requires_complete_binding_and_capture_payload() {
        assert!(
            MissionAttemptState::parse(FlatMissionState {
                status: FlatMissionStatus::Bound,
                case_site_id: Some(CaseSiteId::from("s".to_owned())),
                hostile_group_id: None,
                resolution: None,
                subject_id: None,
                custody_version: None
            })
            .is_err()
        );
        assert!(
            MissionAttemptState::parse(FlatMissionState {
                status: FlatMissionStatus::Committed,
                case_site_id: Some(CaseSiteId::from("s".to_owned())),
                hostile_group_id: Some("h".into()),
                resolution: Some(FlatResolution::Captured),
                subject_id: Some("target".into()),
                custody_version: None
            })
            .is_err()
        );
        assert!(
            MissionAttemptState::parse(FlatMissionState {
                status: FlatMissionStatus::Committed,
                case_site_id: Some(CaseSiteId::from("s".to_owned())),
                hostile_group_id: Some("h".into()),
                resolution: Some(FlatResolution::Captured),
                subject_id: Some("target".into()),
                custody_version: Some(0)
            })
            .is_ok()
        );
    }
    #[test]
    fn commitment_terminal_fields_are_all_or_nothing() {
        assert!(
            CommitmentState::parse(FlatCommitmentStatus::Reserved, 10, Some(11), None).is_err()
        );
        assert!(
            CommitmentState::parse(
                FlatCommitmentStatus::Cancelled,
                10,
                Some(11),
                Some(FlatCommitmentReason::CancelledByParticipant)
            )
            .is_ok()
        );
        assert!(
            CommitmentState::parse(
                FlatCommitmentStatus::Fulfilled,
                10,
                Some(11),
                Some(FlatCommitmentReason::ParticipantDead)
            )
            .is_err()
        );
    }

    #[test]
    fn relationship_states_reject_partial_routes_and_outcomes() {
        assert!(
            parse_courtship(
                FlatCourtshipKind::Formal,
                None,
                None,
                10,
                FlatCourtshipStatus::Active,
                None,
                None
            )
            .is_err()
        );
        assert!(MarriageState::parse(FlatMarriageStatus::Active, Some(1)).is_err());
        assert!(PregnancyState::parse(FlatPregnancyStatus::Born, None, Some(10)).is_err());
        assert!(PregnancyState::parse(FlatPregnancyStatus::Active, None, None).is_ok());
    }
}
