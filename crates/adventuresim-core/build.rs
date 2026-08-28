#![expect(
    unexpected_cfgs,
    reason = "shared catalog types enable runtime-only derives outside the build script"
)]

use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
#[path = "src/combat_style.rs"]
mod combat_style;
#[expect(
    dead_code,
    reason = "the build script imports the shared item schema but does not exercise runtime helpers"
)]
#[path = "src/item_catalog_schema.rs"]
mod item_catalog_schema;
#[expect(
    dead_code,
    reason = "the build script imports the shared item validator but exercises only its preflight surface"
)]
#[path = "src/item_catalog_validation.rs"]
mod item_catalog_validation;
#[expect(
    dead_code,
    reason = "the build script imports shared item references but reads only the validation subset"
)]
#[path = "src/item_references.rs"]
mod item_references;
#[path = "src/organization_catalog_validation.rs"]
mod organization_catalog_validation;
#[path = "src/quest_catalog_validation.rs"]
mod quest_catalog_validation;
#[expect(
    unexpected_cfgs,
    dead_code,
    unused_imports,
    reason = "the shared runtime encounter module is also compiled as a build-script schema"
)]
#[path = "src/road_encounter_catalog.rs"]
mod road_encounter_catalog;
#[path = "src/threat_escalation_limits.rs"]
mod threat_escalation_limits;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(runtime_catalog)");
    println!("cargo:rustc-cfg=runtime_catalog");
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    compile_organizations(&root);
    compile_items(&root);
    compile_road_encounters(&root);
    let content = root.join("content/quests");
    println!("cargo:rerun-if-changed={}", content.display());
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("content/quests must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    files.sort();

    let mut documents = Vec::new();
    let mut source_files = Vec::new();
    let mut dialogue_variant_sources = Vec::new();
    let mut digest = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&file).unwrap();
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(text.as_bytes());
        // As with dialogue, quest YAML deliberately uses JSON's strict,
        // diff-friendly YAML subset so deployed binaries need no YAML parser.
        let value = item_catalog_validation::parse_document(&text)
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
        collect_dialogue_variant_sources(
            &value,
            &text,
            &relative,
            documents.len(),
            &mut dialogue_variant_sources,
        );
        documents.push(value);
        source_files.push(relative);
    }
    assert!(!documents.is_empty(), "content/quests contains no YAML");
    quest_catalog_validation::validate_documents(&documents, &source_files)
        .unwrap_or_else(|error| panic!("{error}"));
    let json = serde_json::to_string(&documents).unwrap();
    let digest = format!("{:x}", digest.finalize());
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("quest_catalog.rs"),
        format!(
            "pub const QUEST_CATALOG_JSON: &str = {json:?};\n\
             pub const QUEST_DIALOGUE_VARIANT_SOURCE_MAP_JSON: &str = {sources:?};\n\
             pub const QUEST_CATALOG_DIGEST: &str = {digest:?};\n",
            sources = serde_json::to_string(&dialogue_variant_sources).unwrap(),
        ),
    )
    .unwrap();
}

