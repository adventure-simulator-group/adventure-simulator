#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

#[path = "src/authoring_schema.rs"]
mod authoring_schema;
use authoring_schema::{
    AuthoringDocument as BuildDocument, AuthoringEffect as BuildEffect,
    AuthoringFragment as BuildFragment, AuthoringResponse as BuildResponse,
    AuthoringRole as BuildRole, AuthoringTurn as BuildTurn, Condition, PromptMode,
};

fn validate_fragment(fragment: &BuildFragment, relative: &str) {
    if let BuildFragment::Runtime { slot } = fragment {
        assert!(
            matches!(
                slot.as_str(),
                "speaker_name"
                    | "speaker_description"
                    | "settlement"
                    | "location"
                    | "landmark"
                    | "symptom"
                    | "witness_circumstance"
                    | "claim"
                    | "uncertainty"
                    | "referral_name"
                    | "referral_description"
                    | "referral_role"
                    | "referral_location"
                    | "time_window"
                    | "described_location"
                    | "evidence"
                    | "proof"
                    | "testimony"
                    | "contract_terms"
                    | "organization_name"
                    | "organization_admission_terms"
                    | "organization_dues_terms"
                    | "organization_rank_standing"
            ),
            "unknown runtime slot {slot} in {relative}"
        );
    }
}

fn validate_effect(effect: &BuildEffect, relative: &str) {
    if let BuildEffect::InvestigationAction { action } = effect {
        assert!(
            matches!(
                action.as_str(),
                "locate"
                    | "identify"
                    | "expose"
                    | "present_proof"
                    | "present_testimony"
                    | "negotiate"
                    | "return_asset"
                    | "release_subject"
                    | "exchange_asset"
                    | "report_to_issuer"
            ),
            "unknown investigation action {action} in {relative}"
        );
    }
}
fn validate_condition_roles(
    value: &Condition,
    roles: &BTreeMap<String, BuildRole>,
    relative: &str,
) {
    match value {
        Condition::All { conditions } | Condition::Any { conditions } => {
            for child in conditions {
                validate_condition_roles(child, roles, relative);
            }
        }
        Condition::Not { condition } => validate_condition_roles(condition, roles, relative),
        Condition::Fact { key, .. } => {
            for role in key.participant_roles() {
                assert!(
                    roles.contains_key(role),
                    "unknown fact role {role} in {relative}"
                );
            }
        }
        Condition::Always => {}
    }
}

fn validate_response_contract(
    response: &BuildResponse,
    roles: &BTreeMap<String, BuildRole>,
    topics: &BTreeSet<&str>,
    relative: &str,
) {
    assert!(
        !response.turns.is_empty(),
        "response has no turns in {relative}"
    );
    let mut testimony_slots = 0usize;
    for turn in &response.turns {
        testimony_slots += validate_turn_contract(turn, roles, topics, relative, true);
    }
    let receives = response
        .effects
        .iter()
        .filter(|effect| matches!(effect, BuildEffect::ReceiveReferredTestimony))
        .count();
    assert!(
        (testimony_slots == 1 && receives == 1) || (testimony_slots == 0 && receives == 0),
        "Runtime(testimony) and receive_referred_testimony must occur exactly once together in {relative}"
    );
    for effect in &response.effects {
        validate_effect_contract(effect, topics, relative, true);
    }
}

fn validate_turn_contract(
    turn: &BuildTurn,
    roles: &BTreeMap<String, BuildRole>,
    topics: &BTreeSet<&str>,
    relative: &str,
    allow_testimony: bool,
) -> usize {
    assert!(
        roles.contains_key(&turn.speaker),
        "unknown speaker role {} in {relative}",
        turn.speaker
    );
    assert!(
        !turn.fragments.is_empty(),
        "dialogue turn has no fragments in {relative}"
    );
    let mut testimony_slots = 0;
    for fragment in &turn.fragments {
        validate_fragment(fragment, relative);
        match fragment {
            BuildFragment::Text { value }
            | BuildFragment::PeriodClaim { value }
            | BuildFragment::AuthoritativeExplanation { value, .. } => {
                assert!(!value.is_empty(), "empty dialogue content in {relative}");
            }
            BuildFragment::Topic { topic, label } => {
                assert!(!label.is_empty(), "empty topic label in {relative}");
                assert!(
                    topics.contains(topic.as_str()),
                    "dangling inline topic {topic} in {relative}"
                );
            }
            BuildFragment::Runtime { slot } if slot == "testimony" => {
                assert!(
                    allow_testimony,
                    "runtime testimony is not supported in this dialogue position in {relative}"
                );
                testimony_slots += 1;
            }
            BuildFragment::Runtime { .. } => {}
        }
    }
    testimony_slots
}

fn validate_effect_contract(
    effect: &BuildEffect,
    topics: &BTreeSet<&str>,
    relative: &str,
    allow_testimony: bool,
) {
    validate_effect(effect, relative);
    match effect {
        BuildEffect::LearnTopic { topic } => assert!(
            topics.contains(topic.as_str()),
            "dangling LearnTopic {topic} in {relative}"
        ),
        BuildEffect::ReceiveReferredTestimony => assert!(
            allow_testimony,
            "receive_referred_testimony is not supported in this dialogue position in {relative}"
        ),
        _ => {}
    }
}

