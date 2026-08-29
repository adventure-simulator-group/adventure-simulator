use adventuresim_core::{
    investigation_action::InvestigationActionKind,
    quest_generation::{CausalBridge, FactorTrace, RouteClass, TemplateFamily},
};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

pub const EVAL_FORMAT_VERSION: u32 = 4;
pub const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1024;
const CHOICE_ID_PREFIX: &str = "choice:";
const CHOICE_ID_DIGEST_HEX_LEN: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceIdError;

impl fmt::Display for ChoiceIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("choice ID must contain the canonical v4 capability digest")
    }
}

impl std::error::Error for ChoiceIdError {}

/// Validated opaque identity for one currently offered evaluator capability.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ChoiceId(String);

impl ChoiceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ChoiceIdError> {
        let value = value.into();
        let Some(digest) = value.strip_prefix(CHOICE_ID_PREFIX) else {
            return Err(ChoiceIdError);
        };
        if digest.len() != CHOICE_ID_DIGEST_HEX_LEN
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(ChoiceIdError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChoiceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ChoiceId {
    type Err = ChoiceIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for ChoiceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerFrame {
    pub version: u32,
    pub case_id: String,
    pub step: u32,
    pub game_minute: u64,
    pub discovery: DiscoveryView,
    pub journal: JournalView,
    pub party: EvaluationPartyView,
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
/// Observer-safe evaluator inputs, not a persisted strategic party projection.
pub struct EvaluationPartyView {
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
    pub choice_id: ChoiceId,
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

impl ChoiceKind {
    pub(crate) const fn metric_key(self) -> &'static str {
        match self {
            Self::EnterTavern => "EnterTavern",
            Self::InterviewWitness => "InterviewWitness",
            Self::Investigate => "Investigate",
            Self::Travel => "Travel",
            Self::Prepare => "Prepare",
            Self::Wait => "Wait",
            Self::Conclude => "Conclude",
        }
    }
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
    pub choice_id: ChoiceId,
    #[serde(default)]
    pub arguments: DecisionArguments,
}

/// A bounded, player-facing classification probe. This records an answer, not
/// hidden reasoning or chain-of-thought.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyClassification {
    pub template_guess: Option<String>,
    pub threat_guess: Option<String>,
    pub confidence_percent: Option<u8>,
}

impl PolicyClassification {
    pub fn validate(&self) -> Result<(), String> {
        for value in [&self.template_guess, &self.threat_guess]
            .into_iter()
            .flatten()
        {
            if value.is_empty()
                || value.len() > 64
                || !value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
            {
                return Err(
                    "policy classification contains an invalid bounded taxonomy value".into(),
                );
            }
        }
        if self
            .confidence_percent
            .is_some_and(|confidence| confidence > 100)
        {
            return Err("policy classification confidence exceeds 100".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRunMetadata {
    pub policy_name: String,
    pub policy_kind: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_revision: String,
    pub requests: u32,
    pub estimated_prompt_tokens: u64,
    pub estimated_completion_tokens: u64,
    pub estimated_cost_microusd: u64,
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
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationOutcome {
    NotAttempted,
    Prepared { tag: String },
}

impl PreparationOutcome {
    pub(crate) const fn is_prepared(&self) -> bool {
        matches!(self, Self::Prepared { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicTraceEvent {
    pub step: u32,
    /// Player-visible in-world time at which this action began.
    pub game_minute: u64,
    pub location: String,
    pub observation_provenance: String,
    pub pre_observation_digest: String,
    pub post_observation_digest: String,
    pub choice_id: ChoiceId,
    pub choice_kind: ChoiceKind,
    /// Exact label presented to the policy when it chose this action.
    pub action_label: String,
    /// Exact dialogue emitted by the evaluation environment.
    pub dialogue: Vec<PublicDialogueLine>,
    pub result: String,
    pub learned: Vec<String>,
    pub learned_claim_ids: Vec<String>,
    /// Structured correction provenance; metrics must not infer it from prose.
    pub corrected_proposition_ids: Vec<String>,
    pub preparation_outcome: PreparationOutcome,
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
    pub initial_observation_digest: String,
    pub initial_classification: PolicyClassification,
    pub events: Vec<PublicTraceEvent>,
    pub solved: bool,
    pub exhausted: bool,
    pub termination: Termination,
    pub termination_error: Option<TerminationErrorCode>,
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

impl Termination {
    pub(crate) const fn metric_key(self) -> &'static str {
        match self {
            Self::Solved => "Solved",
            Self::StepLimit => "StepLimit",
            Self::DeadEnd => "DeadEnd",
            Self::Loop => "Loop",
            Self::PolicyError => "PolicyError",
            Self::BudgetExceeded => "BudgetExceeded",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationErrorCode {
    BudgetExceeded,
    PolicyFailure,
    InvalidDecision,
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
    pub factor_trace: Vec<FactorTrace>,
    pub bridges: Vec<CausalBridge>,
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
    ResolveCarrier(RouteClass),
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum CapabilityIdentity<'a> {
    EnterTavern,
    Interview {
        index: u64,
    },
    Action {
        index: u64,
        action_kind: InvestigationActionKind,
        route: RouteClass,
    },
    Travel {
        site_id: &'a str,
    },
    Prepare {
        tag: &'a str,
    },
    WaitForWitness {
        index: u64,
    },
    Conclude {
        route: RouteClass,
    },
    ResolveCarrier {
        route: RouteClass,
    },
}

impl<'a> From<&'a Capability> for CapabilityIdentity<'a> {
    fn from(capability: &'a Capability) -> Self {
        match capability {
            Capability::EnterTavern => Self::EnterTavern,
            Capability::Interview(index) => Self::Interview {
                index: *index as u64,
            },
            Capability::Action(index, action_kind, route) => Self::Action {
                index: *index as u64,
                action_kind: *action_kind,
                route: *route,
            },
            Capability::Travel(site_id) => Self::Travel { site_id },
            Capability::Prepare(tag) => Self::Prepare { tag },
            Capability::WaitForWitness(index) => Self::WaitForWitness {
                index: *index as u64,
            },
            Capability::Conclude(route) => Self::Conclude { route: *route },
            Capability::ResolveCarrier(route) => Self::ResolveCarrier { route: *route },
        }
    }
}
