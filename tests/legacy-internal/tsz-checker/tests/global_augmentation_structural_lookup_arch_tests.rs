/// Global augmentation property lookup must use structural type identity,
/// not rendered display strings.
///
/// `global_augmentations` is keyed by simple interface identifier names.
/// Rendering a type with `format_type_diagnostic` can produce complex strings
/// like `"Foo<Bar>"` or `"typeof X"` that would never match a key and that
/// vary with printer settings. The correct approach is to use
/// `module_augmentation_lookup_name_for_type` which resolves the identifier
/// structurally via symbol/def/shape queries.
#[test]
fn object_type_augmentation_lookup_uses_structural_name() {
    let src = include_str!("../types/property_access_augmentation.rs");

    assert!(
        !src.contains("self.format_type_diagnostic(object_type)"),
        "`resolve_object_type_global_augmentation` must not use `format_type_diagnostic` \
         to derive the augmentation lookup key; use `module_augmentation_lookup_name_for_type` instead"
    );

    assert!(
        src.contains("module_augmentation_lookup_name_for_type(object_type)"),
        "`resolve_object_type_global_augmentation` must resolve the augmentation key \
         via `module_augmentation_lookup_name_for_type` (structural symbol/def lookup)"
    );
}
