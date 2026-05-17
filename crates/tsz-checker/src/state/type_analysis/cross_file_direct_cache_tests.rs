use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;
use tsz_solver::def::DefinitionStore;

fn parse_bound_source_with_name(
    file_name: &str,
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

#[test]
fn delegate_source_file_type_alias_caches_generic_params() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                export type Leaf<T> = { value: T };
            "#,
    );
    let (requester_arena, mut requester_binder, _) = parse_bound_source_with_name(
        "requester.ts",
        "// synthetic requester with no same-id local symbol",
    );
    let leaf_sym = target_binder.file_locals.get("Leaf").expect("Leaf symbol");
    let leaf_decl = target_binder
        .get_symbol(leaf_sym)
        .expect("Leaf symbol data")
        .declarations[0];
    {
        let requester_binder = Arc::make_mut(&mut requester_binder);
        Arc::make_mut(&mut requester_binder.symbol_arenas)
            .insert(leaf_sym, Arc::clone(&target_arena));
        Arc::make_mut(&mut requester_binder.declaration_arenas)
            .entry((leaf_sym, leaf_decl))
            .or_default()
            .push(Arc::clone(&target_arena));
    }

    let mut ctx = CheckerContext::new_with_shared_def_store(
        requester_arena.as_ref(),
        requester_binder.as_ref(),
        &types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
        Arc::new(DefinitionStore::new()),
    );
    ctx.share_owner_symbol_type_results = true;
    ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&requester_arena),
        Arc::clone(&target_arena),
    ]));
    ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&target_binder),
    ]));
    let mut state = CheckerState { ctx };
    let scope = state.ctx.source_file_symbol_type_cache_scope();
    let target_file_idx = state
        .ctx
        .get_file_idx_for_arena(target_arena.as_ref())
        .expect("target arena should be indexed") as u32;

    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(leaf_sym)
        .expect("source-file generic alias should delegate through the target arena");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(
        params.len(),
        1,
        "Leaf<T> should preserve one type parameter"
    );
    assert_eq!(
        state
            .ctx
            .cached_stable_source_file_symbol_arena_type(leaf_sym, target_file_idx, scope),
        Some((ty, params)),
        "stable source-file symbol-arena cache hits must preserve generic params",
    );
}
