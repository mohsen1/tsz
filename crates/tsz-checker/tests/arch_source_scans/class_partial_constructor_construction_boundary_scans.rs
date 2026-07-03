//! Class partial-constructor construction boundary scans.
//!
//! Class-constructor helpers own AST traversal, declaration timing, publication
//! windows, inheritance substitution, overload selection, and diagnostics.
//! Solver construction for partial static constructor surfaces belongs in
//! `query_boundaries::class_type`.

use std::fs;
use std::path::{Path, PathBuf};

const CLASS_PARTIAL_CONSTRUCTOR_CALLERS: &[&str] = &[
    "src/types/class_type/constructor_parts/helpers.rs",
    "src/types/class_type/constructor_parts/rough_partial.rs",
];
const CLASS_TYPE_BOUNDARY: &str = "src/query_boundaries/class_type.rs";
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
fn class_partial_constructor_callers_route_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "IndexSignature {",
        "CallableShape {",
        "PropertyInfo {",
        "ParamInfo {",
        "TypePredicate {",
        "CallSignature {",
        "TypeParamInfo {",
        ".factory().union2(",
        ".factory().callable(",
        ".factory().lazy(",
        ".factory().application(",
        ".factory().type_param(",
        "factory.union2(",
        "factory.callable(",
        "factory.lazy(",
        "factory.application(",
        "factory.type_param(",
    ];

    let mut violations = Vec::new();
    for caller in CLASS_PARTIAL_CONSTRUCTOR_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "class partial-constructor callers must route solver construction \
         through query_boundaries::class_type:\n{}",
        violations.join("\n")
    );
}

#[test]
fn class_type_boundary_owns_partial_constructor_construction_helpers() {
    let source = fs::read_to_string(checker_path(CLASS_TYPE_BOUNDARY))
        .expect("failed to read query_boundaries/class_type.rs");
    let common =
        fs::read_to_string(checker_path(COMMON_BOUNDARY)).expect("failed to read common.rs");

    for helper in [
        "merged_static_late_bound_index_value_type",
        "static_late_bound_index_signature",
        "partial_static_method_type",
        "partial_static_method_property",
        "partial_static_accessor_property",
        "partial_static_placeholder_property",
        "partial_static_constructor_callable_type",
        "class_constructor_companion_lazy_type",
        "rough_self_instance_lazy_type",
        "rough_self_instance_application_type",
        "class_construct_param",
        "class_type_predicate",
        "class_construct_signature",
        "enclosing_function_type_param_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::class_type must own `{helper}`"
        );
        assert!(
            !defines_fn(&common, helper),
            "query_boundaries::common must not define `{helper}`"
        );
    }

    for construction_pattern in [
        "db.union2(",
        "IndexSignature {",
        "db.callable(",
        "CallableShape {",
        "PropertyInfo {",
        "db.lazy(",
        "db.application(",
        "ParamInfo {",
        "TypePredicate {",
        "CallSignature {",
        "db.type_param(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::class_type should own `{construction_pattern}`"
        );
    }
}
