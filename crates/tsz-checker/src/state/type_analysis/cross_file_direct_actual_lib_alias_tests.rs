use super::*;

#[test]
fn direct_actual_lib_symbol_type_handles_readonly_generic_alias_body_query() {
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
        .get("Readonly")
        .expect("Readonly should resolve to a lib symbol");
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
        .expect("Readonly should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "Readonly should expose T");

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(cached_ty, ty);
    assert_eq!(
        cached_params.len(),
        params.len(),
        "cache hits must preserve generic alias metadata",
    );

    let (cached_result_ty, cached_result_params) = state
        .delegate_cross_arena_symbol_resolution(sym_id)
        .expect("Readonly cache hit should resolve through lib delegation cache");
    assert_eq!(cached_result_ty, ty);
    assert_eq!(
        cached_result_params.len(),
        params.len(),
        "Readonly cache hits must return the cached alias type params",
    );
}

#[test]
fn direct_actual_lib_alias_proof_matches_mapped_utility_fallback_bodies() {
    let lib_files = load_lib_files(&["es5.d.ts", "es2015.iterable.d.ts", "es2019.array.d.ts"]);
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

    for (name, expected_param_count, expected_outcome) in [
        ("Capitalize", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Exclude", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Extract", 2, DirectActualLibAliasBodyOutcome::Success),
        ("FlatArray", 2, DirectActualLibAliasBodyOutcome::Success),
        (
            "IteratorResult",
            2,
            DirectActualLibAliasBodyOutcome::Success,
        ),
        ("Lowercase", 1, DirectActualLibAliasBodyOutcome::Success),
        ("NonNullable", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Omit", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Partial", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Pick", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Record", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Readonly", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Required", 1, DirectActualLibAliasBodyOutcome::Success),
        ("ReturnType", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Uncapitalize", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Uppercase", 1, DirectActualLibAliasBodyOutcome::Success),
        ("WeakKey", 0, DirectActualLibAliasBodyOutcome::Success),
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
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
        assert_eq!(proof.outcome, expected_outcome);
        assert_eq!(
            proof.type_params.len(),
            expected_param_count,
            "{name} should expose its declared type params",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            fallback_body, proof.body,
            "{name} proof must match the existing child-checker fallback body",
        );
        assert_eq!(
            fallback_params.len(),
            proof.type_params.len(),
            "{name} proof must preserve the same type-parameter arity as fallback",
        );
    }
}
