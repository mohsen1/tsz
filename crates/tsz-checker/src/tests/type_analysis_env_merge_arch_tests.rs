use std::fs;

#[test]
fn type_analysis_env_merges_use_deferred_helpers() {
    let cross_file = fs::read_to_string("src/state/type_analysis/cross_file_env_merge.rs")
        .expect("failed to read cross_file_env_merge.rs");
    let enum_member = fs::read_to_string("src/state/type_analysis/computed_helpers_binding.rs")
        .expect("failed to read computed_helpers_binding.rs");

    for forbidden in [
        "type_env.try_borrow_mut",
        "type_environment.try_borrow_mut",
        "could not borrow parent",
        "insert_class_instance_type(",
        "register_class_extends(",
        "insert_def(",
    ] {
        assert!(
            !cross_file.contains(forbidden),
            "cross-file env snapshot merge must not use raw env write path `{forbidden}`"
        );
    }

    for required in [
        "merge_def_if_missing_in_env(",
        "merge_class_instance_if_missing_in_env(",
        "merge_class_extends_if_missing_in_env(",
    ] {
        assert!(
            cross_file.contains(required),
            "cross-file env snapshot merge should route through {required}"
        );
    }

    for forbidden in ["type_env.try_borrow_mut", "env.register_enum_parent("] {
        assert!(
            !enum_member.contains(forbidden),
            "enum member env publication must not use raw env write path `{forbidden}`"
        );
    }
    assert!(
        enum_member.contains("register_enum_parent_in_env("),
        "enum member parent publication should route through register_enum_parent_in_env"
    );
}
