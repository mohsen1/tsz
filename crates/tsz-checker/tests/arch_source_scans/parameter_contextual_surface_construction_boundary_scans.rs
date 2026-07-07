//! Parameter/contextual callable surface construction boundary scans.
//!
//! Parameter checking owns AST position, optionality policy, contextual lookup,
//! and diagnostics. Solver tuple, union, function, and rest-array surfaces for
//! those facts belong in `query_boundaries::checkers::parameters`.

use std::fs;
use std::path::{Path, PathBuf};

const PARAMETER_CHECKER: &str = "src/checkers/parameter_checker.rs";
const CONTEXTUAL_PARAMETERS: &str = "src/types/utilities/contextual_parameters.rs";
const PARAMETER_BOUNDARY: &str = "src/query_boundaries/checkers/parameters.rs";

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
fn parameter_contextual_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    scan_for_patterns(
        PARAMETER_CHECKER,
        &[
            ".ctx.types.factory()",
            ".ctx.types.union2(",
            ".ctx.types.array(",
            ".ctx.types.readonly_type(",
            "factory.union2(",
            "factory.array(",
            "factory.readonly_type(",
        ],
        &mut violations,
    );
    scan_for_patterns(
        CONTEXTUAL_PARAMETERS,
        &[
            ".ctx.types.factory()",
            ".types.factory()",
            ".ctx.types.union2(",
            ".ctx.types.tuple(",
            ".ctx.types.union(",
            ".ctx.types.union_preserve_members(",
            ".ctx.types.intersection(",
            ".ctx.types.function(",
            ".ctx.types.unique_symbol(",
            ".types.union2(",
            ".types.tuple(",
            ".types.union(",
            ".types.union_preserve_members(",
            ".types.intersection(",
            ".types.function(",
            ".types.unique_symbol(",
            "factory.union2(",
            "factory.tuple(",
            "factory.union(",
            "factory.union_preserve_members(",
            "factory.intersection(",
            "factory.function(",
            "tsz_solver::TupleElement {",
            "tsz_solver::ParamInfo {",
            "tsz_solver::FunctionShape",
            "tsz_solver::SymbolRef(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "parameter/contextual surfaces must route solver construction through \
         query_boundaries::checkers::parameters:\n{}",
        violations.join("\n")
    );
}

#[test]
fn parameter_contextual_boundary_owns_surface_helpers() {
    let source = fs::read_to_string(checker_path(PARAMETER_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/parameters.rs");
    let source = production_source_without_comments(&source);

    for helper in [
        "optional_parameter_type_with_undefined",
        "readonly_any_array_type",
        "tuple_type_from_elements",
        "tuple_type_from_element_slice",
        "contextual_rest_tuple_from_signature_tail",
        "union_type",
        "union_preserve_members_type",
        "intersection_type",
        "function_type_from_shape",
        "merge_callable_contextual_types",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::parameters must own `{helper}`"
        );
    }

    for owned_pattern in [
        "db.union2(",
        "db.array(",
        "db.readonly_type(",
        "db.tuple(",
        "db.union(",
        "db.union_preserve_members(",
        "db.intersection(",
        "TupleElement {",
        "ParamInfo {",
        "FunctionShape {",
    ] {
        assert!(
            contains_pattern(&source, owned_pattern),
            "query_boundaries::checkers::parameters should own `{owned_pattern}`"
        );
    }
}
