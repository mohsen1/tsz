//! Binding/destructuring construction boundary scans.
//!
//! Binding-pattern callers gather syntax, contextual typing, relation, and
//! diagnostic facts. Solver tuple/object/property/union construction for those
//! facts belongs in `query_boundaries::binding_patterns`.

use std::fs;
use std::path::{Path, PathBuf};

const BINDING_PATTERN_CALLERS: &[&str] = &[
    "src/state/variable_checking/destructuring.rs",
    "src/state/variable_checking/destructuring/tail.rs",
    "src/types/queries/binding.rs",
];
const BINDING_PATTERN_BOUNDARY: &str = "src/query_boundaries/binding_patterns.rs";
const COMMON_BOUNDARY: &str = "src/query_boundaries/common.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}(")) || source.contains(&format!("fn {name}<"))
}

#[test]
fn binding_pattern_callers_route_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "PropertyInfo::new(",
        "PropertyInfo {",
        "TupleElement {",
        ".factory().union(",
        ".factory().union2(",
        ".factory().object(",
        ".factory().tuple(",
        "factory.union(",
        "factory.union2(",
        "factory.object(",
        "factory.tuple(",
        ".types.union(",
        ".types.union2(",
        ".types.object(",
        ".types.tuple(",
    ];

    let mut violations = Vec::new();
    for caller in BINDING_PATTERN_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "binding/destructuring pattern callers must route solver construction \
         through query_boundaries::binding_patterns:\n{}",
        violations.join("\n")
    );
}

#[test]
fn binding_patterns_boundary_owns_binding_pattern_construction_helpers() {
    let source = fs::read_to_string(checker_path(BINDING_PATTERN_BOUNDARY))
        .expect("failed to read query_boundaries/binding_patterns.rs");
    let common =
        fs::read_to_string(checker_path(COMMON_BOUNDARY)).expect("failed to read common.rs");

    for helper in [
        "binding_pattern_initializer_union_type",
        "binding_pattern_member_union_type",
        "binding_pattern_property",
        "binding_pattern_tuple_element",
        "binding_pattern_object_type",
        "binding_pattern_tuple_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::binding_patterns must own `{helper}`"
        );
        assert!(
            !defines_fn(&common, helper),
            "query_boundaries::common must not define `{helper}`"
        );
    }

    for construction_pattern in [
        "db.union2(",
        "db.union(",
        "PropertyInfo::new(",
        "TupleElement {",
        "db.object(",
        "db.tuple(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::binding_patterns should own `{construction_pattern}`"
        );
    }
}
