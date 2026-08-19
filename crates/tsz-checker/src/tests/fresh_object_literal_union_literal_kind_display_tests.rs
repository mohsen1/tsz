//! Fresh object-literal source types must preserve numeric, boolean, and
//! bigint literal properties (not just string literals) when displayed in a
//! TS2322 assignability message against a union target.
//!
//! Structural rule: when a fresh object literal fails assignment to a union
//! target and tsz renders the whole source object, each source property is kept
//! in literal form exactly when the contextual (target) property type carries a
//! literal of the *same* primitive base — mirroring tsc's
//! `getWidenedLiteralLikeTypeForContextualType` / `isLiteralOfContextualType`.
//!
//! Before the fix the literal-acceptance check only recognized string literals,
//! so a numeric/boolean/bigint property whose target arm carried a matching
//! literal (e.g. `a: 1 | 2`) was wrongly widened to its primitive
//! (`{ a: number; ... }`) while a string sibling (`b: "x" | "y"`) was preserved.
//! tsc keeps every literal kind: `{ a: 1; b: "y"; }`.
//!
//! Property names are varied across cases so the behavior is proven structural,
//! not keyed on a particular identifier spelling.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn numeric_literal_properties_preserved_against_union_target() {
    // `q: 4` matches arm two, `p: 1` matches arm one — no single arm fits, so
    // tsz renders the whole object. Both numeric literals must be preserved.
    let messages = ts2322_messages(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r: R = { p: 1, q: 4 };
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("{ p: 1; q: 4; }")),
        "numeric literal properties should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("number")),
        "numeric literals must not be widened to `number`, got: {messages:?}"
    );
}

#[test]
fn boolean_literal_properties_preserved_against_union_target() {
    let messages = ts2322_messages(
        r#"
type Flag = { on: true; tag: "x" } | { on: false; tag: "y" };
const fl: Flag = { on: true, tag: "y" };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"{ on: true; tag: "y"; }"#)),
        "boolean literal property should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("boolean")),
        "boolean literal must not be widened to `boolean`, got: {messages:?}"
    );
}

#[test]
fn bigint_literal_properties_preserved_against_union_target() {
    let messages = ts2322_messages(
        r#"
type Big = { amt: 1n; tag: "x" } | { amt: 2n; tag: "y" };
const bg: Big = { amt: 1n, tag: "y" };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"{ amt: 1n; tag: "y"; }"#)),
        "bigint literal property should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("bigint")),
        "bigint literal must not be widened to `bigint`, got: {messages:?}"
    );
}

#[test]
fn bigint_only_literal_properties_preserved_against_union_target() {
    // No string sibling to trip the legacy string-only literal-surface gate:
    // both arms are all-bigint, split across arms (`lo: 1n` matches arm one,
    // `hi: 4n` matches arm two). tsc keeps both bigint literals.
    let messages = ts2322_messages(
        r#"
type Range = { lo: 1n; hi: 2n } | { lo: 3n; hi: 4n };
const rg: Range = { lo: 1n, hi: 4n };
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("{ lo: 1n; hi: 4n; }")),
        "bigint-only literal properties should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("bigint")),
        "bigint-only literals must not be widened to `bigint`, got: {messages:?}"
    );
}

#[test]
fn boolean_only_literal_properties_preserved_against_union_target() {
    // All-boolean arms, split across arms (`fst: true` matches arm one,
    // `snd: true` matches arm two). No string sibling, so this only passes
    // once the literal-surface gate recognizes boolean literals too.
    let messages = ts2322_messages(
        r#"
type Pair = { fst: true; snd: false } | { fst: false; snd: true };
const pr: Pair = { fst: true, snd: true };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("{ fst: true; snd: true; }")),
        "boolean-only literal properties should be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("boolean")),
        "boolean-only literals must not be widened to `boolean`, got: {messages:?}"
    );
}

#[test]
fn mixed_numeric_and_string_literal_properties_preserved() {
    let messages = ts2322_messages(
        r#"
type Mix = { code: 1; label: "x" } | { code: 2; label: "y" };
const mx: Mix = { code: 1, label: "y" };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"{ code: 1; label: "y"; }"#)),
        "mixed numeric+string literals should be preserved, got: {messages:?}"
    );
}

#[test]
fn string_literal_properties_still_preserved_against_union_target() {
    // Regression guard: the original string-literal preservation must be intact.
    let messages = ts2322_messages(
        r#"
type Str = { lhs: "p"; rhs: "x" } | { lhs: "q"; rhs: "y" };
const st: Str = { lhs: "p", rhs: "y" };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"{ lhs: "p"; rhs: "y"; }"#)),
        "string literal properties should remain preserved, got: {messages:?}"
    );
}

#[test]
fn bigint_literal_object_property_preserved_against_single_target() {
    // The complementary `literal_expression_display` gap: a fresh bigint object
    // property is interned in widened form, so its literal text must be
    // resurrected from the AST. tsc shows `9n`, not `bigint`.
    let messages = ts2322_messages(
        r#"
const wb: { v: 1n } = { v: 9n };
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Type '9n'")),
        "bigint source literal should display as `9n`, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("'bigint'")),
        "bigint source must not widen to `bigint`, got: {messages:?}"
    );
}

#[test]
fn numeric_source_against_string_literal_target_still_widens() {
    // Negative / per-kind guard: a numeric source property whose target arm
    // carries only string literals has no matching primitive base, so tsc (and
    // now tsz) widens it. Here every property mismatches its arm, so tsc reports
    // per property: the numeric `a` widens to `number`, the string keeps `"q"`.
    let messages = ts2322_messages(
        r#"
type T = { a: "x"; b: 1 } | { a: "y"; b: 2 };
const t: T = { a: 9, b: 7 };
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'number' is not assignable to type '\"x\" | \"y\"'")),
        "numeric source against string-literal target should widen, got: {messages:?}"
    );
}
