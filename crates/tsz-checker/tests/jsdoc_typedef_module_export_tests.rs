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

fn ts2694_diagnostics<'a>(diagnostics: &'a [Diagnostic], member: &str) -> Vec<&'a Diagnostic> {
    let quoted = format!("'{member}'");
    diagnostics
        .iter()
        .filter(|d| d.code == 2694 && d.message_text.contains(&quoted))
        .collect()
}

#[test]
fn import_type_member_that_is_a_value_only_const_export_reports_ts2694() {
    // `FOO` is a plain `const` — a value, not a type. `tsc` reports TS2694
    // ("Namespace '"./types.js"' has no exported member 'FOO'.") because a
    // value-only export is not a valid `import(...).Member` type target,
    // unlike a JSDoc `@typedef` (which tsc treats as a type-only export).
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export const FOO = "foo";
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').FOO;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "FOO").is_empty(),
        "Expected TS2694 for a value-only const export referenced via import('./js').Member, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_type_comment_member_of_a_value_only_const_export_reports_ts2694() {
    // Same rule as the TS `type X = import(...).Y` alias-declaration path
    // above: under TypeScript 7, `import(...).Member` requires `Member` to
    // be type-eligible, through the JSDoc `@type` comment path too — no
    // fallback to the exported value's own type. This is the literal
    // upstream conformance case `jsdocImportTypeReferenceToStringLiteral.ts`
    // (`TypeScript/tests/cases/conformance/jsdoc/`); its committed `.types`
    // baseline (`x : "foo"`, no `.errors.txt`) predates the corpus's repin
    // to `tsgo-port` and is stale. Oracle-verified against the pinned
    // corpus oracle (`scripts/node_modules/@typescript/typescript-linux-x64`,
    // typescript@7.0.2 native, matching `scripts/conformance/typescript-versions.json`'s
    // `current` pin) on a hand-copied repro of the exact fixture:
    // `a.js(1,26): error TS2694: Namespace '"…/b"' has no exported member 'FOO'.`
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export const FOO = "foo";
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').FOO} */
let x;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "FOO").is_empty(),
        "Expected TS2694 for a value-only const export referenced via JSDoc @type import('./js').Member (TS7 dropped the value-type fallback), got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_type_comment_member_of_an_enum_tagged_export_reports_ts2694() {
    // Adjacent case: a `@enum {T}` export is likewise value-only under TS7
    // (`jsdoc_enum_member_assignability_tests.rs`'s
    // `bare_enum_name_in_type_position_is_ts2749` locks the same rule for a
    // *local* bare reference). Reached across a module boundary via
    // `import(...).Member`, tsc reports TS2694 rather than TS2749 (still no
    // value-type fallback) — this is the literal upstream conformance case
    // `enumTagImported.ts` (`TypeScript/tests/cases/conformance/jsdoc/`),
    // oracle-verified against the pinned corpus oracle (typescript@7.0.2
    // native) on a hand-copied repro of the fixture's `type.js`/`mod1.js`
    // pair: two `TS2694`s, one per `import("./mod1").TestEnum` reference.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @enum {string} */
export const TestEnum = {
    ADD: 'add',
    REMOVE: 'remove'
};
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').TestEnum} */
let x;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "TestEnum").is_empty(),
        "Expected TS2694 for an @enum-tagged export referenced via JSDoc @type import('./js').Member, got: {diagnostics:?}"
    );
}

#[test]
fn import_type_member_that_is_a_value_only_function_export_reports_ts2694() {
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export function bar() {}
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').bar;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "bar").is_empty(),
        "Expected TS2694 for a value-only function export referenced via import('./js').Member, got: {diagnostics:?}"
    );
}

#[test]
fn import_type_member_that_is_a_value_only_export_with_renamed_binder_reports_ts2694() {
    // Same structural rule as the `FOO`/`bar` cases above, with a differently
    // named binder — the diagnostic must not depend on the specific
    // identifier chosen.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export const zephyr = 42;
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').zephyr;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "zephyr").is_empty(),
        "Expected TS2694 for a renamed value-only export referenced via import('./js').Member, got: {diagnostics:?}"
    );
}

