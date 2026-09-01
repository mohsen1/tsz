use std::fs;
use std::path::Path;

#[test]
fn computed_enum_member_ts18033_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/state/state_checking_members/statement_helpers.rs"),
    )
    .expect("failed to read statement_helpers.rs");

    assert!(
        source.contains("computed_enum_member_relation_outcome(init_type, TypeId::NUMBER)"),
        "computed enum-member TS18033 diagnostics should route the final relation through the role-specific outcome"
    );
    assert!(
        source.contains("computed_enum_member_relation_outcome(init_type, TypeId::STRING)"),
        "computed enum-member import fallback should route string assignability through the role-specific outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "computed enum-member diagnostics should not regress to raw boolean relation guards"
    );
    assert!(
        !source.contains("assign_relation_outcome(init_type, TypeId::NUMBER)")
            && !source.contains("assign_relation_outcome(init_type, TypeId::STRING)"),
        "computed enum-member diagnostics should not use generic assign relation outcomes"
    );
    assert!(
        source.contains("enum_initializer_evaluation_status(init_idx)"),
        "computed enum-member diagnostics should ask enum_eval for evaluator success"
    );
    for forbidden_helper in [
        "fn would_enum_eval_succeed(",
        "fn is_identifier_evaluatable_in_enum(",
    ] {
        assert!(
            !source.contains(forbidden_helper),
            "statement helpers should not own enum initializer evaluator helper `{forbidden_helper}`"
        );
    }

    let enum_eval_source = fs::read_to_string("src/types/utilities/enum_eval.rs")
        .expect("failed to read enum_eval.rs");
    assert!(
        enum_eval_source.contains("fn enum_initializer_evaluation_status(")
            && enum_eval_source.contains("fn identifier_evaluates_as_enum_initializer("),
        "enum_eval should own TS18033 initializer evaluator status helpers"
    );
}

#[test]
fn computed_enum_member_relation_outcome_uses_computed_enum_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn computed_enum_member_relation_outcome(")
            && source.contains("RelationRequest::computed_enum_member("),
        "computed enum-member diagnostics should have a request-shaped RelationKind::ComputedEnumMember helper"
    );
}
