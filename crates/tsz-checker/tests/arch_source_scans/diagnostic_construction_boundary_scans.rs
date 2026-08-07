//! Diagnostic construction boundary scans.
//!
//! Reporter modules choose display policy and source locations. Interning
//! diagnostic-only object/function/callable/compound solver surfaces belongs
//! in `query_boundaries::diagnostics`.

use std::fs;
use std::path::{Path, PathBuf};

const DIAGNOSTICS_BOUNDARY: &str = "src/query_boundaries/diagnostics.rs";
const DIAGNOSTIC_CONSTRUCTION_MODULES: &[&str] = &[
    "src/error_reporter/core/type_display.rs",
    "src/error_reporter/core/excess_display.rs",
    "src/error_reporter/core_alias_display.rs",
    "src/error_reporter/core_formatting.rs",
    "src/error_reporter/core/diagnostic_source.rs",
    "src/error_reporter/core/diagnostic_source/assignment_formatting.rs",
    "src/error_reporter/core/diagnostic_source/object_literal_targets.rs",
    "src/error_reporter/core/diagnostic_source/static_schema.rs",
    "src/error_reporter/assignability_contextual_display.rs",
    "src/error_reporter/assignability_helpers.rs",
    "src/error_reporter/fingerprint_policy.rs",
    "src/error_reporter/generics.rs",
    "src/error_reporter/property_receiver_formatting.rs",
    "src/error_reporter/properties.rs",
    "src/error_reporter/call_errors/display_formatting.rs",
    "src/error_reporter/call_errors/display_formatting_parameters.rs",
    "src/error_reporter/call_errors/elaboration_array_mismatch.rs",
    "src/error_reporter/call_errors/error_emission.rs",
    "src/error_reporter/call_errors/elaboration.rs",
    "src/state/type_environment/formatting.rs",
];
const DIAGNOSTIC_FUNCTION_CONSTRUCTION_MODULES: &[&str] = &["src/error_reporter/render_failure.rs"];
const DIAGNOSTIC_SOURCE_COLLECTION_MODULES: &[&str] = &[
    "src/error_reporter/core/identifier_source_display.rs",
    "src/error_reporter/core/diagnostic_source/computed_index_source_display.rs",
    "src/error_reporter/core/diagnostic_source/collection_source_display.rs",
    "src/error_reporter/core/diagnostic_source/tuple_source_display.rs",
];

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn is_allowed_shape_return_signature(trimmed: &str) -> bool {
    matches!(
        trimmed,
        ") -> tsz_solver::FunctionShape {" | ") -> tsz_solver::CallableShape {"
    )
}

fn is_allowed_failure_reason_match(line: &str) -> bool {
    line.contains("SubtypeFailureReason::MissingIndexSignature {")
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//")
            || is_allowed_shape_return_signature(trimmed)
            || is_allowed_failure_reason_match(line)
        {
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
fn diagnostic_display_callers_route_solver_shape_construction_through_diagnostics_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().object(",
        ".factory().object_with_index(",
        ".factory().function(",
        ".factory().callable(",
        ".factory().application(",
        ".factory().union(",
        ".factory().union_preserve_members(",
        ".factory().intersection(",
        ".factory().index_access(",
        ".factory().array(",
        ".factory().literal_string(",
        ".factory().literal_string_atom(",
        ".factory().literal_number(",
        ".factory.object(",
        ".factory.object_with_index(",
        ".factory.function(",
        ".factory.callable(",
        ".factory.application(",
        ".factory.union(",
        ".factory.union_preserve_members(",
        ".factory.intersection(",
        ".factory.index_access(",
        ".factory.array(",
        ".factory.literal_string(",
        ".factory.literal_string_atom(",
        ".factory.literal_number(",
        ".types.object(",
        ".types.object_with_index(",
        ".types.function(",
        ".types.callable(",
        ".types.array(",
        ".types.union(",
        ".types.union2(",
        ".types.readonly_type(",
        ".types.literal_string(",
        ".types.literal_string_atom(",
        ".types.literal_number(",
        "PropertyInfo::new(",
        "tsz_solver::utils::union_or_single(",
        "tsz_solver::utils::intersection_or_single(",
        "FunctionShape {",
        "FunctionShape::new(",
        "CallableShape {",
        "ObjectShape {",
        "IndexSignature {",
        "ParamInfo::required(",
    ];

    let mut violations = Vec::new();
    for relative in DIAGNOSTIC_CONSTRUCTION_MODULES {
        scan_for_patterns(relative, FORBIDDEN_PATTERNS, &mut violations);
    }

    const FUNCTION_FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().function(",
        ".factory.function(",
        ".types.function(",
        "FunctionShape {",
        "FunctionShape::new(",
    ];
    for relative in DIAGNOSTIC_FUNCTION_CONSTRUCTION_MODULES {
        scan_for_patterns(relative, FUNCTION_FORBIDDEN_PATTERNS, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "diagnostic display callers must route solver shape and compound \
         construction \
         through query_boundaries::diagnostics:\n{}",
        violations.join("\n")
    );
}

#[test]
fn diagnostic_source_collection_routes_collection_construction_through_diagnostics_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().union(",
        ".factory().union_from_slice(",
        ".factory.union(",
        ".factory.union_from_slice(",
        ".types.union(",
        ".types.union_from_slice(",
        ".types.array(",
        ".types.readonly_type(",
    ];

    let mut violations = Vec::new();
    for relative in DIAGNOSTIC_SOURCE_COLLECTION_MODULES {
        scan_for_patterns(relative, FORBIDDEN_PATTERNS, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "diagnostic source collection display callers must route union/array \
         construction through query_boundaries::diagnostics:\n{}",
        violations.join("\n")
    );
}

#[test]
fn diagnostics_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path(DIAGNOSTICS_BOUNDARY))
        .expect("failed to read query_boundaries/diagnostics.rs");
    for helper in [
        "object_type_from_properties",
        "object_type_from_shape",
        "object_type_preserving_display_properties",
        "shallow_object_property_literals_widened_for_call_parameter_display",
        "object_type_with_unknown_display_members",
        "mapped_property_mismatch_parameter_display_type",
        "display_property_literals_widened_for_related_info",
        "source_display_union_type",
        "source_display_union_type_from_slice",
        "rebuilt_array_source_display_type",
        "display_application_type",
        "display_union_type",
        "display_union_or_single_type",
        "display_union_preserve_members_type",
        "display_intersection_type",
        "display_intersection_or_single_type",
        "display_array_type",
        "display_index_access_type",
        "display_union_with_undefined",
        "display_string_literal_type",
        "display_string_literal_atom_type",
        "display_number_literal_type",
        "display_rest_parameter_type",
        "display_property",
        "mapped_display_property",
        "static_schema_display_property_from_source",
        "function_type_from_shape",
        "function_type_with_params_replaced",
        "function_type_with_return_replaced",
        "function_type_with_params_and_return_replaced",
        "function_type_without_type_params",
        "function_type_from_call_signature_without_type_params",
        "function_type_from_call_signature",
        "callable_type_from_shape",
        "call_only_callable_type",
        "callable_type_with_signatures_replaced",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::diagnostics must own `{helper}`"
        );
    }
}
