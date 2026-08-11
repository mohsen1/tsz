//! Tests for the TS-syntax `type X = import("./mod").A.B` path of qualified
//! JSDoc `@typedef`/`@callback` names, and for the prefix-qualified TS2694
//! display both import-type resolvers share.
//!
//! Structural rules (oracle: typescript@7.0.2):
//! - A dotted JSDoc `@typedef {T} A.B` declares a *qualified* name. The
//!   TS-syntax import-type resolver joins its already-parsed member segments
//!   back into that qualified name before the typedef-surface lookup — the
//!   JSDoc string path's equivalent landed in #17178.
//! - When a dotted reference still fails, tsc reports the first segment
//!   missing under the longest declared namespace prefix
//!   (`Namespace '"m".Dotted' has no exported member 'Missing'`); a
//!   reference whose first segment matches no declared prefix keeps the
//!   unqualified root-segment form.
//!
//! Complements `jsdoc_typedef_module_export_tests.rs` (#17178/#17180), which
//! owns the JSDoc string-path positive/negative matrix.

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
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());

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
fn ts_import_type_alias_resolves_dotted_typedef_qualified_name() {
    // TS-syntax path, renamed binders and a deeper chain: the segment-joined
    // typedef lookup must not depend on chain length 2 or specific names.
    let diagnostics = check_consumer_with_module_source(
        "shapes.js",
        r#"
/** @typedef {string} Outer.Inner.Leaf */
export var marker = 1
"#,
        "consumer.ts",
        r#"
type X = import('./shapes').Outer.Inner.Leaf;
declare const value: X;
const s: string = value;
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for a TS-syntax import-type reference to a \
         deeply dotted `@typedef`, got: {diagnostics:?}"
    );
}

#[test]
fn ts_import_type_alias_dotted_typedef_carries_real_type_not_any() {
    // The resolved qualified typedef must be the declared base type, not a
    // silent `any`: assigning it to a mismatched annotation errors.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
/** @typedef {number} Dotted.Name */
export var dummy = 1
"#,
        "consumer.ts",
        r#"
type X = import('./mod1').Dotted.Name;
declare const value: X;
const bad: string = value;
"#,
    );
    assert!(
        !diagnostics_with_code(&diagnostics, 2322).is_empty(),
        "Expected TS2322 assigning the dotted typedef's number type to a \
         string annotation, got: {diagnostics:?}"
    );
    assert!(
        diagnostics_with_code(&diagnostics, 2694).is_empty(),
        "Expected no TS2694 for a resolvable dotted typedef, got: {diagnostics:?}"
    );
}

#[test]
fn ts_import_type_alias_dotted_missing_member_reports_qualified_ts2694() {
    // Negative, TS-syntax path: the chain fails on the segment *under* the
    // synthesized namespace — tsc qualifies the namespace display with the
    // prefix and names the failing segment, not the root.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
/** @typedef {number} Dotted.Name */
export var dummy = 1
"#,
        "consumer.ts",
        r#"
type X = import('./mod1').Dotted.Missing;
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert!(
        !ts2694.is_empty(),
        "Expected TS2694 for a missing member under a dotted typedef \
         namespace, got: {diagnostics:?}"
    );
    assert!(
        ts2694.iter().all(|d| d
            .message_text
            .contains(".Dotted' has no exported member 'Missing'")),
        "Expected every TS2694 to be qualified with the namespace prefix and \
         name the failing segment, got: {ts2694:?}"
    );
}

#[test]
fn jsdoc_type_dotted_missing_member_reports_qualified_ts2694() {
    // Same qualification rule on the JSDoc string path.
    // NOTE: the JSDoc `@type` path currently reports the same TS2694 twice
    // (comment anchor + declaration anchor) where tsc reports once; that
    // count defect predates this family, so only the message shape is
    // pinned here.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
/** @typedef {number} Dotted.Name */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./mod1').Dotted.Missing} */
var x
export var y = 1
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert!(
        !ts2694.is_empty(),
        "Expected TS2694 for a missing member under a dotted typedef \
         namespace, got: {diagnostics:?}"
    );
    assert!(
        ts2694.iter().all(|d| d
            .message_text
            .contains(".Dotted' has no exported member 'Missing'")),
        "Expected every TS2694 to be qualified with the namespace prefix and \
         name the failing segment, got: {ts2694:?}"
    );
}

#[test]
fn jsdoc_type_dotted_reference_without_declared_prefix_reports_root_ts2694() {
    // Negative control for the prefix query: no declared name starts with
    // the reference's first segment, so the diagnostic stays unqualified
    // and names the root.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
/** @typedef {number} Real */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./mod1').No.Such} */
var x
export var y = 1
"#,
    );
    let ts2694 = diagnostics_with_code(&diagnostics, 2694);
    assert!(
        !ts2694.is_empty(),
        "Expected TS2694 for a dotted reference with no declared prefix, \
         got: {diagnostics:?}"
    );
    assert!(
        ts2694
            .iter()
            .all(|d| d.message_text.contains("has no exported member 'No'")),
        "Expected the TS2694 to name the unresolved root segment, got: {ts2694:?}"
    );
    assert!(
        ts2694.iter().all(|d| !d.message_text.contains(".No'")),
        "Expected the namespace display to stay unqualified, got: {ts2694:?}"
    );
}

#[test]
fn jsdoc_type_namespace_prefix_alone_still_reports_ts2694() {
    // Negative control: referencing only the synthesized namespace prefix is
    // still TS2694 in tsc (`Namespace '"mod1"' has no exported member
    // 'Dotted'`) — the prefix is not itself a type, and the prefix-existence
    // query must not resolve it.
    let diagnostics = check_consumer_with_module_source(
        "mod1.js",
        r#"
/** @typedef {number} Dotted.Name */
export var dummy = 1
"#,
        "consumer.js",
        r#"
/** @type {import('./mod1').Dotted} */
var x
export var y = 1
"#,
    );
    assert!(
        !diagnostics_with_code(&diagnostics, 2694).is_empty(),
        "Expected TS2694 when referencing only the synthesized namespace \
         prefix, got: {diagnostics:?}"
    );
}
