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
        source.contains("assign_relation_outcome(init_type, TypeId::NUMBER)"),
        "computed enum-member TS18033 diagnostics should route the final relation through assign_relation_outcome"
    );
    assert!(
        source.contains("assign_relation_outcome(init_type, TypeId::STRING)"),
        "computed enum-member import fallback should route string assignability through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "computed enum-member diagnostics should not regress to raw boolean relation guards"
    );
}
