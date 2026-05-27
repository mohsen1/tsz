use std::fs;

/// Array-destructuring element diagnostics introduced by
/// `noUncheckedIndexedAccess` should use the pre-resolved exact-anchor relation
/// diagnostic helper instead of a raw relation guard plus a manual TS2322
/// reporter.
#[test]
fn array_destructuring_unchecked_element_uses_relation_diagnostic_helper() {
    let source = fs::read_to_string("src/assignability/assignment_checker/destructuring.rs")
        .expect("failed to read assignment_checker/destructuring.rs");

    assert!(
        source.contains("check_pre_resolved_assignable_or_report_at_exact_anchor"),
        "array destructuring element validation must route through the \
         pre-resolved exact-anchor relation diagnostic helper"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(check_type, target_type)"),
        "array destructuring element validation must not pre-gate TS2322 with \
         a raw diagnostic relation boolean"
    );
}
