//! Tests for JSDoc `import("./mod").A.B[.C…]` references whose head is not
//! a declared dotted `@typedef` (see `jsdoc_dotted_typedef_import_type_tests`
//! for that family).
//!
//! Structural rule (oracle: typescript@7.0.2): a qualified reference
//! requires every segment but the last to resolve in *namespace* meaning —
//! a class/interface/type-alias/enum export is not eligible as a further
//! qualifier. `import("./mod").SomeClass.Member` fails at `SomeClass`
//! itself (`Namespace '"mod"' has no exported member 'SomeClass'`), not at
//! `Member`, even though `SomeClass` is a real, resolvable export. A
//! genuine namespace (from an ambient `.d.ts`) keeps resolving through its
//! nested members, and a failure past a resolved namespace prefix qualifies
//! the namespace display with the segments that did resolve (`Namespace
//! '"mod".NS' has no exported member 'Missing'`). Covers #17181.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Register `sym_id` and every symbol transitively reachable through its
/// `exports`/`members` tables as belonging to `file_idx`. The raw
/// parser+binder test harness below binds each file in isolation and only
/// wires up the target module's *top-level* exports through
/// `resolve_cross_file_export_from_file_with_mode`'s own registration; a
/// nested namespace member (`NS.Foo`) needs its own registration too, which
/// the full checker pipeline's program-wide binding provides but this
/// two-file harness does not.
fn register_transitive_exports(
    binder: &BinderState,
    sym_id: SymbolId,
    file_idx: usize,
    checker: &mut CheckerState,
    seen: &mut FxHashSet<SymbolId>,
) {
    if !seen.insert(sym_id) {
        return;
    }
    checker.ctx.register_symbol_file_target(sym_id, file_idx);
    let Some(symbol) = binder.get_symbol(sym_id) else {
        return;
    };
    if let Some(exports) = &symbol.exports {
        for (_, &child) in exports.iter() {
            register_transitive_exports(binder, child, file_idx, checker, seen);
        }
    }
    if let Some(members) = &symbol.members {
        for (_, &child) in members.iter() {
            register_transitive_exports(binder, child, file_idx, checker, seen);
        }
    }
}

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

fn check_consumer_with_module_source(
    module_name: &str,
    module_source: &str,
    consumer_name: &str,
    consumer_source: &str,
) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut parser_module = ParserState::new(module_name.to_string(), module_source.to_string());
    let root_module = parser_module.parse_source_file();
    let mut binder_module = BinderState::new();
    binder_module.bind_source_file(parser_module.get_arena(), root_module);

    let mut parser_consumer =
        ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_consumer = parser_consumer.parse_source_file();
    let mut binder_consumer = BinderState::new();
    binder_consumer.bind_source_file(parser_consumer.get_arena(), root_consumer);

    let arena_module = Arc::new(parser_module.get_arena().clone());
    let arena_consumer = Arc::new(parser_consumer.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_module), Arc::clone(&arena_consumer)]);

    let binder_module = Arc::new(binder_module);
    let binder_consumer = Arc::new(binder_consumer);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_module),
        Arc::clone(&binder_consumer),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_consumer.as_ref(),
        binder_consumer.as_ref(),
        &types,
        consumer_name.to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());

    if let Some(module_exports) = binder_module.module_exports.get(module_name) {
        let mut seen = FxHashSet::default();
        for (_, &sym_id) in module_exports.iter() {
            register_transitive_exports(&all_binders[0], sym_id, 0, &mut checker, &mut seen);
        }
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    for specifier in local_module_specifiers(module_name) {
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

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == code).collect()
}

#[test]
fn jsdoc_import_type_dotted_member_through_class_reports_ts2694_on_head() {
    // The #17181 repro: `C` is a class (CommonJS expando export), not a
    // namespace, so `import(...).C.Inner` must fail at `C` itself.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
class C {
    s() { }
}
module.exports.C = C
"#,
        "c.js",
        r#"
/** @typedef {import('./mod1').C.Inner} Y */
/** @param {Y} c */
function demo3(c) { c }
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert_eq!(
        ts2694.len(),
        1,
        "Expected exactly one TS2694 for a dotted member path through a class, got: {diagnostics:?}"
    );
    assert!(
        ts2694[0]
            .message_text
            .contains("has no exported member 'C'"),
        "Expected TS2694 to blame the class head 'C', not 'Inner', got: {:?}",
        ts2694[0].message_text
    );
    assert!(
        !ts2694[0].message_text.contains(".C'"),
        "Expected the namespace display to stay unqualified (no '.C' prefix), got: {:?}",
        ts2694[0].message_text
    );
}

#[test]
fn jsdoc_import_type_dotted_member_through_es_class_reports_ts2694_on_head() {
    // Same shape via a real ES export (not a CommonJS expando), renamed
    // binder — the class-not-namespace gate must not depend on the export
    // mechanism or the identifier.
    let diagnostics = check_consumer_with_module_source(
        "widgets.js",
        r#"
export class Widget {
    render() {}
}
"#,
        "consumer.js",
        r#"
/** @typedef {import('./widgets').Widget.Sub} Y */
/** @param {Y} c */
function use(c) { c }
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert_eq!(
        ts2694.len(),
        1,
        "Expected exactly one TS2694, got: {diagnostics:?}"
    );
    assert!(
        ts2694[0]
            .message_text
            .contains("has no exported member 'Widget'"),
        "Expected TS2694 to blame 'Widget', got: {:?}",
        ts2694[0].message_text
    );
}

#[test]
fn jsdoc_import_type_dotted_member_through_genuine_namespace_resolves() {
    // Positive control: a real ambient namespace keeps resolving through
    // its nested member.
    let diagnostics = check_consumer_with_module_source(
        "mod2.d.ts",
        r#"
export namespace NS {
    export interface Foo { x: number }
}
"#,
        "consumer.js",
        r#"
/** @typedef {import('./mod2').NS.Foo} Y */
/** @param {Y} c */
function demo(c) { c.x }
"#,
    );
    assert!(
        diagnostics_with_code(&diagnostics, 2694).is_empty(),
        "Expected no TS2694 for a dotted member path through a genuine namespace, \
         got: {diagnostics:?}"
    );
}

#[test]
fn jsdoc_import_type_dotted_member_missing_past_genuine_namespace_qualifies_display() {
    // Negative control on the genuine-namespace path: a failure past a
    // resolved namespace prefix qualifies the namespace display with the
    // segments that did resolve, unlike the class-head case above.
    let diagnostics = check_consumer_with_module_source(
        "mod3.d.ts",
        r#"
export namespace NS {
    export interface Foo { x: number }
}
"#,
        "consumer.js",
        r#"
/** @typedef {import('./mod3').NS.Missing} Z */
/** @param {Z} c */
function demo(c) { c }
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert_eq!(
        ts2694.len(),
        1,
        "Expected exactly one TS2694, got: {diagnostics:?}"
    );
    assert!(
        ts2694[0]
            .message_text
            .contains(".NS' has no exported member 'Missing'"),
        "Expected the namespace display to be qualified with the resolved 'NS' \
         prefix, got: {:?}",
        ts2694[0].message_text
    );
}
