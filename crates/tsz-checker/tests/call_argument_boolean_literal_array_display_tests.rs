//! Regression tests for boolean-literal array source display in TS2345
//! argument-not-assignable messages.
//!
//! When a fresh boolean-literal array (`true[]` / `false[]`) is the source of a
//! call-argument mismatch against a `boolean` parameter, `tsc` widens the source
//! display to `boolean[]`. tsz decides this from the array element `TypeId`
//! structure (see `query_boundaries::common::boolean_literal_array_display_type`)
//! rather than pattern-matching the rendered `"true[]"` / `"false[]"` text, per
//! the §25 anti-hardcoding rule. These tests lock the rendered output.

use tsz_checker::test_utils::check_source_code_messages as check;

/// Compile `source` and return the single TS2345 message, asserting exactly one
/// is produced.
fn single_ts2345(source: &str) -> String {
    let mut messages: Vec<String> = check(source)
        .into_iter()
        .filter(|(code, _)| *code == 2345)
        .map(|(_, message)| message)
        .collect();
    assert_eq!(messages.len(), 1, "expected one TS2345, got: {messages:?}");
    messages.remove(0)
}

/// `true[]` against a `boolean` parameter widens the source display to `boolean[]`.
#[test]
fn true_array_argument_against_boolean_param_widens_to_boolean_array() {
    let message =
        single_ts2345("declare const a: true[]; declare function f(x: boolean): void; f(a);");
    assert!(
        message.contains("Argument of type 'boolean[]'") && !message.contains("true[]"),
        "source should widen true[] to boolean[], got: {message}"
    );
}

/// Independence from the literal value: `false[]` behaves identically to `true[]`.
#[test]
fn false_array_argument_against_boolean_param_widens_to_boolean_array() {
    let message =
        single_ts2345("declare const a: false[]; declare function f(x: boolean): void; f(a);");
    assert!(
        message.contains("Argument of type 'boolean[]'"),
        "source should widen false[] to boolean[], got: {message}"
    );
}

/// The widening is gated on a `boolean` parameter: against a non-boolean
/// parameter the literal source display is preserved (matching prior behavior).
#[test]
fn true_array_argument_against_non_boolean_param_keeps_literal_display() {
    let message =
        single_ts2345("declare const a: true[]; declare function f(x: string): void; f(a);");
    assert!(
        message.contains("Argument of type 'true[]'"),
        "non-boolean target should keep the literal source display, got: {message}"
    );
}
