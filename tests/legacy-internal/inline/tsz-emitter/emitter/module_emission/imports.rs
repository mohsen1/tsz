//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/module_emission/imports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 774cbf690c69c2a62b11ec842a960fc4ea667b818f2aa57372071129f65806db 1871 import_alias_redeclaration_requires_import_equals
    #[test]
    fn import_alias_redeclaration_requires_import_equals() {
        assert!(
            crate::import_usage::contains_identifier_occurrence_before_shadow(
                "import M = Z.I;\nM.bar();",
                "M",
            )
        );
        assert!(
            !crate::import_usage::contains_identifier_occurrence_before_shadow(
                "import M from \"pkg\";\nM.bar();",
                "M",
            )
        );
    }
// TSZ_INLINE_TEST_END 774cbf690c69c2a62b11ec842a960fc4ea667b818f2aa57372071129f65806db