fn compile_road_encounters(root: &Path) {
    let content = root.join("content/encounters");
    println!("cargo:rerun-if-changed={}", content.display());
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("content/encounters must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    files.sort();
    let mut definitions = Vec::new();
    let mut sources = Vec::new();
    let mut digest = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&file).unwrap();
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(text.as_bytes());
        let document: road_encounter_catalog::CatalogDocument =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}: {error}"));
        for definition in document.encounters {
            let encoded_id = serde_json::to_string(&definition.id).unwrap();
            let offset = text
                .find(&encoded_id)
                .unwrap_or_else(|| panic!("{relative}: cannot locate encounter ID"));
            sources.push(road_encounter_catalog::EncounterSource {
                id: definition.id.clone(),
                file: relative.clone(),
                line: (text[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1) as u32,
            });
            definitions.push(definition);
        }
    }
    assert!(
        !definitions.is_empty(),
        "content/encounters contains no encounters"
    );
    road_encounter_catalog::validate_definitions(&definitions)
        .unwrap_or_else(|error| panic!("road encounter catalog: {error}"));
    let item_content = root.join("content/items");
    let mut item_ids = std::collections::BTreeSet::new();
    for file in fs::read_dir(item_content)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
    {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        if let Some(items) = value.get("items").and_then(serde_json::Value::as_array) {
            item_ids.extend(
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    road_encounter_catalog::validate_item_references(&definitions, |id| item_ids.contains(id))
        .unwrap_or_else(|error| panic!("road encounter catalog: {error}"));
    let json = serde_json::to_string(&definitions).unwrap();
    let sources = serde_json::to_string(&sources).unwrap();
    let digest = format!("{:x}", digest.finalize());
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("road_encounter_catalog.rs"),
        format!(
            "pub const ROAD_ENCOUNTER_CATALOG_JSON: &str = {json:?};\n\
             pub const ROAD_ENCOUNTER_SOURCE_MAP_JSON: &str = {sources:?};\n\
             pub const ROAD_ENCOUNTER_CATALOG_DIGEST: &str = {digest:?};\n"
        ),
    )
    .unwrap();
}

fn compile_items(root: &Path) {
    let content = root.join("content/items");
    println!("cargo:rerun-if-changed={}", content.display());
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("content/items must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    files.sort();
    let mut documents = Vec::new();
    let mut source_files = Vec::new();
    let mut sources = Vec::new();
    let mut item_sources = Vec::new();
    let mut digest = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&file).unwrap();
        assert!(
            text.len() <= item_catalog_validation::MAX_SOURCE_BYTES,
            "{relative}: item catalog source is too large"
        );
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(text.as_bytes());
        let value = item_catalog_validation::parse_document(&text)
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
        collect_item_sources(&value, &text, &relative, &mut item_sources);
        documents.push(value);
        source_files.push(relative);
        sources.push(text);
    }
    assert!(!documents.is_empty(), "content/items contains no YAML");
    item_catalog_validation::validate_documents_with_sources(&documents, &source_files, &sources)
        .unwrap_or_else(|error| panic!("{error}"));
    let icons = root.join("crates/strategic-web/static/icons/game");
    for document in &documents {
        for item in document["items"].as_array().unwrap() {
            let id = item["id"].as_str().unwrap();
            let icon = item["presentation"]["icon"].as_str().unwrap();
            assert!(
                icons.join(format!("{icon}.svg")).is_file(),
                "item {id}.presentation.icon: missing vendored icon {icon}.svg"
            );
        }
    }
    let json = serde_json::to_string(&documents).unwrap();
    let source_map = serde_json::to_string(&item_sources).unwrap();
    let digest = format!("{:x}", digest.finalize());
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("item_catalog.rs"),
        format!(
            "pub const ITEM_CATALOG_JSON: &str = {json:?};\n\
             pub const ITEM_CATALOG_SOURCE_MAP_JSON: &str = {source_map:?};\n\
             pub const ITEM_CATALOG_DIGEST: &str = {digest:?};\n"
        ),
    )
    .unwrap();
}

/// Record the exact authored `id` token for each item while the build owns the
/// parsed document and its raw source. IDs are unique after validation, so the
/// resulting map remains stable even when definitions move between files.
fn collect_item_sources(
    value: &serde_json::Value,
    text: &str,
    file: &str,
    out: &mut Vec<serde_json::Value>,
) {
    let Some(items) = value.get("items").and_then(serde_json::Value::as_array) else {
        return;
    };
    let mut cursor = 0usize;
    for item in items {
        let Some(id) = item.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let encoded_id = serde_json::to_string(id).unwrap();
        let start = locate_item_id_token(text, cursor, &encoded_id)
            .unwrap_or_else(|| panic!("{file}: could not locate item {id:?} ID token"));
        cursor = start + encoded_id.len();
        let before = &text[..start];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(1, |line| line.chars().count() + 1);
        out.push(serde_json::json!({
            "id": id,
            "file": file,
            "line": line,
            "column": column,
        }));
    }
}

fn locate_item_id_token(text: &str, cursor: usize, encoded_id: &str) -> Option<usize> {
    let mut search = cursor;
    while let Some(relative) = text[search..].find("\"id\"") {
        let key_start = search + relative;
        let after_key = &text[key_start + "\"id\"".len()..];
        let key_whitespace = after_key.len() - after_key.trim_start().len();
        let rest = after_key.trim_start();
        let Some(after_colon) = rest.strip_prefix(':') else {
            search = key_start + "\"id\"".len();
            continue;
        };
        let value_whitespace = after_colon.len() - after_colon.trim_start().len();
        let value_start = key_start + "\"id\"".len() + key_whitespace + 1 + value_whitespace;
        if text[value_start..].starts_with(encoded_id) {
            return Some(value_start);
        }
        search = key_start + "\"id\"".len();
    }
    None
}

