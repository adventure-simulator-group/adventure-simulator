use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn scalar_paths(value: &Value, path: &mut Vec<String>, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                path.push(key.to_owned());
                scalar_paths(value, path, out);
                path.pop();
            }
        }
        Value::Array(seq) => {
            for (index, value) in seq.iter().enumerate() {
                path.push(index.to_string());
                scalar_paths(value, path, out);
                path.pop();
            }
        }
        _ => out.push(path.join(".")),
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
    for file in files {
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
        let value: Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid {relative}: {e}"));
        let json = value.clone();
        let mut paths = Vec::new();
        scalar_paths(&value, &mut Vec::new(), &mut paths);
        // YAML is deliberately one field per line. Source spans are compiler-derived, never authored.
        for path in paths {
            let leaf = path.rsplit('.').next().unwrap();
            let needle = format!("\"{leaf}\":");
            let line = text
                .lines()
                .position(|line| line.trim_start().starts_with(&needle))
                .map_or(1, |i| i + 1);
            let column = text
                .lines()
                .nth(line - 1)
                .and_then(|line| line.find(&needle))
                .map_or(1, |i| i + 1);
            sources.push(
                serde_json::json!({"path": path, "file": relative, "line": line, "column": column}),
            );
        }
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
