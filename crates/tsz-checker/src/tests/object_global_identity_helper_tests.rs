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
