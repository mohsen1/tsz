//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/default_import_alias_rewrite.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6ffab0747ac6b702a5f58a020db03a14341af366a1f3362e183cffb8b1088058 378 rewrites_bare_default_import_alias_type_references
    #[test]
    fn rewrites_bare_default_import_alias_type_references() {
        assert_eq!(
            DeclarationEmitter::rewrite_bare_type_reference_to_default_alias(
                r#"import("mod/ctor").ExtendedCtor<Ctor>"#,
                "Ctor",
                "mod",
            ),
            r#"import("mod/ctor").ExtendedCtor<import("mod").default>"#,
        );
    }
// TSZ_INLINE_TEST_END 6ffab0747ac6b702a5f58a020db03a14341af366a1f3362e183cffb8b1088058

// TSZ_INLINE_TEST_BEGIN 7940a9ced00d68c2c67c20b0fc07e73b56ae30fb3f7866d46334c838e21c44a8 390 bare_default_import_alias_rewrite_ignores_property_names_and_qualified_names
    #[test]
    fn bare_default_import_alias_rewrite_ignores_property_names_and_qualified_names() {
        assert_eq!(
            DeclarationEmitter::rewrite_bare_type_reference_to_default_alias(
                r#"{ Ctor: string; nested: ns.Ctor; value: "Ctor"; item?: Ctor }"#,
                "Ctor",
                "mod",
            ),
            r#"{ Ctor: string; nested: ns.Ctor; value: "Ctor"; item?: import("mod").default }"#,
        );
    }
// TSZ_INLINE_TEST_END 7940a9ced00d68c2c67c20b0fc07e73b56ae30fb3f7866d46334c838e21c44a8
