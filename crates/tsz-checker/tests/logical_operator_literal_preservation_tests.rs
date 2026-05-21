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
use crate::test_utils::{check_source_strict, check_source_strict_codes as check_strict};

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

#[test]
fn logical_or_type_parameter_assignment_reports_whole_expression() {
    let source = r#"
function fn1<T, U>(t: T, u: U) {
    var r4: {} = t || u;
}

function fn2<U, V>(u: U, v: V) {
    var r6: {} = u || v;
}
"#;

    let diagnostics = check_source_strict(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected one TS2322 per logical-or assignment, got: {diagnostics:?}"
    );
    let has_t_u_display = ts2322.iter().any(|diag| {
        diag.message_text
            .contains("Type 'U | NonNullable<T>' is not assignable to type '{}'.")
            || diag
                .message_text
                .contains("Type 'NonNullable<T> | U' is not assignable to type '{}'.")
    });
    let has_u_v_display = ts2322.iter().any(|diag| {
        diag.message_text
            .contains("Type 'V | NonNullable<U>' is not assignable to type '{}'.")
            || diag
                .message_text
                .contains("Type 'NonNullable<U> | V' is not assignable to type '{}'.")
    });
    assert!(
        has_t_u_display,
        "expected whole-expression display for T || U, got: {ts2322:?}"
    );
    assert!(
        has_u_v_display,
        "expected whole-expression display for U || V, got: {ts2322:?}"
    );
}

// ---------------------------------------------------------------------------
// Right-operand literal preservation in `const` results (issue #9765).
//
// The result of `lhs && rhs` / `lhs || rhs` unions the operand types without
// widening literals; the widening to base primitive only happens at mutable
// (`let`/`var`) binding sites. tsz previously widened a literal right operand
// eagerly (`"yes"` -> `string`), producing a too-wide `const` result type and
// false-positive TS2322s. These tests pin the corrected behavior.
// ---------------------------------------------------------------------------

/// `const x = a && "yes"` with `a: 0 | 1` infers `0 | "yes"`, assignable to the
/// matching literal-union annotation without TS2322.
#[test]
fn const_and_preserves_string_literal_right_operand() {
    let source = r#"
declare const a: 0 | 1;
const x = a && "yes";
const y: 0 | "yes" = x;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for `0 | \"yes\"` result, got: {codes:?}"
    );
}

/// Numeric-literal right operand: `a && 9` infers `0 | 9`.
#[test]
fn const_and_preserves_numeric_literal_right_operand() {
    let source = r#"
declare const a: 0 | 1;
const w = a && 9;
const wy: 0 | 9 = w;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for `0 | 9` result, got: {codes:?}"
    );
}

/// `||` variant: `a || "yes"` infers `1 | "yes"`.
#[test]
fn const_or_preserves_string_literal_right_operand() {
    let source = r#"
declare const a: 0 | 1;
const x = a || "yes";
const y: 1 | "yes" = x;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for `1 | \"yes\"` result, got: {codes:?}"
    );
}

/// Boolean left operand: `b && "yes"` infers `false | "yes"`.
#[test]
fn const_and_with_boolean_left_preserves_right_literal() {
    let source = r#"
declare const b: boolean;
const z = b && "yes";
const zy: false | "yes" = z;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for `false | \"yes\"` result, got: {codes:?}"
    );
}

/// Structural rule, not a spelling: different declared union and different
/// literal still preserves the right operand.
#[test]
fn const_and_preserves_right_literal_renamed_shapes() {
    let source = r#"
declare const flag: 2 | 3;
const value = flag && "ready";
const checked: 2 | "ready" = value;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for `2 | \"ready\"` result, got: {codes:?}"
    );
}

/// Nested logical expression preserves every fresh literal operand.
#[test]
fn const_nested_logical_preserves_literals() {
    let source = r#"
declare const a: 0 | 1;
declare const b: boolean;
const x = a && (b && "yes");
const y: 0 | false | "yes" = x;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for nested `0 | false | \"yes\"`, got: {codes:?}"
    );
}

/// Negative control: a genuinely wrong annotation still reports TS2322 with the
/// preserved (literal) result type, proving the literal is not silently widened
/// away.
#[test]
fn const_and_literal_result_still_reports_real_mismatch() {
    let source = r#"
declare const a: 0 | 1;
const x = a && "yes";
const y: 0 | "no" = x;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 for `0 | \"yes\"` not assignable to `0 | \"no\"`, got: {codes:?}"
    );
}

/// Control: mutable (`let`) bindings still widen the fresh literal operand to
/// its base primitive, matching tsc's `getWidenedLiteralType`. The `0` from the
/// declared left operand is preserved, so the result is `0 | string` and is not
/// assignable to a too-narrow literal annotation.
#[test]
fn let_and_widens_literal_right_operand() {
    let source = r#"
declare const a: 0 | 1;
let x = a && "yes";
const probe: 0 | "yes" = x;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322: `let` widens `\"yes\"` to `string` (result `0 | string`), got: {codes:?}"
    );
}
