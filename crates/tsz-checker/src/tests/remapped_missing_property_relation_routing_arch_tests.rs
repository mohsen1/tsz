use std::fs;
use std::path::Path;

#[test]
fn remapped_missing_property_skip_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/assignability_helpers.rs"),
    )
    .expect("failed to read assignability_helpers.rs");

    let helper = source
        .split("pub(crate) fn try_report_concrete_remapped_mapped_missing_property")
        .nth(1)
        .and_then(|rest| {
            rest.split("fn report_concrete_remapped_mapped_missing_property")
                .next()
        })
        .expect("failed to isolate remapped missing-property helper");

    assert!(
        helper.contains("assign_relation_outcome(evaluated, target).related"),
        "remapped missing-property diagnostic skip should route through assign_relation_outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard(evaluated, target)"),
        "remapped missing-property diagnostic skip should not use the raw boolean relation guard"
    );
}