fn validate_document(document: &BuildDocument, relative: &str, global_ids: &mut BTreeSet<String>) {
    for conversation in &document.conversations {
        assert!(
            global_ids.insert(conversation.id.clone()),
            "duplicate conversation id {relative}:{}",
            conversation.id
        );
        assert!(
            conversation
                .roles
                .values()
                .any(|role| role.kind == "player"),
            "conversation has no player role {relative}:{}",
            conversation.id
        );
        assert!(
            conversation.roles.values().any(|role| role.kind == "npc"),
            "conversation has no npc role {relative}:{}",
            conversation.id
        );
        for (role_name, role) in &conversation.roles {
            assert!(
                matches!(role.kind.as_str(), "player" | "npc"),
                "unknown role kind {relative}:{role_name}"
            );
            assert!(
                role.min > 0 && role.max >= role.min,
                "invalid role cardinality {relative}:{role_name}"
            );
        }
        let topic_ids = conversation
            .topics
            .iter()
            .map(|topic| topic.id.as_str())
            .collect::<BTreeSet<_>>();
        for response in &conversation.on_start {
            validate_condition_roles(&response.conditions, &conversation.roles, relative);
            assert!(
                global_ids.insert(format!("{}:start:{}", conversation.id, response.id)),
                "duplicate start response id {relative}:{}",
                response.id
            );
            validate_response_contract(response, &conversation.roles, &topic_ids, relative);
            assert!(
                !response
                    .turns
                    .iter()
                    .flat_map(|turn| &turn.fragments)
                    .any(|fragment| matches!(
                        fragment,
                        BuildFragment::Runtime { slot } if slot == "testimony"
                    ))
                    && !response
                        .effects
                        .iter()
                        .any(|effect| matches!(effect, BuildEffect::ReceiveReferredTestimony)),
                "on_start does not support authoritative testimony in {relative}:{}",
                response.id
            );
            assert!(
                response.prompt.is_none(),
                "on_start response cannot prompt in {relative}"
            );
        }
        for (index, response) in conversation.on_start.iter().enumerate() {
            assert!(
                !conversation.on_start[index + 1..]
                    .iter()
                    .any(|other| other.priority == response.priority
                        && other.conditions == response.conditions),
                "ambiguous equal-priority start responses in {relative}"
            );
        }
        for topic in &conversation.topics {
            validate_condition_roles(&topic.conditions, &conversation.roles, relative);
            assert!(
                global_ids.insert(format!("{}:{}", conversation.id, topic.id)),
                "duplicate topic id {relative}:{}",
                topic.id
            );
            assert!(
                !topic.responses.is_empty(),
                "topic has no responses {relative}:{}",
                topic.id
            );
            for response in &topic.responses {
                validate_condition_roles(&response.conditions, &conversation.roles, relative);
                assert!(
                    !response.id.is_empty(),
                    "response has empty id {relative}:{}",
                    topic.id
                );
                validate_response_contract(response, &conversation.roles, &topic_ids, relative);
                assert!(
                    global_ids.insert(format!("{}:{}:{}", conversation.id, topic.id, response.id)),
                    "duplicate response id {relative}:{}",
                    response.id
                );
                for turn in &response.turns {
                    assert!(
                        conversation.roles.contains_key(&turn.speaker),
                        "unknown speaker role {relative}:{}",
                        turn.speaker
                    );
                    assert!(
                        !turn.fragments.is_empty(),
                        "dialogue turn has no fragments {relative}:{}",
                        response.id
                    );
                    for fragment in &turn.fragments {
                        validate_fragment(fragment, relative);
                    }
                }
                for effect in &response.effects {
                    validate_effect(effect, relative);
                }
                if let Some(prompt) = &response.prompt {
                    assert!(
                        global_ids.insert(format!("{}:prompt:{}", conversation.id, prompt.id)),
                        "duplicate prompt id {relative}:{}",
                        prompt.id
                    );
                    assert!(
                        conversation.roles.contains_key(&prompt.respondent),
                        "unknown respondent role {relative}:{}",
                        prompt.respondent
                    );
                    assert!(
                        prompt.choices.len() >= 2
                            && prompt.min_choices > 0
                            && prompt.max_choices >= prompt.min_choices
                            && prompt.max_choices <= prompt.choices.len(),
                        "invalid prompt bounds {relative}:{}",
                        prompt.id
                    );
                    assert!(
                        prompt.mode == PromptMode::Multi
                            || (prompt.min_choices == 1 && prompt.max_choices == 1),
                        "single-choice prompt has invalid bounds {relative}:{}",
                        prompt.id
                    );
                    assert!(
                        prompt.mode != PromptMode::YesNo || prompt.choices.len() == 2,
                        "yes/no prompt must have two choices {relative}:{}",
                        prompt.id
                    );
                    let mut choices = BTreeSet::new();
                    for choice in &prompt.choices {
                        assert!(
                            choices.insert(&choice.id),
                            "duplicate choice id {relative}:{}",
                            choice.id
                        );
                        for turn in &choice.result_turns {
                            validate_turn_contract(
                                turn,
                                &conversation.roles,
                                &topic_ids,
                                relative,
                                false,
                            );
                        }
                        for effect in &choice.effects {
                            validate_effect_contract(effect, &topic_ids, relative, false);
                        }
                    }
                }
            }
            for (index, response) in topic.responses.iter().enumerate() {
                assert!(
                    !topic.responses[index + 1..]
                        .iter()
                        .any(|other| other.priority == response.priority
                            && other.conditions == response.conditions),
                    "ambiguous equal-priority responses {relative}:{}",
                    topic.id
                );
            }
        }
    }
}