/// The quest authoring format is the strict JSON YAML subset. Record the
/// exact scalar token for each variant template while the build compiler owns
/// both the raw source and its parsed structural path.
fn collect_dialogue_variant_sources(
    value: &serde_json::Value,
    text: &str,
    file: &str,
    document: usize,
    out: &mut Vec<serde_json::Value>,
) {
    let Some(variants) = value
        .get("dialogue_variants")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    let mut cursor = 0usize;
    for (index, variant) in variants.iter().enumerate() {
        let Some(template) = variant.get("template").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let encoded = serde_json::to_string(template).unwrap();
        let start = text[cursor..]
            .find(&encoded)
            .map(|offset| cursor + offset)
            .unwrap_or_else(|| panic!("{file}: could not locate dialogue variant template token"));
        cursor = start + encoded.len();
        let before = &text[..start];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(1, |line| line.chars().count() + 1);
        out.push(serde_json::json!({
            "document": document,
            "path": format!("dialogue_variants.{index}.template"),
            "file": file,
            "line": line,
            "column": column,
            "value_json": encoded,
        }));
    }
}

fn compile_organizations(root: &Path) {
    // Organization roles and kinds are validated here before embedding.
    let content = root.join("content/organizations");
    let policies_path = root.join("content/settlement-policies.yaml");
    let promotions_path = root.join("content/organization-promotion-transitions.yaml");
    println!("cargo:rerun-if-changed={}", content.display());
    println!("cargo:rerun-if-changed={}", policies_path.display());
    println!("cargo:rerun-if-changed={}", promotions_path.display());
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("content/organizations must exist")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    files.sort();

    let mut documents = Vec::new();
    let mut source_files = Vec::new();
    let mut digest = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&file).unwrap();
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(text.as_bytes());
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}: {error}"));
        match value {
            serde_json::Value::Array(values) => {
                source_files.extend(std::iter::repeat_n(relative, values.len()));
                documents.extend(values);
            }
            value => {
                documents.push(value);
                source_files.push(relative);
            }
        }
    }
    assert!(
        !documents.is_empty(),
        "content/organizations contains no YAML"
    );

    let policy_relative = policies_path
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let policy_text = fs::read_to_string(&policies_path).unwrap();
    digest.update(policy_relative.as_bytes());
    digest.update([0]);
    digest.update(policy_text.as_bytes());
    let policies: serde_json::Value = serde_json::from_str(&policy_text)
        .unwrap_or_else(|error| panic!("{policy_relative}: {error}"));
    organization_catalog_validation::validate_documents(
        &documents,
        &source_files,
        &policies,
        &policy_relative,
    )
    .unwrap_or_else(|error| panic!("{error}"));

    let promotions_text = fs::read_to_string(&promotions_path).unwrap();
    digest.update(b"content/organization-promotion-transitions.yaml");
    digest.update([0]);
    digest.update(promotions_text.as_bytes());
    let promotions: serde_json::Value = serde_json::from_str(&promotions_text)
        .unwrap_or_else(|error| panic!("organization promotion transitions: {error}"));
    let promotions = promotions
        .as_array()
        .expect("promotion transitions must be an array");
    let mut seen = std::collections::BTreeSet::new();
    for transition in promotions {
        let organization_id = transition["organization_id"]
            .as_str()
            .expect("promotion organization_id");
        let from = transition["from_role_id"]
            .as_str()
            .expect("promotion from_role_id");
        let to = transition["to_role_id"]
            .as_str()
            .expect("promotion to_role_id");
        assert!(
            from != to && seen.insert((organization_id, from, to)),
            "duplicate or reflexive promotion transition"
        );
        let organization = documents
            .iter()
            .find(|document| document["id"] == organization_id)
            .expect("promotion references unknown organization");
        let roles = organization["roles"].as_array().unwrap();
        assert!(
            roles.iter().any(|role| role["id"] == from),
            "promotion references unknown source role"
        );
        assert!(
            roles.iter().any(|role| role["id"] == to),
            "promotion references unknown target role"
        );
    }

    let catalog = serde_json::json!({
        "organizations": documents,
        "promotion_transitions": promotions,
        "settlement_policies": policies,
    });
    let json = serde_json::to_string(&catalog).unwrap();
    let digest = format!("{:x}", digest.finalize());
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("organization_catalog.rs"),
        format!(
            "pub const ORGANIZATION_CATALOG_JSON: &str = {json:?};\n\
             pub const ORGANIZATION_CATALOG_DIGEST: &str = {digest:?};\n"
        ),
    )
    .unwrap();
}
