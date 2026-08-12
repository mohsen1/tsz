//! Display optionality of JavaScript function parameters (issue #17227).
//!
//! `tsc` models an unannotated parameter of an untyped JS function with two
//! independent signals: its `minArgumentCount` is relaxed (call-arity leniency,
//! covered by `js_file_function_parameters_as_optional_tests`), but it is never
//! `isOptionalParameter`, so every *display* surface renders it as required
//! (`x: any`, not `x?: any`). Only genuine optionality — a `?` token (illegal in
//! JS), an initializer, or a JSDoc `[bracket]`/`=`-suffix parameter — shows the
//! `?`. These tests pin that split on the diagnostic-message surface, where the
//! divergence was observed (`conformance/salsa/moduleExportAssignment2.ts`).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;
use tsz_common::common::ModuleKind;

/// Return the `TS2339` "does not exist" message for a checked JS source, which
/// renders the offending function type — the surface where the bug appeared.
fn ts2339_message(source: &str) -> String {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    check_source(source, "a.js", options)
        .into_iter()
        .find(|d| d.code == 2339)
        .map(|d| d.message_text)
        .unwrap_or_default()
}

// --- A bare, unannotated JS parameter renders as required. ---

#[test]
fn bare_js_parameter_renders_required_not_optional() {
    let msg = ts2339_message("function f(tree) {}\nf.missing;\n");
    assert!(
        msg.contains("(tree: any) => void"),
        "expected required-parameter render, got: {msg:?}"
    );
    assert!(
        !msg.contains("tree?"),
        "bare JS parameter must not render a `?` marker, got: {msg:?}"
    );
}

#[test]
fn bare_js_parameter_renders_required_renamed_binder() {
    // The fix is structural, not keyed on `f`/`tree`.
    let msg = ts2339_message("function greet(node) {}\ngreet.missing;\n");
    assert!(
        msg.contains("(node: any) => void"),
        "expected required-parameter render, got: {msg:?}"
    );
    assert!(!msg.contains("node?"), "got: {msg:?}");
}

#[test]
fn multiple_bare_js_parameters_all_render_required() {
    let msg = ts2339_message("function f(a, b) {}\nf.missing;\n");
    assert!(
        msg.contains("(a: any, b: any) => void"),
        "expected both parameters required, got: {msg:?}"
    );
    assert!(
        !msg.contains('?'),
        "no parameter should render `?`, got: {msg:?}"
    );
}

// --- Genuine optionality must still render the `?` marker. ---

#[test]
fn jsdoc_bracket_optional_parameter_keeps_question_mark() {
    // `@param {number} [a]` is genuinely optional and must keep the `?`.
    let msg = ts2339_message("/** @param {number} [a] */\nfunction f(a) {}\nf.missing;\n");
    assert!(
        msg.contains("a?:"),
        "JSDoc bracket-optional parameter must keep `?`, got: {msg:?}"
    );
}

#[test]
fn initializer_parameter_keeps_question_mark() {
    // A default value makes the parameter genuinely optional; `?` is shown.
    let msg = ts2339_message("function f(a = 1) {}\nf.missing;\n");
    assert!(
        msg.contains("a?:"),
        "initializer parameter must keep `?`, got: {msg:?}"
    );
}
