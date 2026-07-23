//! Closed set of party commands that can be queued for leader approval.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::spacetimedb::RecruitmentRequirements;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum PartyAction {
    TravelToSettlement {
        settlement_id: String,
    },
    TravelToCaseSite {
        case_site_id: String,
    },
    RemovePartyMember {
        character_id: u64,
    },
    CreateRecruitmentRole {
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
        save_role: bool,
    },
    UpdateRecruitmentRole {
        role_id: u64,
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
    },
    DeleteRecruitmentRole {
        role_id: u64,
    },
    AcceptJoinRequest {
        request_id: u64,
    },
    RejectJoinRequest {
        request_id: u64,
    },
    AcceptContract {
        contract_id: String,
    },
    AbandonContract {
        contract_id: String,
    },
    ReportContract {
        contract_id: String,
    },
    AutoresolveMission {
        mission_id: String,
    },
    UpdatePartyCheckTargets {
        medicine: f32,
        command: f32,
        religion: f32,
    },
    SetInventoryQuantityTarget {
        item_id: String,
        quantity: u32,
    },
    DisbandParty {
        party_id: String,
    },
    RequestTacticalServer {
        mission_id: String,
        scene_key: String,
    },
    CancelMission {
        mission_id: String,
    },
    PerformInvestigation {
        action_id: String,
        method: String,
        expected_version: u32,
    },
}

impl PartyAction {
    pub(super) fn requires_ready_party(&self) -> bool {
        matches!(
            self,
            Self::TravelToCaseSite { .. }
                | Self::AutoresolveMission { .. }
                | Self::RequestTacticalServer { .. }
                | Self::PerformInvestigation { .. }
        )
    }

    pub(super) fn kind(&self) -> String {
        match self {
            Self::TravelToSettlement { .. } | Self::TravelToCaseSite { .. } => "travel".into(),
            Self::RemovePartyMember { .. } => "kick".into(),
            Self::CreateRecruitmentRole { .. } => "add_role".into(),
            Self::UpdateRecruitmentRole { .. } => "edit_role".into(),
            Self::DeleteRecruitmentRole { .. } => "delete_role".into(),
            Self::AcceptJoinRequest { .. } => "accept_join".into(),
            Self::RejectJoinRequest { .. } => "reject_join".into(),
            Self::AcceptContract { .. } => "accept_contract".into(),
            Self::AbandonContract { .. } => "abandon_contract".into(),
            Self::ReportContract { .. } => "report_contract".into(),
            Self::AutoresolveMission { .. } => "autoresolve".into(),
            Self::UpdatePartyCheckTargets { .. } => "party_checks".into(),
            Self::SetInventoryQuantityTarget { .. } => "party_inventory".into(),
            Self::DisbandParty { .. } => "disband_party".into(),
            Self::RequestTacticalServer { .. } => "initiate_combat".into(),
            Self::CancelMission { .. } => "cancel_mission".into(),
            Self::PerformInvestigation { .. } => "investigate".into(),
        }
    }

    pub(super) fn summary(&self) -> String {
        match self {
            Self::TravelToSettlement { settlement_id, .. } => {
                format!("Travel to settlement {settlement_id}")
            }
            Self::TravelToCaseSite { case_site_id, .. } => {
                format!("Travel to case site {case_site_id}")
            }
            Self::RemovePartyMember { character_id } => {
                format!("Remove party member {character_id}")
            }
            Self::CreateRecruitmentRole { name, quantity, .. } => {
                format!("Add {quantity} {name} slot(s)")
            }
            Self::UpdateRecruitmentRole { name, .. } => format!("Edit recruitment role {name}"),
            Self::DeleteRecruitmentRole { .. } => "Delete a recruitment role".into(),
            Self::AcceptJoinRequest { request_id } => format!("Accept join request {request_id}"),
            Self::RejectJoinRequest { request_id } => format!("Reject join request {request_id}"),
            Self::AcceptContract { contract_id } => format!("Accept contract {contract_id}"),
            Self::AbandonContract { contract_id } => format!("Abandon contract {contract_id}"),
            Self::ReportContract { contract_id } => format!("Report contract {contract_id}"),
            Self::AutoresolveMission { mission_id } => {
                format!("Autoresolve mission {mission_id}")
            }
            Self::UpdatePartyCheckTargets { .. } => "Change party skill targets".into(),
            Self::SetInventoryQuantityTarget { .. } => "Manage party inventory targets".into(),
            Self::DisbandParty { .. } => "Disband the party".into(),
            Self::RequestTacticalServer { .. } => "Initiate tactical combat".into(),
            Self::CancelMission { .. } => "Cancel tactical combat".into(),
            Self::PerformInvestigation { method, .. } => {
                format!("Perform investigation action: {}", method.replace('_', " "))
            }
        }
    }

