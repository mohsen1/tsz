//! Tests for TS2498: Module uses 'export =' and cannot be used with 'export *'.
//!
//! When a module uses `export = <expr>`, wildcard re-exports
//! (`export * from 'module'` or `export * as ns from 'module'`) are invalid
//! because ES module namespace objects cannot be constructed from a CommonJS
//! single-value export.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Set up a two-file scenario where file "a.ts" has `export = {}` and file "b.ts"
/// re-exports from it. Returns diagnostics from checking file "b.ts".
fn check_reexport_of_export_equals(b_source: &str) -> Vec<(u32, String)> {
    // File a.ts: uses export =
    let mut parser_a = ParserState::new("a.ts".to_string(), "export = {};".to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    // File b.ts: re-exports from a
    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // Wire up module resolution: make a.ts exports available to b.ts
    if let Some(a_exports) = binder_a.module_exports.get("a.ts").cloned() {
        binder_b.module_exports.insert("./a".to_string(), a_exports);
    }

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root_b);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn export_star_as_from_export_equals_module_emits_ts2498() {
    let diagnostics = check_reexport_of_export_equals("export * as ns from './a';");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export * as ns from` a module using `export =`. Got: {diagnostics:?}"
    );
}

#[test]
fn export_star_from_export_equals_module_emits_ts2498() {
    let diagnostics = check_reexport_of_export_equals("export * from './a';");
    let ts2498_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2498)
        .collect();
    assert!(
        !ts2498_errors.is_empty(),
        "Expected TS2498 for `export * from` a module using `export =`. Got: {diagnostics:?}"
    );
}

#[test]
fn named_export_from_export_equals_module_no_ts2498() {
    let diagnostics = check_reexport_of_export_equals("export { } from './a';");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2498),
        "Should NOT emit TS2498 for named re-exports. Got: {diagnostics:?}"
    );
}

/// Set up a two-file scenario where file "a.ts" has normal exports (no export =).
fn check_reexport_of_normal_module(b_source: &str) -> Vec<(u32, String)> {
    // File a.ts: normal ES module exports
    let mut parser_a = ParserState::new("a.ts".to_string(), "export const x = 1;".to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    // File b.ts: re-exports from a
    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // Wire up module resolution
    if let Some(a_exports) = binder_a.module_exports.get("a.ts").cloned() {
        binder_b.module_exports.insert("./a".to_string(), a_exports);
    }

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root_b);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn export_star_from_normal_module_no_ts2498() {
    let diagnostics = check_reexport_of_normal_module("export * as ns from './a';");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2498),
        "Should NOT emit TS2498 for normal module re-exports. Got: {diagnostics:?}"
    );
}
