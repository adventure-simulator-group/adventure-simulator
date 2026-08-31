use std::path::{Path, PathBuf};

/// Resolve a bare fixture stem from its committed asset directory while
/// preserving paths and filenames supplied explicitly.
#[must_use]
pub fn resolve_fixture_path(selector: &str, directory: &str, extension: &str) -> PathBuf {
    let explicit = Path::new(selector);
    if explicit.components().count() != 1 || explicit.extension().is_some() {
        return explicit.to_path_buf();
    }

    let relative = Path::new(directory)
        .join(selector)
        .with_extension(extension);
    if relative.is_file() {
        return relative;
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_stems_resolve_but_explicit_paths_are_preserved() {
        assert!(
            resolve_fixture_path("dense-woodland", "assets/tactical-scenes", "json")
                .ends_with("assets/tactical-scenes/dense-woodland.json")
        );
        let explicit = PathBuf::from("custom/scenes/test.fixture");
        assert_eq!(
            resolve_fixture_path(explicit.to_str().unwrap(), "assets/tactical-scenes", "json"),
            explicit
        );
    }
}
