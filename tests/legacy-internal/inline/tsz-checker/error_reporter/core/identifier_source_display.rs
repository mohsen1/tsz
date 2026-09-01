//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/core/identifier_source_display.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fb758404c06c22171489755128e276c0cc6782acc92fe0dcfad6791442b0754d 324 stable_declaration_resolves_to_variable_decl_node
    /// Resolving `Symbol::stable_declarations.first()` for a `let`
    /// initialized with an array-literal must return the same variable
    /// declaration node that `Symbol::declarations[0]` points at in the
    /// same binder. This is the invariant that the new code path relies
    /// on for behavior-equivalence with the legacy `NodeIndex` lookup.
    #[test]
    fn stable_declaration_resolves_to_variable_decl_node() {
        let source = "let xs = [{ a: 1 }, { a: 2 }];\n".to_string();

        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let sym_id = binder.file_locals.get("xs").expect("variable symbol xs");
        let symbol = binder.symbols.get(sym_id).expect("symbol data");
        let stable = *symbol
            .stable_declarations
            .first()
            .expect("variable xs must have at least one stable_declarations entry");
        assert!(
            stable.is_known(),
            "variable xs must have a known stable_declarations[0] span"
        );
        let legacy_node_idx = *symbol
            .declarations
            .first()
            .expect("variable xs must have at least one declarations entry");

        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(stable)
            .expect("node_at_stable_location must resolve the variable-decl span");

        assert_eq!(
            resolved_idx, legacy_node_idx,
            "StableLocation must rehydrate to the same NodeIndex as declarations[0]"
        );
        let resolved_node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in arena");
        assert_eq!(resolved_node.pos, stable.pos);
        assert_eq!(resolved_node.end, stable.end);

        // Sanity: the rehydrated index is actually a VariableDeclaration
        // we can walk for an initializer (the production code path).
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must be a VariableDeclaration");
        assert!(
            decl.initializer.is_some(),
            "let xs = [...] must have a populated initializer"
        );
    }
// TSZ_INLINE_TEST_END fb758404c06c22171489755128e276c0cc6782acc92fe0dcfad6791442b0754d

// TSZ_INLINE_TEST_BEGIN 1e65b5e5c7a2443c0098c34d50327b8cf0bcf93cecaf6afff6a3217b4ae6336e 389 stable_location_round_trips_across_arena_reparse_for_var_decl
    /// Phase 5 load-bearing scenario: capture a `StableLocation` from one
    /// binder/arena, drop it, re-parse the same source with a fresh
    /// arena, and verify the captured location still resolves correctly
    /// against the new arena. This proves
    /// `identifier_array_object_literal_source_display` survives Phase 5
    /// arena eviction-and-rehydrate.
    #[test]
    fn stable_location_round_trips_across_arena_reparse_for_var_decl() {
        let source = "let xs = [{ a: 1 }, { a: 2 }];\nlet other = 1;\n".to_string();

        // Capture the first arena's StableLocation for `xs`, then let
        // the first arena/binder go out of scope.
        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder.file_locals.get("xs").expect("variable symbol xs");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            *symbol
                .stable_declarations
                .first()
                .expect("variable xs must have a stable_declarations entry")
        };
        assert!(
            captured.is_known(),
            "captured StableLocation must carry a real (pos, end) span"
        );

        // Fresh parse + bind of the identical source. The captured
        // StableLocation must resolve in this new arena.
        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(captured)
            .expect("captured StableLocation must rehydrate against a freshly parsed arena");
        let node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in the new arena");
        assert_eq!(node.pos, captured.pos);
        assert_eq!(node.end, captured.end);

        // Sanity: still walks as a VariableDeclaration with an
        // array-literal initializer.
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must still be a VariableDeclaration");
        assert!(decl.initializer.is_some());

        // The new binder's `declarations[0]` NodeIndex should agree with
        // the helper's resolution — binder population is deterministic
        // for identical source text.
        let sym_id = binder
            .file_locals
            .get("xs")
            .expect("variable symbol xs in reparsed binder");
        let new_symbol = binder
            .symbols
            .get(sym_id)
            .expect("symbol data in reparsed binder");
        assert_eq!(
            resolved_idx,
            *new_symbol
                .declarations
                .first()
                .expect("reparsed variable xs must have a declarations entry"),
            "re-resolution must agree with the re-parsed binder's NodeIndex"
        );
    }
// TSZ_INLINE_TEST_END 1e65b5e5c7a2443c0098c34d50327b8cf0bcf93cecaf6afff6a3217b4ae6336e

// TSZ_INLINE_TEST_BEGIN ed6d706c8d6b83e4134fb0988ea1cec0a961a228492248cedef30b7ed193e63d 470 stable_location_round_trips_for_boolean_initializer
    /// Regression: a variable whose declaration has only an
    /// `initializer` (no array-literal shape) should also survive the
    /// `StableLocation` round-trip, exercising the
    /// `identifier_literal_initializer_source_display` code path.
    #[test]
    fn stable_location_round_trips_for_boolean_initializer() {
        let source = "let flag = true;\n".to_string();

        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder
                .file_locals
                .get("flag")
                .expect("variable symbol flag");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            *symbol
                .stable_declarations
                .first()
                .expect("variable flag must have a stable_declarations entry")
        };
        assert!(captured.is_known());

        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(captured)
            .expect("captured StableLocation must rehydrate against the reparsed arena");
        let decl = resolved_arena
            .get_variable_declaration_at(resolved_idx)
            .expect("rehydrated NodeIndex must be a VariableDeclaration");
        // The initializer must be present (and untyped); this is exactly
        // what `identifier_literal_initializer_source_display` checks
        // before walking the initializer.
        assert!(decl.initializer.is_some());
        assert!(decl.type_annotation.is_none());
    }
// TSZ_INLINE_TEST_END ed6d706c8d6b83e4134fb0988ea1cec0a961a228492248cedef30b7ed193e63d
