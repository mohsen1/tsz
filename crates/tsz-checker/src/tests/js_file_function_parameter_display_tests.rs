//! Display parity for JS-inferred function parameters (#17227).
//!
//! Structural rule: in a checked JS file a bare, unannotated parameter is
//! optional for *weak call-arity* (tsc's `minArgumentCount`) but `tsc` still
//! *renders* it as required — `(tree: any) => void`, never `(tree?: any)`.
//! The two signals are independent: `ParamInfo::optional` drives arity and
//! subtyping, while `ParamInfo::suppress_display_optional` (set only for the
//! bare-JS case) makes the printer render the parameter required via
//! `displays_optional()`. A genuine `?`, an initializer, or a JSDoc
//! bracket/`=`-optional tag must still print the `?`.
//!
//! Oracle: `typescript@7.0.2`, `--allowJs --checkJs`. The message anchor is the
//! TS2339 property-miss whose text embeds the full rendered signature.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

const PROPERTY_DOES_NOT_EXIST: u32 = 2339;

/// Collect the TS2339 message texts produced for a single `.js` source.
fn js_2339_messages(source: &str) -> Vec<String> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "a.js", options)
        .into_iter()
        .filter(|d| d.code == PROPERTY_DOES_NOT_EXIST)
        .map(|d| d.message_text)
        .collect()
}

/// Collect the TS2339 message texts produced for a single `.ts` source.
fn ts_2339_messages(source: &str) -> Vec<String> {
    check_source(source, "a.ts", CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code == PROPERTY_DOES_NOT_EXIST)
        .map(|d| d.message_text)
        .collect()
}

fn assert_renders_required(messages: &[String], required: &str, optional: &str) {
    assert!(
        messages.iter().any(|m| m.contains(required)),
        "expected a rendered signature containing {required:?}, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains(optional)),
        "the spurious optional marker {optional:?} must not appear, got: {messages:?}"
    );
}

#[test]
fn bare_js_parameter_renders_required() {
    let messages = js_2339_messages("function f(tree) { }\nf.missing;\n");
    assert_renders_required(&messages, "tree: any", "tree?: any");
}

#[test]
fn multiple_bare_js_parameters_all_render_required() {
    let messages = js_2339_messages("function f(a, b) { }\nf.missing;\n");
    assert!(
        messages.iter().any(|m| m.contains("(a: any, b: any)")),
        "both bare params should render required, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains('?')),
        "no bare param should carry an optional marker, got: {messages:?}"
    );
}

// The binder names must not drive the rule: rename the function and the param.
#[test]
fn bare_js_parameter_renders_required_renamed_binder() {
    let messages = js_2339_messages("function handler(node) { }\nhandler.missing;\n");
    assert_renders_required(&messages, "node: any", "node?: any");
}

// --- Controls: a genuinely optional parameter must still print `?`. ---

#[test]
fn jsdoc_bracket_optional_parameter_keeps_marker() {
    let messages =
        js_2339_messages("/**\n * @param {number} [a]\n */\nfunction f(a) { }\nf.missing;\n");
    assert!(
        messages.iter().any(|m| m.contains("a?:")),
        "a JSDoc bracket-optional param must keep its `?`, got: {messages:?}"
    );
}

#[test]
fn initializer_parameter_keeps_marker() {
    let messages = js_2339_messages("function f(a = 1) { }\nf.missing;\n");
    assert!(
        messages.iter().any(|m| m.contains("a?:")),
        "a defaulted param is optional and must keep its `?`, got: {messages:?}"
    );
}

// --- Control: a JSDoc-typed required param renders required (already correct). ---

#[test]
fn jsdoc_typed_required_parameter_renders_required() {
    let messages =
        js_2339_messages("/**\n * @param {number} a\n */\nfunction f(a) { }\nf.missing;\n");
    assert_renders_required(&messages, "a: number", "a?:");
}

// --- Control: the TS path was never wrong; guard it against regression. ---

#[test]
fn ts_annotated_parameter_renders_required() {
    let messages = ts_2339_messages("function g(tree: any) { }\n(g as any).x;\ng.missing;\n");
    assert_renders_required(&messages, "tree: any", "tree?: any");
}
