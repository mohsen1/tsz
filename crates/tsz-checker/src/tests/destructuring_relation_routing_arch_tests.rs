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

#[test]
fn destructuring_default_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/assignability/assignment_checker/destructuring.rs")
        .expect("failed to read assignment_checker/destructuring.rs");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source
            .matches("assign_relation_outcome(default_type,target_type).related")
            .count()
            >= 2
            && compact_source.contains("assign_relation_outcome(prop_type,target_type).related")
            && compact_source
                .contains("assign_relation_outcome(source_type,target_prop_type).related"),
        "destructuring default/property relation checks should route through relation outcomes"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(default_type,target_type)")
            && !compact_source.contains("diagnostic_relation_boolean_guard(prop_type,target_type)")
            && !compact_source
                .contains("diagnostic_relation_boolean_guard(source_type,target_prop_type)",),
        "destructuring default/property relation checks should not use raw boolean guards"
    );
}

#[test]
fn object_rest_destructuring_uses_single_relation_outcome() {
    let source = fs::read_to_string("src/assignability/assignment_checker/destructuring.rs")
        .expect("failed to read assignment_checker/destructuring.rs");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source
            .contains("letoutcome=self.assign_relation_outcome(source,rest_target_type);")
            && compact_source.contains("ifoutcome.related{return;}"),
        "object rest destructuring should use the shared relation outcome for the rest target decision"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(source,rest_target_type)"),
        "object rest destructuring must not pre-gate the rest target decision with a raw boolean guard"
    );
}
