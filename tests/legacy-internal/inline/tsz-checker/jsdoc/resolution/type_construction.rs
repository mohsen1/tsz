//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9368aa025d7a240d574e2d76bdca8e7f8453e0c709220ede93e21e481b4c6e39 1962 resolve_jsdoc_assigned_value_type_sees_legacy_prototype_property_statement
    #[test]
    fn resolve_jsdoc_assigned_value_type_sees_legacy_prototype_property_statement() {
        let source = r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#;
        let options = crate::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..crate::context::CheckerOptions::default()
        };
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);
        assert_eq!(
            checker
                .resolve_jsdoc_assigned_value_type("C.prototype.x")
                .map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
    }
// TSZ_INLINE_TEST_END 9368aa025d7a240d574e2d76bdca8e7f8453e0c709220ede93e21e481b4c6e39
