use std::fs;
use std::path::Path;

#[test]
fn assignability_reporter_relation_probes_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/assignability.rs"),
    )
    .expect("failed to read assignability.rs");
    let missing_property_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/error_reporter/assignability_missing_property_satisfaction.rs"),
    )
    .expect("failed to read assignability_missing_property_satisfaction.rs");

    let missing_property_helper = missing_property_source
        .split("fn missing_property_is_satisfied_by_source")
        .nth(1)
        .and_then(|tail| tail.split("}\n}").next())
        .expect("failed to isolate missing-property satisfaction helper");
    let missing_property_compact = missing_property_helper
        .split_whitespace()
        .collect::<String>();
    assert!(
        missing_property_compact.contains(
            "missing_property_read_relation_outcome(source_prop.type_id,target_prop.type_id)"
        ),
        "missing-property read compatibility should route through missing_property_read_relation_outcome"
    );
    assert!(
        missing_property_helper.contains(
            "bivariant_callbacks_relation_outcome(source_prop.type_id, target_prop.type_id)"
        ),
        "missing-property method read compatibility should route through the bivariant RelationOutcome"
    );
    assert!(
        missing_property_compact.contains(
            "missing_property_write_relation_outcome(target_prop.write_type,source_prop.write_type,)"
        ),
        "missing-property write compatibility should route through missing_property_write_relation_outcome"
    );
    assert!(
        !missing_property_helper.contains("assign_relation_outcome(")
            && !missing_property_helper.contains("diagnostic_relation_boolean_guard("),
        "missing-property compatibility should not use generic assign or raw diagnostic boolean guards"
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
        exact_optional_helper
            .contains("exact_optional_source_filter_relation_outcome(m, target_eval)"),
        "exact optional mismatch filtering should route through exact_optional_source_filter_relation_outcome"
    );
    assert!(
        !exact_optional_helper.contains("assign_relation_outcome(")
            && !exact_optional_helper.contains("diagnostic_relation_boolean_guard(m, target_eval)"),
        "exact optional mismatch filtering should not use generic assign or the raw diagnostic boolean guard"
    );
}

#[test]
fn assignability_reporter_relation_outcomes_use_dedicated_requests() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation_outcome_helpers.rs");

    for (helper, request) in [
        (
            "fn missing_property_read_relation_outcome(",
            "RelationRequest::missing_property_read(",
        ),
        (
            "fn missing_property_write_relation_outcome(",
            "RelationRequest::missing_property_write(",
        ),
        (
            "fn concrete_remapped_mapped_missing_property_relation_outcome(",
            "RelationRequest::concrete_remapped_mapped_missing_property(",
        ),
        (
            "fn exact_optional_source_filter_relation_outcome(",
            "RelationRequest::exact_optional_source_filter(",
        ),
    ] {
        assert!(
            source.contains(helper) && source.contains(request),
            "{helper} must build {request}"
        );
    }
}
