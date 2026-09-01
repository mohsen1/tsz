//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/jsdoc/parsing_import_attributes.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b5f31ecdb83128bb33860d623546f13b8c51262c1351609131d3848316b21334 69 import_type_tolerates_resolution_mode_attributes_argument
    #[test]
    fn import_type_tolerates_resolution_mode_attributes_argument() {
        // The inline `import("m", { ... }).Member` type carries an attributes
        // argument; `parse_jsdoc_import_type` must skip it and still recover the
        // specifier and member name.
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(
                r#"import("foo", { with: { "resolution-mode": "import" } }).Member"#
            ),
            Some(("foo".to_string(), Some("Member".to_string())))
        );
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(
                r#"import("foo", { with: { "resolution-mode": "require" } })"#
            ),
            Some(("foo".to_string(), None))
        );
    }
// TSZ_INLINE_TEST_END b5f31ecdb83128bb33860d623546f13b8c51262c1351609131d3848316b21334

// TSZ_INLINE_TEST_BEGIN 71dcdc837390e202ed32df74eaa4c4c3a8301c6dce0620abec5001cd6dcb1b4a 88 import_type_reads_resolution_mode_override
    #[test]
    fn import_type_reads_resolution_mode_override() {
        use crate::context::ResolutionModeOverride;
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(
                r#"import("foo", { with: { "resolution-mode": "import" } }).Member"#
            ),
            Some(ResolutionModeOverride::Import)
        );
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(
                r#"import("foo", { with: { "resolution-mode": "require" } }).Member"#
            ),
            Some(ResolutionModeOverride::Require)
        );
        assert_eq!(
            CheckerState::jsdoc_import_type_resolution_mode(r#"import("foo").Member"#),
            None
        );
    }
// TSZ_INLINE_TEST_END 71dcdc837390e202ed32df74eaa4c4c3a8301c6dce0620abec5001cd6dcb1b4a