    pub(super) fn reducer_call(&self, actor_id: u64) -> (&'static str, Vec<Value>) {
        match self {
            Self::TravelToSettlement { settlement_id } => (
                "travel_to_settlement",
                vec![json!(actor_id), json!(settlement_id)],
            ),
            Self::TravelToCaseSite { case_site_id } => (
                "travel_to_case_site",
                vec![json!(actor_id), json!({ "value": case_site_id })],
            ),
            Self::RemovePartyMember { character_id } => (
                "remove_party_member",
                vec![json!(actor_id), json!(character_id)],
            ),
            Self::CreateRecruitmentRole {
                name,
                quantity,
                requirements,
                weapon_precision,
                save_role,
            } => (
                "create_recruitment_role",
                vec![
                    json!(actor_id),
                    json!(name),
                    json!(quantity),
                    json!(requirements),
                    json!(weapon_precision),
                    json!(save_role),
                ],
            ),
            Self::UpdateRecruitmentRole {
                role_id,
                name,
                quantity,
                requirements,
                weapon_precision,
            } => (
                "update_recruitment_role",
                vec![
                    json!(actor_id),
                    json!(role_id),
                    json!(name),
                    json!(quantity),
                    json!(requirements),
                    json!(weapon_precision),
                ],
            ),
            Self::DeleteRecruitmentRole { role_id } => (
                "delete_recruitment_role",
                vec![json!(actor_id), json!(role_id)],
            ),
            Self::AcceptJoinRequest { request_id } => (
                "accept_party_join_request",
                vec![json!(actor_id), json!(request_id)],
            ),
            Self::RejectJoinRequest { request_id } => (
                "reject_party_join_request",
                vec![json!(actor_id), json!(request_id)],
            ),
            Self::AcceptContract { contract_id } => {
                ("accept_contract", vec![json!(actor_id), json!(contract_id)])
            }
            Self::AbandonContract { contract_id } => (
                "abandon_contract",
                vec![json!(actor_id), json!(contract_id)],
            ),
            Self::ReportContract { contract_id } => {
                ("report_contract", vec![json!(actor_id), json!(contract_id)])
            }
            Self::AutoresolveMission { mission_id } => (
                "autoresolve_mission",
                vec![json!(actor_id), json!(mission_id)],
            ),
            Self::UpdatePartyCheckTargets {
                medicine,
                command,
                religion,
            } => (
                "update_party_check_targets",
                vec![
                    json!(actor_id),
                    json!(medicine),
                    json!(command),
                    json!(religion),
                ],
            ),
            Self::SetInventoryQuantityTarget { item_id, quantity } => (
                "set_inventory_quantity_target",
                vec![
                    json!(actor_id),
                    json!(true),
                    json!(item_id),
                    json!(quantity),
                ],
            ),
            Self::DisbandParty { party_id } => {
                ("disband_party", vec![json!(actor_id), json!(party_id)])
            }
            Self::RequestTacticalServer {
                mission_id,
                scene_key,
            } => (
                "request_tactical_server",
                vec![json!(actor_id), json!(mission_id), json!(scene_key)],
            ),
            Self::CancelMission { mission_id } => (
                "cancel_mission_request",
                vec![json!(actor_id), json!(mission_id)],
            ),
            Self::PerformInvestigation {
                action_id,
                method,
                expected_version,
            } => (
                "perform_investigation_action",
                vec![
                    json!(actor_id),
                    json!(action_id),
                    json!(method),
                    json!(expected_version),
                ],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_rebinds_actor_from_the_typed_variant() {
        let action = PartyAction::TravelToCaseSite {
            case_site_id: "case-site-7".into(),
        };
        assert_eq!(
            action.reducer_call(42),
            (
                "travel_to_case_site",
                vec![json!(42), json!({ "value": "case-site-7" })]
            )
        );
    }

    #[test]
    fn action_payload_round_trips() {
        let action = PartyAction::CancelMission {
            mission_id: "mission-3".into(),
        };
        let encoded = serde_json::to_string(&action).unwrap();
        assert_eq!(
            serde_json::from_str::<PartyAction>(&encoded).unwrap(),
            action
        );
    }

    #[test]
    fn recruitment_role_kinds_are_stable_and_keep_ids_in_the_payload() {
        let edit = PartyAction::UpdateRecruitmentRole {
            role_id: 17,
            name: "Scout".into(),
            quantity: 1,
            requirements: RecruitmentRequirements::default(),
            weapon_precision: 0.0,
        };
        let delete = PartyAction::DeleteRecruitmentRole { role_id: 17 };
        assert_eq!(edit.kind(), "edit_role");
        assert_eq!(delete.kind(), "delete_role");
        assert!(
            serde_json::to_string(&edit)
                .unwrap()
                .contains("\"role_id\":17")
        );
        assert!(
            serde_json::to_string(&delete)
                .unwrap()
                .contains("\"role_id\":17")
        );
    }
}
