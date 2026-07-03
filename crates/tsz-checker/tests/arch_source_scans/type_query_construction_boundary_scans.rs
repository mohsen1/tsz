//! Const value/type-query construction boundary scans.
//!
//! Type-node callers validate syntax, symbols, declarations, and fallback
//! policy. Solver literal/property/object/tuple construction for accepted
//! const query facts belongs in `query_boundaries::type_query_construction`.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_QUERY_CALLERS: &[&str] = &[
    "src/types/type_node_merged_value_query.rs",
    "src/types/type_node_advanced.rs",
];
const TYPE_QUERY_BOUNDARY: &str = "src/query_boundaries/type_query_construction.rs";
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
fn type_query_callers_route_const_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "PropertyInfo::new(",
        "PropertyInfo {",
        "ObjectShape {",
        "TupleElement {",
        ".factory().literal_string(",
        ".factory().literal_number(",
        ".factory().literal_boolean(",
        ".factory().object(",
        ".factory().object_with_index(",
        ".factory().tuple(",
        ".types.literal_string(",
        ".types.literal_number(",
        ".types.literal_boolean(",
        ".types.object(",
        ".types.tuple(",
    ];

    let mut violations = Vec::new();
    for caller in TYPE_QUERY_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "const type-query callers must route solver construction through \
         query_boundaries::type_query_construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn type_query_boundary_owns_const_query_construction_helpers() {
    let source = fs::read_to_string(checker_path(TYPE_QUERY_BOUNDARY))
        .expect("failed to read query_boundaries/type_query_construction.rs");
    let common =
        fs::read_to_string(checker_path(COMMON_BOUNDARY)).expect("failed to read common.rs");

    for helper in [
        "const_query_literal_string_type",
        "const_query_literal_number_type",
        "const_query_literal_boolean_type",
        "const_query_readonly_property",
        "const_query_object_literal_type",
        "const_query_array_to_enum_object_type",
        "const_query_tuple_element",
        "const_query_tuple_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_query_construction must own `{helper}`"
        );
        assert!(
            !defines_fn(&common, helper),
            "query_boundaries::common must not define `{helper}`"
        );
    }

    for construction_pattern in [
        "db.literal_string(",
        "db.literal_number(",
        "db.literal_boolean(",
        "PropertyInfo {",
        "ObjectShape {",
        "db.object_with_index(",
        "db.object(",
        "TupleElement {",
        "db.tuple(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_query_construction should own `{construction_pattern}`"
        );
    }
}
