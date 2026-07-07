//! Awaited assignability-normalization boundary scans.
//!
//! The checker owns recursion depth, memo/clamp policy, and thenable fallback
//! decisions for `Awaited<T>` assignability normalization. Solver shape reads
//! and rebuilt object/array/union/tuple/application/conditional shells belong in
//! `query_boundaries::checkers::promise`.

use std::fs;
use std::path::{Path, PathBuf};

const OBJECT_NORMALIZATION: &str = "src/checkers/promise_checker_object_normalization.rs";
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

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = read_production_source(relative);
    for pattern in patterns {
        if contains_pattern(&source, pattern) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

#[test]
fn awaited_assignability_normalization_routes_solver_surfaces_through_promise_boundary() {
    let mut violations = Vec::new();
    scan_for_patterns(
        OBJECT_NORMALIZATION,
        &[
            "query_boundaries::common::array_element_type(",
            "query_boundaries::common::union_members(",
            "query_boundaries::common::tuple_elements(",
            "query_boundaries::common::application_info(",
            "query_boundaries::common::object_shape_id(",
            "query_boundaries::common::get_conditional_type_id(",
            "query_boundaries::common::has_property_by_str(",
            ".ctx.types.factory().array(",
            ".ctx.types.factory().union(",
            ".ctx.types.factory().tuple(",
            ".ctx.types.factory().application(",
            ".ctx.types.object_shape(",
            ".ctx.types.conditional_type(",
            "tsz_solver::TupleElement",
            "tsz_solver::PropertyInfo",
            "tsz_solver::ObjectShape",
        ],
        &mut violations,
    );
    scan_for_patterns(
        PROMISE_CHECKER,
        &[
            "let query::PromiseTypeKind::Object(",
            ".ctx.types.object_shape(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "Awaited assignability normalization must route solver shape reads and \
         rebuilt shells through query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_awaited_assignability_normalization_helpers() {
    let boundary = read_production_source(PROMISE_BOUNDARY);
    for helper in [
        "promise_object_symbol_id",
        "awaited_assignability_array_with_mapped_element",
        "awaited_assignability_union_has_raw_awaited_distribution",
        "awaited_assignability_union_with_mapped_members_if_changed",
        "awaited_assignability_union_with_mapped_members",
        "awaited_assignability_tuple_with_mapped_elements",
        "awaited_assignability_application_with_mapped_args",
        "awaited_assignability_object_with_mapped_slots",
        "raw_awaited_conditional_for_assignability",
        "awaited_assignability_type_has_then_property",
    ] {
        assert!(
            defines_fn(&boundary, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for owned_pattern in [
        "db.object_shape(",
        "db.object_with_index(",
        "db.conditional_type(",
        "db.array(",
        "db.union(",
        "db.tuple(",
        "db.application(",
        "PropertyInfo {",
        "ObjectShape {",
        "array_element_type(",
        "union_members(",
        "tuple_elements(",
        "application_info(",
        "get_conditional_type_id(",
        "has_property_by_str(",
    ] {
        assert!(
            contains_pattern(&boundary, owned_pattern),
            "query_boundaries::checkers::promise should own `{owned_pattern}`"
        );
    }
}

#[test]
fn awaited_assignability_normalization_callers_use_promise_boundary_helpers() {
    let object_normalization = read_production_source(OBJECT_NORMALIZATION);
    for helper in [
        "awaited_assignability_object_with_mapped_slots",
        "awaited_assignability_array_with_mapped_element",
        "awaited_assignability_union_has_raw_awaited_distribution",
        "awaited_assignability_union_with_mapped_members_if_changed",
        "awaited_assignability_union_with_mapped_members",
        "awaited_assignability_tuple_with_mapped_elements",
        "awaited_assignability_application_with_mapped_args",
        "raw_awaited_conditional_for_assignability",
        "awaited_assignability_type_has_then_property",
    ] {
        assert!(
            contains_pattern(&object_normalization, &format!("promise_query::{helper}(")),
            "promise_checker_object_normalization.rs must route through `{helper}`"
        );
    }

    let promise_checker = read_production_source(PROMISE_CHECKER);
    assert!(
        contains_pattern(&promise_checker, "query::promise_object_symbol_id("),
        "promise_checker.rs must route promise object symbol lookup through the boundary"
    );
}
