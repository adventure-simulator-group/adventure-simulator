use std::collections::BTreeSet;

use proc_macro2::Span;
use quote::ToTokens;
use regex::Regex;
use syn::{
    Attribute, Expr, ExprIf, ExprLit, ExprMatch, ImplItemFn, ItemFn, ItemImpl, ItemMod, Lit, Macro,
    spanned::Spanned, visit::Visit,
};

use crate::config::{Config, ScopeKind};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Finding {
    pub rule: String,
    pub path: String,
    pub item: String,
    pub fingerprint: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FunctionSize {
    pub path: String,
    pub item: String,
    pub lines: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LiteralOccurrence {
    pub value: String,
    pub path: String,
    pub line: usize,
}

pub struct ScanResult {
    pub production_lines: usize,
    pub functions: Vec<FunctionSize>,
    pub findings: Vec<Finding>,
    pub literals: Vec<LiteralOccurrence>,
    pub seen_items: BTreeSet<String>,
}

pub fn scan(path: &str, source: &str, syntax: &syn::File, config: &Config) -> ScanResult {
    let mut scanner = Scanner {
        path,
        lines: source.lines().collect(),
        config,
        current_item: "<module>".into(),
        module_path: Vec::new(),
        impl_name: None,
        test_ranges: Vec::new(),
        functions: Vec::new(),
        findings: Vec::new(),
        literals: Vec::new(),
        seen_items: BTreeSet::new(),
        suppress_literals: 0,
        debug_names: BTreeSet::new(),
    };
    scanner.visit_file(syntax);
    let structural_exempt = config.scopes.iter().any(|scope| {
        scope.item.is_none()
            && path_matches(&scope.path, path)
            && matches!(
                scope.kind,
                ScopeKind::Fixture | ScopeKind::AuthoredCatalog | ScopeKind::ShaderSource
            )
    });
    let production_lines = if structural_exempt {
        scanner.functions.clear();
        0
    } else {
        count_lines(source, &scanner.test_ranges)
    };
    ScanResult {
        production_lines,
        functions: scanner.functions,
        findings: scanner.findings,
        literals: scanner.literals,
        seen_items: scanner.seen_items,
    }
}

struct Scanner<'a> {
    path: &'a str,
    lines: Vec<&'a str>,
    config: &'a Config,
    current_item: String,
    module_path: Vec<String>,
    impl_name: Option<String>,
    test_ranges: Vec<(usize, usize)>,
    functions: Vec<FunctionSize>,
    findings: Vec<Finding>,
    literals: Vec<LiteralOccurrence>,
    seen_items: BTreeSet<String>,
    suppress_literals: usize,
    debug_names: BTreeSet<String>,
}

impl Scanner<'_> {
    fn scoped(&self, kinds: &[ScopeKind]) -> bool {
        self.config.scopes.iter().any(|scope| {
            kinds.contains(&scope.kind)
                && path_matches(&scope.path, self.path)
                && scope
                    .item
                    .as_deref()
                    .is_none_or(|item| item == self.current_item)
        })
    }

    fn enter_function(
        &mut self,
        name: String,
        span: Span,
        debug_names: BTreeSet<String>,
        body: impl FnOnce(&mut Self),
    ) {
        let previous = std::mem::replace(&mut self.current_item, name.clone());
        let previous_debug = std::mem::replace(&mut self.debug_names, debug_names);
        self.seen_items.insert(name.clone());
        self.functions.push(FunctionSize {
            path: self.path.into(),
            item: name,
            lines: count_range(&self.lines, span.start().line, span.end().line),
        });
        body(self);
        self.debug_names = previous_debug;
        self.current_item = previous;
    }

    fn qualify(&self, name: impl AsRef<str>) -> String {
        if self.module_path.is_empty() {
            name.as_ref().into()
        } else {
            format!("{}::{}", self.module_path.join("::"), name.as_ref())
        }
    }

    fn record_branch(&mut self, expression: &Expr, line: usize) {
        if self.scoped(&[
            ScopeKind::Fixture,
            ScopeKind::AuthoredCatalog,
            ScopeKind::ShaderSource,
            ScopeKind::BoundaryAdapter,
        ]) {
            return;
        }
        let mut probe = BranchProbe::default();
        probe.visit_expr(expression);
        for literal in probe.strings {
            self.findings.push(Finding {
                rule: "raw-string-branch".into(),
                path: self.path.into(),
                item: self.current_item.clone(),
                fingerprint: format!("string:{literal}"),
                line,
            });
        }
        if probe.debug_output {
            self.findings.push(Finding {
                rule: "debug-output-branch".into(),
                path: self.path.into(),
                item: self.current_item.clone(),
                fingerprint: "debug-format".into(),
                line,
            });
        }
        let branch = compact_tokens(expression);
        for name in &self.debug_names {
            if branch.contains(name) {
                self.findings.push(Finding {
                    rule: "debug-output-branch".into(),
                    path: self.path.into(),
                    item: self.current_item.clone(),
                    fingerprint: format!("debug-local:{name}"),
                    line,
                });
            }
        }
    }

    fn record_literal(&mut self, literal: &Lit, line: usize) {
        if self.suppress_literals > 0
            || self.scoped(&[
                ScopeKind::Fixture,
                ScopeKind::AuthoredCatalog,
                ScopeKind::ShaderSource,
                ScopeKind::BoundaryAdapter,
            ])
        {
            return;
        }
        let Some(value) = normalized_literal(literal) else {
            return;
        };
        let intrinsically_clear = value
            .parse::<f64>()
            .is_ok_and(|number| number == 0.0 || number == 1.0);
        if !intrinsically_clear && !value.is_empty() {
            self.literals.push(LiteralOccurrence {
                value: value.clone(),
                path: self.path.into(),
                line,
            });
        }
        let context = self
            .lines
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or("");
        for family in &self.config.semantic_families {
            if family.literals.iter().any(|literal| literal == &value)
                && !family
                    .owners
                    .iter()
                    .any(|owner| path_matches(owner, self.path))
                && Regex::new(&family.identifier_pattern)
                    .expect("configuration was validated")
                    .is_match(context)
            {
                self.findings.push(Finding {
                    rule: format!("semantic-family:{}", family.name),
                    path: self.path.into(),
                    item: self.current_item.clone(),
                    fingerprint: value.clone(),
                    line,
                });
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_test(&node.attrs) {
            remember_range(&mut self.test_ranges, node.span());
            return;
        }
        if let Some((_, items)) = &node.content {
            self.module_path.push(node.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module_path.pop();
        }
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_test(&node.attrs) {
            remember_range(&mut self.test_ranges, node.span());
            return;
        }
        let name = self.qualify(configured_name(node.sig.ident.to_string(), &node.attrs));
        self.enter_function(name, node.span(), debug_bindings(&node.block), |scanner| {
            syn::visit::visit_block(scanner, &node.block);
        });
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let previous = self.impl_name.replace(compact_tokens(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_name = previous;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if is_test(&node.attrs) {
            remember_range(&mut self.test_ranges, node.span());
            return;
        }
        let name = self.qualify(configured_name(
            format!(
                "{}::{}",
                self.impl_name.as_deref().unwrap_or("impl"),
                node.sig.ident
            ),
            &node.attrs,
        ));
        self.enter_function(name, node.span(), debug_bindings(&node.block), |scanner| {
            syn::visit::visit_block(scanner, &node.block);
        });
    }

    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        self.record_branch(&node.cond, node.if_token.span.start().line);
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        for arm in &node.arms {
            let pattern = compact_tokens(&arm.pat);
            for literal in quoted_strings(&pattern) {
                if !self.scoped(&[ScopeKind::BoundaryAdapter]) {
                    self.findings.push(Finding {
                        rule: "raw-string-branch".into(),
                        path: self.path.into(),
                        item: self.current_item.clone(),
                        fingerprint: format!("string:{literal}"),
                        line: arm.pat.span().start().line,
                    });
                }
            }
        }
        self.record_branch(&node.expr, node.match_token.span.start().line);
        syn::visit::visit_expr_match(self, node);
    }

    fn visit_expr_lit(&mut self, node: &'ast ExprLit) {
        self.record_literal(&node.lit, node.span().start().line);
        syn::visit::visit_expr_lit(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        self.suppress_literals += 1;
        syn::visit::visit_macro(self, node);
        self.suppress_literals -= 1;
    }
}

#[derive(Default)]
struct BranchProbe {
    strings: Vec<String>,
    debug_output: bool,
}

impl<'ast> Visit<'ast> for BranchProbe {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.strings.push(node.value());
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let name = node
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let tokens = node.tokens.to_string();
        if matches!(name.as_deref(), Some("format" | "format_args"))
            && (tokens.contains(": ?") || tokens.contains(":# ?") || tokens.contains(":?"))
        {
            self.debug_output = true;
        }
        syn::visit::visit_macro(self, node);
    }
}

fn normalized_literal(literal: &Lit) -> Option<String> {
    match literal {
        Lit::Str(value) => Some(value.value()),
        Lit::Int(value) => Some(value.base10_digits().into()),
        Lit::Float(value) => Some(value.base10_digits().into()),
        _ => None,
    }
}

fn quoted_strings(tokens: &str) -> Vec<String> {
    let expression = format!("match value {{ {tokens} => (), _ => () }}");
    syn::parse_str::<ExprMatch>(&expression)
        .ok()
        .map(|parsed| {
            let mut probe = BranchProbe::default();
            probe.visit_pat(&parsed.arms[0].pat);
            probe.strings
        })
        .unwrap_or_default()
}

fn compact_tokens(value: &impl ToTokens) -> String {
    value.to_token_stream().to_string().replace(' ', "")
}

fn debug_bindings(block: &syn::Block) -> BTreeSet<String> {
    let mut bindings = DebugBindingProbe::default();
    bindings.visit_block(block);
    bindings.names
}

#[derive(Default)]
struct DebugBindingProbe {
    names: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for DebugBindingProbe {
    fn visit_local(&mut self, node: &'ast syn::Local) {
        let debug_initializer = node.init.as_ref().is_some_and(|initializer| {
            let tokens = compact_tokens(&initializer.expr);
            tokens.contains("format!") && tokens.contains(":?")
        });
        if debug_initializer && let syn::Pat::Ident(binding) = &node.pat {
            self.names.insert(binding.ident.to_string());
        }
        syn::visit::visit_local(self, node);
    }
}

fn is_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .meta
                    .to_token_stream()
                    .to_string()
                    .contains("test"))
    })
}

fn configured_name(name: String, attributes: &[Attribute]) -> String {
    let conditions = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .map(|attribute| compact_tokens(&attribute.meta))
        .collect::<Vec<_>>();
    if conditions.is_empty() {
        name
    } else {
        format!("{name}@{}", conditions.join("+"))
    }
}

fn remember_range(ranges: &mut Vec<(usize, usize)>, span: Span) {
    ranges.push((span.start().line, span.end().line));
}

fn count_lines(source: &str, excluded: &[(usize, usize)]) -> usize {
    source
        .lines()
        .enumerate()
        .filter(|(index, line)| {
            let number = index + 1;
            meaningful(line)
                && !excluded
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(&number))
        })
        .count()
}

