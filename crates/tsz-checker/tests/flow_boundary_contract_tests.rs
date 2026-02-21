use std::fs;

#[test]
fn control_flow_contains_type_parameter_checks_use_flow_query_boundary() {
    let src = fs::read_to_string("src/control_flow.rs")
        .expect("failed to read src/control_flow.rs for architecture guard");

    assert!(
        src.contains("query::contains_type_parameters("),
        "control_flow type-parameter cacheability checks should route through query_boundaries::flow_analysis"
    );
    assert!(
        !src.contains("tsz_solver::type_queries::contains_type_parameters_db("),
        "control_flow should not call solver type_queries::contains_type_parameters_db directly"
    );
}

#[test]
fn control_flow_assignability_helpers_use_flow_query_boundary() {
    let src = fs::read_to_string("src/control_flow.rs")
        .expect("failed to read src/control_flow.rs for architecture guard");

    assert!(
        src.contains("query::is_assignable_with_env("),
        "FlowAnalyzer assignability should route through query_boundaries::flow_analysis"
    );
    assert!(
        src.contains("query::is_assignable_strict_null("),
        "FlowAnalyzer strict-null assignability should route through query_boundaries::flow_analysis"
    );
}
