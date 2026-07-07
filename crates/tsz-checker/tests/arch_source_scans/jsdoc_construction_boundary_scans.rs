//! JSDoc construction boundary scans.
//!
//! JSDoc resolution code parses source text and gathers facts. Raw solver
//! `FunctionShape`, `ObjectShape`, indexed-object construction, and expression
//! type construction belongs in `query_boundaries::jsdoc_construction`.

use std::fs;
use std::path::{Path, PathBuf};

const JSDOC_CONSTRUCTION_CALLERS: &[&str] = &[
    "src/jsdoc/resolution/generic_typedef.rs",
    "src/jsdoc/resolution/type_construction.rs",
    "src/jsdoc/resolution/name_resolution.rs",
    "src/jsdoc/params_type_strings.rs",
    "src/jsdoc/diagnostics_import_type_constraints.rs",
    "src/jsdoc/lookup.rs",
];

const JSDOC_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/jsdoc_construction.rs";

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

#[test]
fn jsdoc_callers_route_solver_shape_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "FunctionShape",
        "ObjectShape",
        "IndexSignature",
        ".function(",
        ".object(",
        ".object_with_index(",
        ".callable(",
        ".array(",
        ".union(",
        ".union2(",
        ".intersection(",
        ".intersection2(",
        ".application(",
        ".tuple(",
        ".index_access(",
        ".keyof(",
        "factory().lazy(",
        ".lazy(",
        "factory().readonly_type(",
        ".readonly_type(",
        "factory().literal_string(",
        ".literal_string(",
        "factory().literal_boolean(",
        ".literal_boolean(",
        "factory().literal_number(",
        ".literal_number(",
        ".mapped(",
        ".conditional(",
        ".type_param(",
        "TupleElement {",
        "MappedType {",
        "ConditionalType {",
        "TypeParamInfo {",
        "TypeParamOrigin::User",
        "ParamInfo {",
        "ParamInfo::required(",
        "ParamInfo::optional(",
        "ParamInfo::rest(",
        "ParamInfo::unnamed(",
        "PropertyInfo::new(",
        "PropertyInfo {",
        "TypePredicate {",
    ];

    let mut violations = Vec::new();
    for module in JSDOC_CONSTRUCTION_CALLERS {
        scan_for_patterns(module, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "JSDoc construction callers must route solver shape construction \
         through query_boundaries::jsdoc_construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn jsdoc_construction_boundary_owns_helpers_and_shape_literals() {
    let source = fs::read_to_string(checker_path(JSDOC_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/jsdoc_construction.rs");
    let common = fs::read_to_string(checker_path("src/query_boundaries/common.rs"))
        .expect("failed to read query_boundaries/common.rs");

    for helper in [
        "jsdoc_object_index_fact",
        "jsdoc_empty_object_type",
        "jsdoc_object_type",
        "jsdoc_object_index_type",
        "jsdoc_function_type",
        "jsdoc_array_type",
        "jsdoc_union_type",
        "jsdoc_union_pair_type",
        "jsdoc_intersection_type",
        "jsdoc_intersection_pair_type",
        "jsdoc_application_type",
        "jsdoc_index_access_type",
        "jsdoc_keyof_type",
        "jsdoc_lazy_type",
        "jsdoc_readonly_type",
        "jsdoc_literal_string_type",
        "jsdoc_literal_boolean_type",
        "jsdoc_literal_number_type",
        "jsdoc_type_param_info",
        "jsdoc_type_param_type",
        "jsdoc_tuple_type",
        "jsdoc_tuple_element",
        "jsdoc_param_info",
        "jsdoc_property_info",
        "jsdoc_type_predicate",
        "jsdoc_mapped_type",
        "jsdoc_conditional_type",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::jsdoc_construction must own `{helper}`"
        );
        assert!(
            !common.contains(&format!("fn {helper}(")),
            "JSDoc construction helper `{helper}` must not drift into common.rs"
        );
    }

    for shape_pattern in [
        "FunctionShape {",
        "ObjectShape {",
        "IndexSignature {",
        "TupleElement {",
        "MappedType {",
        "ConditionalType {",
        "TypeParamInfo {",
        "TypeParamOrigin::User",
        "ParamInfo {",
        "PropertyInfo {",
        "TypePredicate {",
    ] {
        assert!(
            source.contains(shape_pattern),
            "query_boundaries::jsdoc_construction should own `{shape_pattern}`"
        );
    }
}
