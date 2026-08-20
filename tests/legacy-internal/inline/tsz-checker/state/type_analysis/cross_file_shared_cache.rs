//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/cross_file_shared_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1adf6f8b4e7ffa37b091ba8a510600dac628acdc903c9cd01bdd0e0726321acc 255 shared_actual_lib_delegation_hit_populates_file_local_symbol_cache
    #[test]
    fn shared_actual_lib_delegation_hit_populates_file_local_symbol_cache() {
        let lib_files = load_lib_files(&[
            "es2015.iterable.d.ts",
            "es2020.symbol.wellknown.d.ts",
            "es2025.iterator.d.ts",
        ]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        state.ctx.set_lib_contexts(lib_contexts);
        state.ctx.set_actual_lib_file_count(lib_files.len());

        let array_iterator_type = state
            .resolve_lib_type_by_name("ArrayIterator")
            .expect("ArrayIterator should resolve through lib contexts");
        let shared = Arc::new(dashmap::DashMap::new());
        shared.insert(
            shared_actual_lib_delegation_cache_key("ArrayIterator"),
            Some(array_iterator_type),
        );
        state.ctx.shared_lib_type_cache = Some(shared);

        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("ArrayIterator")
            .expect("ArrayIterator should resolve to a lib symbol");
        let (cached_type, params) = state
            .cached_shared_actual_lib_delegation(sym_id, "ArrayIterator")
            .expect("shared actual-lib cache should return known TypeIds");

        assert_eq!(cached_type, array_iterator_type);
        assert!(
            !params.is_empty(),
            "shared actual-lib cache hits must preserve generic metadata"
        );
        assert_eq!(
            state.ctx.symbol_types.get(&sym_id),
            Some(array_iterator_type)
        );
        assert!(
            state.ctx.lib_delegation_cache.contains_symbol_type(sym_id),
            "shared hits should warm the file-local delegation cache"
        );
    }
// TSZ_INLINE_TEST_END 1adf6f8b4e7ffa37b091ba8a510600dac628acdc903c9cd01bdd0e0726321acc

// TSZ_INLINE_TEST_BEGIN 8f1cc66b8fc08ea0a3af98f21b6957e160c66886c40febf675ce5dbfc5e4e72a 322 shared_actual_lib_delegation_name_accepts_dom_builtin_libs
    #[test]
    fn shared_actual_lib_delegation_name_accepts_dom_builtin_libs() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLElement")
            .expect("HTMLElement should resolve to a DOM lib symbol");

        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(dom_arena), false),
            Some("HTMLElement".to_string())
        );
        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(dom_arena), true),
            None,
            "requests that still need cross-file delegation must not use the shared name cache",
        );
    }
// TSZ_INLINE_TEST_END 8f1cc66b8fc08ea0a3af98f21b6957e160c66886c40febf675ce5dbfc5e4e72a

// TSZ_INLINE_TEST_BEGIN 270576b60fb1e49807834fbfe7b06871b53bde5023c59b1309170b08ff6729c9 360 shared_actual_lib_delegation_name_rejects_external_package_declarations
    #[test]
    fn shared_actual_lib_delegation_name_rejects_external_package_declarations() {
        let mut parser = ParserState::new(
            "node_modules/pkg/index.d.ts".to_string(),
            "export interface ExternalFixture { value: string; }".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "node_modules/pkg/index.d.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("ExternalFixture")
            .expect("external package interface symbol");

        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(arena.as_ref()), false),
            None,
            "only built-in TypeScript libs may use the shared actual-lib name cache",
        );
    }
// TSZ_INLINE_TEST_END 270576b60fb1e49807834fbfe7b06871b53bde5023c59b1309170b08ff6729c9

// TSZ_INLINE_TEST_BEGIN 512be7a11580cd846d5ab9793f7652fe7642d7cfb535258c354c0bc297df344d 394 shared_actual_lib_value_declaration_cache_roundtrip_warms_local_cache
    #[test]
    fn shared_actual_lib_value_declaration_cache_roundtrip_warms_local_cache() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        state
            .ctx
            .set_all_binders(Arc::new(vec![Arc::clone(&binder)]));
        let shared: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());
        state.ctx.shared_lib_type_cache = Some(shared);

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");
        let decl_idx = state
            .get_cross_file_symbol(sym_id)
            .expect("Window symbol should be visible")
            .value_declaration;
        let shared_name = state
            .shared_actual_lib_value_declaration_name(sym_id, dom_arena)
            .expect("DOM value declarations should be shared-cache eligible");

        assert!(
            state
                .cached_shared_actual_lib_value_declaration(&shared_name, dom_arena, decl_idx, 1)
                .is_none(),
            "cache should be empty before the first write"
        );

        state.cache_shared_actual_lib_value_declaration(
            &shared_name,
            dom_arena,
            decl_idx,
            1,
            tsz_solver::TypeId::STRING,
        );

        assert_eq!(
            state.cached_shared_actual_lib_value_declaration(&shared_name, dom_arena, decl_idx, 1),
            Some(tsz_solver::TypeId::STRING)
        );
        assert_eq!(
            state
                .ctx
                .lib_delegation_cache
                .declaration_node_type(dom_arena, decl_idx, 1),
            Some(tsz_solver::TypeId::STRING),
            "shared hits should warm the file-local declaration-node cache"
        );
    }
// TSZ_INLINE_TEST_END 512be7a11580cd846d5ab9793f7652fe7642d7cfb535258c354c0bc297df344d

