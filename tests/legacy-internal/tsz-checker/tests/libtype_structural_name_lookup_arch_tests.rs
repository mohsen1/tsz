/// Library-type name classification must use structural type identity,
/// not rendered display strings.
///
/// `is_well_known_lib_type_name` and `is_nominal_lib_object_type_name` compare
/// against bare identifier strings ("Promise", "Array", "Window", …). Deriving
/// the lookup key via `format_type_diagnostic` produces the full rendered form
/// ("Promise<string>", "Array<number>"), which never matches. The correct approach
/// is `named_type_display_name`, which returns the declared identifier name from
/// structural symbol/def queries — bare, printer-independent, and identical for
/// both generic and non-generic references to the same lib type.
#[test]
fn call_inference_lib_type_check_uses_structural_name() {
    let src = include_str!("../types/computation/call_inference.rs");

    assert!(
        !src.contains("let fallback_name = self.format_type_diagnostic(instantiated);"),
        "`fill_unresolved_contextual_substitution_from_constraints` must not use \
         `format_type_diagnostic` to derive the lib-type lookup key; \
         use `named_type_display_name` instead"
    );

    assert!(
        src.contains("named_type_display_name(instantiated)"),
        "`fill_unresolved_contextual_substitution_from_constraints` must derive the \
         lib-type name via `named_type_display_name` (structural symbol/def lookup)"
    );
}

#[test]
fn nominal_lib_object_type_check_uses_structural_name() {
    let src = include_str!("../types/computation/call/nominal_lib_object_callbacks.rs");

    assert!(
        !src.contains("let name = self.format_type_diagnostic(ty);"),
        "`nominal_lib_object_type` must not use `format_type_diagnostic` to derive \
         the nominal-lib-object lookup key; use `named_type_display_name` instead"
    );

    assert!(
        src.contains("named_type_display_name(ty)"),
        "`nominal_lib_object_type` must derive the nominal-lib-object name via \
         `named_type_display_name` (structural symbol/def lookup)"
    );
}
