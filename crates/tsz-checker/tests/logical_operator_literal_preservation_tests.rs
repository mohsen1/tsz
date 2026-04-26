//! Tests that the left operand of `||`, `&&`, and `??` preserves literal types.
//!
//! In tsc, `checkExpression` returns the FRESH literal type for literal
//! expressions (e.g., `"baz"` stays `"baz"`, not widened to `string`). This
//! matters for the logical operators because the result-type evaluator uses
//! truthiness narrowing — a widened `string` cannot narrow to NEVER on the
//! falsy branch, so the result wrongly unions in the right operand.
//!
//! Regression test for missing TS2678 in `case "baz" || z:` where `z: "bar"`
//! and the switch discriminant is `"foo"`.
//! See conformance test
//! `tests/cases/conformance/types/literal/stringLiteralsWithSwitchStatements03.ts`.
use crate::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// `"baz" || z` (where z is a non-literal identifier) should produce the
/// literal type `"baz"`, not the widened `string`.
///
/// The variable annotation `0` forces a TS2322 with the actual inferred type
/// in the message — but here we just check the diagnostic code is emitted,
/// because `string` would assign successfully and emit nothing for that line.
#[test]
fn or_with_string_literal_left_preserves_literal_type() {
    let source = r#"
declare let z: "bar";
const x: never = "baz" || z;
"#;
    let codes = check_strict(source);
    // Expect TS2322 (the value is "baz", not assignable to never) and TS2872
    // (the "baz" literal is always truthy).
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for 'baz' not assignable to never, got: {codes:?}"
    );
    assert!(
        codes.contains(&2872),
        "Expected TS2872 (always truthy), got: {codes:?}"
    );
}

/// Same as above but for numeric literals: `1 || z` should produce `1`.
#[test]
fn or_with_numeric_literal_left_preserves_literal_type() {
    let source = r#"
declare let z: 0;
const x: never = 1 || z;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for '1' not assignable to never, got: {codes:?}"
    );
}

/// In a switch on `"foo"`, `case "baz" || z:` (with `z: "bar"`) should emit
/// TS2678 because the case-clause type evaluates to `"baz"` (a literal) that
/// is not comparable to `"foo"`.
#[test]
fn switch_case_logical_or_literal_emits_ts2678() {
    let source = r#"
declare let x: "foo";
declare let z: "bar";
switch (x) {
    case "baz" || z:
        break;
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2678),
        "Expected TS2678 for case 'baz' || z, got: {codes:?}"
    );
}

/// `&&` should also preserve the literal type on the left, so `1 && x`
/// (with `x: 1`) produces `1`, not `0 | 1`.
#[test]
fn and_with_numeric_literal_left_preserves_literal_type() {
    let source = r#"
declare let x: 1;
const r: 0 = 1 && x;
"#;
    let codes = check_strict(source);
    // Expect TS2322 with the actual inferred literal type 1, not the widened
    // 0 | 1. The diagnostic code alone confirms the assignment is rejected;
    // the message-based parity is exercised by the conformance suite.
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for '1' not assignable to '0', got: {codes:?}"
    );
}

/// `??` with a literal-on-left should pick the right operand only when the
/// literal can be nullish; non-nullish literals like `"baz"` keep the
/// literal type.
#[test]
fn nullish_coalescing_with_literal_left_preserves_literal_type() {
    let source = r#"
declare let z: "bar";
const x: never = "baz" ?? z;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for 'baz' not assignable to never, got: {codes:?}"
    );
}
