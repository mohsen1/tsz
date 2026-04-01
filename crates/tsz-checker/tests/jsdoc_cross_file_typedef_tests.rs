use crate::context::CheckerOptions;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

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

fn check_types_file_with_jsdoc_global(
    types_source: &str,
    js_source: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let mut parser_types = ParserState::new("types.ts".to_string(), types_source.to_string());
    let root_types = parser_types.parse_source_file();
    let mut binder_types = BinderState::new();
    binder_types.bind_source_file(parser_types.get_arena(), root_types);

    let mut parser_js = ParserState::new("other.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let arena_types = Arc::new(parser_types.get_arena().clone());
    let arena_js = Arc::new(parser_js.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_types), Arc::clone(&arena_js)]);

    let binder_types = Arc::new(binder_types);
    let binder_js = Arc::new(binder_js);
    let all_binders = Arc::new(vec![Arc::clone(&binder_types), Arc::clone(&binder_js)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_types.as_ref(),
        binder_types.as_ref(),
        &types,
        "types.ts".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root_types);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn check_js_file_with_types_diagnostics(
    types_name: &str,
    types_source: &str,
    js_name: &str,
    js_source: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
    let mut parser_types = ParserState::new(types_name.to_string(), types_source.to_string());
    let root_types = parser_types.parse_source_file();
    let mut binder_types = BinderState::new();
    binder_types.bind_source_file(parser_types.get_arena(), root_types);

    let mut parser_js = ParserState::new(js_name.to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let arena_types = Arc::new(parser_types.get_arena().clone());
    let arena_js = Arc::new(parser_js.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_types), Arc::clone(&arena_js)]);

    let binder_types = Arc::new(binder_types);
    let binder_js = Arc::new(binder_js);
    let all_binders = Arc::new(vec![Arc::clone(&binder_types), Arc::clone(&binder_js)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_js.as_ref(),
        binder_js.as_ref(),
        &types,
        js_name.to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    for specifier in local_module_specifiers(types_name) {
        resolved_module_paths.insert((1, specifier.clone()), 0);
        resolved_modules.insert(specifier);
    }
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.check_source_file(root_js);

    checker.ctx.diagnostics
}

fn check_js_file_with_types(
    types_name: &str,
    types_source: &str,
    js_name: &str,
    js_source: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    check_js_file_with_types_diagnostics(types_name, types_source, js_name, js_source, options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn cross_file_jsdoc_typedef_is_visible_from_ts_type_reference() {
    let codes = check_types_file_with_jsdoc_global(
        r#"
export interface F {
    (): E;
}
export interface D<T extends F = F> {}
"#,
        r#"/** @typedef {import("./types").D} E */"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&2304),
        "Expected no TS2304 for cross-file JSDoc typedef visible from TS file, got codes: {codes:?}"
    );
}

#[test]
fn cross_file_generic_jsdoc_typedef_preserves_arity_and_constraints() {
    let codes = check_types_file_with_jsdoc_global(
        r#"
declare var actually: Everything<{ a: number }, undefined, { c: 1, d: 1 }, number, string>;
"#,
        r#"
/**
 * @template {{ a: number, b: string }} T,U A Comment
 * @template {{ c: boolean }} V trailing prose should not become params
 * @template W
 * @template X That last one had no comment
 * @typedef {{ t: T, u: U, v: V, w: W, x: X }} Everything
 */
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&2304),
        "Expected generic cross-file JSDoc typedef to stay visible from TS, got codes: {codes:?}"
    );
    assert!(
        codes.contains(&2344),
        "Expected TS2344 from generic JSDoc typedef constraint checking, got codes: {codes:?}"
    );
}

#[test]
fn js_file_jsdoc_import_typedef_at_eof_is_visible_to_prior_type_tag() {
    let codes = check_js_file_with_types(
        "interfaces.d.ts",
        r#"
export interface Bar {
    prop: string
}
"#,
        "usage.js",
        r#"
/** @type {Bar} */
export let bar;

/** @typedef {import("./interfaces").Bar} Bar */
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&7005),
        "Expected EOF JSDoc import typedef to provide the prior @type annotation without TS7005, got codes: {codes:?}"
    );
}

#[test]
fn exported_js_variable_with_jsdoc_type_is_not_implicit_any() {
    let codes = check_js_file_with_types(
        "types.d.ts",
        "",
        "usage.js",
        r#"
/** @type {string} */
export let bar;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&7005),
        "Expected exported JSDoc-typed variable to suppress TS7005, got codes: {codes:?}"
    );
}

#[test]
fn imported_jsdoc_typedef_does_not_conflict_with_exported_source_symbol_name() {
    let codes = check_js_file_with_types(
        "file.ts",
        r#"
class Foo {
    x: number;
}

declare global {
    var module: any;
}

export = Foo;
"#,
        "something.js",
        r#"
/** @typedef {typeof import("./file")} Foo */
/** @typedef {(foo: Foo) => string} FooFun */

module.exports = /** @type {FooFun} */(void 0);
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_unused_locals: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&2300),
        "Expected imported JSDoc typedef alias to avoid cross-file TS2300, got codes: {codes:?}"
    );
}

#[test]
fn same_file_jsdoc_typedef_still_conflicts_with_local_class_name() {
    let codes = check_js_file_with_types(
        "types.d.ts",
        "",
        "usage.js",
        r#"
class Foo {}
/** @typedef {number} Foo */
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        codes.contains(&2300),
        "Expected same-file JSDoc typedef/class name collision to keep TS2300, got codes: {codes:?}"
    );
}

#[test]
#[ignore] // TODO: JSDoc namespace property access emits extra TS2339 through require() boundary
fn jsdoc_namespace_type_from_required_declaration_module_preserves_ts2454() {
    let diagnostics = check_js_file_with_types_diagnostics(
        "mod1.d.ts",
        r#"
export interface Bar {
    prop: string
}

export class Baz {
    prop: string
}
"#,
        "use.js",
        r#"
var mod = require("./mod1");

/** @type {mod.Bar} */
let c;
c.prop;

/** @type {mod.Baz} */
let d;
d.prop;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    let rendered: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect();

    assert!(
        codes.contains(&2454),
        "Expected TS2454 for require()-namespace JSDoc types from declaration modules, got diagnostics: {rendered:?}"
    );
    assert!(
        !codes.contains(&18048),
        "Did not expect TS18048 once JSDoc namespace types resolve to direct typed exports, got diagnostics: {rendered:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Did not expect TS2339 once JSDoc namespace member types preserve property shape, got diagnostics: {rendered:?}"
    );
}
