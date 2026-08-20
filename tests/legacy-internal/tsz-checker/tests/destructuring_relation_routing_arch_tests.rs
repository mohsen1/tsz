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
            .matches("destructuring_relation_outcome(default_type,target_type")
            .count()
            >= 2
            && compact_source
                .contains("destructuring_relation_outcome(prop_type,target_type).related")
            && compact_source
                .contains("destructuring_relation_outcome(source_type,target_prop_type).related"),
        "destructuring default/property relation checks should route through destructuring relation outcomes"
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
            .contains("letoutcome=self.destructuring_relation_outcome(source,rest_target_type);")
            && compact_source.contains("ifoutcome.related{return;}"),
        "object rest destructuring should use the destructuring relation outcome for the rest target decision"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(source,rest_target_type)"),
        "object rest destructuring must not pre-gate the rest target decision with a raw boolean guard"
    );
}

#[test]
fn destructuring_relation_outcome_uses_destructuring_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn destructuring_relation_outcome(")
            && source.contains("RelationRequest::destructuring(source, target)"),
        "destructuring assignment diagnostics should have a request-shaped RelationKind::Destructuring helper"
    );
}

#[test]
fn binding_pattern_default_inference_uses_relation_outcomes() {
    let source = fs::read_to_string("src/types/queries/binding.rs")
        .expect("failed to read types/queries/binding.rs");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert_eq!(
        compact_source
            .matches("destructuring_relation_outcome(init_type,element_type).related")
            .count(),
        2,
        "object and array binding default inference should route element/default compatibility through destructuring relation outcomes"
    );
    assert!(
        !compact_source.contains("is_assignable_to(init_type,element_type)"),
        "binding default inference should not use raw boolean assignability gates"
    );
}

#[test]
fn state_destructuring_default_inference_uses_relation_outcome() {
    // The default/element-type merge this guards lives in
    // `assign_binding_pattern_symbol_types_with_request_reporting`, split out
    // of `destructuring.rs` into its own file to stay under that file's arch
    // size ratchet.
    let source =
        fs::read_to_string("src/state/variable_checking/destructuring_widened_any_report.rs")
            .expect("failed to read state/variable_checking/destructuring_widened_any_report.rs");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source.contains("destructuring_relation_outcome(init_type,element_type).related"),
        "state destructuring default inference should route element/default compatibility through destructuring relation outcomes"
    );
    assert!(
        !compact_source.contains("is_assignable_to(init_type,element_type)"),
        "state destructuring default inference should not use a raw boolean assignability gate"
    );
}
