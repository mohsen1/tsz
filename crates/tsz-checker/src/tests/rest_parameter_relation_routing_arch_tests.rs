use std::fs;
use std::path::Path;

#[test]
fn rest_parameter_array_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/parameter_checker.rs"),
    )
    .expect("failed to read parameter_checker.rs");

    let function_start = source
        .find("fn check_rest_parameter_types")
        .expect("find rest parameter validation helper");
    // The helper used to be terminated by the banner comment that introduced the
    // module's in-file `#[cfg(test)]` blocks. Those moved to
    // `src/tests/parameter_checker_tests.rs` when the module was split to hold the
    // 2000-line ceiling, so the banner is gone and the helper is now the last item
    // in the file. Fall back to end-of-file rather than pinning this contract to
    // the presence of a comment that has nothing to do with what it asserts.
    let function_end = function_start
        + source[function_start..]
            .find(
                "// =============================================================================",
            )
            .unwrap_or(source.len() - function_start);
    let helper = &source[function_start..function_end];
    let compact_helper: String = helper.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helper
            .contains("rest_parameter_relation_outcome(effective_type,readonly_any_array).related"),
        "rest parameter effective type (declared + optional `| undefined`) should route array compatibility through relation outcome"
    );
    assert!(
        compact_helper.contains(
            "rest_parameter_relation_outcome(array_check_type,readonly_any_array).related"
        ),
        "rest parameter resolved type should route array compatibility through relation outcome"
    );
    assert!(
        compact_helper
            .contains("rest_parameter_relation_outcome(init_type,readonly_any_array).related"),
        "rest parameter initializer type should route array compatibility through relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "TS2370 rest parameter array diagnostics should not use raw boolean relation guards"
    );
    assert!(
        !compact_helper.contains("assign_relation_outcome(declared_type,readonly_any_array)")
            && !compact_helper
                .contains("assign_relation_outcome(array_check_type,readonly_any_array)")
            && !compact_helper.contains("assign_relation_outcome(init_type,readonly_any_array)"),
        "TS2370 rest parameter array diagnostics should use the role-specific relation outcome"
    );
}

#[test]
fn rest_parameter_relation_outcome_uses_rest_parameter_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn rest_parameter_relation_outcome(")
            && source.contains("RelationRequest::rest_parameter("),
        "rest parameter array diagnostics should have a request-shaped RelationKind::RestParameter helper"
    );
}

#[test]
fn rest_tuple_element_array_like_probe_uses_relation_outcome_boundary() {
    let helper_source = fs::read_to_string("src/types/type_node_helpers.rs")
        .expect("failed to read type_node_helpers.rs");
    let boundary_source = fs::read_to_string("src/query_boundaries/type_checking_utilities.rs")
        .expect("failed to read type_checking_utilities.rs");
    let compact_helper: String = helper_source
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();

    assert!(
        compact_helper.contains("letenv=self.ctx.type_environment.borrow();")
            && compact_helper.contains(
                "rest_element_array_like_relation_outcome(self.ctx.types,&*env,t,readonly_any_array,).related"
            ),
        "TS2574 rest tuple element array-like probes should consume an env-backed outcome boundary"
    );
    assert!(
        !compact_helper.contains("self.ctx.types.is_assignable_to(t,readonly_any_array)"),
        "TS2574 rest tuple element array-like probes should not call raw TypeDatabase assignability"
    );
    assert!(
        boundary_source.contains("fn rest_element_array_like_relation_outcome(")
            && boundary_source.contains("relation_queries::query_relation_with_resolver("),
        "type-node rest element relation truth should live behind a resolver-aware query boundary"
    );
}
