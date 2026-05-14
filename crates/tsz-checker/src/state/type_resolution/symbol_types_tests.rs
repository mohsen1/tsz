use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, ParserState};

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
    assert!(
        params.is_empty(),
        "an existing local def for the colliding SymbolId should keep local type-alias resolution local"
    );
}
