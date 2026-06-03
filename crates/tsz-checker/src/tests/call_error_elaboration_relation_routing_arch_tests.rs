use std::fs;
use std::path::Path;

#[test]
fn call_error_object_and_array_elaboration_use_request_shaped_relations() {
    let object_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/error_reporter/call_errors/elaboration_object_properties.rs"),
    )
    .expect("failed to read call object elaboration source");
    let call_arg_elaboration = object_source
        .split("/// Elaborate array literal element mismatches for variable declarations.")
        .next()
        .expect("missing call argument elaboration prefix");
    let variable_initializer_elaboration = object_source
        .split("/// Elaborate array literal element mismatches for variable declarations.")
        .nth(1)
        .expect("missing variable initializer elaboration suffix");

    assert!(
        call_arg_elaboration
            .matches("call_arg_relation_outcome(")
            .count()
            >= 12,
        "call object/array elaboration should route parameter-derived relation probes through call_arg_relation_outcome"
    );
    assert!(
        call_arg_elaboration.contains("return_relation_outcome(body_type, expected_ret)"),
        "callback return elaboration should route return-expression probes through return_relation_outcome"
    );
    assert!(
        !call_arg_elaboration.contains("assign_relation_outcome("),
        "call argument elaboration should not regress to generic assign relation outcomes"
    );
    assert!(
        variable_initializer_elaboration
            .contains("variable_initializer_relation_outcome(init_type, declared_type)"),
        "variable initializer array elaboration should route the whole-initializer gate through variable_initializer_relation_outcome"
    );
    assert!(
        !variable_initializer_elaboration.contains("assign_relation_outcome("),
        "variable initializer array elaboration should not regress to generic assign relation outcomes"
    );

    let array_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/error_reporter/call_errors/elaboration_array_mismatch.rs"),
    )
    .expect("failed to read call array elaboration source");
    assert!(
        array_source.contains("call_arg_relation_outcome(elem_type, target_element)"),
        "call array mismatch elaboration should route element probes through call_arg_relation_outcome"
    );
    assert!(
        !array_source.contains("assign_relation_outcome("),
        "call array mismatch elaboration should not regress to generic assign relation outcomes"
    );
}
