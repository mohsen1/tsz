//! Tests for tsc-parity treatment of JSDoc `@typedef` declarations as
//! type-only exported members of `.js`/`.mjs`/`.cjs` modules.
//!
//! Three invariants under test:
//!
//! 1. `import { Name } from './file.js'` does NOT emit TS2305 when `file.js`
//!    declares `@typedef Name`.
//! 2. `import('./file.js').Name` does NOT emit TS2694 when `file.js` declares
//!    `@typedef Name` — the import-type expression resolves to the typedef
//!    body (or to `any` when the body itself is unresolvable).
//! 3. The body of a `@typedef {Generic<UnknownA, UnknownB>}` is recursively
//!    validated for unresolvable type arguments, emitting TS2304 — not just
//!    the base `Generic` name.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn local_module_specifiers(file_name: &str) -> Vec<String> {
    let base = file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .rsplit('\\')
        .next()
        .unwrap_or(file_name);
    let mut specs = vec![format!("./{base}")];
    for suffix in [
        ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
        ".mjs", ".cjs",
    ] {
        if let Some(stem) = base.strip_suffix(suffix) {
            specs.push(format!("./{stem}"));
            break;
        }
    }
    specs
}

fn check_consumer_with_js_typedef_source(
    js_source: &str,
    consumer_name: &str,
    consumer_source: &str,
) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut parser_js = ParserState::new("types.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let mut parser_consumer =
        ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_consumer = parser_consumer.parse_source_file();
    let mut binder_consumer = BinderState::new();
    binder_consumer.bind_source_file(parser_consumer.get_arena(), root_consumer);

    let arena_js = Arc::new(parser_js.get_arena().clone());
    let arena_consumer = Arc::new(parser_consumer.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_js), Arc::clone(&arena_consumer)]);

    let binder_js = Arc::new(binder_js);
    let binder_consumer = Arc::new(binder_consumer);
    let all_binders = Arc::new(vec![Arc::clone(&binder_js), Arc::clone(&binder_consumer)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_consumer.as_ref(),
        binder_consumer.as_ref(),
        &types,
        consumer_name.to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    for specifier in local_module_specifiers("types.js") {
        resolved_module_paths.insert((1, specifier.clone()), 0);
        resolved_modules.insert(specifier);
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_consumer);
    checker.ctx.diagnostics
}

fn check_js_diagnostics_only(js_source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut parser_js = ParserState::new("types.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let arena_js = Arc::new(parser_js.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_js)]);
    let binder_js = Arc::new(binder_js);
    let all_binders = Arc::new(vec![Arc::clone(&binder_js)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_js.as_ref(),
        binder_js.as_ref(),
        &types,
        "types.js".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root_js);
    checker.ctx.diagnostics
}

#[test]
fn jsdoc_typedef_in_js_module_suppresses_ts2305_on_named_import() {
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export {};
/** @typedef {{ a: number }} ExportedAlias */
"#,
        "consumer.d.ts",
        r#"
import { ExportedAlias as Local } from './types.js';
type Use = Local;
"#,
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2305),
        "Expected no TS2305 when importing a JSDoc @typedef from a JS module, got: {codes:?}"
    );
}

#[test]
fn jsdoc_typedef_in_js_module_suppresses_ts2694_on_import_type_member() {
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export {};
/** @typedef {{ a: number }} ExportedAlias */
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').ExportedAlias;
"#,
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2694),
        "Expected no TS2694 when referencing a JSDoc @typedef via import('./js').Member, got: {codes:?}"
    );
}

#[test]
fn jsdoc_typedef_with_unresolvable_body_still_suppresses_member_diagnostics() {
    // Even when the typedef body cannot resolve (because Keyword and
    // ParamValueTyped are undefined), tsc still treats the typedef as an
    // exported member of the JS module, so import / import-type lookups must
    // not emit TS2305 / TS2694. The body errors are reported separately as
    // TS2304s on the typedef itself.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export {};
/** @typedef {Record<Keyword, ParamValueTyped>} ParamStateRecord */
"#,
        "consumer.d.ts",
        r#"
import { ParamStateRecord as _PSR } from './types.js';
type FromImportType = import('./types.js').ParamStateRecord;
type Use = _PSR | FromImportType;
"#,
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2305),
        "Expected no TS2305 even when typedef body has unresolved names, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2694),
        "Expected no TS2694 even when typedef body has unresolved names, got: {codes:?}"
    );
}

