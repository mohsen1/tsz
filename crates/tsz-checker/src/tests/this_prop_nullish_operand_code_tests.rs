//! A nullish property-access operand of a relational/arithmetic operator whose
//! receiver has no nameable text (e.g. a `this`-rooted `this.version`) is
//! reported as the object being possibly nullish (TS2532/TS2531/TS2533), not the
//! literal-value TS18050.
//!
//! Structural rule: tsc reports `this.x >= n` (where `this.x: number | undefined`)
//! as TS2532 "Object is possibly 'undefined'". tsz could not produce a simple
//! name for the `this`-rooted access (`expression_text` returns `None`), so the
//! property-access branch fell through to the TS18050 "value cannot be used here"
//! fallback reserved for literal `null`/`undefined`. The fix routes the unnameable
//! receiver through `report_nullish_object`, like an element access already does.
//! Owner: `error_reporter/operator_errors.rs` `emit_nullish_operand_error`.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS18050: u32 = 18050; // The value '<x>' cannot be used here.
const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2532: u32 = 2532; // Object is possibly 'undefined'.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn this_prop_nullish_relational_operand_is_ts2532_not_ts18050() {
    let codes = check_strict(
        r#"
class C {
  constructor(private version: number | undefined) {}
  check(n: number): boolean {
    return this.version >= n;
  }
}
"#,
    );
    assert_eq!(
        count(&codes, TS2532),
        1,
        "this-rooted nullish operand reports TS2532: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS18050),
        0,
        "must not use the literal-value TS18050 for a property access: {codes:?}"
    );
}

#[test]
fn nameable_prop_nullish_operand_keeps_ts18048() {
    // Control: a property access WITH nameable text keeps the named TS18048,
    // exactly as tsc does (`'a.b' is possibly 'undefined'`).
    let codes = check_strict(
        r#"
declare const a: { b: number | undefined };
declare const n: number;
const r = a.b >= n;
"#,
    );
    assert_eq!(
        count(&codes, TS18048),
        1,
        "nameable property access keeps named TS18048: {codes:?}"
    );
    assert_eq!(count(&codes, TS2532), 0, "{codes:?}");
    assert_eq!(count(&codes, TS18050), 0, "{codes:?}");
}

#[test]
fn literal_undefined_operand_still_ts18050() {
    // Control: a literal `undefined` operand still gets TS18050 (the `is_literal`
    // path, untouched by the fix).
    let codes = check_strict(
        r#"
declare const n: number;
const r = undefined >= n;
"#,
    );
    assert_eq!(
        count(&codes, TS18050),
        1,
        "literal `undefined` keeps TS18050: {codes:?}"
    );
}
