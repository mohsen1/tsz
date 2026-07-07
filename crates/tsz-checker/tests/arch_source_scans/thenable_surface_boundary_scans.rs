//! Structural thenable surface boundary scans.
//!
//! Await/promise checking owns receiver evaluation, recursion policy,
//! `this`-type validation, diagnostics, and fallback decisions. Solver-backed
//! `then` property lookup, callable/function signature surfaces, and callback
//! payload reads belong in `query_boundaries::checkers::promise`.

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
fn promise_checker_routes_structural_thenable_surfaces_through_boundary() {
    let thenable_slice = source_between_markers(
        PROMISE_CHECKER,
        "fn extract_awaited_type_from_valid_thenable(",
        "pub(crate) fn promise_like_type_argument_from_base(",
    );

    let mut violations = Vec::new();
    for pattern in [
        "query_boundaries::property_access",
        "resolve_property_access(",
        "query_boundaries::class::member_call_signatures(",
        "member_call_signatures(",
        "query::call_signatures_for_type(",
        "query::function_shape_for_type(",
        "query::union_members(",
        ".intern_string(\"then\")",
        ".params.first()",
        ".success_type()",
        "extract_first_param_from_callback",
    ] {
        if contains_pattern(&thenable_slice, pattern) {
            violations.push(format!("thenable extraction slice contains `{pattern}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "structural thenable extraction must route solver-backed `then`, \
         signature, and callback-payload surfaces through \
         query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_structural_thenable_surface_helpers() {
    let boundary = read_production_source(PROMISE_BOUNDARY);
    for helper in [
        "thenable_property_type",
        "thenable_signature_surfaces",
        "thenable_callback_value_type",
        "thenable_callback_value_union",
    ] {
        assert!(
            defines_fn(&boundary, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for owned_pattern in [
        "resolve_property_access(",
        "db.intern_string(\"then\")",
        "CallSignature {",
        "call_signatures_for_type(db,",
        "function_shape_for_type(db,",
        "union_members(db,",
        ".params.first()",
        "ThenableSignatureSurface",
    ] {
        assert!(
            contains_pattern(&boundary, owned_pattern),
            "query_boundaries::checkers::promise should own `{owned_pattern}`"
        );
    }
}

#[test]
fn promise_checker_uses_structural_thenable_boundary_helpers() {
    let thenable_slice = source_between_markers(
        PROMISE_CHECKER,
        "fn extract_awaited_type_from_valid_thenable(",
        "pub(crate) fn promise_like_type_argument_from_base(",
    );

    for helper in [
        "thenable_property_type",
        "thenable_signature_surfaces",
        "thenable_callback_value_type",
        "thenable_callback_value_union",
    ] {
        assert!(
            contains_pattern(&thenable_slice, &format!("query::{helper}(")),
            "promise_checker.rs thenable extraction must route through `{helper}`"
        );
    }

    let promise_checker = read_production_source(PROMISE_CHECKER);
    assert!(
        !contains_pattern(&promise_checker, "fn extract_first_param_from_callback("),
        "callback payload extraction belongs in the promise boundary"
    );
}
