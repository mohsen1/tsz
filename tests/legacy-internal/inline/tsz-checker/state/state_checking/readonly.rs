//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking/readonly.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d399146394fde6e7072efa62a2a8c1bcf86173f47e2b61f9c1f5d17508aa60f7 1823 get_class_name_from_expression_resolves_named_class_expression_return
    #[test]
    fn get_class_name_from_expression_resolves_named_class_expression_return() {
        use tsz_parser::parser::syntax_kind_ext;

        let source = r#"
const C = class D {
    static #field = D.#method();
    static #method() { return 42; }
    static getClass() { return D; }
};

C.getClass().#method;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );

        checker.check_source_file(root);

        let call_idx = find_node_by_text_and_kind(
            parser.get_arena(),
            source,
            syntax_kind_ext::CALL_EXPRESSION,
            "C.getClass()",
        )
        .expect("expected to find `C.getClass()` call expression");

        assert_eq!(
            checker.get_class_name_from_expression(call_idx),
            Some("D".to_string())
        );
    }
// TSZ_INLINE_TEST_END d399146394fde6e7072efa62a2a8c1bcf86173f47e2b61f9c1f5d17508aa60f7
