use adventuresim_core::{
    investigation_action::InvestigationActionKind,
    quest_generation::{RouteClass, TemplateFamily},
};
use serde::{Deserialize, Serialize};

pub const EVAL_FORMAT_VERSION: u32 = 2;
pub const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerFrame {
    pub version: u32,
    pub case_id: String,
    pub step: u32,
    pub game_minute: u64,
    pub discovery: DiscoveryView,
    pub journal: JournalView,
    pub party: PartyView,
    pub legal_choices: Vec<LegalChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryView {
    pub problem_summary: String,
    pub consequence_summary: String,
    pub learned_at: String,
    pub referrals: Vec<WitnessReferral>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessReferral {
    pub witness_id: String,
    pub display_name: String,
    pub physical_description: String,
    pub expected_location: String,
    pub interviewed: bool,
    pub availability: WitnessAvailability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessAvailability {
    Available,
    ScheduledElsewhere,
    AwaitingReturn,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalView {
    pub claims: Vec<PublicClaim>,
    pub evidence: Vec<PublicEvidence>,
    pub locations: Vec<PublicLocation>,
    pub corrections: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicClaim {
    pub proposition_id: String,
    pub source: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicEvidence {
    pub evidence_id: String,
    pub description: String,
    pub discovery_source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicLocation {
    pub label: String,
    pub resolution: LocationResolution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationResolution {
    Approximate,
    Exact,
    Visited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartyView {
    pub members: u8,
    pub terrain_skill: u8,
    pub insight: u8,
    pub perception: u8,
    pub combat_readiness: u8,
    pub supplies: u16,
    pub equipment_tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegalChoice {
    /// Opaque stable capability ID. It is the only authority accepted back.
    pub choice_id: String,
    pub kind: ChoiceKind,
    pub label: String,
    pub typed_arguments: ChoiceArguments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    EnterTavern,
    InterviewWitness,
    Investigate,
    Travel,
    Prepare,
    Wait,
    Conclude,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceArguments {
    pub allowed: Vec<ArgumentValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentValue {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub version: u32,
    pub choice_id: String,
    #[serde(default)]
    pub arguments: DecisionArguments,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionArguments {
    pub selection: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicDialogueLine {
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicTraceEvent {
    pub step: u32,
    /// Player-visible in-world time at which this action began.
    pub game_minute: u64,
    pub location: String,
    pub frame_digest: String,
    pub choice_id: String,
    pub choice_kind: ChoiceKind,
    /// Exact label presented to the policy when it chose this action.
    pub action_label: String,
    /// Exact dialogue emitted by the evaluation environment.
    pub dialogue: Vec<PublicDialogueLine>,
    pub result: String,
    pub learned: Vec<String>,
    /// Structured correction provenance; metrics must not infer it from prose.
    pub corrected_proposition_ids: Vec<String>,
    pub game_minutes: u32,
    pub resource_cost: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicQuestTrace {
    pub version: u32,
    pub case_id: String,
    pub policy: String,
    pub title: String,
    pub problem_summary: String,
    pub events: Vec<PublicTraceEvent>,
    pub solved: bool,
    pub exhausted: bool,
    pub termination: Termination,
    pub route: Option<RouteClass>,
    pub semantic_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Solved,
    StepLimit,
    DeadEnd,
    Loop,
    PolicyError,
    BudgetExceeded,
}

/// This type must never be passed to a policy or serialized into public trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeveloperCaseAnalysis {
    pub family: TemplateFamily,
    pub canonical_case_id: String,
    pub canonical_cause: String,
    pub generation_seed: u64,
    pub catalog_revision: String,
    pub true_site: String,
    pub factor_ids: Vec<String>,
    pub plausibility_factors: Vec<u32>,
    pub curation_factors: Vec<u32>,
    pub bridge_ids: Vec<String>,
    pub generator_manifest_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Capability {
    EnterTavern,
    Interview(usize),
    Action(usize, InvestigationActionKind, RouteClass),
    Travel(String),
    Prepare(String),
    WaitForWitness(usize),
    Conclude(RouteClass),
}
