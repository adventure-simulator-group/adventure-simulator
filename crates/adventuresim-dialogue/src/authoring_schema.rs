// The runtime deliberately deserializes this strict preflight schema without
// reading its fields; build.rs reads them for semantic validation.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn one() -> u8 {
    1
}
fn one_usize() -> usize {
    1
}
fn first_response() -> ResolutionPolicy {
    ResolutionPolicy::FirstResponse
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringDocument {
    pub conversations: Vec<AuthoringConversation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringConversation {
    pub id: String,
    pub roles: BTreeMap<String, AuthoringRole>,
    #[serde(default)]
    pub on_start: Vec<AuthoringResponse>,
    pub topics: Vec<AuthoringTopic>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringRole {
    pub kind: String,
    #[serde(default = "one")]
    pub min: u8,
    #[serde(default = "one")]
    pub max: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringTopic {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub initially_known: bool,
    #[serde(default)]
    pub conditions: Condition,
    pub responses: Vec<AuthoringResponse>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringResponse {
    pub id: String,
    pub priority: i32,
    #[serde(default)]
    pub conditions: Condition,
    pub turns: Vec<AuthoringTurn>,
    #[serde(default)]
    pub effects: Vec<AuthoringEffect>,
    pub prompt: Option<AuthoringPrompt>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringTurn {
    pub speaker: String,
    pub fragments: Vec<AuthoringFragment>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringPrompt {
    pub id: String,
    pub respondent: String,
    pub mode: PromptMode,
    #[serde(default = "one_usize")]
    pub min_choices: usize,
    #[serde(default = "one_usize")]
    pub max_choices: usize,
    #[serde(default = "first_response")]
    pub resolution: ResolutionPolicy,
    pub choices: Vec<AuthoringChoice>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    YesNo,
    Single,
    Multi,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPolicy {
    FirstResponse,
    Unanimous,
    Majority,
    AllRespondents,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringChoice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub effects: Vec<AuthoringEffect>,
    #[serde(default)]
    pub result_turns: Vec<AuthoringTurn>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthoringFragment {
    Text { value: String },
    Topic { topic: String, label: String },
    PeriodClaim { value: String },
    AuthoritativeExplanation { reference: String, value: String },
    Runtime { slot: String },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthoringEffect {
    LearnTopic { topic: String },
    AcceptContract { contract: String },
    ReportContract { contract: String },
    BeginApprenticeship { profession: String },
    SetFlag { flag: String, value: bool },
    ReceiveReferredTestimony,
    InvestigationAction { action: String },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    #[default]
    Always,
    All {
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
    Fact {
        key: FactKey,
        equals: FactValue,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactKey {
    ParticipantProfession { role: String },
    ParticipantOrganization { role: String },
    ParticipantReligion { role: String },
    ParticipantAgeBand { role: String },
    ParticipantSex { role: String },
    ParticipantLocalRole { role: String },
    ParticipantStatus { role: String },
    ParticipantLanguageCompatibility { left: String, right: String },
    ParticipantFamiliarity { left: String, right: String },
    ParticipantClothingCategory { role: String },
    ParticipantHasVisibleClothing { role: String },
    ParticipantPriorInteraction { left: String, right: String },
    ParticipantCount { role: String },
    ParticipantPresent { role: String },
    ParticipantRumorCase { role: String },
    ParticipantReferralContact { role: String },
    KnownClaim,
    KnownLead,
    PriorQuestioning { role: String },
    Confidence,
    LanguageCheck { left: String, right: String },
    SocialCheck,
    PartyLeader { role: String },
    Service { role: String },
    Location,
    LocationRole,
    LocalCircumstance,
    TimePeriod,
    ContractState { contract: String },
    Flag { flag: String },
}

impl FactKey {
    pub fn participant_roles(&self) -> impl Iterator<Item = &str> {
        let roles: [Option<&str>; 2] = match self {
            Self::ParticipantProfession { role }
            | Self::ParticipantOrganization { role }
            | Self::ParticipantReligion { role }
            | Self::ParticipantAgeBand { role }
            | Self::ParticipantSex { role }
            | Self::ParticipantLocalRole { role }
            | Self::ParticipantStatus { role }
            | Self::ParticipantClothingCategory { role }
            | Self::ParticipantHasVisibleClothing { role }
            | Self::ParticipantCount { role }
            | Self::ParticipantPresent { role }
            | Self::ParticipantRumorCase { role }
            | Self::ParticipantReferralContact { role }
            | Self::PriorQuestioning { role }
            | Self::PartyLeader { role }
            | Self::Service { role } => [Some(role.as_str()), None],
            Self::ParticipantLanguageCompatibility { left, right }
            | Self::ParticipantFamiliarity { left, right }
            | Self::ParticipantPriorInteraction { left, right }
            | Self::LanguageCheck { left, right } => [Some(left.as_str()), Some(right.as_str())],
            _ => [None, None],
        };
        roles.into_iter().flatten()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FactValue {
    Bool(bool),
    Integer(i64),
    Text(String),
}
