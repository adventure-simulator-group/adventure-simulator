//! Shared, deterministic dialogue model and evaluator.
//!
//! Content is compiled from repository YAML by `build.rs`; deployed binaries never read YAML.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/dialogue_catalog.rs"));

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CatalogDocument {
    pub conversations: Vec<Conversation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub roles: BTreeMap<String, Role>,
    pub topics: Vec<Topic>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
pub struct Topic {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub initially_known: bool,
    #[serde(default)]
    pub conditions: Condition,
    pub responses: Vec<Response>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
pub struct Turn {
    pub speaker: String,
    pub fragments: Vec<Fragment>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fragment {
    Text { value: String },
    Topic { topic: String, label: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Prompt {
    pub id: String,
    pub respondent: String,
    pub mode: PromptMode,
    #[serde(default = "first_response")]
    pub resolution: ResolutionPolicy,
    pub choices: Vec<Choice>,
}
fn first_response() -> ResolutionPolicy {
    ResolutionPolicy::FirstResponse
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// Closed, auditable effect vocabulary. Clients submit only response/choice IDs.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    LearnTopic { topic: String },
    AcceptQuest { quest: String },
    TurnInQuest { quest: String },
    BeginApprenticeship { profession: String },
    ExamineDisease,
    RecruitRole { role: String },
    SetFlag { flag: String, value: bool },
}

/// Typed fact predicates. New game systems extend this enum and the server-side fact resolver.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FactKey {
    ParticipantProfession { role: String },
    ParticipantFamiliarity { left: String, right: String },
    ParticipantClothingCategory { role: String },
    Service { role: String },
    Location,
    TimePeriod,
    QuestState { quest: String },
    Flag { flag: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FactValue {
    Bool(bool),
    Integer(i64),
    Text(String),
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
    pub path: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueError {
    DuplicateId(String),
    UnknownRole(String),
    InvalidCardinality(String),
    AmbiguousPriority { topic: String, priority: i32 },
    NoEligibleResponse(String),
    InvalidPrompt(String),
}

pub fn catalog() -> &'static [CatalogDocument] {
    static CATALOG: OnceLock<Vec<CatalogDocument>> = OnceLock::new();
    CATALOG.get_or_init(|| {
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
    let eligible: Vec<_> = topic
        .responses
        .iter()
        .filter(|r| facts.matches(&r.conditions))
        .collect();
    let best = eligible
        .iter()
        .map(|r| r.priority)
        .max()
        .ok_or_else(|| DialogueError::NoEligibleResponse(topic.id.clone()))?;
    let mut winners = eligible.into_iter().filter(|r| r.priority == best);
    let winner = winners.next().unwrap();
    if winners.next().is_some() {
        return Err(DialogueError::AmbiguousPriority {
            topic: topic.id.clone(),
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
            for topic in &conversation.topics {
                if !ids.insert(format!("{}:{}", conversation.id, topic.id)) {
                    errors.push(DialogueError::DuplicateId(topic.id.clone()));
                }
                for response in &topic.responses {
                    for turn in &response.turns {
                        if !conversation.roles.contains_key(&turn.speaker) {
                            errors.push(DialogueError::UnknownRole(turn.speaker.clone()));
                        }
                    }
                    if let Some(prompt) = &response.prompt {
                        if !conversation.roles.contains_key(&prompt.respondent)
                            || prompt.choices.len() < 2
                            || (prompt.mode == PromptMode::YesNo && prompt.choices.len() != 2)
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

/// Builds a web-editor URL from centrally configured repository/ref values.
/// It rejects non-repository paths so browser markup can never disclose local paths.
pub fn github_edit_url(repository: &str, git_ref: &str, source: &SourceRef) -> Option<String> {
    if repository.is_empty()
        || git_ref.is_empty()
        || source.file.contains('\\')
        || source.file.starts_with('/')
        || source.file.split('/').any(|part| part == "..")
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
        source
            .file
            .split('/')
            .map(encode)
            .collect::<Vec<_>>()
            .join("/"),
        source.line
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiled_catalog_is_valid_and_has_source_spans() {
        validate(catalog()).unwrap();
        assert!(!source_map().is_empty());
        assert!(
            source_map()
                .iter()
                .all(|s| s.file.starts_with("content/dialogue/") && s.line > 0 && s.column > 0)
        );
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
    fn known_intersection_is_eligible() {
        let c = find_conversation("service-professions").unwrap();
        let known = BTreeSet::from(["profession".to_owned(), "quest".to_owned()]);
        let eligible = eligible_topics(c, &known, &FactContext::default());
        assert!(eligible.iter().any(|t| t.id == "profession"));
    }
    #[test]
    fn multi_party_and_prompt_are_first_class() {
        let c = find_conversation("shop-with-assistant").unwrap();
        assert_eq!(c.roles["customers"].max, 4);
        let r = &c.topics[0].responses[0];
        assert!(r.turns.iter().any(|t| t.speaker == "assistant"));
        assert_eq!(r.prompt.as_ref().unwrap().mode, PromptMode::Single);
    }
    #[test]
    fn source_urls_are_safe_and_web_editable() {
        let s = SourceRef {
            path: "x".into(),
            file: "content/dialogue/a file.yaml".into(),
            line: 7,
            column: 2,
        };
        assert_eq!(
            github_edit_url("owner/repo", "main", &s).unwrap(),
            "https://github.com/owner/repo/edit/main/content/dialogue/a%20file.yaml#L7"
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
}