#[test]
fn jsdoc_typedef_body_emits_ts2304_for_unresolved_generic_type_args() {
    // The typedef base `Box` resolves (it is declared in the same file), but
    // `UnknownA` and `UnknownB` are unresolvable identifiers. Both must be
    // reported via TS2304 — tsc validates the whole typedef body, not just
    // the base name. Uses a locally-defined generic `Box` so this test does
    // not depend on lib types.
    let diagnostics = check_js_diagnostics_only(
        r#"
export {};
/**
 * @template K, V
 * @typedef {{ k: K, v: V }} Box
 */
/** @typedef {Box<UnknownA, UnknownB>} BoxUse */
"#,
    );
    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    let messages: Vec<&str> = ts2304.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'UnknownA'")),
        "Expected TS2304 mentioning 'UnknownA' inside the typedef body, got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("'UnknownB'")),
        "Expected TS2304 mentioning 'UnknownB' inside the typedef body, got: {messages:?}"
    );
}

#[test]
fn jsdoc_typedef_body_does_not_emit_ts2304_for_resolved_generic_type_args() {
    // Sanity check: when all generic type arguments resolve to other
    // locally-defined typedef aliases, no TS2304 is emitted — the type-arg
    // recursion must only flag *unresolvable* identifiers.
    let diagnostics = check_js_diagnostics_only(
        r#"
export {};
/** @typedef {{ a: number }} ResolvedA */
/** @typedef {{ b: number }} ResolvedB */
/**
 * @template K, V
 * @typedef {{ k: K, v: V }} Box
 */
/** @typedef {Box<ResolvedA, ResolvedB>} BoxUse */
"#,
    );
    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "Expected no TS2304 when all generic type arguments resolve, got: {ts2304:?}"
    );
}

#[test]
fn jsdoc_typedef_body_template_param_args_are_not_flagged_as_unresolved() {
    // `T` is declared as a `@template` parameter on the outer typedef; the
    // type-arg recursion must skip it instead of reporting TS2304.
    let diagnostics = check_js_diagnostics_only(
        r#"
export {};
/**
 * @template U
 * @typedef {{ x: U }} Wrapper
 */
/**
 * @template T
 * @typedef {Wrapper<T>} OuterUse
 */
"#,
    );
    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "Expected no TS2304 when typedef references its own @template parameter, got: {ts2304:?}"
    );
}

#[test]
fn jsdoc_mapped_type_tag_scopes_parameter_for_nested_template() {
    let diagnostics = check_js_diagnostics_only(
        r#"
/** @typedef {'parseHTML'|'styleLayout'} TaskGroupIds */

/**
 * @type {{[P in TaskGroupIds]: {id: P, label: string}}}
 */
const taskGroups = {
    parseHTML: { id: 'parseHTML', label: 'Parse HTML & CSS' },
    styleLayout: { id: 'styleLayout', label: 'Style & Layout' },
};

module.exports = { taskGroups };
"#,
    );
    let p_errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains("'P'"))
        .collect();
    assert!(
        p_errors.is_empty(),
        "Expected no TS2304 for mapped type parameter P inside JSDoc @type template, got: {p_errors:?}"
    );
}

#[test]
fn jsdoc_import_type_typedef_alias_is_visible_to_later_typedefs() {
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @typedef {'parseHTML'|'styleLayout'} TaskGroupIds */

/**
 * @typedef TaskGroup
 * @property {TaskGroupIds} id
 * @property {string} label
 */

const taskGroups = {
    parseHTML: { id: 'parseHTML', label: 'Parse HTML & CSS' },
    styleLayout: { id: 'styleLayout', label: 'Style & Layout' },
};

module.exports = { taskGroups };
"#,
        "index.js",
        r#"
const { taskGroups } = require('./types.js');

/** @typedef {import('./types.js').TaskGroup} TaskGroup */

/**
 * @typedef TaskNode
 * @prop {TaskGroup} group
 */

class MainThreadTasks {
    /**
     * @param {TaskGroup} x
     * @param {TaskNode} y
     */
    constructor(x, y) {}
}

module.exports = MainThreadTasks;
"#,
    );
    let task_group_errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2552) && d.message_text.contains("'TaskGroup'"))
        .collect();
    assert!(
        task_group_errors.is_empty(),
        "Expected imported JSDoc typedef alias TaskGroup to resolve in later typedefs, got: {task_group_errors:?}"
    );
}
