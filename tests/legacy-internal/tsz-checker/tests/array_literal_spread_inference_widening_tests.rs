//! Tests for issue #9741: fresh literal elements of a spread-containing array
//! literal must be widened during `T[]` inference.
//!
//! When inferring the element type `T` of `arr: T[]` from an array literal that
//! mixes a spread with non-spread literal elements (e.g. `[...base, "x"]`), the
//! inferred element-type candidate is a single union (`number | "x"`). tsc's
//! `getWidenedLiteralType` widens the fresh literal members of that union
//! independently, yielding `T = string | number`. The bug preserved the literal
//! members (`T = number | "x"`), masking a TS2322.
//!
//! The structural rule: when a type parameter is inferred from array-literal
//! element types and the inferred element type is a union containing fresh
//! literal members, those fresh members are widened even when the union also
//! carries non-literal members.

use crate::test_utils::check_source_diagnostics;

fn ts2322(src: &str) -> Vec<String> {
    check_source_diagnostics(src)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn spread_then_literal_widens_during_array_inference() {
    // `T` widens to `string | number`, so the narrow literal target is rejected.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x"]);
const c: number | "x" = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn spread_then_literal_accepts_widened_target() {
    // The widened element type `string | number` must be accepted.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x"]);
const c: string | number = r;
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn literal_then_spread_widens_regardless_of_position() {
    let errs = ts2322(
        r#"
declare function pick<U>(items: U[]): U;
const nums = [1, 2];
const r = pick(["x", ...nums]);
const c: number | "x" = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn spread_with_multiple_trailing_literals_widens() {
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x", "y"]);
const c: number | "x" = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn spread_with_boolean_literal_widens_to_boolean() {
    // Adjacent shape: a boolean literal mixed with a numeric spread widens to
    // `number | boolean`, not `number | true`.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, true]);
const c: number | true = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn plain_literal_array_still_widens_control() {
    // Control: a plain literal array (no spread) must keep widening to `string`.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const r = f(["x", "y"]);
const c: "x" | "y" = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn spread_of_literal_array_still_widens_control() {
    // Control: spreading a literal-typed array (its elements already widened to
    // `string`) plus another literal still widens.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const sbase = ["a", "b"];
const r = f([...sbase, "c"]);
const c: "a" | "b" | "c" = r;
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}

#[test]
fn const_type_param_preserves_literals_negative_control() {
    // Negative control: a `const` type parameter must still preserve literal
    // element types (no widening), so the narrow target is accepted.
    let errs = ts2322(
        r#"
declare function f<const T>(arr: T[]): T;
const base = [1, 2] as const;
const r = f([...base, "x"]);
const c: 1 | 2 | "x" = r;
"#,
    );
    assert!(
        errs.is_empty(),
        "expected no TS2322 for const T, got: {errs:?}"
    );
}

#[test]
fn nullable_literal_array_keeps_literals_boundary() {
    // Boundary: a plain literal array whose element union also carries a
    // nullable (`null`) member is left un-widened. The fix only widens when the
    // non-literal members are widened primitives (the spread-of-widened-array
    // shape); nullable-mixed unions keep their literal members, so the literal
    // target is still accepted.
    let errs = ts2322(
        r#"
declare function f<T>(arr: T[]): T;
const r = f([true, 1, null, "yes"]);
const c: true | 1 | "yes" | null = r;
"#,
    );
    assert!(
        errs.is_empty(),
        "expected no TS2322 for nullable-mixed literal array, got: {errs:?}"
    );
}
