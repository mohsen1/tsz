use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::{TypeInterner, function_shape_for_type};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
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

fn parse_bound_source(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    parse_bound_source_with_name("fixture.ts", source)
}

fn direct_function_type_for_source(source: &str, name: &str) -> Option<TypeId> {
    let (arena, binder, types) = parse_bound_source(source);
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let sym = binder.file_locals.get(name).expect("function symbol");

    state.direct_source_file_function_declaration_type(sym, binder.as_ref(), arena.as_ref(), true)
}

#[test]
fn direct_source_file_function_declaration_lowers_annotated_signature() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Payload { value: number; }
                export function summarize(payload: Payload, label: string): string {
                    return label + payload.value;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let summarize_sym = binder
        .file_locals
        .get("summarize")
        .expect("function symbol");

    let result = state
        .direct_source_file_function_declaration_type(
            summarize_sym,
            binder.as_ref(),
            arena.as_ref(),
            true,
        )
        .expect("fully annotated source function should lower directly");
    let shape = function_shape_for_type(&types, result)
        .expect("direct source function lowering should produce a function type");

    assert_eq!(shape.params.len(), 2);
    assert_ne!(shape.params[0].type_id, TypeId::UNKNOWN);
    assert_ne!(shape.params[0].type_id, TypeId::ERROR);
    assert_eq!(shape.params[1].type_id, TypeId::STRING);
    assert_eq!(shape.return_type, TypeId::STRING);
}

#[test]
fn direct_source_file_function_declaration_lowers_local_alias_signature() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Payload { value: number; }
                type MaybePayload = Payload | null;
                export function summarize(payload: MaybePayload): string {
                    return payload ? String(payload.value) : "";
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let summarize_sym = binder
        .file_locals
        .get("summarize")
        .expect("function symbol");

    let result = state
        .direct_source_file_function_declaration_type(
            summarize_sym,
            binder.as_ref(),
            arena.as_ref(),
            true,
        )
        .expect("local alias with lowerable body should lower directly");
    let shape = function_shape_for_type(&types, result)
        .expect("direct source function lowering should produce a function type");

    assert_eq!(shape.params.len(), 1);
    assert_ne!(shape.params[0].type_id, TypeId::UNKNOWN);
    assert_ne!(shape.params[0].type_id, TypeId::ERROR);
    assert_eq!(shape.return_type, TypeId::STRING);
}

#[test]
fn direct_source_file_function_declaration_rejects_inferred_signature_parts() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                export function missingReturn(value: number) {
                    return value;
                }
                export function missingParam(value): string {
                    return "";
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };

    for name in ["missingReturn", "missingParam"] {
        let sym = binder.file_locals.get(name).expect("function symbol");
        assert!(
            state
                .direct_source_file_function_declaration_type(
                    sym,
                    binder.as_ref(),
                    arena.as_ref(),
                    true,
                )
                .is_none(),
            "{name} should fall back when any signature type is inferred",
        );
    }
}

#[test]
fn direct_source_file_function_declaration_rejects_non_local_annotation_types() {
    let direct_import = direct_function_type_for_source(
        r#"
                import type { ImportedPayload } from "./types";
                export function summarize(payload: ImportedPayload): string {
                    return "";
                }
            "#,
        "summarize",
    );
    assert!(
        direct_import.is_none(),
        "imported annotation should fall back to the child checker",
    );

    let alias_to_import = direct_function_type_for_source(
        r#"
                import type { ImportedPayload } from "./types";
                type LocalPayload = ImportedPayload;
                export function summarize(payload: LocalPayload): string {
                    return "";
                }
            "#,
        "summarize",
    );
    assert!(
        alias_to_import.is_none(),
        "local alias to an imported annotation should fall back to the child checker",
    );
}

#[test]
fn direct_source_file_function_declaration_rejects_generic_signature() {
    let result = direct_function_type_for_source(
        r#"
                export function identity<T>(value: T): T {
                    return value;
                }
            "#,
        "identity",
    );

    assert!(
        result.is_none(),
        "generic source functions need the child checker for scoped type parameters",
    );
}

#[test]
fn delegate_explicit_cross_file_source_function_lowers_annotated_signature() {
    let (target_arena, target_binder, types) = parse_bound_source_with_name(
        "target.ts",
        r#"
                interface Payload { value: number; }
                export function summarize(payload: Payload, label: string): string {
                    return label + payload.value;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) = parse_bound_source_with_name(
        "requester.ts",
        "// synthetic requester with explicit symbol-file ownership only",
    );
    let summarize_sym = target_binder
        .file_locals
        .get("summarize")
        .expect("function symbol");

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
    let mut symbol_file_index = FxHashMap::default();
    symbol_file_index.insert(summarize_sym, 1);
    ctx.set_global_symbol_file_index(Arc::new(symbol_file_index));
    let mut state = CheckerState { ctx };

    let (ty, params) = state
        .delegate_cross_arena_symbol_resolution(summarize_sym)
        .expect("annotated source function should lower through explicit file target");
    let shape = function_shape_for_type(&types, ty)
        .expect("direct cross-file function lowering should produce a function type");

    assert!(params.is_empty());
    assert_eq!(shape.params.len(), 2);
    assert_ne!(shape.params[0].type_id, TypeId::UNKNOWN);
    assert_ne!(shape.params[0].type_id, TypeId::ERROR);
    assert_eq!(shape.params[1].type_id, TypeId::STRING);
    assert_eq!(shape.return_type, TypeId::STRING);
    assert_eq!(
        state.ctx.cached_cross_file_symbol_type(summarize_sym, 1),
        Some((ty, params)),
        "explicit cross-file function result should be cached by file target",
    );
}
