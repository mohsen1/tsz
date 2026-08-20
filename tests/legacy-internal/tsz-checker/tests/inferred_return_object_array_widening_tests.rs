//! Tests for inferred function-return-type literal widening of object and array
//! literals (block-bodied functions).
//!
//! tsc infers a function's return type as `getWidenedType(getUnionType(returns))`.
//! `getUnionType` de-freshes *primitive* literal members (so a multi-branch
//! `"a" | "b"` survives unwidened), but fresh **object/array** literal structure
//! is still widened by `getWidenedType` regardless of union membership:
//!
//! ```ts
//! function f() { return { a: 1, b: "x" }; } // () => { a: number; b: string }
//! function g() { return [1, 2, 3]; }        // () => number[]
//! function h(c: boolean) { if (c) return { v: 1 }; return { v: 2 }; } // () => { v: number }
//! ```
//!
//! tsz previously widened these only when the inferred union collapsed to a
//! single *primitive* literal, so block-bodied object/array returns kept their
//! fresh literal members (`{ a: 1 }`, `(1 | 2 | 3)[]`). That under-widening was a
//! semantic divergence — it suppressed the `TS2322` tsc reports when such a
//! return is assigned to a narrower literal target — not merely a `.d.ts`
//! cosmetic. Per-property `as const` subtrees stay preserved, and a multi-branch
//! primitive literal union is still preserved.
//!
//! Binder names are varied across cases per the anti-hardcoding contract.

use crate::test_utils::check_source_diagnostics;

fn ts2322(src: &str) -> Vec<String> {
    check_source_diagnostics(src)
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn object_literal_return_widens_property_literals() {
    // The inferred return type is `{ a: number; b: string }`, so assigning it to
    // a narrower `{ a: 1 }` target is a TS2322 (tsc parity). Without widening the
    // return would be `{ a: 1; b: "x" }` and this assignment would wrongly pass.
    let errs = ts2322(
        r#"
function makeConfig() { return { a: 1, b: "x" }; }
const narrowed: { a: 1 } = makeConfig();
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "expected one TS2322 from the widened object return, got: {errs:?}"
    );
    assert!(
        errs[0].contains("number"),
        "TS2322 should report the widened `number` property, got: {errs:?}"
    );
}

#[test]
fn object_literal_return_widened_target_is_clean() {
    // The widened return `{ id: number; label: string }` is assignable to the
    // matching target — no false positive from over-widening.
    let errs = ts2322(
        r#"
function buildEntry() { return { id: 1, label: "n" }; }
const accepted: { id: number; label: string } = buildEntry();
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn array_literal_return_widens_element_literals() {
    let narrow = ts2322(
        r#"
function listNumbers() { return [1, 2, 3]; }
const onlyOnes: 1[] = listNumbers();
"#,
    );
    assert_eq!(
        narrow.len(),
        1,
        "expected TS2322: widened `number[]` is not assignable to `1[]`, got: {narrow:?}"
    );

    let wide = ts2322(
        r#"
function listValues() { return [10, 20]; }
const anyNumbers: number[] = listValues();
"#,
    );
    assert!(
        wide.is_empty(),
        "widened `number[]` must be assignable to `number[]`, got: {wide:?}"
    );
}

#[test]
fn multiple_object_returns_widen_and_dedupe() {
    // `{ v: 1 } | { v: 2 }` widens to `{ v: number }` (each fresh member widens,
    // then dedupes), so the narrow `{ v: 1 }` target is a TS2322.
    let errs = ts2322(
        r#"
function selectPayload(flag: boolean) {
    if (flag) return { v: 1 };
    return { v: 2 };
}
const pinned: { v: 1 } = selectPayload(true);
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "expected one TS2322 from the collapsed widened union, got: {errs:?}"
    );
}

#[test]
fn nested_object_and_array_return_widen_deeply() {
    let errs = ts2322(
        r#"
function shape() { return { outer: { inner: 1 }, tags: [true] }; }
const deep: { outer: { inner: 1 }; tags: true[] } = shape();
"#,
    );
    assert!(
        !errs.is_empty(),
        "expected TS2322: nested `inner`/`tags` widen to `number`/`boolean[]`, got: {errs:?}"
    );
}

#[test]
fn array_of_object_literals_return_widens_elements() {
    let narrow = ts2322(
        r#"
function makeRows() { return [{ id: 1 }]; }
const literalRows: { id: 1 }[] = makeRows();
"#,
    );
    assert_eq!(
        narrow.len(),
        1,
        "expected TS2322: element widens to a `number` id property, got: {narrow:?}"
    );

    let wide = ts2322(
        r#"
function makeRecords() { return [{ id: 1 }]; }
const widenedRows: { id: number }[] = makeRecords();
"#,
    );
    assert!(wide.is_empty(), "expected no TS2322, got: {wide:?}");
}

#[test]
fn per_property_const_assertion_in_return_is_preserved() {
    // `{ kind: "tracked" as const, count: 1 }` widens the fresh `count` to
    // `number` while preserving the const-asserted `kind: "tracked"`.
    let keep = ts2322(
        r#"
function describeState() { return { kind: "tracked" as const, count: 1 }; }
const matching: { kind: "tracked"; count: number } = describeState();
"#,
    );
    assert!(
        keep.is_empty(),
        "const-asserted `kind` must stay `\"tracked\"`, got: {keep:?}"
    );

    let mismatch = ts2322(
        r#"
function describeMode() { return { kind: "tracked" as const, total: 1 }; }
const wrong: { kind: "other"; total: number } = describeMode();
"#,
    );
    assert_eq!(
        mismatch.len(),
        1,
        "expected TS2322: preserved `kind: \"tracked\"` is not `\"other\"`, got: {mismatch:?}"
    );
}

#[test]
fn whole_expression_const_assertion_return_is_preserved() {
    // `return { a: 1 } as const` keeps `{ readonly a: 1 }` — not widened.
    let errs = ts2322(
        r#"
function frozen() { return { a: 1 } as const; }
const literal: { readonly a: 1 } = frozen();
"#,
    );
    assert!(
        errs.is_empty(),
        "const-asserted return must not widen, got: {errs:?}"
    );
}

#[test]
fn multi_branch_primitive_literal_return_union_is_preserved() {
    // A primitive literal union is NOT widened (tsc parity): `choose` returns
    // `"a" | "b"`, so a single-member `"a"` target is a TS2322.
    let errs = ts2322(
        r#"
function chooseTag(flag: boolean) {
    if (flag) return "a";
    return "b";
}
const onlyA: "a" = chooseTag(true);
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "expected TS2322: `\"a\" | \"b\"` is not assignable to `\"a\"`, got: {errs:?}"
    );
}

#[test]
fn single_primitive_literal_return_widens() {
    // A lone fresh primitive literal still widens (`return 1` → `number`).
    let errs = ts2322(
        r#"
function answer() { return 1; }
const pinnedOne: 1 = answer();
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "expected TS2322: widened `number` is not assignable to `1`, got: {errs:?}"
    );
}

#[test]
fn renamed_binders_take_the_same_widening_path() {
    // Anti-hardcoding: identical structure, fully renamed binders — the widening
    // must be structural, not keyed on identifier text.
    let errs = ts2322(
        r#"
function ζ_builder() { return { ωkey: 1, λname: "n" }; }
const ψ_target: { ωkey: 1 } = ζ_builder();
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "renamed binders must widen identically, got: {errs:?}"
    );
}
