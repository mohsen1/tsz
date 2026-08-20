//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN babafa3c210bf5f5f970f3b8b6c3a127f25dd7780b96133afd3bca0f796708fa 1360 stable_symbol_fallback_rejects_in_flight_any_sentinel
    #[test]
    fn stable_symbol_fallback_rejects_in_flight_any_sentinel() {
        let mut parser = ParserState::new("test.ts".to_string(), "const payload = 1;".to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let symbol = binder.file_locals.get("payload").expect("payload symbol");
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            arena,
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        checker.ctx.symbol_types.insert(symbol, TypeId::ANY);
        assert_eq!(
            FlowAnalyzer::from_ctx(&checker.ctx).fallback_cached_stable_symbol_type(symbol),
            Some(TypeId::ANY),
            "resolved semantic any remains valid generic inference evidence"
        );

        checker.ctx.symbol_resolution_set.insert(symbol);
        assert_eq!(
            FlowAnalyzer::from_ctx(&checker.ctx).fallback_cached_stable_symbol_type(symbol),
            None,
            "an in-flight any sentinel must not become current-pass flow evidence"
        );
    }
// TSZ_INLINE_TEST_END babafa3c210bf5f5f970f3b8b6c3a127f25dd7780b96133afd3bca0f796708fa
