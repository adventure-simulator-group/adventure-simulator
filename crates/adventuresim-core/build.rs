use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    monsters: Vec<Identified>,
    #[serde(default)]
    evidence: Vec<Identified>,
    #[serde(default)]
    witness_demographics: Vec<Identified>,
    #[serde(default)]
    circumstances: Vec<Identified>,
    #[serde(default)]
    sites: Vec<Identified>,
    #[serde(default)]
    descriptions: Vec<Identified>,
    #[serde(default)]
    templates: Vec<Identified>,
    #[serde(default)]
    relations: Vec<Relation>,
    #[serde(default)]
    bridges: Vec<Identified>,
}

#[derive(Deserialize)]
struct Identified {
    id: String,
}

#[derive(Deserialize)]
struct Relation {
    id: String,
    candidates: Vec<RelationCandidate>,
}

#[derive(Deserialize)]
struct RelationCandidate {
    id: String,
    plausibility: u32,
    curation: u32,
    #[serde(default)]
    hard_zero_reason: Option<String>,
    #[serde(default)]
    required_bridge: Option<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.' | b':')
        })
}

fn insert(ids: &mut BTreeSet<String>, kind: &str, id: &str, file: &str) {
    assert!(valid_id(id), "{file}: invalid {kind} id {id:?}");
    assert!(
        ids.insert(format!("{kind}:{id}")),
        "{file}: duplicate {kind} id {id}"
    );
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
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
    let mut digest = Sha256::new();
    let mut ids = BTreeSet::new();
    let mut bridge_ids = BTreeSet::new();
    let mut pending_bridges = Vec::new();
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
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}: {error}"));
        let document: Document = serde_json::from_value(value.clone())
            .unwrap_or_else(|error| panic!("{relative}: invalid quest catalog: {error}"));
        for item in &document.monsters {
            insert(&mut ids, "monster", &item.id, &relative);
        }
        for item in &document.evidence {
            insert(&mut ids, "evidence", &item.id, &relative);
        }
        for item in &document.witness_demographics {
            insert(&mut ids, "witness_demographic", &item.id, &relative);
        }
        for item in &document.circumstances {
            insert(&mut ids, "circumstance", &item.id, &relative);
        }
        for item in &document.sites {
            insert(&mut ids, "site", &item.id, &relative);
        }
        for item in &document.descriptions {
            insert(&mut ids, "description", &item.id, &relative);
        }
        for item in &document.templates {
            insert(&mut ids, "template", &item.id, &relative);
        }
        for item in &document.bridges {
            insert(&mut ids, "bridge", &item.id, &relative);
            bridge_ids.insert(item.id.clone());
        }
        for relation in &document.relations {
            insert(&mut ids, "relation", &relation.id, &relative);
            assert!(
                !relation.candidates.is_empty(),
                "{relative}: relation {} has no candidates",
                relation.id
            );
            let mut candidates = BTreeSet::new();
            for candidate in &relation.candidates {
                assert!(
                    candidates.insert(&candidate.id),
                    "{relative}: relation {} has duplicate candidate {}",
                    relation.id,
                    candidate.id
                );
                assert!(
                    valid_id(&candidate.id),
                    "{relative}: relation {} has invalid candidate id {}",
                    relation.id,
                    candidate.id
                );
                let zero = candidate.plausibility == 0 || candidate.curation == 0;
                assert_eq!(
                    zero,
                    candidate.hard_zero_reason.is_some(),
                    "{relative}: relation {} candidate {} must pair zero weight with a reason",
                    relation.id,
                    candidate.id
                );
                if let Some(reason) = &candidate.hard_zero_reason {
                    assert!(
                        !reason.trim().is_empty(),
                        "{relative}: empty hard-zero reason in {}",
                        relation.id
                    );
                }
                if let Some(bridge) = &candidate.required_bridge {
                    pending_bridges.push((relative.clone(), relation.id.clone(), bridge.clone()));
                }
            }
        }
        documents.push(value);
    }
    for (file, relation, bridge) in pending_bridges {
        assert!(
            bridge_ids.contains(&bridge),
            "{file}: relation {relation} references missing bridge {bridge}"
        );
    }
    assert!(!documents.is_empty(), "content/quests contains no YAML");
    let json = serde_json::to_string(&documents).unwrap();
    let digest = format!("{:x}", digest.finalize());
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("quest_catalog.rs"),
        format!(
            "pub const QUEST_CATALOG_JSON: &str = {json:?};\n\
             pub const QUEST_CATALOG_DIGEST: &str = {digest:?};\n"
        ),
    )
    .unwrap();
}
