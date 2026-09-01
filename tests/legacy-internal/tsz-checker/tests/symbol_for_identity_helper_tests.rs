#[test]
fn symbol_for_initializer_uses_global_identity_helper() {
    let source = include_str!("../types/computation/helpers.rs");
    assert!(
        source.contains("identifier_resolves_to_unshadowed_global(access.expression, \"Symbol\")"),
        "`Symbol.for` initializer detection must route through the shared global identity helper"
    );
    assert!(
        !source.contains("get_identifier_text(access.expression)\n            .is_some_and(|name| name == \"Symbol\")\n            && !self.known_global_value_has_local_shadow(access.expression, \"Symbol\")"),
        "`Symbol.for` initializer detection must not duplicate a spelling-plus-shadow check"
    );
}
