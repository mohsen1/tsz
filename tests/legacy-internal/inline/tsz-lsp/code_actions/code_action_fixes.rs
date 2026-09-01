//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/code_actions/code_action_fixes.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 42f46f19084f7c308ae1d0969199a669bc96b8b05742f521de8b951bbe447eab 724 missing_type_annotation_fix_is_only_advertised_for_variable_exports
    #[test]
    fn missing_type_annotation_fix_is_only_advertised_for_variable_exports() {
        let variable_fixes = CodeFixRegistry::fixes_for_error_code(9010);
        assert!(
            variable_fixes
                .iter()
                .any(|(fix_name, _, _, _)| *fix_name == "fixMissingTypeAnnotationOnExports"),
            "TS9010 should advertise fixMissingTypeAnnotationOnExports"
        );

        let function_fixes = CodeFixRegistry::fixes_for_error_code(9007);
        assert!(
            function_fixes
                .iter()
                .all(|(fix_name, _, _, _)| *fix_name != "fixMissingTypeAnnotationOnExports"),
            "TS9007 must not advertise fixMissingTypeAnnotationOnExports until the server implements function return edits"
        );

        assert!(
            CodeFixRegistry::supported_error_codes().contains(&9010),
            "TS9010 should remain in supported_error_codes"
        );
        assert!(
            !CodeFixRegistry::supported_error_codes().contains(&9007),
            "TS9007 should not be in supported_error_codes"
        );
    }
// TSZ_INLINE_TEST_END 42f46f19084f7c308ae1d0969199a669bc96b8b05742f521de8b951bbe447eab
