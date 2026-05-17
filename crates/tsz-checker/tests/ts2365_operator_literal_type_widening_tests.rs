//! Tests for TS2365 operator error message display when parameter type annotations
//! are numeric literal types.
//!
//! Structural rule: When a function parameter has a numeric literal type annotation
//! (e.g., `two: 2`), the TS2365 error message must display the widened primitive
//! type (`'number'`), NOT the raw literal annotation text (`'2'`). The
//! `operator_type_parameter_annotation_text_for_expression` helper must only
//! treat annotation text as a type parameter name when the text begins with a
//! letter, `_`, or `$` — the valid identifier-start characters for TypeScript
//! type parameter names.
//!
//! Adjacent cases verified:
//! 1. Single-digit numeric literal annotation (`two: 2`) — must widen to `'number'`.
//! 2. Multi-digit numeric literal annotation (`forty_two: 42`) — must widen to `'number'`.
//! 3. Canonical 7-error case from `compiler/relationalOperatorComparable.ts`.
//!
//! Related conformance test: `compiler/relationalOperatorComparable.ts`.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2365_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, msg)| (code == 2365).then_some(msg))
        .collect()
}

// =========================================================================
// Numeric literal type annotations — must be widened to primitive
// =========================================================================

/// Repro: `function f(two: 2) { false < two }` was producing
/// "Operator '<' cannot be applied to types 'boolean' and '2'"
/// instead of the correctly widened "... and 'number'".
#[test]
fn ts2365_single_digit_literal_annotation_widens_to_number() {
    let src = r#"
function f(two: 2) {
    false < two;
}
"#;
    let msgs = ts2365_messages(src);
    assert!(
        !msgs.is_empty(),
        "Expected TS2365 for `false < two` where `two: 2`; got none"
    );
    for msg in &msgs {
        assert!(
            msg.contains("'number'"),
            "TS2365 message should contain 'number' (widened), not the raw literal; got: {msg}"
        );
        assert!(
            !msg.contains("'2'"),
            "TS2365 message must NOT contain raw literal '2'; got: {msg}"
        );
    }
}

/// Variation with a multi-digit numeric literal annotation (`42`).
/// The rule must hold regardless of how many digits the literal has.
#[test]
fn ts2365_multi_digit_literal_annotation_widens_to_number() {
    let src = r#"
function g(forty_two: 42) {
    false < forty_two;
}
"#;
    let msgs = ts2365_messages(src);
    assert!(
        !msgs.is_empty(),
        "Expected TS2365 for `false < forty_two` where `forty_two: 42`; got none"
    );
    for msg in &msgs {
        assert!(
            msg.contains("'number'"),
            "TS2365 message should contain 'number' (widened); got: {msg}"
        );
        assert!(
            !msg.contains("'42'"),
            "TS2365 message must NOT contain raw literal '42'; got: {msg}"
        );
    }
}

/// Zero literal annotation — `0` starts with a digit and must widen to `'number'`.
#[test]
fn ts2365_zero_literal_annotation_widens_to_number() {
    let src = r#"
function h(x: 0) {
    false < x;
}
"#;
    let msgs = ts2365_messages(src);
    assert!(
        !msgs.is_empty(),
        "Expected TS2365 for `false < x` where `x: 0`; got none"
    );
    for msg in &msgs {
        assert!(
            msg.contains("'number'"),
            "TS2365 message should contain 'number' (widened); got: {msg}"
        );
        assert!(
            !msg.contains("'0'"),
            "TS2365 message must NOT contain raw literal '0'; got: {msg}"
        );
    }
}

// =========================================================================
// Canonical conformance case — 7 errors matching tsc
// =========================================================================

/// Mirrors the TypeScript conformance test `relationalOperatorComparable.ts`.
/// tsc emits exactly 7 TS2365 errors with widened primitive type names.
#[test]
fn ts2365_relational_operator_comparable_seven_errors() {
    let src = r#"
var a: boolean;
var b: number;
var c: string;

var r1 = b < a;
var r2 = b <= a;
var r3 = b >= a;
var r4 = b > a;
var r5 = a < b;
var r6 = a < b;
var r7 = c < b;
"#;
    let msgs = ts2365_messages(src);
    assert_eq!(
        msgs.len(),
        7,
        "expected 7 TS2365 messages; got {}",
        msgs.len()
    );
    for msg in &msgs {
        assert!(
            msg.contains("'number'") || msg.contains("'boolean'") || msg.contains("'string'"),
            "each TS2365 message must use widened primitive names; got: {msg}"
        );
    }
}
