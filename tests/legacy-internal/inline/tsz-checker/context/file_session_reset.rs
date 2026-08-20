//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/file_session_reset.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 747807a4cfb32b15a3db787a9eef99a4c0147ba80a81af40a4f824f043917bc8 699 reset_clears_diagnostic_buffers_and_node_keyed_caches
    #[test]
    fn reset_clears_diagnostic_buffers_and_node_keyed_caches() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Populate via direct field access (we control the test).
        ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
            "test.ts".to_string(),
            0,
            1,
            "test".to_string(),
            0,
        ));
        ctx.diagnostic_indices.emitted.insert((0, 1));
        ctx.instantiation_depth.set(7);

        assert_eq!(ctx.diagnostics.len(), 1);
        assert_eq!(ctx.diagnostic_indices.emitted.len(), 1);
        assert_eq!(ctx.instantiation_depth.get(), 7);

        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert!(ctx.diagnostic_indices.emitted.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }
// TSZ_INLINE_TEST_END 747807a4cfb32b15a3db787a9eef99a4c0147ba80a81af40a4f824f043917bc8

// TSZ_INLINE_TEST_BEGIN e64d8a612050b390079207dc8f65ec182d32fb1c3e19f896d11487be24c91884 728 reset_clears_recovery_sites
    #[test]
    fn reset_clears_recovery_sites() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.recover_any(NodeIndex(42), RecoveryReason::YieldOutsideGenerator);
        assert_eq!(ctx.recovery_sites_snapshot().len(), 1);

        ctx.reset_for_next_file();

        assert!(ctx.recovery_sites_snapshot().is_empty());
    }
// TSZ_INLINE_TEST_END e64d8a612050b390079207dc8f65ec182d32fb1c3e19f896d11487be24c91884

// TSZ_INLINE_TEST_BEGIN 8d765c69162b8433c49dd34475fe25dd4c7e0985e8eba9af30ca60b2bbc26ea1 743 reset_clears_file_local_lookup_caches_and_flags
    #[test]
    fn reset_clears_file_local_lookup_caches_and_flags() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.types_extending_array.insert(tsz_solver::TypeId::NUMBER);
        ctx.abstract_constructor_types
            .insert(tsz_solver::TypeId::STRING);
        ctx.protected_constructor_types
            .insert(tsz_solver::TypeId::BOOLEAN);
        ctx.private_constructor_types
            .insert(tsz_solver::TypeId::UNKNOWN);
        ctx.namespace_member_resolution_cache
            .borrow_mut()
            .entry("React".to_string())
            .or_default()
            .insert("Component".to_string(), Some(tsz_binder::SymbolId(1)));
        ctx.export_equals_named_cache.borrow_mut().insert(
            (0, "pkg".to_string(), "Thing".to_string(), Vec::new()),
            Some(tsz_binder::SymbolId(2)),
        );
        ctx.nested_namespace_candidates_cache
            .borrow_mut()
            .insert("JSX".to_string(), vec![(0, tsz_binder::SymbolId(3))]);
        ctx.nested_namespace_candidates_cache_complete.set(true);
        ctx.reexport_resolution_cache
            .borrow_mut()
            .insert((0, "Thing".to_string()), Some((tsz_binder::SymbolId(4), 1)));
        ctx.jsdoc_global_typedef_lookup_cache
            .miss_cache
            .borrow_mut()
            .insert("Callback".to_string());
        ctx.jsdoc_global_typedef_lookup_cache
            .in_progress
            .borrow_mut()
            .insert("Callback".to_string());
        ctx.lib_heritage_in_progress
            .insert("HTMLElement".to_string());
        ctx.request_cache_counters.request_cache_hits = 1;
        ctx.flow_shared
            .narrowing_cache
            .resolve_cache
            .borrow_mut()
            .insert(tsz_solver::TypeId::STRING, tsz_solver::TypeId::NUMBER);
        ctx.in_satisfies_operand = true;

        ctx.reset_for_next_file();

        assert!(ctx.types_extending_array.is_empty());
        assert!(ctx.abstract_constructor_types.is_empty());
        assert!(ctx.protected_constructor_types.is_empty());
        assert!(ctx.private_constructor_types.is_empty());
        assert!(ctx.namespace_member_resolution_cache.borrow().is_empty());
        assert!(ctx.export_equals_named_cache.borrow().is_empty());
        assert!(ctx.nested_namespace_candidates_cache.borrow().is_empty());
        assert!(!ctx.nested_namespace_candidates_cache_complete.get());
        assert!(ctx.reexport_resolution_cache.borrow().is_empty());
        assert!(
            ctx.jsdoc_global_typedef_lookup_cache
                .miss_cache
                .borrow()
                .is_empty()
        );
        assert!(
            ctx.jsdoc_global_typedef_lookup_cache
                .in_progress
                .borrow()
                .is_empty()
        );
        assert!(ctx.lib_heritage_in_progress.is_empty());
        assert_eq!(ctx.request_cache_counters.request_cache_hits, 0);
        assert!(
            ctx.flow_shared
                .narrowing_cache
                .resolve_cache
                .borrow()
                .is_empty()
        );
        assert!(!ctx.in_satisfies_operand);
    }
