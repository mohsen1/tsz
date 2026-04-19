use crate::context::CheckerOptions;
use crate::state::CheckerState;
use crate::test_utils::check_source;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

const IMPORT_DEFER_SOURCE: &str = r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface Promise<T> {
  then<U>(f: (x: T) => U): Promise<U>;
}
interface RegExp {}
interface String {}

declare module "./a.js" {
  export function foo(): void;
}

import.defer("./a.js").then(ns => {
  ns.foo();
});
"#;

fn import_defer_diagnostics(module: ModuleKind) -> Vec<(u32, String)> {
    check_source(
        IMPORT_DEFER_SOURCE,
        "test.ts",
        CheckerOptions {
            module,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

fn cross_file_import_defer_diagnostics() -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.ts".to_string(), "export const value = 1;".to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let b_source = r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface Promise<T> {}
interface RegExp {}
interface String {}

import.defer("./a.js");
"#;
    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

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
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.report_unresolved_imports = true;

    // Mirror what `build_module_resolution_maps` produces: both the
    // extension-stripped canonical form and the extension-bearing form are
    // registered, since users legitimately write either.
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a".to_string()), 0);
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a".to_string());
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

#[test]
fn import_defer_then_callback_is_contextually_typed() {
    let diagnostics = import_defer_diagnostics(ModuleKind::ES2020);

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 7006),
        "Expected import.defer(...).then callback to be contextually typed, got: {diagnostics:?}"
    );
}

#[test]
fn import_defer_emits_ts1323_for_unsupported_module_kind() {
    let diagnostics = import_defer_diagnostics(ModuleKind::ES2015);

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 1323),
        "Expected TS1323 for import.defer under ES2015 modules, got: {diagnostics:?}"
    );
}

#[test]
fn import_defer_cross_file_js_specifier_does_not_emit_ts2307() {
    let diagnostics = cross_file_import_defer_diagnostics();

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2307),
        "Expected import.defer('./a.js') to resolve via module-specifier candidates, got: {diagnostics:?}"
    );
}
