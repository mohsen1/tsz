use super::*;
use crate::context::{CheckerContext, CheckerOptions};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeParamInfo;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefinitionStore;

fn cache_test_state<'a>(
    arena: &'a NodeArena,
    binder: &'a BinderState,
    types: &'a TypeInterner,
) -> CheckerState<'a> {
    let mut ctx = CheckerContext::new_with_shared_def_store(
        arena,
        binder,
        types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.share_owner_symbol_type_results = true;
    ctx.current_file_idx = 3;
    CheckerState { ctx }
}

#[test]
fn generic_source_file_symbol_arena_results_use_stable_cache() {
    let arena = NodeArena::default();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let state = cache_test_state(&arena, &binder, &types);
    let sym_id = SymbolId(11);
    let file_idx = 7;
    let scope = 0xCAFE_BABE_DEAD_BEEF;
    let params = vec![TypeParamInfo {
        name: types.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::TypeParamOrigin::User,
    }];

    state.cache_symbol_arena_or_cross_file_symbol_type(
        sym_id,
        file_idx,
        scope,
        true,
        TypeId::STRING,
        params,
    );

    assert_eq!(
        state
            .ctx
            .cached_stable_source_file_symbol_arena_type(sym_id, file_idx as u32, scope)
            .map(|(type_id, params)| (type_id, params.len())),
        Some((TypeId::STRING, 1)),
        "source-file symbols already passed the structural stability gate",
    );
    assert_eq!(
        state.ctx.cached_source_file_symbol_arena_type(
            sym_id,
            file_idx as u32,
            scope,
            state.ctx.current_file_idx as u32,
        ),
        None,
    );
    assert_eq!(
        state
            .cached_symbol_arena_or_cross_file_symbol_type(sym_id, file_idx, scope, true)
            .map(|(type_id, params)| (type_id, params.len())),
        Some((TypeId::STRING, 1)),
    );
}

#[test]
fn non_generic_source_file_symbol_arena_results_stay_stable() {
    let arena = NodeArena::default();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let state = cache_test_state(&arena, &binder, &types);
    let sym_id = SymbolId(12);
    let file_idx = 8;
    let scope = 0xDEAD_BEEF_CAFE_BABE;

    state.cache_symbol_arena_or_cross_file_symbol_type(
        sym_id,
        file_idx,
        scope,
        true,
        TypeId::NUMBER,
        Vec::new(),
    );

    assert_eq!(
        state
            .ctx
            .cached_stable_source_file_symbol_arena_type(sym_id, file_idx as u32, scope)
            .map(|(type_id, params)| (type_id, params.len())),
        Some((TypeId::NUMBER, 0)),
    );
    assert_eq!(
        state.ctx.cached_source_file_symbol_arena_type(
            sym_id,
            file_idx as u32,
            scope,
            state.ctx.current_file_idx as u32,
        ),
        None,
    );
}

#[test]
fn declaration_symbol_arena_delegation_does_not_need_parent_symbol_targets() {
    let mut declaration_parser = ParserState::new(
        "lib.dom.d.ts".to_string(),
        "interface Declared { value: string }".to_string(),
    );
    declaration_parser.parse_source_file();
    let declaration_arena = declaration_parser.get_arena();

    let mut source_parser = ParserState::new(
        "source.ts".to_string(),
        "interface Local { value: string }".to_string(),
    );
    source_parser.parse_source_file();
    let source_arena = source_parser.get_arena();

    assert!(
        !super::super::cross_file_overlay_gate::symbol_delegation_needs_parent_targets(
            CrossArenaSymbolMissSource::SymbolArena,
            declaration_arena,
            false,
        ),
        "declaration-file symbol arena children already inherit shared cross-file indices",
    );
    assert!(
        super::super::cross_file_overlay_gate::symbol_delegation_needs_parent_targets(
            CrossArenaSymbolMissSource::SymbolArena,
            source_arena,
            false,
        ),
        "source-file symbol arena children still need parent overlay discoveries",
    );
    assert!(
        super::super::cross_file_overlay_gate::symbol_delegation_needs_parent_targets(
            CrossArenaSymbolMissSource::DeclarationArena,
            declaration_arena,
            false,
        ),
        "non-symbol-arena delegation keeps the existing overlay behavior",
    );
    assert!(
        super::super::cross_file_overlay_gate::symbol_delegation_needs_parent_targets(
            CrossArenaSymbolMissSource::SymbolArena,
            declaration_arena,
            true,
        ),
        "explicit symbol-file target delegation keeps the existing overlay behavior",
    );
}
