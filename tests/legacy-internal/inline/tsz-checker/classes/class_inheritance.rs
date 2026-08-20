//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/classes/class_inheritance.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ff2611f0d0419f8a9e8a989c1e7b580e0d3daea83195f4df286779dd08755f98 571 declared_parent_fallback_detects_cycle_without_registered_graph_edges
    #[test]
    fn declared_parent_fallback_detects_cycle_without_registered_graph_edges() {
        let source = r#"
class C extends E {}
class D extends C {}
class E extends D {}
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
            Default::default(),
        );

        let c_sym = checker
            .ctx
            .binder
            .file_locals
            .get("C")
            .expect("class C symbol should exist");
        let cycle_checker = ClassInheritanceChecker::new(&mut checker.ctx);
        let parents = cycle_checker.get_parents_for_cycle_search(c_sym);

        assert_eq!(
            parents.len(),
            1,
            "C should have exactly one declared parent"
        );
        let parent_name = cycle_checker
            .ctx
            .binder
            .get_symbol(parents[0])
            .map(|s| s.escaped_name.clone())
            .unwrap_or_default();
        assert_eq!(parent_name, "E");
        assert!(
            cycle_checker.detects_cycle_dfs(c_sym, &parents),
            "fallback parent traversal should detect C -> E -> D -> C cycle without pre-registered edges"
        );
    }
// TSZ_INLINE_TEST_END ff2611f0d0419f8a9e8a989c1e7b580e0d3daea83195f4df286779dd08755f98
