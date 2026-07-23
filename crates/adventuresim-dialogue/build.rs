#![allow(dead_code)]

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct BuildDocument {
    conversations: Vec<BuildConversation>,
}
#[derive(Deserialize)]
struct BuildConversation {
    id: String,
    roles: BTreeMap<String, BuildRole>,
    topics: Vec<BuildTopic>,
}
#[derive(Deserialize)]
struct BuildRole {
    kind: String,
    #[serde(default = "build_one")]
    min: u8,
    #[serde(default = "build_one")]
    max: u8,
}
#[derive(Deserialize)]
struct BuildTopic {
    id: String,
    #[serde(default)]
    conditions: serde_json::Value,
    responses: Vec<BuildResponse>,
}
#[derive(Deserialize)]
struct BuildResponse {
    id: String,
    priority: i32,
    #[serde(default)]
    conditions: serde_json::Value,
    turns: Vec<BuildTurn>,
    #[serde(default)]
    effects: Vec<BuildEffect>,
    prompt: Option<BuildPrompt>,
}
#[derive(Deserialize)]
struct BuildTurn {
    speaker: String,
    fragments: Vec<BuildFragment>,
}
#[derive(Deserialize)]
struct BuildPrompt {
    id: String,
    respondent: String,
    mode: String,
    #[serde(default = "build_one_usize")]
    min_choices: usize,
    #[serde(default = "build_one_usize")]
    max_choices: usize,
    choices: Vec<BuildChoice>,
}
#[derive(Deserialize)]
struct BuildChoice {
    id: String,
    #[serde(default)]
    effects: Vec<BuildEffect>,
    #[serde(default)]
    result_turns: Vec<BuildTurn>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BuildFragment {
    Text { value: String },
    Topic { topic: String, label: String },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BuildEffect {
    LearnTopic { topic: String },
    AcceptContract { contract: String },
    ReportContract { contract: String },
    BeginApprenticeship { profession: String },
    ExamineDisease,
    SetFlag { flag: String, value: bool },
}
fn build_one() -> u8 {
    1
}
fn build_one_usize() -> usize {
    1
}

fn validate_condition(value: &serde_json::Value, relative: &str) {
    if value.is_null() {
        return;
    }
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("condition must be an object in {relative}"));
    match object.get("op").and_then(serde_json::Value::as_str) {
        Some("always") => {}
        Some("all" | "any") => {
            for child in object
                .get("conditions")
                .and_then(serde_json::Value::as_array)
                .unwrap_or_else(|| panic!("compound condition has no children in {relative}"))
            {
                validate_condition(child, relative);
            }
        }
        Some("not") => validate_condition(
            object
                .get("condition")
                .unwrap_or_else(|| panic!("not condition has no child in {relative}")),
            relative,
        ),
        Some("fact") => {
            let kind = object
                .get("key")
                .and_then(serde_json::Value::as_object)
                .and_then(|key| key.get("kind"))
                .and_then(serde_json::Value::as_str);
            assert!(
                matches!(
                    kind,
                    Some(
                        "participant_profession"
                            | "participant_familiarity"
                            | "participant_clothing_category"
                            | "participant_count"
                            | "party_leader"
                            | "service"
                            | "location"
                            | "time_period"
                            | "contract_state"
                            | "flag"
                    )
                ) && object.contains_key("equals"),
                "unknown or incomplete fact condition in {relative}"
            );
        }
        _ => panic!("unknown condition operation in {relative}"),
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
        for topic in &conversation.topics {
            validate_condition(&topic.conditions, relative);
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
                validate_condition(&response.conditions, relative);
                assert!(
                    !response.id.is_empty(),
                    "response has empty id {relative}:{}",
                    topic.id
                );
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
                        prompt.mode == "multi"
                            || (prompt.min_choices == 1 && prompt.max_choices == 1),
                        "single-choice prompt has invalid bounds {relative}:{}",
                        prompt.id
                    );
                    assert!(
                        prompt.mode != "yes_no" || prompt.choices.len() == 2,
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
                            assert!(
                                conversation.roles.contains_key(&turn.speaker),
                                "unknown result speaker role {relative}:{}",
                                turn.speaker
                            );
                            assert!(
                                !turn.fragments.is_empty(),
                                "dialogue result turn has no fragments {relative}:{}",
                                choice.id
                            );
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
