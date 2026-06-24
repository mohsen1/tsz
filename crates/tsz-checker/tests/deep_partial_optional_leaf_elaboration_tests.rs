//! Regression: an object-literal value matched against an **optional** property
//! whose declared type is a still-deferred generic application (the canonical
//! shape being the recursive `DeepPartial`-style mapped type
//! `{ [K in keyof T]?: DP<T[K]> }`) must elaborate to the single deepest leaf
//! mismatch the way `tsc` does — not a duplicate/mis-rendered diagnostic.
//!
//! Root cause: the optional property gives the relation target a union member
//! `DP<number> | undefined`. The subtype-failure explanation collapsed to a
//! `NoUnionMemberMatches` over `[DP<number>, undefined]`, rendering the
//! *unevaluated* application `DP<number>` instead of the leaf it resolves to
//! (`number`). Because the assignment elaboration separately reports the
//! evaluated `number` leaf at the same anchor, the differing message defeated
//! diagnostic dedup and surfaced a spurious second TS2322. The required
//! (non-optional) form never built the `| undefined` union, so it was clean —
//! which masked the gap.
//!
//! Fix: when a `T | undefined`-shaped target has a sole non-nullish member `T`
//! and the source is a scalar (primitive / literal), elaborate `S` against `T`
//! directly (resolving `T`), exactly as `tsc` does for a nullable target.

use tsz_checker::test_utils::check_source_strict_messages_without_missing_libs;

/// `(code, message)` pairs anchored at the `n`th 1-based line, with the
/// surrounding test noise (TS2318 missing-lib) already filtered out.
fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_strict_messages_without_missing_libs(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

fn all_codes(source: &str) -> Vec<u32> {
    check_source_strict_messages_without_missing_libs(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// Single recursion level: the optional `b?` value resolves to `number`.
/// Exactly one TS2322 against `number`, with no `DP<number>` aggregate.
#[test]
fn recursive_optional_mapped_leaf_reports_resolved_number_once() {
    let msgs = ts2322_messages(
        r#"
type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> };
const v: DeepPartial<{ b: number }> = { b: "x" };
"#,
    );
    assert_eq!(
        msgs,
        vec!["Type 'string' is not assignable to type 'number'.".to_string()],
        "must report the resolved leaf once, not a duplicate `DeepPartial<number>` aggregate",
    );
}

/// Nested object value: the deepest optional leaf still resolves to `number`.
#[test]
fn nested_recursive_optional_mapped_leaf_reports_resolved_number_once() {
    let msgs = ts2322_messages(
        r#"
type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> };
const v: DeepPartial<{ outer: { inner: number } }> = { outer: { inner: "x" } };
"#,
    );
    assert_eq!(
        msgs,
        vec!["Type 'string' is not assignable to type 'number'.".to_string()],
        "the deepest leaf must resolve and report once: {msgs:?}",
    );
}

/// Anti-hardcoding: renamed alias / type-parameter / property binders take the
/// identical structural path — the fix must not key on any name.
#[test]
fn recursive_optional_mapped_leaf_independent_of_binder_names() {
    let msgs = ts2322_messages(
        r#"
type Loose<Shape> = { [Key in keyof Shape]?: Loose<Shape[Key]> };
const payload: Loose<{ amount: number }> = { amount: "x" };
"#,
    );
    assert_eq!(
        msgs,
        vec!["Type 'string' is not assignable to type 'number'.".to_string()],
        "renamed binders must reach the same resolved-leaf elaboration: {msgs:?}",
    );
}

/// `undefined` is a valid value for the optional property — the `| undefined`
/// member must be preserved (no spurious error).
#[test]
fn recursive_optional_mapped_accepts_undefined_value() {
    let codes = all_codes(
        r#"
type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> };
const v: DeepPartial<{ b: number }> = { b: undefined };
"#,
    );
    assert!(
        !codes.contains(&2322),
        "an explicit `undefined` for an optional property is valid: {codes:?}",
    );
}

/// A valid leaf value must stay clean (the fix must not over-report).
#[test]
fn recursive_optional_mapped_valid_leaf_is_clean() {
    let codes = all_codes(
        r#"
type DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> };
const v: DeepPartial<{ b: number }> = { b: 5 };
"#,
    );
    assert!(
        !codes.contains(&2322),
        "a matching leaf value must not error: {codes:?}",
    );
}

/// The non-optional (required) recursive mapped form was already correct; pin it
/// so the optional fix keeps parity with it.
#[test]
fn required_recursive_mapped_leaf_reports_resolved_number_once() {
    let msgs = ts2322_messages(
        r#"
type DeepReq<T> = { [K in keyof T]: DeepReq<T[K]> };
const v: DeepReq<{ b: number }> = { b: "x" };
"#,
    );
    assert_eq!(
        msgs,
        vec!["Type 'string' is not assignable to type 'number'.".to_string()],
        "required and optional recursive mapped leaves must agree: {msgs:?}",
    );
}

/// Sole-real-member nullable target with a close string literal: `tsc`
/// elaborates `"appel"` against `"apple"` directly (TS2322), never the TS2820
/// `… | undefined` "Did you mean" form. Same root, surfaced without a mapped
/// type at all.
#[test]
fn sole_member_nullable_literal_uses_direct_mismatch_not_suggestion() {
    let pairs: Vec<(u32, String)> = check_source_strict_messages_without_missing_libs(
        r#"
type Fruit = "apple" | undefined;
const f: Fruit = "appel";
"#,
    );
    assert_eq!(
        pairs,
        vec![(
            2322,
            "Type '\"appel\"' is not assignable to type '\"apple\"'.".to_string()
        )],
        "a sole-real-member nullable target must elaborate against the real member directly: {pairs:?}",
    );
}

/// Object sources keep their per-property elaboration: a sole-member nullable
/// object target with a property-type mismatch still anchors the leaf, and a
/// missing required property still surfaces TS2741 — neither path is disturbed.
#[test]
fn sole_member_nullable_object_source_unaffected() {
    let leaf = ts2322_messages(
        r#"
const o: { a: number } | undefined = { a: "x" };
"#,
    );
    assert_eq!(
        leaf,
        vec!["Type 'string' is not assignable to type 'number'.".to_string()],
        "object property mismatch still anchors the leaf: {leaf:?}",
    );

    let missing = all_codes(
        r#"
const o: { a: number; b: string } | undefined = { a: 1 };
"#,
    );
    assert!(
        missing.contains(&2741),
        "a missing required property still surfaces TS2741: {missing:?}",
    );
}