struct Scanner<'a> {
    text: &'a str,
    at: usize,
    file: &'a str,
    document: usize,
    out: Vec<serde_json::Value>,
}
impl<'a> Scanner<'a> {
    fn ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn string_end(&self, start: usize) -> usize {
        let mut i = start + 1;
        let bytes = self.text.as_bytes();
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                return i + 1;
            } else {
                i += 1;
            }
        }
        panic!("unterminated JSON string")
    }
    fn pos(&self, at: usize) -> (usize, usize) {
        let before = &self.text[..at];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(1, |s| s.chars().count() + 1);
        (line, column)
    }
    fn value(&mut self, path: &mut Vec<String>) {
        self.ws();
        let start = self.at;
        match self.text.as_bytes()[self.at] {
            b'{' => {
                self.at += 1;
                self.ws();
                while self.text.as_bytes()[self.at] != b'}' {
                    let key_start = self.at;
                    let end = self.string_end(key_start);
                    let key: String = serde_json::from_str(&self.text[key_start..end]).unwrap();
                    self.at = end;
                    self.ws();
                    assert_eq!(self.text.as_bytes()[self.at], b':');
                    self.at += 1;
                    path.push(key);
                    self.value(path);
                    path.pop();
                    self.ws();
                    if self.text.as_bytes()[self.at] == b',' {
                        self.at += 1;
                        self.ws();
                    } else {
                        break;
                    }
                }
                self.at += 1;
            }
            b'[' => {
                self.at += 1;
                self.ws();
                let mut index = 0;
                while self.text.as_bytes()[self.at] != b']' {
                    path.push(index.to_string());
                    self.value(path);
                    path.pop();
                    index += 1;
                    self.ws();
                    if self.text.as_bytes()[self.at] == b',' {
                        self.at += 1;
                        self.ws();
                    } else {
                        break;
                    }
                }
                self.at += 1;
            }
            b'"' => {
                self.at = self.string_end(start);
                self.record(path, start, self.at);
            }
            _ => {
                while self
                    .text
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b',' | b']' | b'}'))
                {
                    self.at += 1;
                }
                self.record(path, start, self.at);
            }
        }
    }
    fn record(&mut self, path: &[String], start: usize, end: usize) {
        let (line, column) = self.pos(start);
        let value: serde_json::Value = serde_json::from_str(&self.text[start..end]).unwrap();
        self.out.push(serde_json::json!({"document":self.document,"path":path.join("."),"file":self.file,"line":line,"column":column,"value_json":serde_json::to_string(&value).unwrap()}));
    }
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let content = root.join("content/dialogue");
    println!("cargo:rerun-if-changed={}", content.display());
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("content/dialogue must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    files.sort();
    let mut docs = Vec::new();
    let mut sources = Vec::new();
    let mut digest = Sha256::new();
    let mut global_ids = BTreeSet::new();
    for (document, file) in files.into_iter().enumerate() {
        let text = fs::read_to_string(&file).unwrap();
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(text.as_bytes());
        // JSON is a strict, diff-friendly subset of YAML. Keeping the authoring format
        // to this subset gives serde_json's mature validation without shipping a parser.
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid {relative}: {e}"));
        let typed: BuildDocument = serde_json::from_value(value.clone())
            .unwrap_or_else(|e| panic!("invalid dialogue schema {relative}: {e}"));
        validate_document(&typed, &relative, &mut global_ids);
        let json = value.clone();
        let mut scanner = Scanner {
            text: &text,
            at: 0,
            file: &relative,
            document,
            out: Vec::new(),
        };
        scanner.value(&mut Vec::new());
        scanner.ws();
        assert_eq!(scanner.at, text.len());
        sources.extend(scanner.out);
        docs.push(json);
    }
    let catalog = serde_json::to_string(&docs).unwrap();
    let source_map = serde_json::to_string(&sources).unwrap();
    let digest = format!("{:x}", digest.finalize());
    let generated = format!(
        "pub const CATALOG_JSON: &str = {catalog:?};\npub const SOURCE_MAP_JSON: &str = {source_map:?};\npub const CATALOG_DIGEST: &str = {digest:?};\n"
    );
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("dialogue_catalog.rs"),
        generated,
    )
    .unwrap();
}
