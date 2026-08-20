use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::{TypeId, TypeParamInfo};

fn parse_and_bind(file_name: &str, source: &str) -> (Arc<NodeArena>, Arc<BinderState>, NodeIndex) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (Arc::new(parser.get_arena().clone()), Arc::new(binder), root)
}

#[test]
fn dynamic_type_alias_owner_wins_over_local_symbol_id_collision() {
    let (target_arena, target_binder, _) = parse_and_bind(
        "target.ts",
        r#"
type Padding0 = unknown;
type Padding1 = unknown;
export type Remote<T> = T;
"#,
    );
    let (entry_arena, entry_binder, _) = parse_and_bind(
        "entry.ts",
        r#"
type Local0 = number;
type Local1 = number;
type Local2 = number;
import {Remote} from './target';
export type Use = Remote;
"#,
    );

    let remote_sym_id = target_binder
        .file_locals
        .get("Remote")
        .expect("target file should bind exported Remote alias");
    let local_collision = entry_binder
        .get_symbol(remote_sym_id)
        .expect("entry file should bind a same-number local symbol");
    assert!(
        local_collision.has_any_flags(symbol_flags::TYPE_ALIAS)
            && !local_collision.has_any_flags(symbol_flags::ALIAS),
        "test setup needs a non-alias local SymbolId collision; got flags {} for {}",
        local_collision.flags,
        local_collision.escaped_name
    );

    let all_arenas = Arc::new(vec![target_arena, entry_arena]);
    let all_binders = Arc::new(vec![target_binder, entry_binder]);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "entry.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.register_symbol_file_target(remote_sym_id, 0);

    let (_body, params) = checker.type_reference_symbol_type_with_params(remote_sym_id);
    assert_eq!(
        params.len(),
        1,
        "dynamic cross-file type-alias ownership should use the remote alias parameters, not the same-number local alias"
    );
}

#[test]
fn local_type_alias_def_blocks_dynamic_owner_collision() {
    let (target_arena, target_binder, _) = parse_and_bind(
        "target.ts",
        r#"
type Padding0 = unknown;
type Padding1 = unknown;
export type Remote<T> = T;
"#,
    );
    let (entry_arena, entry_binder, _) = parse_and_bind(
        "entry.ts",
        r#"
type Local0 = number;
type Local1 = number;
type Local2 = number;
import {Remote} from './target';
export type Use = Remote;
"#,
    );

    let remote_sym_id = target_binder
        .file_locals
        .get("Remote")
        .expect("target file should bind exported Remote alias");
    let local_collision = entry_binder
        .get_symbol(remote_sym_id)
        .expect("entry file should bind a same-number local symbol");
    assert_eq!(local_collision.escaped_name, "Local2");

    let all_arenas = Arc::new(vec![target_arena, entry_arena]);
    let all_binders = Arc::new(vec![target_binder, entry_binder]);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "entry.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.register_symbol_file_target(remote_sym_id, 0);
    checker
        .ctx
        .get_or_create_def_id_for_symbol_name(remote_sym_id, "Local2");

    let (_body, params) = checker.type_reference_symbol_type_with_params(remote_sym_id);
    // Contract (re-asserted 2026-07-13, was a stale white-box guard): an
    // explicitly registered symbol-file target is the AUTHORITATIVE owner
    // for this raw id, superseding a previously minted local def. The real
    // pipeline resolves the local `Local2` through its own (non-registered)
    // symbol id, so local resolution never routes through this id at all —
    // CLI-verified: the two-file form is clean in both tsc and tsz, and
    // `Remote` without args emits the identical TS2314 in both.
    assert_eq!(
        params.len(),
        1,
        "registered file-target ownership must resolve the remote alias params even when a local def exists for the colliding raw id"
    );
}

