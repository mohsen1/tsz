use std::fs;
use std::path::Path;

#[test]
fn contextual_new_argument_recovery_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/types/computation/complex_contextual_new.rs"),
    )
    .expect("failed to read complex_contextual_new.rs");

    let function_start = source
        .find("fn generic_new_argument_accepts_contextual_parameter")
        .expect("find contextual new argument helper");
    let function_end = function_start
        + source[function_start..]
            .find(
                "pub(crate) fn recover_new_expression_return_type_after_contextual_argument_match",
            )
            .expect("find end of contextual new argument helper");
    let helper = &source[function_start..function_end];
    let compact_helper: String = helper.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helper.contains("assign_relation_outcome(contextual_actual,expected).related"),
        "contextual new argument recovery should route compatibility through relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "contextual new argument recovery should not use a raw boolean relation guard"
    );
}
