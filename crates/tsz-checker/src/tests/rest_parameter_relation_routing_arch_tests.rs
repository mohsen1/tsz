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
    let function_end = function_start
        + source[function_start..]
            .find(
                "// =============================================================================",
            )
            .expect("find end of rest parameter validation helper");
    let helper = &source[function_start..function_end];
    let compact_helper: String = helper.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helper
            .contains("assign_relation_outcome(declared_type,readonly_any_array).related"),
        "rest parameter declared type should route array compatibility through relation outcome"
    );
    assert!(
        compact_helper
            .contains("assign_relation_outcome(array_check_type,readonly_any_array).related"),
        "rest parameter resolved type should route array compatibility through relation outcome"
    );
    assert!(
        compact_helper.contains("assign_relation_outcome(init_type,readonly_any_array).related"),
        "rest parameter initializer type should route array compatibility through relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "TS2370 rest parameter array diagnostics should not use raw boolean relation guards"
    );
}
