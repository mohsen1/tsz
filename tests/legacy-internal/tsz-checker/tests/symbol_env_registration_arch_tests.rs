use std::fs;

#[test]
fn symbol_env_registration_uses_deferred_env_helpers() {
    let source = fs::read_to_string("src/state/type_analysis/symbol_env_registration.rs")
        .expect("failed to read symbol_env_registration.rs");

    for forbidden in [
        "type_env.try_borrow_mut",
        "try_borrow_mut FAILED",
        "insert_symbol_type_and_mirror(",
        "env.insert_def(",
        "env.insert_def_with_params(",
        "env.insert_class_instance_type(",
        "env.register_enum_parent(",
    ] {
        assert!(
            !source.contains(forbidden),
            "symbol result publication must not use raw env write path `{forbidden}`"
        );
    }

    for required in [
        "register_symbol_type_in_envs(",
        "register_def_auto_params_in_envs(",
        "register_class_instance_in_envs(",
        "register_enum_parent_in_envs(",
    ] {
        assert!(
            source.contains(required),
            "symbol result publication should route through {required}"
        );
    }
}
