//! Promise application classification boundary scans.
//!
//! Promise checking owns lib identity, AST alias lowering, recursion policy,
//! and diagnostics. Solver-backed `Application`, `Lazy`, `TypeQuery`, and
//! object-shape classification belongs in `query_boundaries::checkers::promise`.

use std::fs;
use std::path::{Path, PathBuf};

const PROMISE_CHECKER: &str = "src/checkers/promise_checker.rs";
const PROMISE_BOUNDARY: &str = "src/query_boundaries/checkers/promise.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                }
                ('\n', _) => stripped.push('\n'),
                _ => {}
            }
            continue;
        }

        if in_string {
            stripped.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('"', _) => {
                in_string = true;
                stripped.push(ch);
            }
            ('/', Some('/')) => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        stripped.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
            }
            _ => stripped.push(ch),
        }
    }

    stripped
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn contains_pattern(source: &str, pattern: &str) -> bool {
    source.contains(pattern) || compact(source).contains(&compact(pattern))
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

fn read_production_source(relative: &str) -> String {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    production_source_without_comments(&source)
}

fn source_between_markers(relative: &str, start_marker: &str, end_marker: &str) -> String {
    let source = read_production_source(relative);
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("failed to find `{start_marker}` in {relative}"));
    let after_start = &source[start..];
    let end = after_start
        .find(end_marker)
        .unwrap_or_else(|| panic!("failed to find `{end_marker}` in {relative}"));
    after_start[..end].to_owned()
}

#[test]
fn promise_checker_does_not_match_promise_type_kind_directly() {
    let promise_checker = read_production_source(PROMISE_CHECKER);

    let mut violations = Vec::new();
    for pattern in [
        "PromiseTypeKind",
        "classify_promise_type(",
        "symbol_ref_to_symbol_id(",
    ] {
        if contains_pattern(&promise_checker, pattern) {
            violations.push(format!("{PROMISE_CHECKER} contains `{pattern}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "promise_checker.rs must route promise type-kind classification through \
         query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_checker_routes_application_classification_through_boundary() {
    let slices = [
        (
            "builtin promise-like application and alias body",
            source_between_markers(
                PROMISE_CHECKER,
                "pub(crate) fn builtin_promise_like_application_arg(",
                "fn type_node_contains_builtin_promise_like_name(",
            ),
        ),
        (
            "promise-like return type argument",
            source_between_markers(
                PROMISE_CHECKER,
                "pub fn promise_like_return_type_argument(",
                "pub(super) fn extract_awaited_type_from_thenable(",
            ),
        ),
        (
            "promise-like base argument",
            source_between_markers(
                PROMISE_CHECKER,
                "pub(crate) fn promise_like_type_argument_from_base(",
                "fn promise_symbol_and_decl_file(",
            ),
        ),
        (
            "lowered promise-like alias application",
            source_between_markers(
                PROMISE_CHECKER,
                "pub(crate) fn promise_like_type_argument_from_alias(",
                "pub(crate) fn promise_like_type_argument_from_class(",
            ),
        ),
    ];

    let mut violations = Vec::new();
    for (name, source) in &slices {
        for pattern in [
            "query::PromiseTypeKind::Application",
            "query::PromiseTypeKind::Lazy",
            "query::PromiseTypeKind::TypeQuery",
            "query::PromiseTypeKind::Object",
            "query::classify_promise_type(self.ctx.types",
            "symbol_ref_to_symbol_id(sym_ref)",
        ] {
            if contains_pattern(source, pattern) {
                violations.push(format!("{name} slice contains `{pattern}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "promise application/base classification must route solver-backed \
         promise type-shape reads through query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_application_classification_helpers() {
    let boundary = read_production_source(PROMISE_BOUNDARY);
    for helper in [
        "promise_application_parts",
        "promise_base_symbol_id",
        "promise_base_matches",
        "promise_reference_matches",
        "promise_type_matches_through_applications",
        "promise_lazy_def_id",
        "promise_type_is_object",
    ] {
        assert!(
            defines_fn(&boundary, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for owned_pattern in [
        "PromiseTypeKind::Application {",
        "PromiseTypeKind::Lazy(",
        "PromiseTypeKind::TypeQuery(",
        "PromiseTypeKind::Object(_)",
        "symbol_ref_to_symbol_id(",
    ] {
        assert!(
            contains_pattern(&boundary, owned_pattern),
            "query_boundaries::checkers::promise should own `{owned_pattern}`"
        );
    }
}

#[test]
fn promise_checker_uses_application_classification_boundary_helpers() {
    let promise_checker = read_production_source(PROMISE_CHECKER);

    for helper in [
        "promise_application_parts",
        "promise_base_symbol_id",
        "promise_base_matches",
        "promise_reference_matches",
        "promise_type_matches_through_applications",
        "promise_lazy_def_id",
        "promise_type_is_object",
    ] {
        assert!(
            contains_pattern(&promise_checker, &format!("query::{helper}(")),
            "promise_checker.rs must route through `{helper}`"
        );
    }
}
