//! `tsc`'s `elaborateArrowFunction` only drills into an arrow/function-expression
//! body's return expression when **no** parameter carries an explicit type
//! annotation (`some(node.parameters, hasType)` makes the elaborator bail). When
//! a parameter is annotated — even with a type that matches the contextual
//! parameter — the assignability failure is reported at the function-type level
//! (the parameter-contravariance frame, anchored at the property / argument),
//! not on the body return expression.
//!
//! Structural rule: when an object-literal property value (or call argument
//! reached through an object literal) is an expression-bodied arrow/function
//! whose body return also mismatches, tsz used to anchor the TS2322 at the body
//! expression and drop the function-type frame whenever the body mismatched,
//! regardless of parameter annotations. The direct-callback-argument path
//! already gated this on annotated parameters; the object-literal-property path
//! did not. Both now share `function_value_has_explicit_param_annotation`.
//!
//! Refs the diagnostic-parity (`hold`) goal. Binder names are varied across the
//! cases so no identifier is load-bearing.

use crate::test_utils::check_source;
use tsz_common::options::checker::CheckerOptions;

fn ts2322(source: &str) -> Vec<(u32, String)> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", opts)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn assert_function_type_frame(messages: &[(u32, String)], src_fn: &str, tgt_fn: &str) {
    assert!(
        messages
            .iter()
            .any(|(_, m)| m.contains(src_fn) && m.contains(tgt_fn)),
        "expected the function-type frame `{src_fn}` ≰ `{tgt_fn}`, got: {messages:?}"
    );
    // The body-anchored bare-return form would be the *only* message and would
    // not mention the function types at all.
    assert!(
        messages.iter().any(|(_, m)| m.contains("=>")),
        "expected a function-type display in the frame, got: {messages:?}"
    );
}

// --- object-literal property, conflicting annotated param + wrong body ---

#[test]
fn property_arrow_annotated_param_conflict_reports_function_frame() {
    // `(x: string) => 42` against `(x: number) => string`: both the parameter and
    // the return mismatch. tsc reports the function-type frame (parameter
    // contravariance), not the body return `42`.
    let messages = ts2322(
        r#"
interface Sink { cb: (x: number) => string; }
const sink: Sink = { cb: (x: string) => 42 };
"#,
    );
    assert_function_type_frame(&messages, "(x: string) => number", "(x: number) => string");
}

// --- object-literal property, annotated param that MATCHES + wrong body ---

#[test]
fn property_arrow_annotated_matching_param_still_reports_function_frame() {
    // `(value: number) => 42` against `(value: number) => string`: the parameter
    // matches, but because it is *annotated* tsc still reports the function-type
    // frame (with a nested return mismatch), not the body return.
    let messages = ts2322(
        r#"
interface Box { run: (value: number) => string; }
const box: Box = { run: (value: number) => 42 };
"#,
    );
    assert_function_type_frame(
        &messages,
        "(value: number) => number",
        "(value: number) => string",
    );
}

// --- two params, only one annotated (some(parameters, hasType)) ---

#[test]
fn property_arrow_one_annotated_param_reports_function_frame() {
    // Only the first parameter is annotated; `some(parameters, hasType)` is true,
    // so tsc bails the body drill and reports the function-type frame.
    let messages = ts2322(
        r#"
interface Pair { combine: (first: number, second: number) => string; }
const pair: Pair = { combine: (first: string, second) => 7 };
"#,
    );
    assert_function_type_frame(
        &messages,
        "(first: string, second: number) => number",
        "(first: number, second: number) => string",
    );
}

// --- call argument reached through an object literal ---

#[test]
fn object_literal_argument_arrow_annotated_param_reports_function_frame() {
    // The object literal is a call argument; the same gate applies on the
    // argument path.
    let messages = ts2322(
        r#"
declare function consume(opts: { handler: (n: number) => string }): void;
consume({ handler: (n: string) => 99 });
"#,
    );
    assert_function_type_frame(&messages, "(n: string) => number", "(n: number) => string");
}

// --- control: NO parameter annotation -> body drill is preserved ---

#[test]
fn property_arrow_unannotated_param_still_drills_into_body() {
    // No parameter annotation: the arrow is fully contextually typed, so tsc
    // (and tsz) anchor the TS2322 at the body return `1` with the bare
    // `number ≰ string` message — the function-type frame must NOT appear.
    let messages = ts2322(
        r#"
interface Sink { cb: (x: number) => string; }
const sink: Sink = { cb: (x) => 1 };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|(_, m)| m == "Type 'number' is not assignable to type 'string'."),
        "expected body-anchored bare return mismatch, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|(_, m)| m.contains("=>")),
        "unannotated-param arrow must drill into the body, not show a function-type frame: {messages:?}"
    );
}

// --- control: block body already bails (isBlock) regardless of annotation ---

#[test]
fn property_arrow_block_body_reports_function_frame() {
    let messages = ts2322(
        r#"
interface Sink { cb: (x: number) => string; }
const sink: Sink = { cb: (x: string) => { return 5; } };
"#,
    );
    assert_function_type_frame(&messages, "(x: string) => number", "(x: number) => string");
}

// --- control: correct callbacks produce no error ---

#[test]
fn property_arrow_compatible_callbacks_are_clean() {
    let messages = ts2322(
        r#"
interface Sink { cb: (x: number) => string; }
const a: Sink = { cb: (x) => String(x) };
const b: Sink = { cb: (x: number) => "ok" };
"#,
    );
    assert!(
        messages.is_empty(),
        "compatible callbacks must not error, got: {messages:?}"
    );
}