#[test]
fn import_type_member_that_is_a_type_only_interface_export_does_not_report_ts2694() {
    // Adjacent positive control: an `interface` export IS type-eligible, so
    // no TS2694 fires — only the value-only shapes above are rejected.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export interface Shape {
    a: number;
}
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').Shape;
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Shape").is_empty(),
        "Expected no TS2694 for a type-only interface export, got: {diagnostics:?}"
    );
}

// Adjacent matrix for #17162: a CommonJS expando export
// (`module.exports.Member = Member` / `exports.Member = Member`) records no
// SymbolId in the binder's export tables — those only track ES `export`
// syntax — so the plain symbol lookup in `resolve_jsdoc_import_type_reference`
// never sees it. When the expando's RHS is a class declaration's own
// identifier, the reference is type-eligible and must not report TS2694.

#[test]
fn jsdoc_typedef_import_type_member_resolves_module_exports_expando_class() {
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
class C {
    s() {}
}
module.exports.C = C
"#,
        "consumer.js",
        r#"
/** @typedef {import('./types.js').C} X */
/** @param {X} c */
function demo(c) {
    c.s()
}
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "C").is_empty(),
        "Expected no TS2694 for a `module.exports.C = C` expando class export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_type_comment_member_resolves_module_exports_expando_class() {
    // Same structural rule as above, but through the actual JSDoc `@type`
    // comment path (matches the upstream repro exactly) rather than a
    // `@typedef` indirection.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
class C {
    s() {}
}
module.exports.C = C
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').C} */
let x;
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "C").is_empty(),
        "Expected no TS2694 for a `module.exports.C = C` expando class export via @type, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_member_resolves_bare_exports_expando_class_renamed_binder() {
    // Adjacent case: the short `exports.X = X` form (no `module.` prefix),
    // with a differently named binder — the fix must not depend on the
    // specific identifier `C` or the `module.exports.` spelling. Exercised
    // through the same JSDoc `@typedef`/`import(...).Member` string-parsing
    // path as #17162's actual repro (the TS-syntax `type X = import(...).Y`
    // alias-declaration path is a separate resolver, untouched by this fix).
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
class Widget {
    render() {}
}
exports.Widget = Widget
"#,
        "consumer.js",
        r#"
/** @typedef {import('./types.js').Widget} X */
/** @param {X} w */
function demo(w) {
    w.render()
}
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Widget").is_empty(),
        "Expected no TS2694 for an `exports.Widget = Widget` expando class export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_member_expando_function_export_still_reports_ts2694() {
    // Negative control: an expando export whose RHS is a plain function (not
    // a class) is still value-only — TS7 dropped constructor-function
    // inference, so a bare type-position reference must still report TS2694.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
function bar() {}
module.exports.bar = bar
"#,
        "consumer.d.ts",
        r#"
type Use = import('./types.js').bar;
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "bar").is_empty(),
        "Expected TS2694 for a `module.exports.bar = bar` expando function export, got: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Qualified (dotted) JSDoc `@typedef` member references — #17162.
//
// A dotted JSDoc `@typedef {T} A.B` declares a *qualified* type name; the
// module exports it under its full path. A JSDoc `import("./mod").A.B`
// reference must resolve to that typedef, not report TS2694 for the first
// segment `A`. Before the fix, `parse_jsdoc_import_type` truncated the member
// to the head segment, so #17139's value-only-export check saw a present
// qualified typedef as a missing member.
// ---------------------------------------------------------------------------

