#[test]
fn direct_actual_lib_symbol_type_handles_plain_iterator_object_with_params() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts"]);
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

    let iterator_object_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("IteratorObject")
        .expect("IteratorObject should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&iterator_object_sym_id)
        .map(std::convert::AsRef::as_ref);
    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            iterator_object_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("unaugmented IteratorObject should lower through the direct lib path");

    assert_ne!(
        ty,
        TypeId::UNKNOWN,
        "IteratorObject should not lower to UNKNOWN"
    );
    assert_ne!(
        ty,
        TypeId::ERROR,
        "IteratorObject should not lower to ERROR"
    );
    assert!(
        !params.is_empty(),
        "IteratorObject should preserve generic type parameters",
    );
}

#[test]
fn direct_actual_lib_symbol_type_allows_iterator_without_declaration_arena_proof() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts", "esnext.iterator.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let iterator_sym_id = binder
        .file_locals
        .get("Iterator")
        .expect("Iterator should resolve to a lib symbol");
    let iterator_decls = binder
        .get_symbol(iterator_sym_id)
        .expect("Iterator symbol should exist")
        .declarations
        .clone();
    let declaration_arenas = std::sync::Arc::make_mut(&mut binder.declaration_arenas);
    for decl_idx in iterator_decls {
        declaration_arenas.remove(&(iterator_sym_id, decl_idx));
    }

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

    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&iterator_sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            iterator_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Iterator should still lower through the direct lib path");

    assert_ne!(ty, TypeId::UNKNOWN, "Iterator should not lower to UNKNOWN");
    assert_ne!(ty, TypeId::ERROR, "Iterator should not lower to ERROR");
    assert!(
        !params.is_empty(),
        "Iterator should preserve generic type parameters",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_non_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts", "decorators.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("DecoratorMetadataObject")
        .expect("DecoratorMetadataObject should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("non-generic actual-lib alias body should lower directly");

    assert!(
        params.is_empty(),
        "DecoratorMetadataObject should be non-generic",
    );
    assert!(
        crate::query_boundaries::common::lazy_def_id(&types, ty).is_none(),
        "direct alias result should return the registered alias body, not the opaque Lazy alias",
    );

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(cached_ty, ty);
    assert!(cached_params.is_empty());
}

#[test]
fn direct_actual_lib_symbol_type_handles_property_key_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PropertyKey")
        .expect("PropertyKey should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);
    let symbol = state
        .get_cross_file_symbol(sym_id)
        .expect("PropertyKey symbol should be available")
        .clone();

    let proof = state
        .direct_actual_lib_type_alias_body(
            sym_id,
            &symbol,
            "PropertyKey",
            delegate_arena.expect("PropertyKey should have a delegate arena"),
        )
        .expect("PropertyKey should have a proven actual-lib alias body");
    assert_eq!(proof.outcome, DirectActualLibAliasBodyOutcome::Success);
    assert!(proof.type_params.is_empty(), "PropertyKey is non-generic",);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("PropertyKey should lower through the direct alias body path");
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "PropertyKey should remain non-generic");
}

#[test]
fn direct_actual_lib_symbol_type_handles_record_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
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

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Record")
        .expect("Record should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Record should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 2, "Record should expose K and T");
}

#[test]
fn direct_actual_lib_symbol_type_handles_intl_non_generic_alias_bodies() {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2018.intl.d.ts",
        "es2020.intl.d.ts",
        "es2023.intl.d.ts",
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

    for name in [
        "LocalesArgument",
        "NumberFormatOptionsCurrencyDisplay",
        "NumberFormatOptionsSignDisplay",
        "NumberFormatOptionsStyle",
        "NumberFormatOptionsUseGrouping",
        "NumberFormatPartTypes",
        "NumberFormatRangePartTypes",
        "UnicodeBCP47LocaleIdentifier",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .or_else(|| state.resolve_lib_namespace_export_symbol("Intl", name))
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(
            proof.outcome,
            DirectActualLibAliasBodyOutcome::Success,
            "{name} should be admitted in the direct alias allowlist",
        );
        assert!(
            proof.type_params.is_empty(),
            "{name} should remain non-generic",
        );

        let (direct_ty, direct_params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(delegate_arena),
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through direct alias path"));
        assert_ne!(
            direct_ty,
            TypeId::UNKNOWN,
            "{name} should not lower to UNKNOWN"
        );
        assert_ne!(direct_ty, TypeId::ERROR, "{name} should not lower to ERROR");
        assert!(
            direct_params.is_empty(),
            "{name} should stay non-generic on direct path",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            direct_ty, fallback_body,
            "{name} direct alias body must match child-checker fallback body",
        );
        assert!(
            fallback_params.is_empty(),
            "{name} fallback should remain non-generic",
        );
    }
}
