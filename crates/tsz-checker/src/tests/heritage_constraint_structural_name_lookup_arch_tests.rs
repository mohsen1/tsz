/// Heritage type-name resolution for generic constraints must use structural
/// identity, not rendered display strings.
///
/// `symbol_id_for_heritage_type_name` is called as a last-resort fallback when
/// checking whether a type satisfies a heritage constraint. The previous
/// implementation called `format_type_diagnostic`, stripped a possible
/// `"globalThis."` prefix, and validated identifier characters — all artefacts
/// of using the printer as an identity oracle.
///
/// The correct approach is `named_type_display_name`, which returns the type's
/// declared identifier name from structural symbol/def/shape queries (or `None`
/// for unnamed types). This is deterministic, printer-independent, and avoids
/// the character-set guard that was needed only to filter out rendered non-names.
#[test]
fn heritage_type_name_resolution_uses_structural_lookup() {
    let src = include_str!("../checkers/generic_checker/recursive_heritage_constraint.rs");

    assert!(
        !src.contains("self.format_type_diagnostic(type_id)"),
        "`symbol_id_for_heritage_type_name` must not use `format_type_diagnostic` \
         to derive the heritage type name; use `named_type_display_name` instead"
    );

    assert!(
        src.contains("self.named_type_display_name(type_id)"),
        "`symbol_id_for_heritage_type_name` must resolve the type name structurally \
         via `named_type_display_name`"
    );

    assert!(
        !src.contains("strip_prefix(\"globalThis.\")"),
        "`symbol_id_for_heritage_type_name` must not strip a `globalThis.` prefix; \
         `named_type_display_name` produces bare identifier names without renderer artefacts"
    );
}