// TSZ_INLINE_TEST_END 8d765c69162b8433c49dd34475fe25dd4c7e0985e8eba9af30ca60b2bbc26ea1

// TSZ_INLINE_TEST_BEGIN 52417e8eac4b920bebbef87f145d9b7f7867c3ee686e30bd3449370f6f21eadc 826 reset_clears_switch_literal_flow_caches
    #[test]
    fn reset_clears_switch_literal_flow_caches() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.flow_shared
            .flow_switch_case_literal_cache
            .borrow_mut()
            .insert(1, Some(tsz_solver::TypeId::STRING));
        ctx.flow_shared
            .flow_switch_all_distinct_literals_cache
            .borrow_mut()
            .insert(2, true);

        ctx.reset_for_next_file();

        assert!(
            ctx.flow_shared
                .flow_switch_case_literal_cache
                .borrow()
                .is_empty()
        );
        assert!(
            ctx.flow_shared
                .flow_switch_all_distinct_literals_cache
                .borrow()
                .is_empty()
        );
    }
// TSZ_INLINE_TEST_END 52417e8eac4b920bebbef87f145d9b7f7867c3ee686e30bd3449370f6f21eadc

// TSZ_INLINE_TEST_BEGIN 29b23547fd59360da815e7e36514a1fb80a20c02d9ff3924144e95185983b66d 858 reset_clears_lib_type_resolution_caches
    #[test]
    fn reset_clears_lib_type_resolution_caches() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.lib_type_resolution_caches
            .types
            .insert("ShadowedLib".to_string(), Some(tsz_solver::TypeId::STRING));
        ctx.lib_type_resolution_caches
            .lazy_members
            .borrow_mut()
            .insert(
                (tsz_common::interner::Atom(1), tsz_common::interner::Atom(2)),
                Some(tsz_solver::TypeId::NUMBER),
            );
        ctx.lib_type_resolution_caches
            .lazy_member_receiver_properties
            .borrow_mut()
            .insert(
                (tsz_solver::def::DefId(3), tsz_common::interner::Atom(4)),
                Some(tsz_solver::TypeId::BOOLEAN),
            );
        ctx.lib_type_resolution_caches
            .lazy_member_receivers
            .borrow_mut()
            .insert(tsz_solver::def::DefId(5), false);

        ctx.reset_for_next_file();

        assert!(ctx.lib_type_resolution_caches.types.is_empty());
        assert!(
            ctx.lib_type_resolution_caches
                .lazy_members
                .borrow()
                .is_empty()
        );
        assert!(
            ctx.lib_type_resolution_caches
                .lazy_member_receiver_properties
                .borrow()
                .is_empty()
        );
        assert!(
            ctx.lib_type_resolution_caches
                .lazy_member_receivers
                .borrow()
                .is_empty()
        );
    }
// TSZ_INLINE_TEST_END 29b23547fd59360da815e7e36514a1fb80a20c02d9ff3924144e95185983b66d

// TSZ_INLINE_TEST_BEGIN f27ae75e9595a0af431e16267e4515bb2dd8f2e66d2584625d56cff00e44bedd 910 reset_is_idempotent
    #[test]
    fn reset_is_idempotent() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.reset_for_next_file();
        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }
// TSZ_INLINE_TEST_END f27ae75e9595a0af431e16267e4515bb2dd8f2e66d2584625d56cff00e44bedd