#[test]
fn jsdoc_import_type_dotted_typedef_member_suppresses_ts2694() {
    // The exact #17162 repro shape: a dotted `@typedef {number} Dotted.Name`
    // referenced through `import("./types.js").Dotted.Name` in a checked-JS
    // `@type`. tsc reports nothing; tsz must not report TS2694 on `Dotted`.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @typedef {number} Dotted.Name */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Dotted.Name} */
var dot
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Dotted").is_empty(),
        "Expected no TS2694 on the head segment of a dotted `@typedef Dotted.Name`, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_deep_dotted_typedef_member_suppresses_ts2694() {
    // Breadth: the fix keeps every `.`-joined segment, so a three-level
    // qualified typedef `A.B.C` resolves the same way — the head segment `A`
    // must not be reported as a missing member.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @typedef {string} A.B.C */
export {}
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').A.B.C} */
var deep
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "A").is_empty(),
        "Expected no TS2694 on the head segment of a deep dotted `@typedef A.B.C`, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_dotted_typedef_member_binder_name_varies() {
    // Anti-hardcoding: the resolution is structural, not keyed to any specific
    // identifier. A differently-named qualified typedef resolves identically.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @typedef {boolean} Outer9.Flag$ */
export {}
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Outer9.Flag$} */
var flag
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Outer9").is_empty(),
        "Expected no TS2694 for a renamed qualified `@typedef Outer9.Flag$`, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_qualified_missing_head_still_reports_ts2694() {
    // Negative control: when the module declares no matching qualified typedef
    // and no `Nope` export, the head segment is genuinely absent — tsc reports
    // TS2694 on the first segment, and so must tsz. Guards that the additive
    // full-path typedef lookup does not swallow real missing-member errors.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export {}
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Nope.Name} */
var bad
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "Nope").is_empty(),
        "Expected TS2694 on the head segment `Nope` of an unresolved qualified reference, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_single_segment_missing_still_reports_ts2694() {
    // Guard: the single-segment resolution path is byte-for-byte unchanged — a
    // plain missing member still reports TS2694.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
export {}
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Missing} */
var bad
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "Missing").is_empty(),
        "Expected TS2694 for a single-segment missing member, got: {diagnostics:?}"
    );
}

// #17176: a JSDoc `@type`/`@param` annotation whose `import(...).Member`
// reference fails to resolve used to report TS2694 twice — once from the
// eager per-file `@type`/`@param`-tag validation probe (which only needs a
// resolved-or-not boolean for an unrelated TS2304 decision, but leaked the
// import resolver's own diagnostic as a side effect) and once from the
// authoritative per-declaration/per-parameter resolver, at two positions
// that both differed from tsc's single report anchored at the member-name
// token inside the comment. The tests below pin the count for each shape
// from the issue's adjacent-case matrix, plus the exact anchor for the
// `@type` shapes where the fix also corrected the position.

#[test]
fn jsdoc_type_tag_import_member_not_found_reports_once_at_member_token() {
    let consumer_source = r#"
/** @type {import('./types.js').Missing} */
let w;
"#;
    let diagnostics =
        check_consumer_with_js_typedef_source("export {}\n", "consumer.js", consumer_source);
    let hits = ts2694_diagnostics(&diagnostics, "Missing");
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS2694 for an unresolved @type import-type member, got: {diagnostics:?}"
    );
    let member_start = consumer_source.find("Missing").unwrap() as u32;
    assert_eq!(
        hits[0].start, member_start,
        "Expected TS2694 anchored at the `Missing` token inside the comment, got: {:?}",
        hits[0]
    );
}

