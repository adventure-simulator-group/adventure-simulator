use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
#[path = "src/quest_catalog_validation.rs"]
mod quest_catalog_validation;

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
    let mut source_files = Vec::new();
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
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}: {error}"));
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
             pub const QUEST_CATALOG_DIGEST: &str = {digest:?};\n"
        ),
    )
    .unwrap();
}