// TSZ_INLINE_TEST_BEGIN 2c1ccc66c7ea66b2d1ae53b02d3d70c44fc931cc9bc9151d02cbf1ffc2cca711 463 shared_actual_lib_value_declaration_name_rejects_program_augmentations
    #[test]
    fn shared_actual_lib_value_declaration_name_rejects_program_augmentations() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);

        let mut augment_parser = ParserState::new(
            "augment.ts".to_string(),
            "export {}; declare global { interface Window { tszAugment: string; } }".to_string(),
        );
        let augment_root = augment_parser.parse_source_file();
        let mut augment_binder = BinderState::new();
        augment_binder.bind_source_file(augment_parser.get_arena(), augment_root);
        let augment_binder = Arc::new(augment_binder);

        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        state
            .ctx
            .set_all_binders(Arc::new(vec![Arc::clone(&binder), augment_binder]));

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");

        assert_eq!(
            state.shared_actual_lib_value_declaration_name(sym_id, dom_arena),
            None,
            "program-wide global augmentations must keep value declarations out of the shared cache",
        );
    }
// TSZ_INLINE_TEST_END 2c1ccc66c7ea66b2d1ae53b02d3d70c44fc931cc9bc9151d02cbf1ffc2cca711

// TSZ_INLINE_TEST_BEGIN b1a9e29bcea80229ed8d5d8fadcaf7f56e105ab73ee31a44cbebaf82d84f17c1 510 shared_actual_lib_class_delegation_name_accepts_scripthost_class
    #[test]
    fn shared_actual_lib_class_delegation_name_accepts_scripthost_class() {
        // `scripthost.d.ts` uses `declare class SafeArray<T>` – one of the few
        // builtin lib files that actually uses the `class` keyword, so its
        // symbols carry `symbol_flags::CLASS` from the binder.
        let lib_files = load_lib_files(&["scripthost.d.ts"]);
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "let s: SafeArray<number>;".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };

        let scripthost_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("SafeArray")
            .expect("SafeArray should be in lib symbol table from scripthost.d.ts");

        assert!(
            state
                .shared_actual_lib_class_delegation_name(sym_id, Some(scripthost_arena), false)
                .is_some(),
            "CLASS symbols in builtin lib arenas should produce a cache name"
        );
        assert_eq!(
            state.shared_actual_lib_class_delegation_name(sym_id, Some(scripthost_arena), true),
            None,
            "needs_cross_file_delegation=true must skip the lib class cache"
        );
    }
// TSZ_INLINE_TEST_END b1a9e29bcea80229ed8d5d8fadcaf7f56e105ab73ee31a44cbebaf82d84f17c1

// TSZ_INLINE_TEST_BEGIN 07d9e2cc378fb0ae1708e479085fb94e8594bcddf6e2f14e7e14bba046f6e777 556 shared_actual_lib_class_delegation_cache_roundtrip
    #[test]
    fn shared_actual_lib_class_delegation_cache_roundtrip() {
        let lib_files = load_lib_files(&["scripthost.d.ts"]);
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "let s: SafeArray<number>;".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        state.ctx.set_lib_contexts(lib_contexts);
        state.ctx.set_actual_lib_file_count(lib_files.len());

        let shared: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());
        state.ctx.shared_lib_type_cache = Some(shared.clone());

        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("SafeArray")
            .expect("SafeArray should be in lib symbol table");

        // Cache miss before write
        assert!(
            state
                .cached_shared_actual_lib_class_delegation(sym_id, "SafeArray")
                .is_none(),
            "cache should be empty initially"
        );

        // Write a sentinel type (raw id; well above the built-in reservation range)
        let sentinel = tsz_solver::TypeId(9000);
        state.cache_shared_actual_lib_class_delegation("SafeArray", sentinel);

        // Verify DashMap has the entry with the correct key prefix
        assert!(
            shared.contains_key("\0actual-lib-class-delegation:SafeArray"),
            "cache write must use the expected key prefix"
        );

        // A second write is a no-op (or_insert semantics — first writer wins)
        let sentinel2 = tsz_solver::TypeId(9001);
        state.cache_shared_actual_lib_class_delegation("SafeArray", sentinel2);
        let stored = shared
            .get("\0actual-lib-class-delegation:SafeArray")
            .expect("entry must exist after write");
        assert_eq!(
            *stored,
            Some(sentinel),
            "first writer wins; second write must not overwrite"
        );
    }
// TSZ_INLINE_TEST_END 07d9e2cc378fb0ae1708e479085fb94e8594bcddf6e2f14e7e14bba046f6e777

// TSZ_INLINE_TEST_BEGIN 6d7156ed950f0cccc20020cac932762633c955e0a8cfb72ab84e03ffce545dfa 629 shared_actual_lib_class_delegation_name_rejects_non_class
    #[test]
    fn shared_actual_lib_class_delegation_name_rejects_non_class() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };
        let dom_arena = lib_files[0].arena.as_ref();

        // `Window` in DOM is an interface, not a class — should not enter
        // the class-instance shared cache.
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");
        assert_eq!(
            state.shared_actual_lib_class_delegation_name(sym_id, Some(dom_arena), false),
            None,
            "interface symbols must not match the CLASS-only lib class cache"
        );
    }
// TSZ_INLINE_TEST_END 6d7156ed950f0cccc20020cac932762633c955e0a8cfb72ab84e03ffce545dfa
