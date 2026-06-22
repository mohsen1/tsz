//! Regression tests for precise excess-property reporting on discriminated
//! unions whose discriminant is an **enum member** (`Kind.A`).
//!
//! Structural rule (owner: `object_literal_direct_unit_discriminants` in
//! `state_checking/property/excess_property_tail.rs`): to report the precise
//! TS2353 ("Object literal may only specify known properties") at the offending
//! property of a discriminated-union assignment, the checker collects the
//! source object literal's discriminant property values. The purely-syntactic
//! `literal_type_from_initializer` resolves string/number/boolean/`undefined`
//! literals but not an enum-member reference (`Kind.A` is a
//! `PropertyAccessExpression`). When it misses, the collector now falls back to
//! the initializer's computed type, which yields the *nominal* enum-literal
//! unit type matching the target member's discriminant. Without it the
//! discriminant match failed and the check fell back to a generic TS2322.
//!
//! Both paths are gated on `is_unit_type`, so a non-discriminant initializer
//! (e.g. `obj.x: number`) is never mistaken for a discriminant.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect();
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn enum_member_discriminant_excess_property_is_ts2353() {
    // tsz previously emitted a generic TS2322 at the binding; tsc emits the
    // precise excess-property TS2353 at `b`.
    assert_eq!(
        codes(
            r#"
enum Kind { A, B }
type T = { k: Kind.A; a: number } | { k: Kind.B; b: string };
const bad: T = { k: Kind.A, b: "x" };
"#,
        ),
        vec![2353],
        "enum-member discriminant excess property should be TS2353",
    );
}

#[test]
fn enum_member_discriminant_excess_property_renamed_binders() {
    // Not keyed on `Kind`/`A`/`k` — proves the structural rule.
    assert_eq!(
        codes(
            r#"
enum Tag { First, Second }
type Shape = { kind: Tag.First; x: number } | { kind: Tag.Second; y: string };
const bad: Shape = { kind: Tag.First, y: "oops" };
"#,
        ),
        vec![2353],
        "renamed enum discriminant excess property should be TS2353",
    );
}

#[test]
fn const_enum_member_discriminant_excess_property_is_ts2353() {
    assert_eq!(
        codes(
            r#"
const enum CE { A, B }
type T = { k: CE.A; a: number } | { k: CE.B; b: string };
const bad: T = { k: CE.A, b: "x" };
"#,
        ),
        vec![2353],
        "const-enum discriminant excess property should be TS2353",
    );
}

#[test]
fn string_enum_member_discriminant_excess_property_is_ts2353() {
    assert_eq!(
        codes(
            r#"
enum SK { A = "a", B = "b" }
type T = { k: SK.A; a: number } | { k: SK.B; b: string };
const bad: T = { k: SK.A, b: "x" };
"#,
        ),
        vec![2353],
        "string-enum discriminant excess property should be TS2353",
    );
}

#[test]
fn enum_member_correct_member_is_clean() {
    // Selecting the right member with only its own properties is valid.
    assert!(
        codes(
            r#"
enum Kind { A, B }
type T = { k: Kind.A; a: number } | { k: Kind.B; b: string };
const ok: T = { k: Kind.A, a: 1 };
"#,
        )
        .is_empty(),
        "correct enum-member object should be clean",
    );
}

#[test]
fn enum_member_correct_discriminant_wrong_value_is_ts2322() {
    // Right member selected, wrong property value: a genuine TS2322, not TS2353.
    assert_eq!(
        codes(
            r#"
enum Kind { A, B }
type T = { k: Kind.A; a: number } | { k: Kind.B; b: string };
const bad: T = { k: Kind.A, a: "str" };
"#,
        ),
        vec![2322],
        "wrong property value should be TS2322",
    );
}
