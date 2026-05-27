use std::fs;

/// Enum-element object-literal member diagnostics should use the canonical
/// exact-anchor relation diagnostic helper instead of a raw relation guard plus
/// a manual TS2322 reporter.
#[test]
fn enum_member_object_literal_validation_uses_relation_diagnostic_helper() {
    let source = fs::read_to_string("src/state/variable_checking/core.rs")
        .expect("failed to read variable_checking/core.rs");

    assert!(
        source.contains("check_assignable_or_report_at_exact_anchor_without_source_elaboration"),
        "per-member enum object-literal validation must route through the \
         exact-anchor relation diagnostic helper"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(value_type, enum_element_type)"),
        "per-member enum object-literal validation must not pre-gate TS2322 \
         with a raw diagnostic relation boolean"
    );
}
