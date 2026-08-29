//! Public event wire types and semantic event identity.

use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreLoopEventKind {
    FormParty,
    RequestJoin,
    AcceptJoin,
    AcceptContract,
    Travel,
    Camp,
    AutoresolveVictory,
    AutoresolveDefeat,
    AbandonQuest,
    Recover,
    StoreLoot,
    TurnIn,
    Liquidate,
    Purchase,
    Equip,
    SubmitRepair,
    RetrieveRepair,
    WaitForRepair,
    MedicalDecision,
    BuyMedication,
    AdministerPreparation,
    IllnessRecovered,
    QuestSuppressed,
    Death,
    QuestDecision,
    SafeDepartureWait,
    SafeDepartureWaitRelocated,
    AuthoritySurrender,
    GeneratedDiscoveryAttempt,
    GeneratedDiscoveryResult,
    GeneratedCaseIntake,
    ExpeditionRecovery,
    GeneratedQuestDiscovered,
    GeneratedInvestigationAttempt,
    GeneratedInvestigationAction,
    GeneratedInvestigationWait,
    GeneratedInvestigationReplan,
    GeneratedWitnessDialogue,
    GeneratedQuestCompleted,
    GeneratedQuestClosedExternally,
    Activity,
    Encounter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CoreLoopEventSubject {
    Agent,
    Character {
        character_id: u64,
    },
    DirectContract {
        party_id: String,
        contract_id: String,
    },
    GeneratedCase {
        party_id: String,
        case_id: String,
    },
    InvestigationAction {
        case_id: String,
        action_id: String,
    },
    Item {
        inventory_item_id: u64,
    },
    Encounter {
        party_id: String,
        encounter_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CoreLoopEventSemanticKey {
    pub(super) agent_id: u32,
    pub(super) kind: CoreLoopEventKind,
    pub(super) subject: CoreLoopEventSubject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CoreLoopEventPayload {
    pub(super) kind: CoreLoopEventKind,
    pub(super) subject: CoreLoopEventSubject,
    pub(super) detail: String,
}

impl CoreLoopEventPayload {
    pub(super) fn agent(kind: CoreLoopEventKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Agent,
            detail: detail.into(),
        }
    }

    pub(super) fn direct_contract(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        contract_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::DirectContract {
                party_id: party_id.into(),
                contract_id: contract_id.into(),
            },
            detail: detail.into(),
        }
    }

    pub(super) fn character(
        kind: CoreLoopEventKind,
        character_id: u64,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Character { character_id },
            detail: detail.into(),
        }
    }

    pub(super) fn generated_case(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        case_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::GeneratedCase {
                party_id: party_id.into(),
                case_id: case_id.into(),
            },
            detail: detail.into(),
        }
    }

    pub(super) fn investigation_action(
        kind: CoreLoopEventKind,
        case_id: impl Into<String>,
        action_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::InvestigationAction {
                case_id: case_id.into(),
                action_id: action_id.into(),
            },
            detail: detail.into(),
        }
    }

    pub(super) fn item(
        kind: CoreLoopEventKind,
        inventory_item_id: u64,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Item { inventory_item_id },
            detail: detail.into(),
        }
    }

    pub(super) fn encounter(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        encounter_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Encounter {
                party_id: party_id.into(),
                encounter_id: encounter_id.into(),
            },
            detail: detail.into(),
        }
    }

    pub(super) fn semantic_key(&self, agent_id: u32) -> CoreLoopEventSemanticKey {
        CoreLoopEventSemanticKey {
            agent_id,
            kind: self.kind.clone(),
            subject: self.subject.clone(),
        }
    }

    pub(super) fn into_public(self, sequence: u64, agent_id: u32) -> CoreLoopEvent {
        CoreLoopEvent {
            sequence,
            agent_id,
            kind: self.kind,
            detail: self.detail,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopEvent {
    pub sequence: u64,
    pub agent_id: u32,
    pub kind: CoreLoopEventKind,
    pub detail: String,
}