#[test]
fn jsdoc_type_tag_two_annotations_each_report_once_at_their_own_member_token() {
    // Renamed binders across two independent `@type` tags in the same file:
    // each must report its own TS2694 exactly once, at its own comment's
    // member token — not each other's, and not doubled.
    let consumer_source = r#"
/** @type {import('./types.js').Missing} */
let firstVar;
/** @type {import('./types.js').Missing} */
let secondVar;
"#;
    let diagnostics =
        check_consumer_with_js_typedef_source("export {}\n", "consumer.js", consumer_source);
    let hits = ts2694_diagnostics(&diagnostics, "Missing");
    assert_eq!(
        hits.len(),
        2,
        "Expected exactly two TS2694s, one per @type annotation, got: {diagnostics:?}"
    );
    let first_start = consumer_source.find("Missing").unwrap() as u32;
    let second_start = consumer_source.rfind("Missing").unwrap() as u32;
    let mut starts: Vec<u32> = hits.iter().map(|d| d.start).collect();
    starts.sort_unstable();
    assert_eq!(
        starts,
        vec![first_start, second_start],
        "Expected each TS2694 anchored at its own `Missing` token, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_param_tag_import_member_not_found_reports_once_at_member_token() {
    // Same structural rule as the `@type` case, through the `@param` path.
    // The eager `@param`/`@return` validation scan (`check_jsdoc_typedef_base_types`)
    // must not leak a second TS2694 alongside the authoritative per-parameter
    // resolver (`resolve_jsdoc_param_type_with_pos`), and — the follow-up
    // #17184 left open — the surviving report must anchor at the
    // member-name token inside the comment, matching tsc and the sibling
    // `@type` fix above, not the coarse position it used before.
    let consumer_source = r#"
/** @param {import('./types.js').Missing} x */
function f(x) {}
"#;
    let diagnostics =
        check_consumer_with_js_typedef_source("export {}\n", "consumer.js", consumer_source);
    let hits = ts2694_diagnostics(&diagnostics, "Missing");
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS2694 for an unresolved @param import-type member, got: {diagnostics:?}"
    );
    let member_start = consumer_source.find("Missing").unwrap() as u32;
    assert_eq!(
        hits[0].start, member_start,
        "Expected TS2694 anchored at the `Missing` token inside the comment, got: {:?}",
        hits[0]
    );
}

