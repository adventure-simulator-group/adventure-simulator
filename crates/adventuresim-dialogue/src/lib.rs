//! Shared, deterministic dialogue model and evaluator.
//!
//! Content is compiled from repository YAML by `build.rs`; deployed binaries never read YAML.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

mod authoring_schema;
pub use authoring_schema::{
    Condition, FactKey, FactValue, PromptMode, ResolutionPolicy, TopicCategory,
};

include!(concat!(env!("OUT_DIR"), "/dialogue_catalog.rs"));

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogDocument {
    pub conversations: Vec<Conversation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Conversation {
    pub id: String,
    pub roles: BTreeMap<String, Role>,
    /// Responses evaluated once, authoritatively, when a new session starts.
    #[serde(default)]
    pub on_start: Vec<Response>,
    pub topics: Vec<Topic>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub kind: ParticipantKind,
    #[serde(default = "one")]
    pub min: u8,
    #[serde(default = "one")]
    pub max: u8,
}
fn one() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    Player,
    Npc,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub id: String,
    pub label: String,
    /// Presentation grouping. Discovery remains controlled by the authoritative
    /// known-topic rows; this metadata never makes a topic visible by itself.
    #[serde(default)]
    pub category: TopicCategory,
    #[serde(default)]
    pub initially_known: bool,
    #[serde(default)]
    pub conditions: Condition,
    pub responses: Vec<Response>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub id: String,
    pub priority: i32,
    #[serde(default)]
    pub conditions: Condition,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    pub prompt: Option<Prompt>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub speaker: String,
    pub addressee: Addressee,
    pub fragments: Vec<Fragment>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Addressee {
    /// The concrete participant who caused this response, constrained to the
    /// authored role. This remains singular even when the role permits a party.
    Participant {
        role: String,
    },
    Role {
        role: String,
    },
    Group {
        role: String,
    },
}

impl Addressee {
    pub fn role(&self) -> &str {
        match self {
            Self::Participant { role } | Self::Role { role } | Self::Group { role } => role,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fragment {
    Text {
        value: String,
    },
    Topic {
        topic: String,
        label: String,
    },
    /// A character's period-facing account. This is explicitly distinct from
    /// authoritative rules text and may be incomplete or culturally framed.
    PeriodClaim {
        value: String,
    },
    /// Canonical educational text, keyed to a reusable public reference
    /// section. Herbalism and Alchemy can reuse this convention without
    /// presenting authored claims as simulation truth.
    AuthoritativeExplanation {
        reference: String,
        value: String,
    },
    /// A typed placeholder authored in the catalog and resolved by the
    /// strategic server. Runtime data is never interpreted as dialogue code.
    Runtime {
        slot: RuntimeSlot,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSlot {
    SpeakerName,
    SpeakerDescription,
    Settlement,
    Location,
    Landmark,
    Symptom,
    WitnessCircumstance,
    Claim,
    Uncertainty,
    ReferralName,
    ReferralDescription,
    ReferralRole,
    ReferralLocation,
    TimeWindow,
    DescribedLocation,
    Evidence,
    Proof,
    Testimony,
    ContractTerms,
    OrganizationName,
    OrganizationAdmissionTerms,
    OrganizationDuesTerms,
    OrganizationRoleStanding,
    OrganizationRepresentativeName,
    /// Pairwise, server-resolved address fragments. These are bound for the
    /// actual speaker and addressee(s) of each persisted turn.
    AddresseeTitle,
    SecondPersonSubject,
    SecondPersonObject,
    SecondPersonPossessive,
    SecondPersonPossessivePronoun,
    SecondPersonReflexive,
    SecondPersonBe,
    SecondPersonHave,
    SecondPersonDo,
    SecondPersonWill,
    SecondPersonMay,
    SecondPersonShould,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeBindings {
    values: BTreeMap<RuntimeSlot, String>,
    testimony: Option<Vec<TestimonyLine>>,
}

impl RuntimeBindings {
    pub fn bind(&mut self, slot: RuntimeSlot, value: impl Into<String>) {
        self.values.insert(slot, value.into());
    }

    pub fn bind_testimony(&mut self, testimony: Vec<TestimonyLine>) {
        self.testimony = Some(testimony);
    }

    pub fn resolve(&self, fragments: &[Fragment]) -> Result<Vec<ResolvedFragment>, DialogueError> {
        let mut resolved = Vec::new();
        let mut claim_order = 0u32;
        for fragment in fragments {
            match fragment {
                Fragment::Runtime {
                    slot: RuntimeSlot::Testimony,
                } => {
                    let testimony = self
                        .testimony
                        .as_ref()
                        .ok_or(DialogueError::MissingRuntimeSlot(RuntimeSlot::Testimony))?;
                    for (line_index, line) in testimony.iter().enumerate() {
                        if line.spoken_text.chars().count() > 512
                            || line.claim_text.is_empty()
                            || line.claim_text.chars().count() > 512
                            || line.spoken_text.chars().any(char::is_control)
                            || line.claim_text.chars().any(char::is_control)
                        {
                            return Err(DialogueError::InvalidRuntimeValue(RuntimeSlot::Testimony));
                        }
                        let Some(claim_at) = line.spoken_text.find(&line.claim_text) else {
                            return Err(DialogueError::InvalidRuntimeValue(RuntimeSlot::Testimony));
                        };
                        let claim_end = claim_at + line.claim_text.len();
                        if line_index > 0 {
                            resolved.push(ResolvedFragment::Text { value: " ".into() });
                        }
                        if claim_at > 0 {
                            resolved.push(ResolvedFragment::Text {
                                value: line.spoken_text[..claim_at].into(),
                            });
                        }
                        resolved.push(ResolvedFragment::Claim {
                            value: line.claim_text.clone(),
                            claim_order,
                        });
                        claim_order = claim_order.saturating_add(1);
                        if claim_end < line.spoken_text.len() {
                            resolved.push(ResolvedFragment::Text {
                                value: line.spoken_text[claim_end..].into(),
                            });
                        }
                    }
                }
                Fragment::Runtime { slot } => {
                    let value = self
                        .values
                        .get(slot)
                        .ok_or_else(|| DialogueError::MissingRuntimeSlot(slot.clone()))?;
                    if value.chars().count() > 512 || value.chars().any(char::is_control) {
                        return Err(DialogueError::InvalidRuntimeValue(slot.clone()));
                    }
                    resolved.push(ResolvedFragment::Text {
                        value: value.clone(),
                    });
                }
                Fragment::Text { value } => resolved.push(ResolvedFragment::Text {
                    value: value.clone(),
                }),
                Fragment::Topic { topic, label } => resolved.push(ResolvedFragment::Topic {
                    topic: topic.clone(),
                    label: label.clone(),
                }),
                Fragment::PeriodClaim { value } => resolved.push(ResolvedFragment::PeriodClaim {
                    value: value.clone(),
                }),
                Fragment::AuthoritativeExplanation { reference, value } => {
                    resolved.push(ResolvedFragment::AuthoritativeExplanation {
                        reference: reference.clone(),
                        value: value.clone(),
                    })
                }
            }
        }
        Ok(resolved)
    }

    pub fn resolve_turn(
        &self,
        turn: &Turn,
        authored_sources: &[Option<SourceRef>],
    ) -> Result<ResolvedTurn, DialogueError> {
        if turn.fragments.len() != authored_sources.len() {
            return Err(DialogueError::SourceAlignment);
        }
        let fragments = self.resolve(&turn.fragments)?;
        let ordinary_count = turn
            .fragments
            .iter()
            .filter(|fragment| {
                !matches!(
                    fragment,
                    Fragment::Runtime {
                        slot: RuntimeSlot::Testimony
                    }
                )
            })
            .count();
        let testimony_count = fragments.len().saturating_sub(ordinary_count);
        let mut source_refs = Vec::with_capacity(fragments.len());
        for (fragment, source) in turn.fragments.iter().zip(authored_sources) {
            let count = if matches!(
                fragment,
                Fragment::Runtime {
                    slot: RuntimeSlot::Testimony
                }
            ) {
                testimony_count
            } else {
                1
            };
            source_refs.extend(std::iter::repeat_n(source.clone(), count));
        }
        if source_refs.len() != fragments.len() {
            return Err(DialogueError::SourceAlignment);
        }
        Ok(ResolvedTurn {
            speaker: turn.speaker.clone(),
            addressee: turn.addressee.clone(),
            fragments,
            source_refs,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    pub id: String,
    pub respondent: String,
    pub mode: PromptMode,
    #[serde(default = "one_usize")]
    pub min_choices: usize,
    #[serde(default = "one_usize")]
    pub max_choices: usize,
    #[serde(default = "first_response")]
    pub resolution: ResolutionPolicy,
    pub choices: Vec<Choice>,
}
fn one_usize() -> usize {
    1
}
fn first_response() -> ResolutionPolicy {
    ResolutionPolicy::FirstResponse
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// Authored transcript turns appended after this choice wins resolution.
    #[serde(default)]
    pub result_turns: Vec<Turn>,
}

/// Closed, auditable effect vocabulary. Clients submit only response/choice IDs.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    LearnTopic {
        topic: String,
    },
    AcceptContract {
        contract: String,
    },
    ReportContract {
        contract: String,
    },
    BeginApprenticeship {
        profession: String,
    },
    JoinOrganization,
    PayOrganizationDues,
    RequestOrganizationPromotion {
        #[serde(default)]
        to_role_id: Option<String>,
    },
    PresentOrganization,
    SetFlag {
        flag: String,
        value: bool,
    },
    ReceiveReferredTestimony,
    InvestigationAction {
        action: InvestigationAction,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationAction {
    Locate,
    Identify,
    Expose,
    PresentProof,
    PresentTestimony,
    Negotiate,
    ReturnAsset,
    ReleaseSubject,
    ExchangeAsset,
    ReportToIssuer,
}

/// Persisted and transported dialogue output. Unlike [`Fragment`], this type
/// cannot contain unresolved runtime slots. `Claim` carries only its
/// event-local presentation identity; all proposition and assessment authority
/// remains private to the strategic server.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedFragment {
    Text { value: String },
    Topic { topic: String, label: String },
    PeriodClaim { value: String },
    AuthoritativeExplanation { reference: String, value: String },
    Claim { value: String, claim_order: u32 },
}

impl ResolvedFragment {
    pub fn from_authored(fragment: &Fragment) -> Option<Self> {
        match fragment {
            Fragment::Text { value } => Some(Self::Text {
                value: value.clone(),
            }),
            Fragment::Topic { topic, label } => Some(Self::Topic {
                topic: topic.clone(),
                label: label.clone(),
            }),
            Fragment::PeriodClaim { value } => Some(Self::PeriodClaim {
                value: value.clone(),
            }),
            Fragment::AuthoritativeExplanation { reference, value } => {
                Some(Self::AuthoritativeExplanation {
                    reference: reference.clone(),
                    value: value.clone(),
                })
            }
            Fragment::Runtime { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolvedTurn {
    pub speaker: String,
    pub addressee: Addressee,
    pub fragments: Vec<ResolvedFragment>,
    pub source_refs: Vec<Option<SourceRef>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestimonyLine {
    pub spoken_text: String,
    pub claim_text: String,
}

#[derive(Clone, Debug, Default)]
pub struct FactContext {
    pub facts: BTreeMap<FactKey, FactValue>,
}
impl FactContext {
    pub fn matches(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::All { conditions } => conditions.iter().all(|c| self.matches(c)),
            Condition::Any { conditions } => conditions.iter().any(|c| self.matches(c)),
            Condition::Not { condition } => !self.matches(condition),
            Condition::Fact { key, equals } => self.facts.get(key) == Some(equals),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceRef {
    pub document: usize,
    pub path: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub value_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueError {
    DuplicateId(String),
    UnknownRole(String),
    InvalidCardinality(String),
    AmbiguousPriority { topic: String, priority: i32 },
    NoEligibleResponse(String),
    InvalidPrompt(String),
    EmptyContent(String),
    DanglingTopic(String),
    InvalidTestimonyContract(String),
    SourceAlignment,
    MissingRuntimeSlot(RuntimeSlot),
    InvalidRuntimeValue(RuntimeSlot),
}

fn validate_condition_roles(
    condition: &Condition,
    roles: &BTreeMap<String, Role>,
    errors: &mut Vec<DialogueError>,
) {
    match condition {
        Condition::All { conditions } | Condition::Any { conditions } => {
            for child in conditions {
                validate_condition_roles(child, roles, errors);
            }
        }
        Condition::Not { condition } => validate_condition_roles(condition, roles, errors),
        Condition::Fact { key, .. } => {
            for role in key.participant_roles() {
                if !roles.contains_key(role) {
                    errors.push(DialogueError::UnknownRole(role.into()));
                }
            }
        }
        Condition::Always => {}
    }
}

fn validate_response_semantics(
    conversation: &Conversation,
    response: &Response,
    errors: &mut Vec<DialogueError>,
) {
    validate_condition_roles(&response.conditions, &conversation.roles, errors);
    if response.turns.is_empty() {
        errors.push(DialogueError::EmptyContent(response.id.clone()));
    }
    let mut testimony_slots = 0usize;
    let topic_ids = conversation
        .topics
        .iter()
        .map(|topic| topic.id.as_str())
        .collect::<BTreeSet<_>>();
    for turn in &response.turns {
        testimony_slots +=
            validate_turn_semantics(turn, &response.id, &conversation.roles, &topic_ids, errors);
    }
    let primary_testimony_slots = testimony_slots;
    let receives = response
        .effects
        .iter()
        .filter(|effect| matches!(effect, Effect::ReceiveReferredTestimony))
        .count();
    for effect in &response.effects {
        if let Effect::LearnTopic { topic } = effect
            && !topic_ids.contains(topic.as_str())
        {
            errors.push(DialogueError::DanglingTopic(topic.clone()));
        }
    }
    if let Some(prompt) = &response.prompt {
        for choice in &prompt.choices {
            for turn in &choice.result_turns {
                testimony_slots += validate_turn_semantics(
                    turn,
                    &response.id,
                    &conversation.roles,
                    &topic_ids,
                    errors,
                );
            }
            for effect in &choice.effects {
                match effect {
                    Effect::LearnTopic { topic } if !topic_ids.contains(topic.as_str()) => {
                        errors.push(DialogueError::DanglingTopic(topic.clone()));
                    }
                    Effect::ReceiveReferredTestimony => {
                        errors.push(DialogueError::InvalidTestimonyContract(response.id.clone()));
                    }
                    _ => {}
                }
            }
        }
    }
    if !((primary_testimony_slots == 1 && testimony_slots == 1 && receives == 1)
        || (testimony_slots == 0 && receives == 0))
    {
        errors.push(DialogueError::InvalidTestimonyContract(response.id.clone()));
    }
}

fn validate_turn_semantics(
    turn: &Turn,
    response_id: &str,
    roles: &BTreeMap<String, Role>,
    topic_ids: &BTreeSet<&str>,
    errors: &mut Vec<DialogueError>,
) -> usize {
    if !roles.contains_key(&turn.speaker) {
        errors.push(DialogueError::UnknownRole(turn.speaker.clone()));
    }
    if !roles.contains_key(turn.addressee.role()) {
        errors.push(DialogueError::UnknownRole(turn.addressee.role().to_owned()));
    }
    if turn.fragments.is_empty() {
        errors.push(DialogueError::EmptyContent(response_id.into()));
    }
    let mut testimony_slots = 0;
    for fragment in &turn.fragments {
        match fragment {
            Fragment::Text { value }
            | Fragment::PeriodClaim { value }
            | Fragment::AuthoritativeExplanation { value, .. }
                if value.is_empty() =>
            {
                errors.push(DialogueError::EmptyContent(response_id.into()));
            }
            Fragment::Topic { topic, label } => {
                if label.is_empty() || !topic_ids.contains(topic.as_str()) {
                    errors.push(DialogueError::DanglingTopic(topic.clone()));
                }
            }
            Fragment::Runtime {
                slot: RuntimeSlot::Testimony,
            } => testimony_slots += 1,
            _ => {}
        }
    }
    testimony_slots
}

pub fn catalog() -> &'static [CatalogDocument] {
    static CATALOG: OnceLock<Vec<CatalogDocument>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let _: Vec<authoring_schema::AuthoringDocument> =
            serde_json::from_str(CATALOG_JSON).expect("strict build-shared dialogue schema");
        serde_json::from_str(CATALOG_JSON).expect("build-validated dialogue catalog")
    })
}
pub fn source_map() -> &'static [SourceRef] {
    static SOURCES: OnceLock<Vec<SourceRef>> = OnceLock::new();
    SOURCES
        .get_or_init(|| serde_json::from_str(SOURCE_MAP_JSON).expect("build-generated source map"))
}

pub fn eligible_topics<'a>(
    conversation: &'a Conversation,
    known: &BTreeSet<String>,
    facts: &FactContext,
) -> Vec<&'a Topic> {
    conversation
        .topics
        .iter()
        .filter(|t| (t.initially_known || known.contains(&t.id)) && facts.matches(&t.conditions))
        .collect()
}

pub fn select_response<'a>(
    topic: &'a Topic,
    facts: &FactContext,
) -> Result<&'a Response, DialogueError> {
    select_response_set(&topic.id, &topic.responses, facts)
}

pub fn select_start_response<'a>(
    conversation: &'a Conversation,
    facts: &FactContext,
) -> Result<&'a Response, DialogueError> {
    select_response_set("conversation_start", &conversation.on_start, facts)
}

fn select_response_set<'a>(
    trigger: &str,
    responses: &'a [Response],
    facts: &FactContext,
) -> Result<&'a Response, DialogueError> {
    let eligible: Vec<_> = responses
        .iter()
        .filter(|r| facts.matches(&r.conditions))
        .collect();
    let best = eligible
        .iter()
        .map(|r| r.priority)
        .max()
        .ok_or_else(|| DialogueError::NoEligibleResponse(trigger.to_owned()))?;
    let mut winners = eligible.into_iter().filter(|r| r.priority == best);
    let winner = winners.next().unwrap();
    if winners.next().is_some() {
        return Err(DialogueError::AmbiguousPriority {
            topic: trigger.to_owned(),
            priority: best,
        });
    }
    Ok(winner)
}

pub fn validate(documents: &[CatalogDocument]) -> Result<(), Vec<DialogueError>> {
    let mut errors = Vec::new();
    let mut ids = BTreeSet::new();
    for document in documents {
        for conversation in &document.conversations {
            if !ids.insert(conversation.id.clone()) {
                errors.push(DialogueError::DuplicateId(conversation.id.clone()));
            }
            for (name, role) in &conversation.roles {
                if role.min == 0 || role.max < role.min {
                    errors.push(DialogueError::InvalidCardinality(name.clone()));
                }
            }
            for response in &conversation.on_start {
                if !ids.insert(format!("{}:start:{}", conversation.id, response.id)) {
                    errors.push(DialogueError::DuplicateId(response.id.clone()));
                }
                validate_response_semantics(conversation, response, &mut errors);
                if response
                    .turns
                    .iter()
                    .flat_map(|turn| &turn.fragments)
                    .any(|fragment| {
                        matches!(
                            fragment,
                            Fragment::Runtime {
                                slot: RuntimeSlot::Testimony
                            }
                        )
                    })
                    || response
                        .effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::ReceiveReferredTestimony))
                {
                    errors.push(DialogueError::InvalidTestimonyContract(response.id.clone()));
                }
                if response.prompt.is_some() {
                    errors.push(DialogueError::InvalidPrompt(response.id.clone()));
                }
            }
            for (index, response) in conversation.on_start.iter().enumerate() {
                if conversation.on_start[index + 1..].iter().any(|other| {
                    other.priority == response.priority && other.conditions == response.conditions
                }) {
                    errors.push(DialogueError::AmbiguousPriority {
                        topic: "conversation_start".into(),
                        priority: response.priority,
                    });
                }
            }
            for topic in &conversation.topics {
                validate_condition_roles(&topic.conditions, &conversation.roles, &mut errors);
                if !ids.insert(format!("{}:{}", conversation.id, topic.id)) {
                    errors.push(DialogueError::DuplicateId(topic.id.clone()));
                }
                for response in &topic.responses {
                    if !ids.insert(format!("{}:{}:{}", conversation.id, topic.id, response.id)) {
                        errors.push(DialogueError::DuplicateId(response.id.clone()));
                    }
                    validate_response_semantics(conversation, response, &mut errors);
                    if let Some(prompt) = &response.prompt {
                        if !ids.insert(format!("{}:prompt:{}", conversation.id, prompt.id)) {
                            errors.push(DialogueError::DuplicateId(prompt.id.clone()));
                        }
                        let mut choice_ids = BTreeSet::new();
                        for choice in &prompt.choices {
                            if !choice_ids.insert(&choice.id) {
                                errors.push(DialogueError::DuplicateId(choice.id.clone()));
                            }
                            for turn in &choice.result_turns {
                                if !conversation.roles.contains_key(&turn.speaker) {
                                    errors.push(DialogueError::UnknownRole(turn.speaker.clone()));
                                }
                            }
                        }
                        if !conversation.roles.contains_key(&prompt.respondent)
                            || prompt.choices.len() < 2
                            || prompt.min_choices == 0
                            || prompt.max_choices < prompt.min_choices
                            || prompt.max_choices > prompt.choices.len()
                            || (prompt.mode == PromptMode::YesNo && prompt.choices.len() != 2)
                            || (prompt.mode != PromptMode::Multi
                                && (prompt.min_choices != 1 || prompt.max_choices != 1))
                        {
                            errors.push(DialogueError::InvalidPrompt(prompt.id.clone()));
                        }
                    }
                }
                for (index, response) in topic.responses.iter().enumerate() {
                    if topic.responses[index + 1..].iter().any(|other| {
                        other.priority == response.priority
                            && other.conditions == response.conditions
                    }) {
                        errors.push(DialogueError::AmbiguousPriority {
                            topic: topic.id.clone(),
                            priority: response.priority,
                        });
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn find_conversation(id: &str) -> Option<&'static Conversation> {
    catalog()
        .iter()
        .flat_map(|d| &d.conversations)
        .find(|c| c.id == id)
}

pub fn source_for_start_fragment(
    conversation_id: &str,
    response_id: &str,
    turn: usize,
    fragment: usize,
    field: &str,
) -> Option<&'static SourceRef> {
    for (document_index, document) in catalog().iter().enumerate() {
        if let Some((conversation_index, conversation)) = document
            .conversations
            .iter()
            .enumerate()
            .find(|(_, conversation)| conversation.id == conversation_id)
        {
            let response_index = conversation
                .on_start
                .iter()
                .position(|response| response.id == response_id)?;
            let path = format!(
                "conversations.{conversation_index}.on_start.{response_index}.turns.{turn}.fragments.{fragment}.{field}"
            );
            return source_map()
                .iter()
                .find(|source| source.document == document_index && source.path == path);
        }
    }
    None
}

pub fn source_for_fragment(
    conversation_id: &str,
    topic_id: &str,
    response_id: &str,
    turn: usize,
    fragment: usize,
    field: &str,
) -> Option<&'static SourceRef> {
    for (document_index, document) in catalog().iter().enumerate() {
        if let Some((conversation_index, conversation)) = document
            .conversations
            .iter()
            .enumerate()
            .find(|(_, c)| c.id == conversation_id)
        {
            let topic_index = conversation
                .topics
                .iter()
                .position(|topic| topic.id == topic_id)?;
            let response_index = conversation.topics[topic_index]
                .responses
                .iter()
                .position(|response| response.id == response_id)?;
            let path = format!(
                "conversations.{conversation_index}.topics.{topic_index}.responses.{response_index}.turns.{turn}.fragments.{fragment}.{field}"
            );
            return source_map()
                .iter()
                .find(|source| source.document == document_index && source.path == path);
        }
    }
    None
}

pub fn source_for_topic(conversation_id: &str, topic_id: &str) -> Option<&'static SourceRef> {
    for (document_index, document) in catalog().iter().enumerate() {
        if let Some((conversation_index, conversation)) = document
            .conversations
            .iter()
            .enumerate()
            .find(|(_, c)| c.id == conversation_id)
        {
            let topic_index = conversation
                .topics
                .iter()
                .position(|topic| topic.id == topic_id)?;
            let path = format!("conversations.{conversation_index}.topics.{topic_index}.label");
            return source_map()
                .iter()
                .find(|source| source.document == document_index && source.path == path);
        }
    }
    None
}

pub fn category_for_topic(conversation_id: &str, topic_id: &str) -> Option<TopicCategory> {
    find_conversation(conversation_id)?
        .topics
        .iter()
        .find(|topic| topic.id == topic_id)
        .map(|topic| topic.category)
}

pub fn source_for_choice(
    conversation_id: &str,
    topic_id: &str,
    response_id: &str,
    choice_id: &str,
) -> Option<&'static SourceRef> {
    for (document_index, document) in catalog().iter().enumerate() {
        if let Some((conversation_index, conversation)) = document
            .conversations
            .iter()
            .enumerate()
            .find(|(_, c)| c.id == conversation_id)
        {
            let topic_index = conversation
                .topics
                .iter()
                .position(|topic| topic.id == topic_id)?;
            let response_index = conversation.topics[topic_index]
                .responses
                .iter()
                .position(|response| response.id == response_id)?;
            let choice_index = conversation.topics[topic_index].responses[response_index]
                .prompt
                .as_ref()?
                .choices
                .iter()
                .position(|choice| choice.id == choice_id)?;
            let path = format!(
                "conversations.{conversation_index}.topics.{topic_index}.responses.{response_index}.prompt.choices.{choice_index}.label"
            );
            return source_map()
                .iter()
                .find(|source| source.document == document_index && source.path == path);
        }
    }
    None
}

pub fn source_for_choice_fragment(
    conversation_id: &str,
    topic_id: &str,
    response_id: &str,
    choice_id: &str,
    turn: usize,
    fragment: usize,
    field: &str,
) -> Option<&'static SourceRef> {
    for (document_index, document) in catalog().iter().enumerate() {
        if let Some((conversation_index, conversation)) = document
            .conversations
            .iter()
            .enumerate()
            .find(|(_, conversation)| conversation.id == conversation_id)
        {
            let topic_index = conversation
                .topics
                .iter()
                .position(|topic| topic.id == topic_id)?;
            let response_index = conversation.topics[topic_index]
                .responses
                .iter()
                .position(|response| response.id == response_id)?;
            let choice_index = conversation.topics[topic_index].responses[response_index]
                .prompt
                .as_ref()?
                .choices
                .iter()
                .position(|choice| choice.id == choice_id)?;
            let path = format!(
                "conversations.{conversation_index}.topics.{topic_index}.responses.{response_index}.prompt.choices.{choice_index}.result_turns.{turn}.fragments.{fragment}.{field}"
            );
            return source_map()
                .iter()
                .find(|source| source.document == document_index && source.path == path);
        }
    }
    None
}

/// Builds a web-editor URL from centrally configured repository/ref values.
/// It rejects non-repository paths so browser markup can never disclose local paths.
pub fn github_edit_url(repository: &str, git_ref: &str, source: &SourceRef) -> Option<String> {
    github_edit_url_for_location(repository, git_ref, &source.file, source.line)
}

/// Builds a GitHub editor URL for any compiled repository source location.
pub fn github_edit_url_for_location(
    repository: &str,
    git_ref: &str,
    file: &str,
    line: usize,
) -> Option<String> {
    if repository.is_empty()
        || git_ref.is_empty()
        || line == 0
        || file.contains('\\')
        || file.starts_with('/')
        || file.split('/').any(|part| part == "..")
    {
        return None;
    }
    let encode = |value: &str| {
        value
            .replace('%', "%25")
            .replace('#', "%23")
            .replace('?', "%3F")
            .replace(' ', "%20")
    };
    Some(format!(
        "https://github.com/{}/edit/{}/{}#L{}",
        repository.trim_matches('/'),
        encode(git_ref),
        file.split('/').map(encode).collect::<Vec<_>>().join("/"),
        line
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_categories_are_typed_and_default_to_lore() {
        assert_eq!(
            category_for_topic("service-professions", "quest"),
            Some(TopicCategory::Quest)
        );
        assert_eq!(
            category_for_topic("service-professions", "profession"),
            Some(TopicCategory::Lore)
        );
        assert_eq!(category_for_topic("service-professions", "hidden"), None);
        for (conversation, topic) in [
            ("service-professions", "referred-testimony"),
            ("service-professions", "return-recovered-property"),
            ("service-professions", "expose-false-account"),
            ("organization-representative", "referred-testimony"),
        ] {
            assert_eq!(
                category_for_topic(conversation, topic),
                Some(TopicCategory::Quest)
            );
        }
    }
    #[test]
    fn compiled_catalog_is_valid_and_has_source_spans() {
        validate(catalog()).unwrap();
        assert!(!source_map().is_empty());
        assert!(
            source_map()
                .iter()
                .all(|s| s.file.starts_with("content/dialogue/") && s.line > 0 && s.column > 0)
        );
        let kind_spans: Vec<_> = source_map()
            .iter()
            .filter(|source| source.path.ends_with(".kind"))
            .map(|source| (source.document, source.line, source.column))
            .collect();
        assert!(kind_spans.len() > 4);
        assert_eq!(
            kind_spans.iter().collect::<BTreeSet<_>>().len(),
            kind_spans.len()
        );
        let result_source = source_for_choice_fragment(
            "service-professions",
            "apprenticeship",
            "offer-apprenticeship",
            "yes",
            0,
            0,
            "value",
        )
        .expect("authored prompt results have exact source spans");
        assert_eq!(result_source.file, "content/dialogue/services.yaml");
        assert!(result_source.line > 0);
    }
    #[test]
    fn specificity_uses_typed_facts_and_priority() {
        let c = find_conversation("butcher-greeting-example").unwrap();
        let topic = &c.topics[0];
        let mut f = FactContext::default();
        f.facts
            .insert(FactKey::TimePeriod, FactValue::Text("morning".into()));
        assert_eq!(select_response(topic, &f).unwrap().id, "morning");
    }
    #[test]
    fn participant_organization_role_is_a_typed_multi_membership_fact() {
        let key = FactKey::ParticipantRole {
            role: "speaker".into(),
            profession: "citizen".into(),
        };
        assert_eq!(key.participant_roles().collect::<Vec<_>>(), ["speaker"]);
        let mut facts = FactContext::default();
        facts.facts.insert(key.clone(), FactValue::Bool(true));
        assert!(facts.matches(&Condition::Fact {
            key,
            equals: FactValue::Bool(true),
        }));
    }
    #[test]
    fn conversation_start_selects_contextual_authored_greeting() {
        let conversation = find_conversation("service-professions").unwrap();
        let mut facts = FactContext::default();
        facts.facts.insert(
            FactKey::Service {
                role: "professional".into(),
            },
            FactValue::Text("armor".into()),
        );
        let greeting = select_start_response(conversation, &facts).unwrap();
        assert_eq!(greeting.id, "armourer-greeting");
        let source =
            source_for_start_fragment("service-professions", "armourer-greeting", 0, 0, "value")
                .expect("start-trigger text has an exact source span");
        assert_eq!(source.file, "content/dialogue/services.yaml");
        assert!(source.line > 0);
    }
    #[test]
    fn known_intersection_is_eligible() {
        let c = find_conversation("service-professions").unwrap();
        let known = BTreeSet::from(["profession".to_owned(), "quest".to_owned()]);
        let eligible = eligible_topics(c, &known, &FactContext::default());
        assert!(eligible.iter().any(|t| t.id == "profession"));
    }

    #[test]
    fn service_profession_refers_to_named_representative_without_direct_offer() {
        let conversation = find_conversation("service-professions").unwrap();
        let profession = conversation
            .topics
            .iter()
            .find(|topic| topic.id == "profession")
            .unwrap();
        let apprenticeship = conversation
            .topics
            .iter()
            .find(|topic| topic.id == "apprenticeship")
            .unwrap();
        let mut facts = FactContext::default();
        facts.facts.insert(
            FactKey::Service {
                role: "professional".into(),
            },
            FactValue::Text("merchants".into()),
        );
        assert_eq!(
            select_response(profession, &facts).unwrap().id,
            "merchant-profession"
        );
        assert!(facts.matches(&apprenticeship.conditions));

        facts.facts.insert(
            FactKey::LocalOrganizationRepresentative {
                role: "professional".into(),
            },
            FactValue::Bool(true),
        );
        let referral = select_response(profession, &facts).unwrap();
        assert_eq!(referral.id, "merchant-profession-referral");
        assert!(referral.effects.is_empty());
        assert!(!facts.matches(&apprenticeship.conditions));
        let mut bindings = RuntimeBindings::default();
        bindings.bind(
            RuntimeSlot::OrganizationRepresentativeName,
            "<script>Greta & Co.</script>",
        );
        let resolved = bindings.resolve(&referral.turns[0].fragments).unwrap();
        assert!(resolved.iter().any(|fragment| {
            matches!(fragment, ResolvedFragment::Text { value } if value == "<script>Greta & Co.</script>")
        }));
        assert!(referral.turns[0].fragments.iter().all(|fragment| {
            !matches!(fragment, Fragment::Topic { topic, .. } if topic == "apprenticeship")
        }));
    }
    #[test]
    fn generated_case_resolution_actions_are_compiled_for_services_and_residents() {
        for conversation_id in ["service-professions", "local-resident"] {
            let conversation = find_conversation(conversation_id).unwrap();
            for (topic_id, expected) in [
                (
                    "return-recovered-property",
                    InvestigationAction::ReturnAsset,
                ),
                ("expose-false-account", InvestigationAction::Expose),
            ] {
                let topic = conversation
                    .topics
                    .iter()
                    .find(|topic| topic.id == topic_id)
                    .unwrap();
                assert!(topic.initially_known);
                assert!(topic.responses[0].effects.iter().any(|effect| {
                    matches!(
                        effect,
                        Effect::InvestigationAction { action } if action == &expected
                    )
                }));
            }
        }
    }
    #[test]
    fn exact_referred_witness_has_an_actionable_testimony_topic_in_every_npc_conversation() {
        for (conversation_id, npc_role) in [
            ("service-professions", "professional"),
            ("herbalist-examination", "herbalist"),
            ("religion-service", "cleric"),
            ("local-resident", "local"),
            ("recruitment", "employer"),
            ("organization-representative", "representative"),
        ] {
            let conversation = find_conversation(conversation_id).unwrap();
            let topic = conversation
                .topics
                .iter()
                .find(|topic| topic.id == "referred-testimony")
                .expect("every persistent NPC conversation can address a referred witness");
            let mut facts = FactContext::default();
            facts.facts.insert(
                FactKey::ParticipantReferralContact {
                    role: npc_role.into(),
                },
                FactValue::Bool(true),
            );
            assert!(topic.initially_known);
            assert_eq!(topic.label, "What I saw");
            assert!(facts.matches(&topic.conditions));
            assert!(
                topic.responses[0]
                    .effects
                    .contains(&Effect::ReceiveReferredTestimony)
            );
        }

        let resident = find_conversation("local-resident").unwrap();
        assert!(
            resident
                .topics
                .iter()
                .all(|topic| topic.id != "introduction" && topic.id != "local-problem")
        );
    }

    #[test]
    fn organization_dialogue_effects_never_carry_client_selected_ids() {
        let conversation = find_conversation("organization-representative").unwrap();
        let effects = conversation
            .topics
            .iter()
            .flat_map(|topic| &topic.responses)
            .flat_map(|response| {
                response.effects.iter().chain(
                    response
                        .prompt
                        .iter()
                        .flat_map(|prompt| &prompt.choices)
                        .flat_map(|choice| &choice.effects),
                )
            })
            .collect::<Vec<_>>();
        assert!(effects.contains(&&Effect::JoinOrganization));
        assert!(effects.contains(&&Effect::PayOrganizationDues));
        assert!(effects.contains(&&Effect::RequestOrganizationPromotion { to_role_id: None }));
        assert!(effects.contains(&&Effect::PresentOrganization));
        let encoded = serde_json::to_string(&effects).unwrap();
        assert!(!encoded.contains("organization_id"));
    }

    #[test]
    fn organization_dialogue_discloses_authoritative_terms_before_confirmation() {
        let conversation = find_conversation("organization-representative").unwrap();
        for (topic_id, required_slots, effect) in [
            (
                "join",
                vec!["organization_name", "organization_admission_terms"],
                Effect::JoinOrganization,
            ),
            (
                "dues",
                vec!["organization_dues_terms"],
                Effect::PayOrganizationDues,
            ),
            (
                "promotion",
                vec!["organization_role_standing"],
                Effect::RequestOrganizationPromotion { to_role_id: None },
            ),
        ] {
            let response = &conversation
                .topics
                .iter()
                .find(|topic| topic.id == topic_id)
                .expect("organization business topic")
                .responses[0];
            let spoken_before_prompt = serde_json::to_string(&response.turns).unwrap();
            for slot in required_slots {
                assert!(
                    spoken_before_prompt.contains(slot),
                    "{topic_id} must disclose {slot} before confirmation"
                );
            }
            let prompt = response.prompt.as_ref().expect("business confirmation");
            assert!(
                prompt
                    .choices
                    .iter()
                    .flat_map(|choice| &choice.effects)
                    .any(|candidate| candidate == &effect),
                "{topic_id} mutation must occur only after confirmation"
            );
        }
    }

    #[test]
    fn organization_business_topics_are_reachable_only_when_authorized() {
        fn anchored_topics(response: &Response) -> Vec<String> {
            response
                .turns
                .iter()
                .flat_map(|turn| &turn.fragments)
                .filter_map(|fragment| match fragment {
                    Fragment::Topic { topic, .. } => Some(topic.clone()),
                    _ => None,
                })
                .collect()
        }

        fn reachable_topics(conversation: &Conversation, facts: &FactContext) -> BTreeSet<String> {
            let start = select_start_response(conversation, facts).expect("eligible greeting");
            let mut pending = anchored_topics(start);
            let mut reachable = BTreeSet::new();
            while let Some(topic_id) = pending.pop() {
                if !reachable.insert(topic_id.clone()) {
                    continue;
                }
                let topic = conversation
                    .topics
                    .iter()
                    .find(|topic| topic.id == topic_id)
                    .expect("anchored topic exists");
                assert!(
                    topic.initially_known && facts.matches(&topic.conditions),
                    "greeting flow exposed unauthorized topic {topic_id}"
                );
                let response = select_response(topic, facts).expect("reachable topic response");
                pending.extend(anchored_topics(response));
            }
            reachable
        }

        let conversation = find_conversation("organization-representative").unwrap();
        let states = [
            ("none", false, false, false, vec!["join"]),
            ("none", true, false, false, vec!["join"]),
            ("suspended", false, false, false, vec![]),
            ("suspended", true, false, false, vec!["dues"]),
            ("current", false, false, false, vec!["present"]),
            ("current", false, false, true, vec![]),
            ("current", false, true, false, vec!["promotion", "present"]),
            ("current", false, true, true, vec!["promotion"]),
            ("current", true, false, false, vec!["dues", "present"]),
            ("current", true, false, true, vec!["dues"]),
            (
                "current",
                true,
                true,
                false,
                vec!["dues", "promotion", "present"],
            ),
            ("current", true, true, true, vec!["dues", "promotion"]),
        ];
        for (membership, dues, promotion, presentation, expected) in states {
            let mut facts = FactContext::default();
            facts.facts.insert(
                FactKey::OrganizationMembership {
                    player: "player".into(),
                    representative: "representative".into(),
                },
                FactValue::Text(membership.into()),
            );
            facts.facts.insert(
                FactKey::OrganizationDuesRequired {
                    representative: "representative".into(),
                },
                FactValue::Bool(dues),
            );
            facts.facts.insert(
                FactKey::OrganizationPromotionAvailable {
                    player: "player".into(),
                    representative: "representative".into(),
                },
                FactValue::Bool(promotion),
            );
            facts.facts.insert(
                FactKey::OrganizationPresentation {
                    player: "player".into(),
                    representative: "representative".into(),
                },
                FactValue::Bool(presentation),
            );
            let reachable = reachable_topics(conversation, &facts);
            let actionable = ["join", "dues", "promotion", "present"]
                .into_iter()
                .filter(|topic| reachable.contains(*topic))
                .collect::<BTreeSet<_>>();
            assert_eq!(
                actionable,
                expected.into_iter().collect(),
                "wrong reachable business for membership={membership}, dues={dues}, promotion={promotion}, presentation={presentation}"
            );
        }
    }

    #[test]
    fn build_and_runtime_share_authoring_schema_and_runtime_slot_allowlist() {
        let build = include_str!("../build.rs");
        assert!(build.contains("#[path = \"src/authoring_schema.rs\"]"));
        for slot in [
            "organization_name",
            "organization_admission_terms",
            "organization_dues_terms",
            "organization_role_standing",
            "organization_representative_name",
            "addressee_title",
            "second_person_subject",
            "second_person_object",
            "second_person_possessive",
            "second_person_possessive_pronoun",
            "second_person_reflexive",
            "second_person_be",
            "second_person_have",
            "second_person_do",
            "second_person_will",
            "second_person_may",
            "second_person_should",
        ] {
            assert!(build.contains(&format!("| \"{slot}\"")));
        }
    }

    #[test]
    fn multi_party_and_prompt_are_first_class() {
        let c = find_conversation("shop-with-assistant").unwrap();
        assert_eq!(c.roles["customers"].max, 4);
        let r = &c.topics[0].responses[0];
        assert!(r.turns.iter().any(|t| t.speaker == "assistant"));
        let directed = r
            .turns
            .iter()
            .find(|turn| turn.speaker == "shopkeep" && turn.addressee.role() == "assistant")
            .expect("three-party dialogue directs a singular line to the assistant");
        assert!(!directed.addressee.is_group());
        assert!(directed.fragments.iter().any(|fragment| matches!(
            fragment,
            Fragment::Runtime {
                slot: RuntimeSlot::SecondPersonSubject
            }
        )));
        assert_eq!(r.prompt.as_ref().unwrap().mode, PromptMode::Single);
    }
    #[test]
    fn source_urls_are_safe_and_web_editable() {
        let s = SourceRef {
            document: 0,
            path: "x".into(),
            file: "content/dialogue/a file.yaml".into(),
            line: 7,
            column: 2,
            value_json: "\"x\"".into(),
        };
        assert_eq!(
            github_edit_url("owner/repo", "main", &s).unwrap(),
            "https://github.com/owner/repo/edit/main/content/dialogue/a%20file.yaml#L7"
        );
        assert_eq!(
            github_edit_url_for_location("owner/repo", "feature/items", &s.file, 7).unwrap(),
            "https://github.com/owner/repo/edit/feature/items/content/dialogue/a%20file.yaml#L7"
        );
        assert!(
            github_edit_url_for_location("owner/repo", "main", "content/items/catalog.yaml", 0)
                .is_none()
        );
        let bad = SourceRef {
            file: "../secret".into(),
            ..s
        };
        assert!(github_edit_url("owner/repo", "main", &bad).is_none());
    }
    #[test]
    fn validation_rejects_equal_priority_overlap() {
        let mut documents = catalog().to_vec();
        let topic = &mut documents[0].conversations[0].topics[0];
        let duplicate = topic.responses[0].clone();
        topic.responses.push(duplicate);
        assert!(
            validate(&documents)
                .unwrap_err()
                .iter()
                .any(|error| matches!(error, DialogueError::AmbiguousPriority { .. }))
        );
    }

    #[test]
    fn shared_authoring_schema_is_strict_and_covers_on_start() {
        let unknown = r#"{"conversations":[{"id":"strict","roles":{"player":{"kind":"player"},"npc":{"kind":"npc"}},"on_start":[],"topics":[],"surprise":true}]}"#;
        assert!(
            serde_json::from_str::<authoring_schema::AuthoringDocument>(unknown).is_err(),
            "unknown authoring fields must fail in the schema shared by build and runtime"
        );

        let malformed: CatalogDocument = serde_json::from_str(
            r#"{"conversations":[{"id":"start","roles":{"player":{"kind":"player"},"npc":{"kind":"npc"}},"on_start":[{"id":"bad","priority":0,"turns":[{"speaker":"missing","addressee":{"kind":"participant","role":"player"},"fragments":[{"kind":"text","value":"Hello"}]}],"effects":[],"prompt":null}],"topics":[]}]}"#,
        )
        .unwrap();
        assert!(
            validate(&[malformed]).unwrap_err().iter().any(
                |error| matches!(error, DialogueError::UnknownRole(role) if role == "missing")
            )
        );

        let mut start_testimony = catalog().to_vec();
        let start = start_testimony
            .iter_mut()
            .flat_map(|document| &mut document.conversations)
            .find_map(|conversation| conversation.on_start.first_mut())
            .expect("compiled dialogue has an authored start response");
        let start_id = start.id.clone();
        start.turns[0].fragments.push(Fragment::Runtime {
            slot: RuntimeSlot::Testimony,
        });
        start.effects.push(Effect::ReceiveReferredTestimony);
        assert!(validate(&start_testimony).unwrap_err().iter().any(
            |error| matches!(error, DialogueError::InvalidTestimonyContract(id) if id == &start_id)
        ));
    }

    #[test]
    fn shared_prompt_schema_closes_mode_and_resolution_with_a_real_default() {
        let prefix = r#"{"conversations":[{"id":"prompt","roles":{"player":{"kind":"player"},"npc":{"kind":"npc"}},"on_start":[],"topics":[{"id":"topic","label":"Topic","responses":[{"id":"response","priority":0,"turns":[{"speaker":"npc","addressee":{"kind":"participant","role":"player"},"fragments":[{"kind":"text","value":"Choose."}]}],"effects":[],"prompt":{"id":"choice","respondent":"player","mode":"single","#;
        let suffix = r#""min_choices":1,"max_choices":1,"choices":[{"id":"a","label":"A"},{"id":"b","label":"B"}]}}]}]}]}"#;
        let omitted = format!("{prefix}{suffix}");
        let parsed: authoring_schema::AuthoringDocument = serde_json::from_str(&omitted).unwrap();
        assert_eq!(
            parsed.conversations[0].topics[0].responses[0]
                .prompt
                .as_ref()
                .unwrap()
                .resolution,
            ResolutionPolicy::FirstResponse
        );
        for invalid in [
            format!("{prefix}\"resolution\":null,{suffix}"),
            omitted.replace("\"mode\":\"single\"", "\"mode\":\"bogus\""),
            format!("{prefix}\"resolution\":\"bogus\",{suffix}"),
        ] {
            assert!(serde_json::from_str::<authoring_schema::AuthoringDocument>(&invalid).is_err());
        }
    }

    #[test]
    fn validation_rejects_dangling_topics_and_testimony_contract_mismatches() {
        let mut documents = catalog().to_vec();
        let conversation = &mut documents[0].conversations[0];
        conversation.topics[0].responses[0]
            .effects
            .push(Effect::LearnTopic {
                topic: "does-not-exist".into(),
            });
        conversation.topics[0].responses[0].turns[0]
            .fragments
            .push(Fragment::Runtime {
                slot: RuntimeSlot::Testimony,
            });
        let errors = validate(&documents).unwrap_err();
        assert!(errors.iter().any(
            |error| matches!(error, DialogueError::DanglingTopic(topic) if topic == "does-not-exist")
        ));
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, DialogueError::InvalidTestimonyContract(_)))
        );
    }

    #[test]
    fn prompt_results_share_turn_and_effect_semantic_validation() {
        let mut documents = catalog().to_vec();
        let response = documents
            .iter_mut()
            .flat_map(|document| &mut document.conversations)
            .flat_map(|conversation| &mut conversation.topics)
            .flat_map(|topic| &mut topic.responses)
            .find(|response| response.prompt.is_some())
            .unwrap();
        let speaker = response.turns[0].speaker.clone();
        let addressee = response.turns[0].addressee.clone();
        let choice = &mut response.prompt.as_mut().unwrap().choices[0];
        choice.result_turns.push(Turn {
            speaker: speaker.clone(),
            addressee: addressee.clone(),
            fragments: Vec::new(),
        });
        choice.result_turns.push(Turn {
            speaker,
            addressee,
            fragments: vec![Fragment::Topic {
                topic: "missing-inline-topic".into(),
                label: "missing".into(),
            }],
        });
        choice.effects.push(Effect::LearnTopic {
            topic: "missing-result-topic".into(),
        });
        let errors = validate(&documents).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, DialogueError::EmptyContent(_)))
        );
        assert!(errors.iter().any(
            |error| matches!(error, DialogueError::DanglingTopic(topic) if topic == "missing-result-topic")
        ));
        assert!(errors.iter().any(
            |error| matches!(error, DialogueError::DanglingTopic(topic) if topic == "missing-inline-topic")
        ));
    }

    #[test]
    fn runtime_slots_are_typed_and_resolve_to_inert_text() {
        let authored = vec![
            Fragment::Text {
                value: "Ask ".into(),
            },
            Fragment::Runtime {
                slot: RuntimeSlot::ReferralDescription,
            },
        ];
        let mut bindings = RuntimeBindings::default();
        bindings.bind(
            RuntimeSlot::ReferralDescription,
            "<img src=x onerror=alert(1)>",
        );
        assert_eq!(
            bindings.resolve(&authored).unwrap()[1],
            ResolvedFragment::Text {
                value: "<img src=x onerror=alert(1)>".into()
            }
        );
        assert!(RuntimeBindings::default().resolve(&authored).is_err());
    }

    #[test]
    fn address_fragments_render_titles_pronouns_and_agreeing_verbs() {
        let authored = vec![
            Fragment::Runtime {
                slot: RuntimeSlot::AddresseeTitle,
            },
            Fragment::Text { value: ", ".into() },
            Fragment::Runtime {
                slot: RuntimeSlot::SecondPersonSubject,
            },
            Fragment::Text { value: " ".into() },
            Fragment::Runtime {
                slot: RuntimeSlot::SecondPersonBe,
            },
            Fragment::Text {
                value: " welcome; this book is ".into(),
            },
            Fragment::Runtime {
                slot: RuntimeSlot::SecondPersonPossessivePronoun,
            },
            Fragment::Text { value: ".".into() },
        ];
        let mut familiar = RuntimeBindings::default();
        familiar.bind(RuntimeSlot::AddresseeTitle, "Father");
        familiar.bind(RuntimeSlot::SecondPersonSubject, "thou");
        familiar.bind(RuntimeSlot::SecondPersonBe, "art");
        familiar.bind(RuntimeSlot::SecondPersonPossessivePronoun, "thine");
        let familiar_text = familiar
            .resolve(&authored)
            .unwrap()
            .into_iter()
            .map(|fragment| match fragment {
                ResolvedFragment::Text { value } => value,
                _ => String::new(),
            })
            .collect::<String>();
        assert_eq!(
            familiar_text,
            "Father, thou art welcome; this book is thine."
        );

        let mut formal_plural = RuntimeBindings::default();
        formal_plural.bind(RuntimeSlot::AddresseeTitle, "gentlefolk");
        formal_plural.bind(RuntimeSlot::SecondPersonSubject, "you");
        formal_plural.bind(RuntimeSlot::SecondPersonBe, "are");
        formal_plural.bind(RuntimeSlot::SecondPersonPossessivePronoun, "yours");
        let plural_text = formal_plural
            .resolve(&authored)
            .unwrap()
            .into_iter()
            .map(|fragment| match fragment {
                ResolvedFragment::Text { value } => value,
                _ => String::new(),
            })
            .collect::<String>();
        assert_eq!(
            plural_text,
            "gentlefolk, you are welcome; this book is yours."
        );
    }

    #[test]
    fn testimony_resolves_exact_claim_boundaries_and_order() {
        let authored = vec![Fragment::Runtime {
            slot: RuntimeSlot::Testimony,
        }];
        let mut bindings = RuntimeBindings::default();
        bindings.bind_testimony(vec![
            TestimonyLine {
                spoken_text: "Before repeated words; repeated words after.".into(),
                claim_text: "repeated words".into(),
            },
            TestimonyLine {
                spoken_text: "“The gate was open,” I insist.".into(),
                claim_text: "The gate was open".into(),
            },
        ]);
        assert_eq!(
            bindings.resolve(&authored).unwrap(),
            vec![
                ResolvedFragment::Text {
                    value: "Before ".into()
                },
                ResolvedFragment::Claim {
                    value: "repeated words".into(),
                    claim_order: 0
                },
                ResolvedFragment::Text {
                    value: "; repeated words after.".into()
                },
                ResolvedFragment::Text { value: " ".into() },
                ResolvedFragment::Text {
                    value: "“".into()
                },
                ResolvedFragment::Claim {
                    value: "The gate was open".into(),
                    claim_order: 1
                },
                ResolvedFragment::Text {
                    value: ",” I insist.".into()
                },
            ]
        );
        let encoded = serde_json::to_string(&bindings.resolve(&authored).unwrap()).unwrap();
        for private in [
            "proposition",
            "reliability",
            "factually",
            "demeanor",
            "roll",
            "threshold",
            "chance",
        ] {
            assert!(!encoded.contains(private));
        }
        let source = SourceRef {
            document: 0,
            path: "conversations.0.topics.0.responses.0.turns.0.fragments.0.slot".into(),
            file: "content/dialogue/test.yaml".into(),
            line: 12,
            column: 4,
            value_json: "\"testimony\"".into(),
        };
        let turn = Turn {
            speaker: "npc".into(),
            addressee: Addressee::Role {
                role: "player".into(),
            },
            fragments: authored,
        };
        let resolved_turn = bindings
            .resolve_turn(&turn, &[Some(source.clone())])
            .unwrap();
        assert_eq!(
            resolved_turn.fragments.len(),
            resolved_turn.source_refs.len()
        );
        assert!(
            resolved_turn
                .source_refs
                .iter()
                .all(|resolved| resolved.as_ref() == Some(&source))
        );
    }
}
