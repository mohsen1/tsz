//! Call-candidate spread surface construction boundary scans.
//!
//! Call-candidate collection owns AST traversal, effective argument indexes,
//! spread diagnostics, and fallback policy. Solver construction for sensitive
//! argument placeholders, spread markers, optional tuple-element arguments, and
//! callable rest-shape reads belongs in `query_boundaries::checkers::call`.

use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATE_COLLECTION: &str = "src/checkers/call_checker/candidate_collection.rs";
const NON_TUPLE_SPREAD_SIGNATURE: &str = "src/checkers/call_checker/non_tuple_spread_signature.rs";
const SPREAD_OVERLOAD_SELECTION: &str = "src/checkers/call_checker/spread_overload_selection.rs";
const CALL_BOUNDARY: &str = "src/query_boundaries/checkers/call.rs";

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

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let source = production_source_without_comments(&source);
    for pattern in patterns {
        if contains_pattern(&source, pattern) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

#[test]
fn call_candidate_spread_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();

    scan_for_patterns(
        CANDIDATE_COLLECTION,
        &[
            ".ctx.types.factory()",
            ".types.factory()",
            ".ctx.types.tuple(",
            ".types.tuple(",
            ".ctx.types.union2(",
            ".types.union2(",
            "factory.function(",
            "factory.tuple(",
            "factory.union2(",
            "tsz_solver::FunctionShape",
            "FunctionShape {",
            "tsz_solver::ParamInfo {",
            "ParamInfo {",
            "tsz_solver::TupleElement {",
            "TupleElement {",
            "__sensitive_arg__",
            "__tsz_spread_argument__",
            "function_shape_for_type(",
            "callable_shape_for_type(",
        ],
        &mut violations,
    );
    for relative in [NON_TUPLE_SPREAD_SIGNATURE, SPREAD_OVERLOAD_SELECTION] {
        scan_for_patterns(
            relative,
            &["super::candidate_collection::type_param_variadic_tuple_spread"],
            &mut violations,
        );
    }

    assert!(
        violations.is_empty(),
        "call-candidate spread surfaces must route solver construction through \
         query_boundaries::checkers::call:\n{}",
        violations.join("\n")
    );
}

#[test]
fn call_boundary_owns_candidate_spread_surface_helpers() {
    let caller_source = fs::read_to_string(checker_path(CANDIDATE_COLLECTION))
        .expect("failed to read call_checker/candidate_collection.rs");
    let caller_source = production_source_without_comments(&caller_source);
    let source = fs::read_to_string(checker_path(CALL_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/call.rs");
    let source = production_source_without_comments(&source);

    for helper in [
        "type_param_variadic_tuple_spread",
        "expanded_tuple_spread_len",
        "optional_tuple_element_argument_type",
        "sensitive_argument_placeholder_type",
        "spread_argument_marker_type",
        "generic_type_parameter_spread_marker_type",
        "open_spread_tail_needs_marker",
        "array_spread_rest_param_is_bare_type_param",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::call must own `{helper}`"
        );
        assert!(
            contains_pattern(&caller_source, &format!("{helper}(")),
            "call_checker/candidate_collection.rs must route through `{helper}`"
        );
    }

    for owned_pattern in [
        "SENSITIVE_ARGUMENT_PLACEHOLDER_NAME",
        "SPREAD_ARGUMENT_MARKER_NAME",
        "db.intern_string(SENSITIVE_ARGUMENT_PLACEHOLDER_NAME)",
        "db.intern_string(SPREAD_ARGUMENT_MARKER_NAME)",
        "db.function(FunctionShape {",
        "ParamInfo {",
        "db.union2(",
        "db.tuple(vec![TupleElement {",
        "function_shape_for_type(",
        "callable_shape_for_type(",
        "unwrap_readonly_or_noinfer(",
    ] {
        assert!(
            contains_pattern(&source, owned_pattern),
            "query_boundaries::checkers::call should own `{owned_pattern}`"
        );
    }
}
