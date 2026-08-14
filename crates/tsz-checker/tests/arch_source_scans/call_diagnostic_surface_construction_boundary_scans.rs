//! Call diagnostic surface construction boundary scans.
//!
//! Call-result handling owns call orchestration, argument/return diagnostics,
//! and display-target selection. Solver construction for diagnostic-only
//! object, tuple, and function surfaces belongs in
//! `query_boundaries::checkers::call` and
//! `query_boundaries::construct_signatures`.

use std::fs;
use std::path::{Path, PathBuf};

const CALL_RESULT: &str = "src/types/computation/call_result.rs";
const CALL_RESULT_GENERIC_DISPLAY: &str = "src/types/computation/call_result_generic_display.rs";
const ARGUMENT_COLLECTION: &str = "src/types/computation/call/inner/argument_collection.rs";
const CALL_BOUNDARY: &str = "src/query_boundaries/checkers/call.rs";
const SIGNATURE_BOUNDARY: &str = "src/query_boundaries/construct_signatures.rs";

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
fn call_diagnostic_surfaces_route_solver_construction_through_boundaries() {
    let mut violations = Vec::new();

    let raw_construction_patterns = [
        "std::sync::Arc::new(tsz_solver::FunctionShape {",
        "tsz_solver::FunctionShape {",
        "FunctionShape {",
        "tsz_solver::PropertyInfo::new(",
        "PropertyInfo::new(",
        "ParamInfo {",
        "TupleElement {",
        ".factory().object(",
        ".factory().tuple(",
        ".factory().function(",
        "self.ctx.types.tuple(",
    ];
    scan_for_patterns(CALL_RESULT, &raw_construction_patterns, &mut violations);
    scan_for_patterns(
        CALL_RESULT_GENERIC_DISPLAY,
        &raw_construction_patterns,
        &mut violations,
    );
    scan_for_patterns(
        ARGUMENT_COLLECTION,
        &[".factory().function(", "self.ctx.types.factory().function("],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "call diagnostic display surfaces must route solver construction through \
         query_boundaries::checkers::call or query_boundaries::construct_signatures:\n{}",
        violations.join("\n")
    );
}

#[test]
fn call_diagnostic_surface_callers_use_boundary_helpers() {
    let call_result = fs::read_to_string(checker_path(CALL_RESULT))
        .expect("failed to read types/computation/call_result.rs");
    for helper in [
        "call_checker::call_result_unknown_return_shape",
        "call_checker::call_result_finite_mapped_display_object",
        "call_checker::call_result_literalized_tuple_actual",
        "call_checker::call_result_tuple_tail",
        "call_checker::call_result_spread_rest_tuple_display_target",
    ] {
        assert!(
            call_result.contains(helper),
            "call_result.rs must route call diagnostic display construction through `{helper}`"
        );
    }

    // `generic_callable_mismatch_display_target` (the caller of this one
    // helper) was split out into its own file to stay under the file-size
    // guard (#17449); the routing itself did not move.
    let call_result_generic_display = fs::read_to_string(checker_path(CALL_RESULT_GENERIC_DISPLAY))
        .expect("failed to read types/computation/call_result_generic_display.rs");
    assert!(
        call_result_generic_display
            .contains("call_checker::call_result_generic_callable_display_target"),
        "call_result_generic_display.rs must route call diagnostic display construction through \
         `call_checker::call_result_generic_callable_display_target`"
    );

    let argument_collection = fs::read_to_string(checker_path(ARGUMENT_COLLECTION))
        .expect("failed to read call/inner/argument_collection.rs");
    assert!(
        argument_collection.contains(
            "query_boundaries::construct_signatures::function_type_with_return_replaced("
        ),
        "argument_collection.rs must route callback return surface construction \
         through construct_signatures"
    );
}

#[test]
fn call_boundaries_own_diagnostic_surface_helpers() {
    let call_source = fs::read_to_string(checker_path(CALL_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/call.rs");
    let signature_source = fs::read_to_string(checker_path(SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/construct_signatures.rs");

    for helper in [
        "call_result_unknown_return_shape",
        "call_result_finite_mapped_display_object",
        "call_result_literalized_tuple_actual",
        "call_result_tuple_tail",
        "call_result_spread_rest_tuple_display_target",
        "call_result_generic_callable_display_target",
    ] {
        assert!(
            defines_fn(&call_source, helper),
            "query_boundaries::checkers::call must own `{helper}`"
        );
    }
    assert!(
        defines_fn(&signature_source, "function_type_with_return_replaced"),
        "query_boundaries::construct_signatures must own \
         `function_type_with_return_replaced`"
    );
    assert!(
        defines_fn(&signature_source, "function_type_with_params_replaced"),
        "query_boundaries::construct_signatures must own \
         `function_type_with_params_replaced`"
    );

    for construction_pattern in [
        "Arc::new(FunctionShape {",
        "PropertyInfo::new(",
        "db.object(",
        "TupleElement {",
        "db.tuple(",
    ] {
        assert!(
            call_source.contains(construction_pattern),
            "query_boundaries::checkers::call should own `{construction_pattern}`"
        );
    }
    assert!(
        call_source.contains("db.function(FunctionShape {")
            || call_source.contains("function_type_from_shape(")
            || call_source.contains("function_type_with_return_replaced("),
        "query_boundaries::checkers::call should route function display target \
         construction through its own helper or construct_signatures"
    );
    assert!(
        signature_source.contains("return_type,") && signature_source.contains("FunctionShape {"),
        "query_boundaries::construct_signatures should own function shape replacement"
    );
}
