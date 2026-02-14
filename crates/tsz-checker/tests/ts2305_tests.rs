//! Tests for TS2305 emission ("Module has no exported member")
//!
//! These tests verify that named imports report TS2305 when the resolved
//! module does not export the requested symbol.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_ts2305_emitted_for_missing_export_in_resolved_module() {
    let mut parser_a = ParserState::new(
        "a.ts".to_string(),
        "import { missing } from \"./foo\";".to_string(),
    );
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    merge_shared_lib_symbols(&mut binder_a);
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("foo.ts".to_string(), "export const base = 1;".to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    merge_shared_lib_symbols(&mut binder_b);
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_a.as_ref(),
        binder_a.as_ref(),
        &types,
        "a.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((0, "./foo".to_string()), 1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./foo".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root_a);

    let ts2305_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2305)
        .collect();
    assert!(
        !ts2305_errors.is_empty(),
        "Expected TS2305 error for missing export, got: {:?}",
        checker.ctx.diagnostics
    );
}
