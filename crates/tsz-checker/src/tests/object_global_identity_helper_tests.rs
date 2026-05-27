#[test]
fn prototype_define_property_uses_global_identity_helper() {
    let source = include_str!("../types/computation/complex_constructors.rs");
    assert!(
        source.contains("self.identifier_resolves_to_unshadowed_global(idx, \"Object\")"),
        "`Object.defineProperty` prototype detection must route through the shared global identity helper"
    );
    assert!(
        !source.contains("let is_object_lib_symbol = |sym_id|"),
        "`Object.defineProperty` prototype detection must not duplicate Object lib-symbol matching"
    );
}

#[test]
fn property_error_reporter_uses_global_identity_helper() {
    let source = include_str!("../error_reporter/properties.rs");
    assert!(
        source.contains("identifier_resolves_to_unshadowed_global(access.expression, \"Object\")"),
        "property diagnostics must route global Object recognition through the shared helper"
    );
    assert!(
        !source.contains("fn is_unshadowed_global_object_identifier"),
        "property diagnostics must not keep a local Object global identity helper"
    );
}

#[test]
fn strict_object_paths_use_proven_global_identity_helper() {
    let object_literal_source = include_str!("../types/computation/object_literal/mod.rs");
    assert!(
        object_literal_source.contains("identifier_resolves_to_proven_lib_global")
            && object_literal_source.contains("\"Object\""),
        "`Object.defineProperty` descriptor detection must route through the shared proven-lib global identity helper"
    );
    assert!(
        !object_literal_source.contains("symbol_is_from_actual_or_cloned_lib(sym_id)"),
        "`Object.defineProperty` descriptor detection must not duplicate Object lib-symbol matching"
    );

    let object_assign_source = include_str!("../state/variable_checking/variable_helpers/core.rs");
    assert!(
        object_assign_source.contains("identifier_resolves_to_proven_lib_global")
            && object_assign_source.contains("\"Object\""),
        "`Object.assign` portability detection must route through the shared proven-lib global identity helper"
    );
    assert!(
        !object_assign_source.contains("fn object_assign_receiver_is_lib_object"),
        "`Object.assign` portability detection must not keep a local Object lib-symbol helper"
    );
}
