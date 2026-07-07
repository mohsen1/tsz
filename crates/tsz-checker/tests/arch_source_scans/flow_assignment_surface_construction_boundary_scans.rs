//! Flow assignment surface construction boundary scans.
//!
//! Flow assignment and fallback callers own syntax recovery, narrowing policy,
//! predicate instantiation, and definite-assignment indexing. Solver
//! construction for fallback object/callable/tuple/rest-array/union surfaces
//! belongs in `query_boundaries::flow_analysis`.

use std::fs;
use std::path::{Path, PathBuf};

const FLOW_ASSIGNMENT_CALLERS: &[&str] = &[
    "src/flow/control_flow/assignment_fallback.rs",
    "src/flow/control_flow/assignment.rs",
    "src/flow/control_flow/predicate_resolution.rs",
    "src/flow/control_flow/core.rs",
    "src/flow/flow_analysis/definite.rs",
];
const FLOW_ANALYSIS_BOUNDARY: &str = "src/query_boundaries/flow_analysis.rs";

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
fn flow_assignment_callers_route_surface_construction_through_boundary() {
    let patterns = [
        "PropertyInfo::new(",
        "PropertyInfo::opt(",
        "PropertyInfo {",
        "TupleElement {",
        "CallSignature {",
        "CallableShape {",
        ".factory().object(",
        ".factory().tuple(",
        ".factory().union(",
        ".factory().array(",
        ".factory().function(",
        ".factory().callable(",
        "db.factory().array(",
    ];

    let mut violations = Vec::new();
    for caller in FLOW_ASSIGNMENT_CALLERS {
        scan_for_patterns(caller, &patterns, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "flow assignment callers must route solver construction through \
         query_boundaries::flow_analysis:\n{}",
        violations.join("\n")
    );
}

#[test]
fn flow_analysis_boundary_owns_assignment_surface_helpers() {
    let source = fs::read_to_string(checker_path(FLOW_ANALYSIS_BOUNDARY))
        .expect("failed to read query_boundaries/flow_analysis.rs");

    for helper in [
        "array_type",
        "flow_property",
        "optional_flow_property",
        "flow_tuple_element",
        "flow_call_signature",
        "object_type_from_properties",
        "call_only_callable_type",
        "tuple_type",
        "union_types",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::flow_analysis must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.array(",
        "PropertyInfo::new(",
        "PropertyInfo::opt(",
        "TupleElement {",
        "CallSignature {",
        "db.object(",
        "db.tuple(",
        "union_or_single(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::flow_analysis should own `{construction_pattern}`"
        );
    }
}
