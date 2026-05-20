//! Issue #3137: nested JSDoc generic type arguments must report the
//! deepest unresolved identifier (TS2304), not the outer generic
//! application as a whole. Mirrors tsc's behavior:
//!
//! ```text
//! /** @typedef {Record<string, Array<Missing>>} T */
//! ```
//!
//! tsc reports `Cannot find name 'Missing'.`; tsz historically reported
//! `Cannot find name 'Array<Missing>'.` because the `@typedef` body
//! validator only peeled one generic layer.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_js(js_source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..Default::default()
    };

    let mut parser = ParserState::new("nested.js".to_string(), js_source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = Arc::new(parser.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena)]);
    let binder = Arc::new(binder);
    let all_binders = Arc::new(vec![Arc::clone(&binder)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "nested.js".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker.ctx.diagnostics
}

/// `@typedef {Outer<Inner<Missing>>} T` — the missing identifier is
/// nested two generic levels deep. tsc reports `'Missing'`, not
/// `'Inner<Missing>'`.
#[test]
fn nested_generic_missing_name_reports_inner_identifier() {
    let diagnostics = check_js(
        r#"
export {};
/**
 * @template I
 * @typedef {{ inner: I }} Inner
 */
/**
 * @template O
 * @typedef {{ outer: O }} Outer
 */
/** @typedef {Outer<Inner<Missing>>} T */
"#,
    );

    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    let messages: Vec<&str> = ts2304.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'Missing'")),
        "expected TS2304 for inner 'Missing', got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Inner<")),
        "must not emit TS2304 with the whole nested generic application as the name, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Outer<")),
        "must not emit TS2304 with the outer generic application as the name, got: {messages:?}"
    );
}

/// Multi-argument outer with two distinct unresolved identifiers nested
/// inside separate inner generics — both must be reported, mirroring
/// tsc's per-identifier diagnostic surface.
#[test]
fn nested_generic_reports_each_missing_identifier_in_separate_args() {
    let diagnostics = check_js(
        r#"
export {};
/**
 * @template I
 * @typedef {{ inner: I }} Inner
 */
/**
 * @template A, B
 * @typedef {{ a: A, b: B }} Pair
 */
/** @typedef {Pair<Inner<MissingA>, Inner<MissingB>>} T */
"#,
    );

    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    let messages: Vec<&str> = ts2304.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'MissingA'")),
        "expected TS2304 for 'MissingA', got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("'MissingB'")),
        "expected TS2304 for 'MissingB', got: {messages:?}"
    );
}

/// Sanity: when every nested identifier resolves to a local typedef,
/// no TS2304 is emitted. Guards against the recursive walk
/// over-triggering on resolved names.
#[test]
fn nested_generic_with_all_resolved_args_emits_no_ts2304() {
    let diagnostics = check_js(
        r#"
export {};
/** @typedef {{ a: number }} Resolved */
/**
 * @template I
 * @typedef {{ inner: I }} Inner
 */
/**
 * @template O
 * @typedef {{ outer: O }} Outer
 */
/** @typedef {Outer<Inner<Resolved>>} T */
"#,
    );

    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304.is_empty(),
        "expected no TS2304 when nested args resolve, got: {ts2304:?}"
    );
}

/// Top-level `@typedef {Inner<Missing>}` (the simpler, one-level form
/// from the issue) must continue to emit TS2304 for `Missing`. Anchor
/// to lock the existing one-level behavior alongside the new
/// nested-recursion fix.
#[test]
fn one_level_generic_missing_name_still_reports_inner() {
    let diagnostics = check_js(
        r#"
export {};
/**
 * @template I
 * @typedef {{ inner: I }} Inner
 */
/** @typedef {Inner<Missing>} T */
"#,
    );

    let ts2304: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    let messages: Vec<&str> = ts2304.iter().map(|d| d.message_text.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'Missing'")),
        "expected TS2304 for 'Missing', got: {messages:?}"
    );
}