#[test]
fn jsdoc_param_tag_two_annotations_each_report_once_at_their_own_member_token() {
    // Renamed binders across two independent `@param` tags on two different
    // functions in the same file: each must report its own TS2694 exactly
    // once, at its own comment's member token — not each other's, and not
    // doubled. Mirrors `jsdoc_type_tag_two_annotations_each_report_once_at_their_own_member_token`.
    let consumer_source = r#"
/** @param {import('./types.js').Missing} firstArg */
function firstFn(firstArg) {}
/** @param {import('./types.js').Missing} secondArg */
function secondFn(secondArg) {}
"#;
    let diagnostics =
        check_consumer_with_js_typedef_source("export {}\n", "consumer.js", consumer_source);
    let hits = ts2694_diagnostics(&diagnostics, "Missing");
    assert_eq!(
        hits.len(),
        2,
        "Expected exactly two TS2694s, one per @param annotation, got: {diagnostics:?}"
    );
    let first_start = consumer_source.find("Missing").unwrap() as u32;
    let second_start = consumer_source.rfind("Missing").unwrap() as u32;
    let mut starts: Vec<u32> = hits.iter().map(|d| d.start).collect();
    starts.sort_unstable();
    assert_eq!(
        starts,
        vec![first_start, second_start],
        "Expected each TS2694 anchored at its own `Missing` token, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_param_tag_optional_bracket_name_import_member_not_found_anchors_at_member_token() {
    // Adjacent case: the optional-parameter bracket form (`[name]`) uses a
    // different literal source-text shape than the plain form — the member-
    // token search must still find it via the `optional` fallback pattern.
    let consumer_source = r#"
/** @param {import('./types.js').Missing} [x] */
function f(x) {}
"#;
    let diagnostics =
        check_consumer_with_js_typedef_source("export {}\n", "consumer.js", consumer_source);
    let hits = ts2694_diagnostics(&diagnostics, "Missing");
    assert_eq!(
        hits.len(),
        1,
        "Expected exactly one TS2694 for an unresolved optional @param import-type member, got: {diagnostics:?}"
    );
    let member_start = consumer_source.find("Missing").unwrap() as u32;
    assert_eq!(
        hits[0].start, member_start,
        "Expected TS2694 anchored at the `Missing` token inside the comment, got: {:?}",
        hits[0]
    );
}

// ---------------------------------------------------------------------------
// `@callback` names followed by description text — #17162 residual
// (`conformance/jsdoc/callbackCrossModule.ts`).
//
// tsc takes a `@callback` tag's name as the first whitespace-delimited token
// and treats everything after it as description text, so
// `@callback Con - some kind of continuation` declares `Con` as a type-only
// exported member of the module. tsz's typedef surface parser took the whole
// line remainder as the name, rejected it as a non-identifier, and never
// registered the callback — making `import('./mod').Con` a spurious TS2694.
// A token that itself carries a non-name character (`Con-`) declares nothing
// in tsc (oracle-verified on 7.0.2), and must keep reporting TS2694.
// ---------------------------------------------------------------------------

#[test]
fn jsdoc_callback_with_dash_description_import_type_member_suppresses_ts2694() {
    // The `callbackCrossModule.ts` shape: name, dash, free-text description,
    // then `@param`/`@return` lines, consumed cross-file via a JSDoc `@param`.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Con - some kind of continuation
 * @param {object | undefined} error
 * @return {any} I don't even know what this should return
 */
module.exports = C
function C() {
    this.p = 1
}
"#,
        "consumer.js",
        r#"
/** @param {import('./types.js').Con} k */
function f(k) {
    return k({ ok: true })
}
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Con").is_empty(),
        "Expected no TS2694 for a `@callback Con - description` export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_callback_with_plain_description_renamed_binder_suppresses_ts2694() {
    // Adjacent case: description without a leading dash, differently named
    // binder — the name rule is "first whitespace token", not "text before a
    // dash", and must not be keyed to the fixture's identifier.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Kont9$ fires when the frobnicator settles
 * @param {number} x
 * @return {string}
 */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Kont9$} */
var cb
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Kont9$").is_empty(),
        "Expected no TS2694 for a `@callback Kont9$ description` export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_callback_dotted_name_with_description_suppresses_ts2694() {
    // Adjacent case: a dotted (qualified) callback name followed by a
    // description — the first-token rule composes with the qualified-name
    // support, so the head segment must not be reported missing.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Outer3.Inner - nested continuation
 * @param {number} x
 * @return {string}
 */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Outer3.Inner} */
var cb
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Outer3").is_empty(),
        "Expected no TS2694 for a dotted `@callback Outer3.Inner - description` export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_callback_bare_name_still_resolves() {
    // Guard: a `@callback` with no description (the previously-working shape)
    // keeps resolving.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Plain
 * @param {number} x
 * @return {string}
 */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Plain} */
var cb
"#,
    );
    assert!(
        ts2694_diagnostics(&diagnostics, "Plain").is_empty(),
        "Expected no TS2694 for a bare `@callback Plain` export, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_callback_name_with_attached_dash_still_reports_ts2694() {
    // Negative control (oracle-pinned): `@callback Con- attached-dash text`
    // declares nothing in tsc — the first token `Con-` is not a valid name and
    // there is no fallback to its identifier prefix. Referencing `Con` must
    // keep reporting TS2694.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Con- attached-dash description
 * @param {number} x
 * @return {string}
 */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Con} */
var cb
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "Con").is_empty(),
        "Expected TS2694 when the `@callback` name token carries an attached dash, got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_callback_with_description_does_not_swallow_other_missing_members() {
    // Negative control: registering the described callback must not make other
    // genuinely-missing members resolve.
    let diagnostics = check_consumer_with_js_typedef_source(
        r#"
/** @callback Con - some kind of continuation
 * @param {number} x
 */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./types.js').Missing} */
var bad
"#,
    );
    assert!(
        !ts2694_diagnostics(&diagnostics, "Missing").is_empty(),
        "Expected TS2694 for a missing member alongside a described callback, got: {diagnostics:?}"
    );
}
