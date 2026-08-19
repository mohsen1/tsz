use std::fs;

#[test]
fn type_analysis_env_merges_use_deferred_helpers() {
    let cross_file = fs::read_to_string("src/state/type_analysis/cross_file_env_merge.rs")
        .expect("failed to read cross_file_env_merge.rs");
    let enum_member = {
        let mut merged = fs::read_to_string("src/state/type_analysis/computed_helpers_binding.rs")
            .expect("failed to read computed_helpers_binding.rs");
        merged.push_str(
            &fs::read_to_string("src/state/type_analysis/computed_class_symbol.rs")
                .expect("failed to read computed_class_symbol.rs"),
        );
        merged
    };

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
        "merge_def_if_missing_in_envs(",
        "merge_class_instance_if_missing_in_envs(",
        "merge_class_extends_if_missing_in_envs(",
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
        enum_member.contains("register_enum_parent_in_envs("),
        "enum member parent publication should route through register_enum_parent_in_envs"
    );
}

#[test]
fn unresolved_resolution_env_writes_use_deferred_authority() {
    let resolver =
        fs::read_to_string("src/context/resolver.rs").expect("failed to read context/resolver.rs");
    let lazy =
        fs::read_to_string("src/state/type_environment/lazy.rs").expect("failed to read lazy.rs");
    let env_writes = fs::read_to_string("src/context/def_mapping_env_writes.rs")
        .expect("failed to read def_mapping_env_writes.rs");
    let deferred = fs::read_to_string("src/context/deferred_flow_env_write.rs")
        .expect("failed to read deferred_flow_env_write.rs");

    for (label, source) in [("context resolver", resolver), ("lazy evaluation", lazy)] {
        assert!(
            !source.contains(".insert_unresolved_resolution("),
            "{label} must not mutate only one TypeEnvironment for unresolved-name caches"
        );
        assert!(
            source.contains("register_unresolved_resolution_in_envs("),
            "{label} should route unresolved-name cache writes through register_unresolved_resolution_in_envs"
        );
    }

    assert!(
        env_writes.contains("register_unresolved_resolution_in_envs("),
        "unresolved-name cache writes should have a named env authority wrapper"
    );
    assert!(
        deferred.contains("InsertUnresolvedResolution"),
        "deferred replay must retain the unresolved-name write operation"
    );
}