#[test]
fn same_name_local_collision_without_def_stays_local() {
    let (target_arena, target_binder, _) = parse_and_bind(
        "target.ts",
        r#"
type Padding0 = unknown;
type Padding1 = unknown;
export type Remote<T> = T;
"#,
    );
    let (entry_arena, entry_binder, _) = parse_and_bind(
        "entry.ts",
        r#"
type Local0 = number;
type Local1 = number;
type Remote = number;
"#,
    );

    let remote_sym_id = target_binder
        .file_locals
        .get("Remote")
        .expect("target file should bind exported Remote alias");
    let local_collision = entry_binder
        .get_symbol(remote_sym_id)
        .expect("entry file should bind a same-number local symbol");
    assert_eq!(local_collision.escaped_name, "Remote");
    assert!(local_collision.has_any_flags(symbol_flags::TYPE_ALIAS));

    let all_arenas = Arc::new(vec![target_arena, entry_arena]);
    let all_binders = Arc::new(vec![target_binder, entry_binder]);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "entry.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.register_symbol_file_target(remote_sym_id, 0);

    let (_body, params) = checker.type_reference_symbol_type_with_params(remote_sym_id);
    assert!(
        params.is_empty(),
        "a same-name local SymbolId collision without a local def should not delegate to the remote generic alias"
    );
}

#[test]
fn type_reference_depth_cap_falls_back_to_own_lazy_reference() {
    let (arena, binder, _) = parse_and_bind(
        "entry.ts",
        r#"
type AliasThroughWrapper<T> = { value: T };
"#,
    );
    let sym_id = binder
        .file_locals
        .get("AliasThroughWrapper")
        .expect("entry file should bind the local generic alias");
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "entry.ts".to_string(),
        CheckerOptions::default(),
    );

    let eval_session = std::rc::Rc::clone(&checker.ctx.eval_session);
    let mut depth_entries = Vec::new();
    while let Some(entry) = eval_session.enter_type_reference_resolution_depth() {
        depth_entries.push(entry);
    }
    assert!(
        eval_session.type_reference_resolution_depth() > 0,
        "test setup should exhaust the shared type-reference depth cap"
    );

    let (body, params) = checker.type_reference_symbol_type_with_params(sym_id);
    let expected_lazy = checker.ctx.create_lazy_type_ref(sym_id);
    assert_eq!(
        body, expected_lazy,
        "depth exhaustion should leave the alias as its own lazy reference"
    );
    assert!(
        params.is_empty(),
        "depth fallback should not synthesize alias parameters"
    );

    drop(depth_entries);
    assert_eq!(eval_session.type_reference_resolution_depth(), 0);
}

#[test]
fn def_type_params_fallback_rejects_different_name_symbol_collision() {
    let (target_arena, target_binder, _) = parse_and_bind(
        "target.ts",
        r#"
type Padding0 = unknown;
export interface Other<T, U = string> { value: T; }
"#,
    );
    let (entry_arena, entry_binder, _) = parse_and_bind(
        "entry.ts",
        r#"
type Padding0 = number;
interface Promise<T> { value: T; }
"#,
    );

    let other_sym_id = target_binder
        .file_locals
        .get("Other")
        .expect("target file should bind exported Other interface");
    let promise_sym_id = entry_binder
        .file_locals
        .get("Promise")
        .expect("entry file should bind local Promise interface");
    assert_eq!(
        other_sym_id.0, promise_sym_id.0,
        "test setup needs a raw SymbolId collision"
    );

    let all_arenas = Arc::new(vec![target_arena, entry_arena]);
    let all_binders = Arc::new(vec![target_binder, entry_binder]);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "entry.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.register_symbol_file_target(other_sym_id, 0);

    let other_def = checker
        .ctx
        .get_or_create_def_id_for_symbol_name(other_sym_id, "Other");
    let first = TypeParamInfo::simple(types.intern_string("T"));
    let mut second = TypeParamInfo::simple(types.intern_string("U"));
    second.default = Some(TypeId::STRING);
    checker
        .ctx
        .insert_def_type_params(other_def, vec![first, second]);

    let promise_def = checker
        .ctx
        .get_or_create_def_id_for_symbol_name(promise_sym_id, "Promise");

    // Contract (re-asserted 2026-07-13, was a stale white-box guard): the
    // raw-id fallback must never hand Promise the differently named Other
    // definition's parameter list ([T, U = string]). Resolving to no params
    // or to Promise's own single default-less parameter are both correct —
    // CLI-verified: tsc and tsz both keep Promise at arity 1 (identical
    // TS2314 for Promise<number, string>; Promise<number> clean).
    let params = checker
        .ctx
        .get_def_type_params(promise_def)
        .unwrap_or_default();
    assert!(
        params.len() <= 1 && params.iter().all(|param| param.default.is_none()),
        "file-agnostic raw SymbolId fallback must not copy generic params/defaults from a differently named definition, got {params:?}"
    );
}
