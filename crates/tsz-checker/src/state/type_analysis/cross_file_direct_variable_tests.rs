use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

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

fn interface_decl_and_members(
    binder: &BinderState,
    arena: &tsz_parser::parser::node::NodeArena,
    name: &str,
) -> (SymbolId, NodeIndex, Vec<NodeIndex>) {
    let sym_id = binder
        .file_locals
        .get(name)
        .expect("fixture interface symbol");
    let symbol = binder
        .get_symbol(sym_id)
        .expect("fixture interface symbol data");
    let decl_idx = symbol.declarations[0];
    let members = arena
        .get(decl_idx)
        .and_then(|node| arena.get_interface(node))
        .expect("fixture interface declaration")
        .members
        .nodes
        .clone();

    (sym_id, decl_idx, members)
}

#[test]
fn direct_builtin_lib_variable_annotation_accepts_non_generic_interfaces() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "lib.dom.d.ts",
        r#"
                interface DocumentFixture { readyState: string; }
                interface NavigatorFixture { userAgent: string; }
                declare var documentFixture: DocumentFixture;
                declare var navigatorFixture: NavigatorFixture;
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };

    for name in ["documentFixture", "navigatorFixture"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .expect("fixture variable symbol");
        let result = state
            .direct_builtin_lib_variable_annotation_type(sym_id, binder.as_ref(), arena.as_ref())
            .expect("simple builtin lib interface annotation should stay lazy");

        assert!(
            crate::query_boundaries::common::is_lazy_type(&types, result),
            "{name} should preserve the annotated interface as a lazy type",
        );
        assert!(
            !state.ctx.lib_delegation_cache.contains_symbol_type(sym_id),
            "{name} helper alone should not force eager interface lowering",
        );
    }
}

#[test]
fn direct_builtin_lib_variable_annotation_rejects_alias_and_generic_refs() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "lib.dom.d.ts",
        r#"
                interface BoxFixture<T> { value: T; }
                type AliasFixture = { value: number };
                declare var aliasFixture: AliasFixture;
                declare var boxedFixture: BoxFixture<string>;
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };

    for name in ["aliasFixture", "boxedFixture"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .expect("fixture variable symbol");
        assert!(
            state
                .direct_builtin_lib_variable_annotation_type(
                    sym_id,
                    binder.as_ref(),
                    arena.as_ref(),
                )
                .is_none(),
            "{name} should fall back to normal declaration handling",
        );
    }
}

#[test]
fn direct_builtin_lib_variable_annotation_rejects_non_builtin_arena() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "fixture.d.ts",
        r#"
                interface LeafFixture { value: number; }
                declare var leafFixture: LeafFixture;
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let sym_id = binder
        .file_locals
        .get("leafFixture")
        .expect("fixture variable symbol");

    assert!(
        state
            .direct_builtin_lib_variable_annotation_type(sym_id, binder.as_ref(), arena.as_ref())
            .is_none(),
    );
}

#[test]
fn delegate_cross_arena_builtin_variable_annotation_caches_lazy_interface() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "lib.dom.d.ts",
        r#"
                interface DocumentFixture { readyState: string; }
                declare var documentFixture: DocumentFixture;
            "#,
    );
    let (requester_arena, mut requester_binder, _) =
        parse_bound_source_with_name("fixture.ts", "let value;");
    let document_sym = target_binder
        .file_locals
        .get("documentFixture")
        .expect("fixture variable symbol");
    let document_decl = target_binder
        .get_symbol(document_sym)
        .expect("fixture variable symbol data")
        .declarations[0];
    {
        let requester_binder = Arc::make_mut(&mut requester_binder);
        Arc::make_mut(&mut requester_binder.symbol_arenas)
            .insert(document_sym, Arc::clone(&target_arena));
        Arc::make_mut(&mut requester_binder.declaration_arenas)
            .entry((document_sym, document_decl))
            .or_default()
            .push(Arc::clone(&target_arena));
    }

    let ctx = CheckerContext::new(
        requester_arena.as_ref(),
        requester_binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    state.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&requester_arena),
        Arc::clone(&target_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&target_binder),
    ]));
    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(document_sym)
        .expect("builtin lib variable annotation should delegate directly");

    assert!(params.is_empty());
    assert!(
        crate::query_boundaries::common::is_lazy_type(&types, ty),
        "delegated builtin variable should keep the annotated interface lazy",
    );
    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(document_sym)
        .expect("builtin declaration-file variable should populate lib cache");
    assert_eq!(
        (cached_ty, cached_params),
        (ty, params),
        "lib cache should store the lazy variable annotation result",
    );

    let (cached_result_ty, cached_result_params) = state
        .delegate_cross_arena_symbol_resolution(document_sym)
        .expect("builtin variable cache hit should resolve through lib cache");
    assert_eq!(
        (cached_result_ty, cached_result_params),
        (ty, Vec::new()),
        "cache hits should preserve the lazy builtin variable result",
    );
}

#[test]
fn direct_source_file_interface_lowering_accepts_readonly_array_option_bag_member() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                interface DashboardInputFixture {
                    title: string;
                    logos: readonly string[];
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "target.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let sym_id = binder
        .file_locals
        .get("DashboardInputFixture")
        .expect("fixture interface symbol");

    let (ty, params) = state
        .direct_cross_file_interface_lowering(sym_id, binder.as_ref(), arena.as_ref(), false, true)
        .expect("readonly array option-bag interface should lower directly");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty());
}

#[test]
fn direct_source_file_interface_lowering_rejects_non_readonly_type_operator() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                interface KeysFixture { value: string; }
                interface QueryFixture { key: keyof KeysFixture; }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "target.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let sym_id = binder
        .file_locals
        .get("QueryFixture")
        .expect("fixture interface symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                sym_id,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
    );
}

#[test]
fn direct_source_file_interface_member_simple_types_accept_option_bag_members() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                interface DashboardInputFixture {
                    title: string;
                    logos: readonly string[];
                    heroUrl: string;
                }
                interface SeriesSummaryFixture {
                    mean: number;
                    p95: number;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };

    for name in ["DashboardInputFixture", "SeriesSummaryFixture"] {
        let (_sym_id, decl_idx, members) = interface_decl_and_members(&binder, &arena, name);
        let results = state
            .direct_cross_file_interface_member_simple_types(
                decl_idx,
                &members,
                arena.as_ref(),
                binder.as_ref(),
                None,
                true,
            )
            .expect("simple source-file option-bag members should lower directly");

        assert_eq!(
            results.len(),
            members.len(),
            "{name} should lower every requested member directly",
        );
        assert!(
            results
                .values()
                .all(|ty| !matches!(*ty, TypeId::UNKNOWN | TypeId::ERROR))
        );
    }
}

#[test]
fn direct_source_file_interface_member_simple_types_reject_complex_members() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                function summarizeFixture(): { mean: number } {
                    return { mean: 0 };
                }
                interface ReturnTypeFixture {
                    summary: ReturnType<typeof summarizeFixture>;
                }
                interface MethodFixture {
                    format(): string;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };

    for name in ["ReturnTypeFixture", "MethodFixture"] {
        let (_sym_id, decl_idx, members) = interface_decl_and_members(&binder, &arena, name);
        assert!(
            state
                .direct_cross_file_interface_member_simple_types(
                    decl_idx,
                    &members,
                    arena.as_ref(),
                    binder.as_ref(),
                    None,
                    true,
                )
                .is_none(),
            "{name} should stay on the normal cross-file member path",
        );
    }
}
