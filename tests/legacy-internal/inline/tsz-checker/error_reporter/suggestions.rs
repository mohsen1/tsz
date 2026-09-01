//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/suggestions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1bd73e4df24621d2c41a9af33aa9103f4b2900aaa6858a570dcbf4aa2fc4ca54 975 ascii_spelling_distance_preserves_weighting_and_thresholds
    #[test]
    fn ascii_spelling_distance_preserves_weighting_and_thresholds() {
        assert_distance("ParentNode", "ParentNode", 0.0, Some(0.0));
        assert_distance("array", "Array", 1.0, Some(0.1));
        assert_distance("sting", "string", 1.0, Some(1.0));
        assert_distance("becon", "bacon", 2.0, Some(2.0));
        assert_distance("becon", "bacon", 1.9, None);
        assert_distance("", "Map", 3.0, Some(3.0));
    }
// TSZ_INLINE_TEST_END 1bd73e4df24621d2c41a9af33aa9103f4b2900aaa6858a570dcbf4aa2fc4ca54

// TSZ_INLINE_TEST_BEGIN 4df306fc92ad1742512f000d14099f5adae5968538817c5a2bfafa1d00488bec 985 unicode_spelling_distance_retains_scalar_case_folding
    #[test]
    fn unicode_spelling_distance_retains_scalar_case_folding() {
        assert_distance("École", "école", 1.0, Some(0.1));
        assert_distance("café", "cafe", 1.9, None);
        assert_distance("café", "cafe", 2.0, Some(2.0));
    }
// TSZ_INLINE_TEST_END 4df306fc92ad1742512f000d14099f5adae5968538817c5a2bfafa1d00488bec

// TSZ_INLINE_TEST_BEGIN 6f0ca9c841a36921e396f7d76e1e85159644933f3505656cf8a7e0fab8f181c1 1257 stable_value_declaration_resolves_to_class_node
    /// Resolving `Symbol::stable_value_declaration` for a class via the new
    /// `node_at_stable_location` helper must return the same class node
    /// that `Symbol::value_declaration` points at in the same binder.
    #[test]
    fn stable_value_declaration_resolves_to_class_node() {
        let source = "class Foo extends Bar {}\n".to_string();

        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let sym_id = binder.file_locals.get("Foo").expect("class symbol Foo");
        let symbol = binder.symbols.get(sym_id).expect("symbol data");
        let stable = symbol.stable_value_declaration;
        assert!(
            stable.is_known(),
            "class Foo must have a known stable_value_declaration span"
        );
        let legacy_node_idx = symbol.value_declaration;
        assert!(
            legacy_node_idx.is_some(),
            "class Foo must have a populated value_declaration (NodeIndex)"
        );

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
            .expect("node_at_stable_location must resolve the class span");

        assert_eq!(
            resolved_idx, legacy_node_idx,
            "StableLocation must rehydrate to the same NodeIndex as value_declaration"
        );
        let resolved_node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in arena");
        assert_eq!(resolved_node.pos, stable.pos);
        assert_eq!(resolved_node.end, stable.end);
    }
// TSZ_INLINE_TEST_END 6f0ca9c841a36921e396f7d76e1e85159644933f3505656cf8a7e0fab8f181c1

// TSZ_INLINE_TEST_BEGIN 6f235a9b44d115be1602df89313fd54f5603cfd43fbdf8c6f86c1e250e01ba70 1311 stable_location_round_trips_across_arena_reparse
    /// The load-bearing Phase 5 scenario: capture a `StableLocation` from
    /// one arena, drop it (simulated by a fresh parser), and re-resolve
    /// the same `(pos, end)` against a newly parsed arena. The
    /// rehydrated `NodeIndex` must point at a node with matching span.
    ///
    /// This proves `node_at_stable_location` does NOT depend on arena
    /// identity — only on the `(file_idx, pos, end)` triple.
    #[test]
    fn stable_location_round_trips_across_arena_reparse() {
        let source = "class Foo extends Bar {}\nclass Qux {}\n".to_string();

        // Capture a StableLocation for `Foo` from the first binder, then
        // let the first arena/binder go out of scope.
        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder.file_locals.get("Foo").expect("class symbol Foo");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            symbol.stable_value_declaration
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

        // The new binder's `value_declaration` NodeIndex should agree
        // with the helper's resolution — binder population is
        // deterministic for identical source text.
        let sym_id = binder
            .file_locals
            .get("Foo")
            .expect("class symbol Foo in reparsed binder");
        let new_symbol = binder
            .symbols
            .get(sym_id)
            .expect("symbol data in reparsed binder");
        assert_eq!(
            resolved_idx, new_symbol.value_declaration,
            "re-resolution must agree with the re-parsed binder's NodeIndex"
        );
    }
// TSZ_INLINE_TEST_END 6f235a9b44d115be1602df89313fd54f5603cfd43fbdf8c6f86c1e250e01ba70

// TSZ_INLINE_TEST_BEGIN d3733041c5a38eea8edd9b7fe9126a1c298408c597068efd2111126f9c8ce4b0 1377 stable_location_none_resolves_to_none
    /// `node_at_stable_location` must return `None` for the sentinel
    /// `StableLocation::NONE` (unknown span) so consumers can treat it as
    /// a clean "no declaration" signal.
    #[test]
    fn stable_location_none_resolves_to_none() {
        let arena = tsz_parser::parser::node::NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        let none = tsz_binder::symbols::StableLocation::NONE;
        assert!(ctx.node_at_stable_location(none).is_none());
    }
// TSZ_INLINE_TEST_END d3733041c5a38eea8edd9b7fe9126a1c298408c597068efd2111126f9c8ce4b0
