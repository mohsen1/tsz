use std::fs;
use std::path::Path;

#[test]
fn assignability_reporter_relation_probes_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/assignability.rs"),
    )
    .expect("failed to read assignability.rs");

    let missing_property_helper = source
        .split("fn missing_property_is_satisfied_by_source")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn should_suppress_outer_callback_return_assignability")
                .next()
        })
        .expect("failed to isolate missing-property satisfaction helper");
    assert!(
        missing_property_helper
            .contains("assign_relation_outcome(source_prop.type_id, target_prop.type_id)"),
        "missing-property read compatibility should route through assign_relation_outcome"
    );
    assert!(
        missing_property_helper
            .contains("assign_relation_outcome(target_prop.write_type, source_prop.write_type)"),
        "missing-property write compatibility should route through assign_relation_outcome"
    );
    assert!(
        !missing_property_helper.contains("diagnostic_relation_boolean_guard(source_prop.type_id"),
        "missing-property read compatibility should not use the raw diagnostic boolean guard"
    );
    assert!(
        !missing_property_helper
            .contains("diagnostic_relation_boolean_guard(target_prop.write_type"),
        "missing-property write compatibility should not use the raw diagnostic boolean guard"
    );

    let exact_optional_helper = source
        .split("fn exact_optional_source_for_message")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn format_exact_optional_target_type_for_message")
                .next()
        })
        .expect("failed to isolate exact optional display helper");
    assert!(
        exact_optional_helper.contains("assign_relation_outcome(m, target_eval).related"),
        "exact optional mismatch filtering should route through assign_relation_outcome"
    );
    assert!(
        !exact_optional_helper.contains("diagnostic_relation_boolean_guard(m, target_eval)"),
        "exact optional mismatch filtering should not use the raw diagnostic boolean guard"
    );
}