// TSZ_INLINE_TEST_BEGIN 8635f450e0e91d4af25b1932c39a7aea285c4c98eb4fd233a9aa4a2ce51345e5 924 reset_clears_all_recursion_depth_counters
    #[test]
    fn reset_clears_all_recursion_depth_counters() {
        // The reset helper resets local and session-owned depth counters: four
        // `RefCell<DepthCounter>` (call/circ_ref/overlap/recursion) plus
        // checker/session `Cell` counters. The original "diagnostic
        // buffers" test only exercises `instantiation_depth`. This test
        // locks the semantics of the RefCell-backed counters,
        // including the sticky `exceeded` flag that a careless future
        // refactor (e.g. clearing only `depth` and forgetting `exceeded`)
        // would silently break — and a non-cleared `exceeded` would
        // suppress legitimate TS2589-style depth errors in the next
        // file checked on the reused context.
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Drive each counter past zero and set the sticky exceeded flag.
        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let mut d = depth_cell.borrow_mut();
            assert!(d.enter(), "enter should succeed under max_depth");
            assert!(d.enter(), "second enter should succeed");
            d.mark_exceeded();
            assert_eq!(d.depth(), 2);
            assert!(d.is_exceeded());
        }
        ctx.instantiation_depth.set(11);
        ctx.symbol_resolution_depth.set(12);
        let eval_session = std::rc::Rc::clone(&ctx.eval_session);
        let _eval_depth_entry = eval_session
            .enter_eval_env_depth()
            .expect("pre-reset env-eval depth entry should fit");
        let _app_symbol_depth_entry = eval_session.enter_app_symbol_resolution_depth();
        eval_session.increment_app_symbol_resolution_fuel();
        let _refs_scope = eval_session.enter_refs_resolution_scope();
        eval_session.increment_refs_resolution_fuel();
        let _type_ref_depth_entry = eval_session
            .enter_type_reference_resolution_depth()
            .expect("type-reference depth entry should fit");

        ctx.reset_for_next_file();

        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let d = depth_cell.borrow();
            assert_eq!(d.depth(), 0, "depth not cleared on reset");
            assert!(
                !d.is_exceeded(),
                "exceeded flag not cleared on reset — would silently \
                 suppress real depth errors in the next file",
            );
        }
        assert_eq!(ctx.instantiation_depth.get(), 0);
        assert_eq!(ctx.symbol_resolution_depth.get(), 0);
        assert_eq!(ctx.eval_session.eval_env_depth(), 0);
        assert_eq!(ctx.eval_session.app_symbol_resolution_depth(), 0);
        assert_eq!(ctx.eval_session.app_symbol_resolution_fuel(), 0);
        assert_eq!(ctx.eval_session.refs_resolution_fuel(), 0);
        assert_eq!(ctx.eval_session.type_reference_resolution_depth(), 0);
    }
// TSZ_INLINE_TEST_END 8635f450e0e91d4af25b1932c39a7aea285c4c98eb4fd233a9aa4a2ce51345e5

// TSZ_INLINE_TEST_BEGIN 713bf69fbcda2f49993c06698453f45ea1998e2cd0916a7a5baadc3a6bcdeaac 994 child_contexts_share_type_reference_resolution_depth
    #[test]
    fn child_contexts_share_type_reference_resolution_depth() {
        let parent_arena = NodeArena::default();
        let child_arena = NodeArena::default();
        let parent_binder = BinderState::new();
        let child_binder = BinderState::new();
        let types = TypeInterner::new();
        let parent = fresh_ctx(&parent_arena, &parent_binder, &types);

        let child = CheckerContext::with_parent_cache(
            &child_arena,
            &child_binder,
            &types,
            "child.ts".to_string(),
            CheckerOptions::default(),
            &parent,
        );

        let _entry = parent
            .eval_session
            .enter_type_reference_resolution_depth()
            .expect("type-reference depth entry should fit");
        assert_eq!(
            child.eval_session.type_reference_resolution_depth(),
            1,
            "cross-arena type-reference alias forwarding should share one depth counter"
        );
    }
// TSZ_INLINE_TEST_END 713bf69fbcda2f49993c06698453f45ea1998e2cd0916a7a5baadc3a6bcdeaac
