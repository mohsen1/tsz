//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/cross_file_query.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4c3fc80ea4200ab2b824c0243f259998a0be75dbf5e2d6e2daffcb833d11acf9 815 cross_file_cache_readers_reject_non_interned_type_ids
    #[test]
    fn cross_file_cache_readers_reject_non_interned_type_ids() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, Arc::clone(&store));
        let stale_type = TypeId(10_000);

        assert!(!crate::query_boundaries::common::type_id_is_known_to_db(
            &types, stale_type
        ));

        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::Symbol.as_storage_kind(),
            7,
            11,
            0,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::Interface.as_storage_kind(),
            7,
            12,
            0,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceMemberSimple.as_storage_kind(),
            7,
            21,
            22,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::ClassInstance.as_storage_kind(),
            7,
            13,
            0,
            0,
            stale_type,
            Vec::new(),
        );

        assert_eq!(ctx.cached_cross_file_symbol_type(SymbolId(11), 7), None);
        assert_eq!(ctx.cached_cross_file_interface_type(SymbolId(12), 7), None);
        assert_eq!(
            ctx.cached_cross_file_interface_member_simple_type(NodeIndex(21), NodeIndex(22), 7),
            None
        );
        assert_eq!(
            ctx.cached_cross_file_class_instance_type(SymbolId(13), 7),
            None
        );
    }
// TSZ_INLINE_TEST_END 4c3fc80ea4200ab2b824c0243f259998a0be75dbf5e2d6e2daffcb833d11acf9

// TSZ_INLINE_TEST_BEGIN 0f501a5a36463ae20691278c013f48882403c66cead4156eca7d96c44075e635 877 stable_source_file_symbol_type_cache_accepts_annotated_variable
    #[test]
    fn stable_source_file_symbol_type_cache_accepts_annotated_variable() {
        let (arena, binder, types, sym_id) = bound_symbol_context(
            "export const leaf1: { value: number } = { value: 1 };",
            "leaf1",
        );
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }
// TSZ_INLINE_TEST_END 0f501a5a36463ae20691278c013f48882403c66cead4156eca7d96c44075e635

// TSZ_INLINE_TEST_BEGIN 5950d028bccedffc4f05d883dbb04730e85d416422940cbec0ceb5dfb94a3d0b 893 stable_source_file_symbol_type_cache_accepts_type_alias
    #[test]
    fn stable_source_file_symbol_type_cache_accepts_type_alias() {
        let (arena, binder, types, sym_id) =
            bound_symbol_context("export type Leaf<T> = { value: T };", "Leaf");
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }
// TSZ_INLINE_TEST_END 5950d028bccedffc4f05d883dbb04730e85d416422940cbec0ceb5dfb94a3d0b

// TSZ_INLINE_TEST_BEGIN e979d0dc810e57599e5a379691922aa0edf57c9aebdb6938cbdde42dbbf79963 907 stable_source_file_symbol_type_cache_rejects_inferred_variable
    #[test]
    fn stable_source_file_symbol_type_cache_rejects_inferred_variable() {
        let (arena, binder, types, sym_id) =
            bound_symbol_context("export const leaf1 = { value: 1 };", "leaf1");
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(!ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }
// TSZ_INLINE_TEST_END e979d0dc810e57599e5a379691922aa0edf57c9aebdb6938cbdde42dbbf79963

// TSZ_INLINE_TEST_BEGIN 281cc6b7be6f908d4917c240bb538a612b5763030f4c18f6fd2cb920db138233 921 source_file_symbol_type_cache_keys_scope_and_requester
    #[test]
    fn source_file_symbol_type_cache_keys_scope_and_requester() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, store);
        let sym_id = SymbolId(11);
        let file_idx = 7;
        let scope = 0xCAFE_BABE_DEAD_BEEF;
        let requester_file_idx = 3;

        ctx.cache_cross_file_symbol_type(sym_id, file_idx, TypeId::NUMBER, Vec::new());
        ctx.cache_source_file_symbol_arena_type(
            sym_id,
            file_idx,
            scope,
            requester_file_idx,
            TypeId::STRING,
            Vec::new(),
        );

        assert_eq!(
            ctx.cached_cross_file_symbol_type(sym_id, file_idx)
                .map(|(type_id, _)| type_id),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(sym_id, file_idx, scope, requester_file_idx)
                .map(|(type_id, _)| type_id),
            Some(TypeId::STRING)
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(
                sym_id,
                file_idx,
                scope,
                requester_file_idx + 1
            ),
            None
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(
                sym_id,
                file_idx,
                scope + 1,
                requester_file_idx
            ),
            None
        );
    }
// TSZ_INLINE_TEST_END 281cc6b7be6f908d4917c240bb538a612b5763030f4c18f6fd2cb920db138233

// TSZ_INLINE_TEST_BEGIN b355bff77ac7b947bb4e3b07bf4b366d3bb09e98eec372d2f61150d5d30f6030 973 stable_source_file_symbol_type_cache_key_uses_scope_without_requester
    #[test]
    fn stable_source_file_symbol_type_cache_key_uses_scope_without_requester() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, store);
        let sym_id = SymbolId(11);
        let file_idx = 7;
        let scope = 0xCAFE_BABE_DEAD_BEEF;

        ctx.cache_stable_source_file_symbol_arena_type(
            sym_id,
            file_idx,
            scope,
            TypeId::STRING,
            Vec::new(),
        );

        assert_eq!(
            ctx.cached_stable_source_file_symbol_arena_type(sym_id, file_idx, scope)
                .map(|(type_id, _)| type_id),
            Some(TypeId::STRING)
        );
        assert_eq!(
            ctx.cached_stable_source_file_symbol_arena_type(sym_id, file_idx, scope + 1),
            None
        );
        assert_eq!(
            ctx.cached_cross_file_symbol_type(sym_id, file_idx)
                .map(|(type_id, _)| type_id),
            None
        );
    }
// TSZ_INLINE_TEST_END b355bff77ac7b947bb4e3b07bf4b366d3bb09e98eec372d2f61150d5d30f6030
