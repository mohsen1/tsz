use std::fs;
use std::path::Path;

#[test]
fn decorator_return_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/state/state_checking_members/decorator_signature_checks.rs"),
    )
    .expect("failed to read decorator_signature_checks.rs");
    let class_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/state/state_checking/class.rs"),
    )
    .expect("failed to read state_checking/class.rs");
    let compact_class_source: String = class_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        source.contains("assign_relation_outcome(return_type, expected_return)"),
        "method/accessor decorator return diagnostics should route through assign_relation_outcome"
    );
    assert!(
        source.contains("assign_relation_outcome(return_type, TypeId::VOID)"),
        "void-or-any decorator return diagnostics should route through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(return_type, expected_return)"),
        "method/accessor decorator return diagnostics should not pre-gate with a raw boolean relation"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(return_type, TypeId::VOID)"),
        "void-or-any decorator return diagnostics should not pre-gate with a raw boolean relation"
    );
    assert!(
        compact_class_source
            .contains("return_relation_outcome(return_type,expected_return).related"),
        "class decorator return diagnostics should route through return_relation_outcome"
    );
    assert!(
        !compact_class_source
            .contains("diagnostic_relation_boolean_guard(return_type,expected_return)"),
        "class decorator return diagnostics should not pre-gate with a raw boolean relation"
    );
}
