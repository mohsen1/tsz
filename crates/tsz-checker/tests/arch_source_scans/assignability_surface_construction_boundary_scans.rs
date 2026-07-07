//! Assignability surface construction boundary scans.
//!
//! Assignability callers own relation-preparation policy: namespace export
//! selection, nested normalization, distribution choices, destructuring
//! context, and diagnostic target choice. Solver construction for those
//! transient comparison surfaces belongs in
//! `query_boundaries::assignability::construction`.

use std::fs;
use std::path::{Path, PathBuf};

const ASSIGNABILITY_CALLERS: &[&str] = &[
    "src/assignability/assignability_checker.rs",
    "src/assignability/subtype_identity_checker.rs",
    "src/assignability/index_access_normalization.rs",
    "src/assignability/assignment_checker/assignment_ops.rs",
    "src/assignability/assignment_checker/destructuring.rs",
];
const ASSIGNABILITY_CONSTRUCTION_BOUNDARY: &str =
    "src/query_boundaries/assignability/construction.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    for pattern in patterns {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn assignability_callers_route_surface_construction_through_boundary() {
    let patterns = [
        "PropertyInfo::new(",
        "PropertyInfo {",
        "TupleElement {",
        "Visibility::Public",
        "tsz_solver::PropertyInfo",
        "tsz_solver::TupleElement",
        ".factory().object(",
        ".factory().tuple(",
        ".factory().function(",
        ".factory().index_access(",
        ".factory().union(",
        ".factory().intersection(",
        ".factory().union_preserve_members(",
        ".as_type_database().object(",
        ".as_type_database().tuple(",
        ".readonly_type(",
        ".no_infer(",
        ".array(",
        ".index_access(",
        "FunctionShape{",
        ".types.intersection(",
        "TypeKey",
        "intern(TypeData::",
    ];

    let mut violations = Vec::new();
    for caller in ASSIGNABILITY_CALLERS {
        scan_for_patterns(caller, &patterns, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "assignability callers must route solver construction through \
         query_boundaries::assignability::construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn assignability_boundary_owns_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(ASSIGNABILITY_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/assignability/construction.rs");

    for helper in [
        "assignability_namespace_export_property",
        "assignability_contextual_pattern_property",
        "assignability_tuple_element",
        "assignability_resolved_tuple_element",
        "assignability_resolved_property",
        "assignability_object_type",
        "assignability_empty_object_type",
        "assignability_readonly_type",
        "assignability_noinfer_type",
        "assignability_array_type",
        "assignability_tuple_type",
        "assignability_union_type",
        "assignability_intersection_type",
        "assignability_union_preserve_members",
        "assignability_function_with_return_type",
        "assignability_index_access_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::assignability::construction must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo::new(",
        "TupleElement {",
        "PropertyInfo {",
        "db.object(",
        "db.readonly_type(",
        "db.no_infer(",
        "db.array(",
        "db.tuple(",
        "db.union(",
        "db.intersection(",
        "db.union_preserve_members(",
        "FunctionShape {",
        "db.function(",
        "db.index_access(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::assignability::construction should own `{construction_pattern}`"
        );
    }
}
