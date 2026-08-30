use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use walkdir::WalkDir;

use crate::{
    config::{Baseline, Config, Debt, Exception, FindingBaseline},
    manifests,
    scan::{Finding, FunctionSize, LiteralOccurrence, path_matches, scan},
};

#[derive(Clone, Debug)]
pub struct CensusEntry {
    value: String,
    occurrences: Vec<(String, usize)>,
}

pub struct Report {
    pub diagnostics: Vec<String>,
    pub census: Vec<CensusEntry>,
    pub snapshot: Baseline,
}

pub fn check_repository(
    root: &Path,
    config: &Config,
    baseline: &Baseline,
) -> Result<Report, String> {
    let mut files = BTreeMap::new();
    let mut functions = Vec::new();
    let mut findings = Vec::new();
    let mut literals = Vec::new();
    let mut seen_items: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for entry in WalkDir::new(root.join("crates")) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("rs")
        {
            continue;
        }
        let relative = relative_path(root, entry.path())?;
        if excluded(config, &relative) || test_file(&relative) {
            continue;
        }
        let source = fs::read_to_string(entry.path())
            .map_err(|error| format!("cannot read {relative}: {error}"))?;
        let syntax = syn::parse_file(&source)
            .map_err(|error| format!("cannot parse {relative}: {error}"))?;
        let result = scan(&relative, &source, &syntax, config);
        files.insert(relative.clone(), result.production_lines);
        functions.extend(result.functions);
        findings.extend(result.findings);
        literals.extend(result.literals);
        seen_items.insert(relative, result.seen_items);
    }

    let grouped_findings = group_findings(&findings);
    let mut diagnostics = manifests::check(root)?;
    validate_scopes(config, &files, &seen_items, &mut diagnostics);
    validate_findings(config, baseline, &grouped_findings, &mut diagnostics);
    validate_debt(config, baseline, &files, &functions, &mut diagnostics);
    let census = build_census(literals, config.census_minimum);
    let snapshot = snapshot(config, files, functions, grouped_findings);
    diagnostics.sort();
    diagnostics.dedup();
    Ok(Report {
        diagnostics,
        census,
        snapshot,
    })
}