fn count_range(lines: &[&str], start: usize, end: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| (start..=end).contains(&(index + 1)) && meaningful(line))
        .count()
}

fn meaningful(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with("//")
}

pub fn path_matches(pattern: &str, path: &str) -> bool {
    pattern
        .strip_suffix("**")
        .map_or_else(|| pattern == path, |prefix| path.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        toml::from_str(
            r#"
file_limit = 500
function_limit = 100
[[semantic_families]]
name = "calendar"
literals = ["365"]
identifier_pattern = "(?i)(days|year)"
owners = ["crates/core/src/time.rs"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn path_prefixes_are_explicit() {
        assert!(path_matches(
            "crates/example/**",
            "crates/example/src/lib.rs"
        ));
        assert!(!path_matches("crates/ex/**", "crates/example/src/lib.rs"));
    }

    #[test]
    fn semantic_context_does_not_ban_ordinary_values() {
        let source = "fn sample() {\nlet arbitrary = 365;\nlet duration_days = 365;\n}";
        let syntax = syn::parse_file(source).unwrap();
        let result = scan("crates/app/src/lib.rs", source, &syntax, &config());
        assert_eq!(
            result
                .findings
                .iter()
                .filter(|finding| finding.rule == "semantic-family:calendar")
                .count(),
            1
        );
    }

    #[test]
    fn tests_are_excluded_from_metrics_and_semantics() {
        let source = r#"
fn production() { let arbitrary = 2; }
#[cfg(test)]
mod tests {
    #[test]
    fn fixture() { let duration_days = 365; if "message" == "message" {} }
}
"#;
        let syntax = syn::parse_file(source).unwrap();
        let result = scan("crates/app/src/lib.rs", source, &syntax, &config());
        assert!(result.findings.is_empty());
        assert_eq!(result.functions.len(), 1);
    }

    #[test]
    fn raw_strings_and_debug_derived_locals_cannot_drive_branches() {
        let source = r#"
fn decide(value: Thing) {
    let rendered = format!("{value:?}");
    if rendered.contains("Unavailable") {}
}
"#;
        let syntax = syn::parse_file(source).unwrap();
        let result = scan("crates/app/src/lib.rs", source, &syntax, &config());
        assert!(result.findings.iter().any(|finding| {
            finding.rule == "raw-string-branch" && finding.fingerprint == "string:Unavailable"
        }));
        assert!(
            result
                .findings
                .iter()
                .any(|finding| finding.rule == "debug-output-branch")
        );
    }
}
