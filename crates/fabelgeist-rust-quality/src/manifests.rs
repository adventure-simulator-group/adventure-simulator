use std::{collections::BTreeSet, fs, path::Path};

use toml::Value;
use walkdir::WalkDir;

pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let root_manifest = read_manifest(&root.join("Cargo.toml"))?;
    let workspace = root_manifest
        .get("workspace")
        .and_then(Value::as_table)
        .ok_or("root Cargo.toml has no workspace table")?;
    let members = workspace
        .get("members")
        .and_then(Value::as_array)
        .ok_or("root workspace has no members")?
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    let root_policy = workspace
        .get("lints")
        .and_then(Value::as_table)
        .and_then(|lints| lints.get("clippy"))
        .ok_or("root workspace has no Clippy policy")?;

    let mut diagnostics = Vec::new();
    for entry in WalkDir::new(root.join("crates")).min_depth(2).max_depth(2) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() != "Cargo.toml" {
            continue;
        }
        let manifest_path = entry.path();
        let crate_dir = manifest_path
            .parent()
            .and_then(|path| path.strip_prefix(root).ok())
            .ok_or_else(|| format!("invalid crate manifest path {}", manifest_path.display()))?;
        let relative = crate_dir.to_string_lossy().replace('\\', "/");
        let manifest = read_manifest(manifest_path)?;
        if members.contains(&relative) {
            let inherits = manifest
                .get("lints")
                .and_then(Value::as_table)
                .and_then(|lints| lints.get("workspace"))
                .and_then(Value::as_bool)
                == Some(true);
            if !inherits {
                diagnostics.push(format!(
                    "{relative}/Cargo.toml: workspace crate must declare `[lints] workspace = true`"
                ));
            }
        } else {
            let standalone = manifest.get("workspace").is_some();
            let policy = manifest
                .get("lints")
                .and_then(Value::as_table)
                .and_then(|lints| lints.get("clippy"));
            if !standalone || policy != Some(root_policy) {
                diagnostics.push(format!(
                    "{relative}/Cargo.toml: standalone crate must define a workspace and copy the root Clippy policy exactly"
                ));
            }
        }
    }
    Ok(diagnostics)
}

fn read_manifest(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("invalid {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::*;

    #[test]
    fn checks_workspace_and_standalone_lint_policy() {
        let root = env::temp_dir().join(format!(
            "fabelgeist-rust-quality-manifests-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(root.join("crates/member")).unwrap();
        fs::create_dir_all(root.join("crates/standalone")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/member"]
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
"#,
        )
        .unwrap();
        fs::write(
            root.join("crates/member/Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\n[lints]\nworkspace = true\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/standalone/Cargo.toml"),
            r#"
[package]
name = "standalone"
version = "0.1.0"
[workspace]
[lints.clippy]
all = { level = "warn", priority = -1 }
"#,
        )
        .unwrap();
        assert!(check(&root).unwrap().is_empty());

        fs::write(
            root.join("crates/standalone/Cargo.toml"),
            "[package]\nname = \"standalone\"\nversion = \"0.1.0\"\n[workspace]\n",
        )
        .unwrap();
        assert_eq!(check(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
