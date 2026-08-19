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

// ---------------------------------------------------------------------------
// Fresh-literal union fold (#17721): tsc's `hasExcessProperties` reports a
// FRESH object literal's failing property directly beneath the TS2322 head —
// `Types of property 'X' are incompatible.` with the property relation's own
// chain below and NO `Type 'S' is not assignable to type '<member>'.` member
// frame. The checked member set is the discriminant-matched constituent
// (first-DECLARED discriminant decides), else every object-like member. A
// non-fresh source skips that phase and keeps the member frame. All
// expectations oracle-pinned against typescript@7.0.2 via
// `scripts/conformance/oracle.sh` (--strict).
// ---------------------------------------------------------------------------

fn chain_texts(source: &str, code: u32) -> (String, Vec<(u8, String)>) {
    let diags = check_source_diagnostics(source);
    let diag = diags
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected a TS{code} diagnostic, got: {diags:?}"));
    (
        diag.message_text.clone(),
        diag.related_information
            .iter()
            .map(|info| (info.depth, info.message_text.clone()))
            .collect(),
    )
}

#[test]
fn numeric_fold_drills_matched_arm_without_member_frame() {
    let (head, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r: R = { p: 1, q: 4 };
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ p: 1; q: 4; }' is not assignable to type 'R'."),
        "head should keep literal properties, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'q' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "fold should drill the discriminant-matched arm's property with no member frame"
    );
}

#[test]
fn first_declared_discriminant_decides_matched_arm() {
    // The numeric property is declared FIRST, so it is the discriminant that
    // narrows (tsc iterates `getPropertiesOfType` in declaration order); the
    // later string property is the reported mismatch — even though the string
    // property alone would have narrowed to the other arm.
    let (_, chain) = chain_texts(
        r#"
type Mix = { zeta: 1; alpha: "x" } | { zeta: 2; alpha: "y" };
const mx: Mix = { zeta: 1, alpha: "y" };
"#,
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'alpha' are incompatible.".to_string()),
            (
                1,
                "Type '\"y\"' is not assignable to type '\"x\"'.".to_string()
            ),
        ],
        "first-declared discriminant (zeta) must pick the arm; alpha is the mismatch"
    );

    // Reversed declaration order flips which property discriminates.
    let (_, chain) = chain_texts(
        r#"
type Mix = { alpha: 1; zeta: "x" } | { alpha: 2; zeta: "y" };
const mx: Mix = { alpha: 1, zeta: "y" };
"#,
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'zeta' are incompatible.".to_string()),
            (
                1,
                "Type '\"y\"' is not assignable to type '\"x\"'.".to_string()
            ),
        ],
    );
}

#[test]
fn all_boolean_literal_union_fold() {
    let (head, chain) = chain_texts(
        r#"
type F = { on: true; off: false } | { on: false; off: true };
const ff: F = { on: true, off: true };
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ on: true; off: true; }' is not assignable to type 'F'."),
        "boolean literal head preserved, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'off' are incompatible.".to_string()),
            (
                1,
                "Type 'true' is not assignable to type 'false'.".to_string()
            ),
        ],
    );
}

#[test]
fn all_bigint_literal_union_fold() {
    let (head, chain) = chain_texts(
        r#"
type B = { amt: 1n; lim: 2n } | { amt: 3n; lim: 4n };
const bg: B = { amt: 1n, lim: 4n };
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ amt: 1n; lim: 4n; }' is not assignable to type 'B'."),
        "bigint literal head preserved, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'lim' are incompatible.".to_string()),
            (1, "Type '4n' is not assignable to type '2n'.".to_string()),
        ],
    );
}

#[test]
fn interface_arm_union_fold() {
    let (_, chain) = chain_texts(
        r#"
interface ArmA { sel: 1; val: 2 }
interface ArmB { sel: 3; val: 4 }
const ri: ArmA | ArmB = { sel: 1, val: 4 };
"#,
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'val' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "named (interface) arms fold exactly like anonymous ones"
    );
}

#[test]
fn non_fresh_source_keeps_member_frame() {
    // Negative control: a non-fresh source skips tsc's excess-property phase
    // and keeps the best-member frame beneath the head.
    let (_, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
declare const s: { p: 1; q: 4 };
const r: R = s;
"#,
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (
                0,
                "Type '{ p: 1; q: 4; }' is not assignable to type '{ p: 1; q: 2; }'.".to_string()
            ),
            (1, "Types of property 'q' are incompatible.".to_string()),
            (2, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "non-fresh sources keep the member frame"
    );
}

#[test]
fn satisfies_fold_no_member_frame() {
    let (head, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r = { p: 1, q: 4 } satisfies R;
"#,
        1360,
    );
    assert!(
        head.contains("Type '{ p: 1; q: 4; }' does not satisfy the expected type 'R'."),
        "satisfies head preserved, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'q' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
    );
}

#[test]
fn no_discriminant_match_keeps_property_elaboration() {
    // When no arm matches the written discriminant value, tsc's per-property
    // expression elaboration anchors at the property node against the
    // distributed value union — the fold never fires.
    let messages = ts2322_messages(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r: R = { p: 5, q: 2 };
"#,
    );
    assert_eq!(
        messages,
        vec!["Type '5' is not assignable to type '1 | 3'.".to_string()],
        "no-match case keeps the property-anchored elaboration"
    );
}

#[test]
fn primitive_valued_property_widens_in_fold_head() {
    // `mappedTypeIndexedAccess.ts` shape: the union arms type `value` as PLAIN
    // primitives (string / number), so the fresh source's `value: 3` widens to
    // `number` in the head, while the literal-typed `key` keeps `"foo"` — the
    // per-property same-base rule, not a whole-object preservation.
    let (head, chain) = chain_texts(
        r#"
type Pairs<T> = { [K in keyof T]: { key: K; value: T[K] } };
type Pair<T> = Pairs<T>[keyof T];
type FooBar = { foo: string; bar: number };
let pair: Pair<FooBar> = { key: "foo", value: 3 };
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ key: \"foo\"; value: number; }' is not assignable"),
        "primitive-typed property widens, literal-typed key stays, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'value' are incompatible.".to_string()),
            (
                1,
                "Type 'number' is not assignable to type 'string'.".to_string()
            ),
        ],
    );
}