pub fn print_census(entries: &[CensusEntry], limit: usize) {
    if entries.is_empty() {
        println!("semantic-value census: no repeated candidates");
        return;
    }
    println!(
        "semantic-value census: {} repeated candidate(s), advisory only",
        entries.len()
    );
    for entry in entries.iter().take(limit) {
        let samples = entry
            .occurrences
            .iter()
            .take(3)
            .map(|(path, line)| format!("{path}:{line}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:?}: {} occurrence(s) ({samples})",
            entry.value,
            entry.occurrences.len()
        );
    }
    if entries.len() > limit {
        println!("  ... run the census subcommand for all candidates");
    }
}

pub fn print_baseline(baseline: &Baseline) -> Result<(), String> {
    let encoded = toml::to_string_pretty(baseline)
        .map_err(|error| format!("cannot serialize baseline: {error}"))?;
    print!("{encoded}");
    Ok(())
}

fn validate_scopes(
    config: &Config,
    files: &BTreeMap<String, usize>,
    items: &BTreeMap<String, BTreeSet<String>>,
    diagnostics: &mut Vec<String>,
) {
    for scope in &config.scopes {
        let matching_paths = files
            .keys()
            .filter(|path| path_matches(&scope.path, path))
            .collect::<Vec<_>>();
        if matching_paths.is_empty() {
            diagnostics.push(format!(
                "rust-quality.toml: stale {:?} scope {} matches no production Rust file",
                scope.kind, scope.path
            ));
        } else if let Some(item) = &scope.item
            && !matching_paths
                .iter()
                .any(|path| items.get(*path).is_some_and(|found| found.contains(item)))
        {
            diagnostics.push(format!(
                "rust-quality.toml: stale {:?} scope {}::{item} matches no item",
                scope.kind, scope.path
            ));
        }
    }
}

fn validate_findings(
    config: &Config,
    baseline: &Baseline,
    actual: &BTreeMap<FindingKey, FindingGroup>,
    diagnostics: &mut Vec<String>,
) {
    let baselined = baseline
        .findings
        .iter()
        .map(|finding| (FindingKey::from_baseline(finding), finding.occurrences))
        .collect::<BTreeMap<_, _>>();
    let exceptions = config
        .exceptions
        .iter()
        .map(|exception| (FindingKey::from_exception(exception), exception))
        .collect::<BTreeMap<_, _>>();

    for (key, group) in actual {
        if let Some(expected) = baselined.get(key) {
            if *expected != group.count {
                diagnostics.push(format!(
                    "{}:{}: baseline for {} {} expected {} occurrence(s), found {}; tighten it",
                    key.path, group.first_line, key.rule, key.fingerprint, expected, group.count
                ));
            }
        } else if let Some(exception) = exceptions.get(key) {
            if exception.occurrences != group.count {
                diagnostics.push(format!(
                    "{}:{}: exception for {} {} expected {} occurrence(s), found {}",
                    key.path,
                    group.first_line,
                    key.rule,
                    key.fingerprint,
                    exception.occurrences,
                    group.count
                ));
            }
        } else {
            diagnostics.push(format!(
                "{}:{}: {} in {} ({})",
                key.path, group.first_line, key.rule, key.item, key.fingerprint
            ));
        }
    }
    for (key, expected) in &baselined {
        if !actual.contains_key(key) {
            diagnostics.push(format!(
                "rust-quality-baseline.toml: stale finding {} {} in {}::{} ({} occurrence(s))",
                key.rule, key.fingerprint, key.path, key.item, expected
            ));
        }
    }
    for (key, exception) in &exceptions {
        if !actual.contains_key(key) {
            diagnostics.push(format!(
                "rust-quality.toml: stale exception {} {} in {}::{} ({})",
                key.rule, key.fingerprint, key.path, key.item, exception.reason
            ));
        }
    }
}

fn validate_debt(
    config: &Config,
    baseline: &Baseline,
    files: &BTreeMap<String, usize>,
    functions: &[FunctionSize],
    diagnostics: &mut Vec<String>,
) {
    let file_baseline = baseline
        .files
        .iter()
        .map(|debt| (debt.path.as_str(), debt.ceiling))
        .collect::<BTreeMap<_, _>>();
    for (path, lines) in files {
        if *lines > config.file_limit {
            match file_baseline.get(path.as_str()) {
                Some(ceiling) if lines <= ceiling => {}
                Some(ceiling) => diagnostics.push(format!(
                    "{path}: production file grew to {lines} lines above its {ceiling}-line ceiling"
                )),
                None => diagnostics.push(format!(
                    "{path}: new production file debt is {lines} lines (limit {})",
                    config.file_limit
                )),
            }
        }
    }
    for debt in &baseline.files {
        match files.get(&debt.path) {
            None => diagnostics.push(format!(
                "rust-quality-baseline.toml: stale file debt for missing {}",
                debt.path
            )),
            Some(lines) if *lines <= config.file_limit => diagnostics.push(format!(
                "rust-quality-baseline.toml: {} is now below the file limit; remove its debt entry",
                debt.path
            )),
            _ => {}
        }
    }

    let actual_functions = functions
        .iter()
        .map(|function| {
            (
                (function.path.as_str(), function.item.as_str()),
                function.lines,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let function_baseline = baseline
        .functions
        .iter()
        .filter_map(|debt| {
            debt.item
                .as_deref()
                .map(|item| ((debt.path.as_str(), item), debt.ceiling))
        })
        .collect::<BTreeMap<_, _>>();
    for ((path, item), lines) in &actual_functions {
        if *lines > config.function_limit {
            match function_baseline.get(&(*path, *item)) {
                Some(ceiling) if lines <= ceiling => {}
                Some(ceiling) => diagnostics.push(format!(
                    "{path}::{item}: function grew to {lines} lines above its {ceiling}-line ceiling"
                )),
                None => diagnostics.push(format!(
                    "{path}::{item}: new function debt is {lines} lines (limit {})",
                    config.function_limit
                )),
            }
        }
    }
    for debt in &baseline.functions {
        let Some(item) = debt.item.as_deref() else {
            diagnostics.push(format!(
                "rust-quality-baseline.toml: function debt {} has no item",
                debt.path
            ));
            continue;
        };
        match actual_functions.get(&(debt.path.as_str(), item)) {
            None => diagnostics.push(format!(
                "rust-quality-baseline.toml: stale function debt for {}::{item}",
                debt.path
            )),
            Some(lines) if *lines <= config.function_limit => diagnostics.push(format!(
                "rust-quality-baseline.toml: {}::{item} is now below the function limit; remove its debt entry",
                debt.path
            )),
            _ => {}
        }
    }
}

fn snapshot(
    config: &Config,
    files: BTreeMap<String, usize>,
    mut functions: Vec<FunctionSize>,
    findings: BTreeMap<FindingKey, FindingGroup>,
) -> Baseline {
    let mut file_debt = files
        .into_iter()
        .filter(|(_, lines)| *lines > config.file_limit)
        .map(|(path, ceiling)| Debt {
            path,
            item: None,
            ceiling,
        })
        .collect::<Vec<_>>();
    functions.sort();
    let function_debt = functions
        .into_iter()
        .filter(|function| function.lines > config.function_limit)
        .map(|function| Debt {
            path: function.path,
            item: Some(function.item),
            ceiling: function.lines,
        })
        .collect();
    let finding_debt = findings
        .into_iter()
        .map(|(key, group)| FindingBaseline {
            rule: key.rule,
            path: key.path,
            item: key.item,
            fingerprint: key.fingerprint,
            occurrences: group.count,
        })
        .collect();
    file_debt.sort_by(|left, right| left.path.cmp(&right.path));
    Baseline {
        files: file_debt,
        functions: function_debt,
        findings: finding_debt,
    }
}

fn build_census(literals: Vec<LiteralOccurrence>, minimum: usize) -> Vec<CensusEntry> {
    let mut grouped: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    for literal in literals {
        grouped
            .entry(literal.value)
            .or_default()
            .push((literal.path, literal.line));
    }
    let mut entries = grouped
        .into_iter()
        .filter(|(_, occurrences)| occurrences.len() >= minimum)
        .map(|(value, occurrences)| CensusEntry { value, occurrences })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .occurrences
            .len()
            .cmp(&left.occurrences.len())
            .then_with(|| left.value.cmp(&right.value))
    });
    entries
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FindingKey {
    rule: String,
    path: String,
    item: String,
    fingerprint: String,
}

impl FindingKey {
    fn from_baseline(value: &FindingBaseline) -> Self {
        Self {
            rule: value.rule.clone(),
            path: value.path.clone(),
            item: value.item.clone(),
            fingerprint: value.fingerprint.clone(),
        }
    }

    fn from_exception(value: &Exception) -> Self {
        Self {
            rule: value.rule.clone(),
            path: value.path.clone(),
            item: value.item.clone(),
            fingerprint: value.fingerprint.clone(),
        }
    }
}

struct FindingGroup {
    count: usize,
    first_line: usize,
}

fn group_findings(findings: &[Finding]) -> BTreeMap<FindingKey, FindingGroup> {
    let mut grouped = BTreeMap::new();
    for finding in findings {
        let key = FindingKey {
            rule: finding.rule.clone(),
            path: finding.path.clone(),
            item: finding.item.clone(),
            fingerprint: finding.fingerprint.clone(),
        };
        let group = grouped.entry(key).or_insert(FindingGroup {
            count: 0,
            first_line: finding.line,
        });
        group.count += 1;
        group.first_line = group.first_line.min(finding.line);
    }
    grouped
}

fn excluded(config: &Config, path: &str) -> bool {
    path.contains("/target/")
        || config
            .excluded_paths
            .iter()
            .any(|pattern| path_matches(pattern, path))
}

fn test_file(path: &str) -> bool {
    path.contains("/tests/") || path.ends_with("_test.rs") || path.ends_with("/tests.rs")
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside {}", path.display(), root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_files_are_not_production() {
        assert!(test_file("crates/example/tests/behavior.rs"));
        assert!(test_file("crates/example/src/tests.rs"));
        assert!(!test_file("crates/example/src/lib.rs"));
    }

    #[test]
    fn standalone_build_outputs_are_excluded() {
        let config: Config = toml::from_str("file_limit = 500\nfunction_limit = 100\n").unwrap();
        assert!(excluded(
            &config,
            "crates/example/target/debug/build/generated.rs"
        ));
    }

    #[test]
    fn stale_exceptions_fail() {
        let config = Config {
            file_limit: 500,
            function_limit: 100,
            census_minimum: 3,
            census_summary_limit: 20,
            excluded_paths: Vec::new(),
            scopes: Vec::new(),
            semantic_families: Vec::new(),
            exceptions: vec![Exception {
                rule: "raw-string-branch".into(),
                path: "crates/example/src/lib.rs".into(),
                item: "decide".into(),
                fingerprint: "string:pending".into(),
                occurrences: 1,
                reason: "External protocol adapter value.".into(),
            }],
        };
        let mut diagnostics = Vec::new();
        validate_findings(
            &config,
            &Baseline::default(),
            &BTreeMap::new(),
            &mut diagnostics,
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("stale exception"));
    }

    #[test]
    fn debt_can_shrink_but_cannot_grow_past_its_ceiling() {
        let config = Config {
            file_limit: 500,
            function_limit: 100,
            census_minimum: 3,
            census_summary_limit: 20,
            excluded_paths: Vec::new(),
            scopes: Vec::new(),
            semantic_families: Vec::new(),
            exceptions: Vec::new(),
        };
        let baseline = Baseline {
            files: vec![Debt {
                path: "crates/example/src/lib.rs".into(),
                item: None,
                ceiling: 550,
            }],
            functions: Vec::new(),
            findings: Vec::new(),
        };
        let mut files = BTreeMap::from([("crates/example/src/lib.rs".into(), 525)]);
        let mut diagnostics = Vec::new();
        validate_debt(&config, &baseline, &files, &[], &mut diagnostics);
        assert!(diagnostics.is_empty());

        files.insert("crates/example/src/lib.rs".into(), 551);
        validate_debt(&config, &baseline, &files, &[], &mut diagnostics);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("grew to 551"))
        );
    }
}
