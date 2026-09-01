//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b0a948cc16be2da27dc9ac1b9864318327662200c7c99442786a331101a6e9e2 1571 type_cache_merge_keeps_constructor_type_cache
    #[test]
    fn type_cache_merge_keeps_constructor_type_cache() {
        let mut lhs = empty_cache();
        let rhs = empty_cache();

        rhs.class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(42), TypeId::STRING);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_constructor_type_cache
                .borrow()
                .get(&NodeIndex(42)),
            Some(&TypeId::STRING)
        );
    }
// TSZ_INLINE_TEST_END b0a948cc16be2da27dc9ac1b9864318327662200c7c99442786a331101a6e9e2

// TSZ_INLINE_TEST_BEGIN fadd645a7d273ed9caa2e033700fbc715854987bb6fe867fb43efb2817ca6556 1590 type_cache_merge_keeps_error_class_type_cache_entries
    #[test]
    fn type_cache_merge_keeps_error_class_type_cache_entries() {
        let mut lhs = empty_cache();
        let rhs = empty_cache();

        rhs.class_instance_type_cache
            .borrow_mut()
            .insert(NodeIndex(10), TypeId::ERROR);
        rhs.class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(11), TypeId::ERROR);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_instance_type_cache.borrow().get(&NodeIndex(10)),
            Some(&TypeId::ERROR)
        );
        assert_eq!(
            lhs.class_constructor_type_cache
                .borrow()
                .get(&NodeIndex(11)),
            Some(&TypeId::ERROR)
        );
    }
// TSZ_INLINE_TEST_END fadd645a7d273ed9caa2e033700fbc715854987bb6fe867fb43efb2817ca6556

// TSZ_INLINE_TEST_BEGIN bc2ca2d77de0a5642086c51d2b9db26a24aeb518e0323e3d1266f27329ab4124 1616 invalidate_symbols_clears_class_type_caches
    #[test]
    fn invalidate_symbols_clears_class_type_caches() {
        let mut cache = empty_cache();
        let sym = SymbolId(7);
        cache
            .symbol_dependencies
            .insert(sym, FxHashSet::<SymbolId>::default());
        cache
            .class_instance_type_cache
            .borrow_mut()
            .insert(NodeIndex(1), TypeId::NUMBER);
        cache
            .class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(2), TypeId::STRING);
        cache
            .class_instance_type_to_decl
            .insert(TypeId::BOOLEAN, NodeIndex(3));

        let affected = cache.invalidate_symbols(&[sym]);

        assert_eq!(affected, 1);
        assert!(cache.class_instance_type_cache.borrow().is_empty());
        assert!(cache.class_constructor_type_cache.borrow().is_empty());
        assert!(cache.class_instance_type_to_decl.is_empty());
    }
// TSZ_INLINE_TEST_END bc2ca2d77de0a5642086c51d2b9db26a24aeb518e0323e3d1266f27329ab4124

// TSZ_INLINE_TEST_BEGIN 467f9636dbbcdab40be741224ab235537e8f2d8aeb4bea7d4da400af0fe60839 1643 extract_cache_keeps_definition_names_without_symbol_mapping
    #[test]
    fn extract_cache_keeps_definition_names_without_symbol_mapping() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let name = types.intern_string("ConcatArray");
        let def_id = store.register(DefinitionInfo::interface(name, Vec::new(), Vec::new()));

        let ctx = CheckerContext::new_with_shared_def_store(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );

        let cache = ctx.extract_cache();

        assert_eq!(
            cache.def_to_name.get(&def_id).map(String::as_str),
            Some("ConcatArray")
        );
    }
// TSZ_INLINE_TEST_END 467f9636dbbcdab40be741224ab235537e8f2d8aeb4bea7d4da400af0fe60839

// TSZ_INLINE_TEST_BEGIN f6b4a4103dc8bba61708379f95c99b7fc4ae1e4727aaa950763d0889fddb116a 1669 lib_name_possible_gates_on_index_membership
    #[test]
    fn lib_name_possible_gates_on_index_membership() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let mut ctx = CheckerContext::new_with_shared_def_store(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );

        // No index: every name forces the full scan (behavior unchanged).
        assert!(ctx.lib_name_possible("Anything"));
        assert!(ctx.lib_name_possible("HTMLDivElement"));

        // With an index, only names present in it may be in a lib `file_locals`.
        // A name absent from the index cannot match any `file_locals.get(name)`,
        // so the scan is safely skippable (`lib_name_possible == false`).
        let mut names = FxHashSet::default();
        names.insert("HTMLDivElement".to_string());
        names.insert("Array".to_string());
        ctx.set_lib_file_local_names(Some(Arc::new(names)));

        assert!(ctx.lib_name_possible("HTMLDivElement"));
        assert!(ctx.lib_name_possible("Array"));
        assert!(!ctx.lib_name_possible("MyProjectUtility"));
        assert!(!ctx.lib_name_possible("BuildTuple"));
    }
// TSZ_INLINE_TEST_END f6b4a4103dc8bba61708379f95c99b7fc4ae1e4727aaa950763d0889fddb116a

// TSZ_INLINE_TEST_BEGIN 458ba6a88dc5a8e48e2e35f6eba820b221d23ef164c5334d1e002e2f4c333191 1702 type_cache_merge_dedupes_boxed_def_ids
    #[test]
    fn type_cache_merge_dedupes_boxed_def_ids() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();
        let def_id = tsz_solver::DefId(42);

        lhs.boxed_def_ids
            .insert(tsz_solver::IntrinsicKind::Function, vec![def_id]);
        rhs.boxed_def_ids
            .insert(tsz_solver::IntrinsicKind::Function, vec![def_id]);

        lhs.merge(rhs);

        assert_eq!(
            lhs.boxed_def_ids
                .get(&tsz_solver::IntrinsicKind::Function)
                .map(Vec::as_slice),
            Some(&[def_id][..])
        );
    }
// TSZ_INLINE_TEST_END 458ba6a88dc5a8e48e2e35f6eba820b221d23ef164c5334d1e002e2f4c333191
