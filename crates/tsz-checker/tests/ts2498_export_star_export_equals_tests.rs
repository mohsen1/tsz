//! Tests for TS2498: Module uses 'export =' and cannot be used with 'export *'.
//!
//! When a module uses `export = X`, re-exporting via `export *` or
//! `export * as ns` must emit TS2498.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to set up a two-file project and check file `a.ts` which
/// does `export * as ns from './b'` where `b.ts` has `export = {}`.
fn check_export_star_from_export_equals(source_a: &str, source_b: &str) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.ts".to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    merge_shared_lib_symbols(&mut binder_a);
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), source_b.to_string());
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
    let options = CheckerOptions {
        module: crate::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        arena_a.as_ref(),
        binder_a.as_ref(),
        &types,
        "a.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((0, "./b".to_string()), 1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    checker.check_source_file(root_a);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn export_star_as_ns_from_export_equals_emits_ts2498() {
    let diagnostics =
        check_export_star_from_export_equals("export * as ns from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export * as ns` from a module with `export =`, got: {:?}",
        diagnostics
    );
    assert!(
        ts2498_errors[0].1.contains("export ="),
        "TS2498 message should mention 'export =', got: {}",
        ts2498_errors[0].1
    );
}

#[test]
fn export_star_bare_from_export_equals_emits_ts2498() {
    let diagnostics = check_export_star_from_export_equals("export * from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export *` from a module with `export =`, got: {:?}",
        diagnostics
    );
}

#[test]
fn export_named_from_export_equals_no_ts2498() {
    // Named re-exports should NOT emit TS2498
    let diagnostics =
        check_export_star_from_export_equals("export { default } from './b';", "export = {}");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        ts2498_errors.is_empty(),
        "Named export should NOT emit TS2498, got: {:?}",
        diagnostics
    );
}

#[test]
fn export_star_from_normal_module_no_ts2498() {
    // Normal module (no export =) should not emit TS2498
    let diagnostics =
        check_export_star_from_export_equals("export * as ns from './b';", "export const x = 1;");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        ts2498_errors.is_empty(),
        "Normal module should NOT emit TS2498, got: {:?}",
        diagnostics
    );
}
