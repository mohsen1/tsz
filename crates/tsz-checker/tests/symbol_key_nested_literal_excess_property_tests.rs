//! A nested object literal assigned through a `symbol`-keyed computed property
//! is drilled into for excess-property reporting, the same as one assigned
//! through a `[k: string]` index signature.
//!
//! Regression for #16649. #16637 (landed as #16647) fixed *which* index
//! signature supplies the value type for a symbol-keyed computed property, but
//! the nested-literal excess-property drill-in still did not fire for that
//! path: tsz reported the outer `TS2418` ("not assignable to the index
//! signature") instead of `TS2353` on the offending property inside the nested
//! literal. Per tsc, a fresh object literal in a contextually typed position
//! carries its freshness through the index-signature value type, so the excess
//! property is reported at the inner literal.
//!
//! Oracle: `typescript@7.0.2`, `--noEmit --strict --lib es2024 --target es2022`.
//! Binder names (symbol const, value interface, property names) are varied
//! across the rows so no fixture-name string can drive the decision, and the
//! `[k: string]` form is kept as a control that already behaved correctly.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|(c, _)| *c).collect()
}

// ---- The reported defect ----------------------------------------------------

#[test]
fn symbol_keyed_nested_literal_excess_property_reports_ts2353_not_ts2418() {
    // The nested `{ a: 1, b: 2 }` overshoots `Val`; tsc drills in and reports
    // TS2353 on `b`, not TS2418 on the whole computed property.
    let source = r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const i2: I = { [sym]: { a: 1, b: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "excess property in a symbol-keyed nested literal is TS2353 at the \
         inner literal, not TS2418 at the computed property: {diags:?}"
    );
}

#[test]
fn symbol_keyed_nested_literal_excess_property_renamed_binders() {
    // Same rule, every binder renamed and the target written as a type literal
    // rather than an interface.
    let source = r#"
declare const zq: unique symbol;
interface Payload { alpha: number; }
type Bag = { [k: string]: number; [k: symbol]: Payload };
const bag: Bag = { [zq]: { alpha: 1, beta: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "the drill-in is structural, not keyed on any fixture name: {diags:?}"
    );
}

// ---- Negative: a conforming nested literal stays clean -----------------------

#[test]
fn symbol_keyed_nested_literal_without_excess_property_is_clean() {
    let source = r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const i: I = { [sym]: { a: 1 } };
"#;
    let diags = check_strict(source);
    assert!(
        diags.is_empty(),
        "a nested literal that matches the symbol signature's value type must \
         not error: {diags:?}"
    );
}

// ---- Control: the string-index path already drilled in ----------------------

#[test]
fn string_keyed_nested_literal_excess_property_still_reports_ts2353() {
    let source = r#"
interface Val { a: number; }
interface I { [k: string]: Val; }
const i2: I = { x: { a: 1, b: 2 } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2353],
        "the pre-existing string-index drill-in must be unchanged: {diags:?}"
    );
}

// ---- Residual: the mismatch half of the same drill-in is still outer-only ---

#[test]
fn symbol_keyed_nested_literal_member_mismatch_reports_ts2322() {
    // Same drill-in, other polarity: `a` is present but wrongly typed, so this
    // is a mismatch rather than an excess property. tsc drills into the nested
    // literal either way and reports TS2322 on the member:
    //
    //   error TS2322: Type 'string' is not assignable to type 'number'.
    //
    // tsz reports TS2418 at the computed property. The excess-property half is
    // fixed; this half is not, so the rows above pass while this one pins the
    // remaining gap.
    let source = r#"
declare const sym: unique symbol;
interface Val { a: number; }
interface I { [k: string]: number; [k: symbol]: Val; }
const bad: I = { [sym]: { a: "no" } };
"#;
    let diags = check_strict(source);
    assert_eq!(
        codes(&diags),
        vec![2322],
        "a wrong-typed member in a symbol-keyed nested literal is TS2322 at \
         the member, matching the excess-property drill-in: {diags:?}"
    );
}
