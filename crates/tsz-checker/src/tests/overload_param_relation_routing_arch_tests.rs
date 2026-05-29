use std::{fs, path::PathBuf};

#[test]
fn overload_parameter_signature_check_uses_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/state_checking_members/overload_compatibility.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

    assert!(
        source.contains("assign_relation_outcome(impl_with_any_ret, overload_with_any_ret)"),
        "overload parameter-only compatibility should use the shared relation outcome boundary"
    );
    assert!(
        !source.contains(
            "diagnostic_relation_boolean_guard(impl_with_any_ret, overload_with_any_ret)"
        ),
        "overload parameter-only compatibility should not use the legacy boolean guard"
    );
}
