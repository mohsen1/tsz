//! Tests for TS2589 (excessively deep instantiation) and TS2615 (circular
//! mapped-type property) as they interact with self-referential mapped
//! types. Split out of `ts2589_tests.rs` to stay under the arch-size test
//! file line cap (#16745).

use crate::test_utils::{check_source_code_messages as get_diagnostics, check_source_diagnostics};

/// TS2615: circular mapped type in a type alias with indexed access, where
/// the mapped type constraint resolves to a concrete string literal key.
///
/// Repro from microsoft/TypeScript#30050 (`recursivelyExpandingUnionNoStackoverflow.ts`):
/// `type N<T, K extends string> = T | { [P in K]: N<T, K> }[K];`
/// `type M = N<number, "M">;`
///
/// Verified directly against pinned `typescript@7.0.2`: tsc emits ONLY
/// TS2615 here, not TS2589 — the mapped-type property-circularity check and
/// the excessively-deep-instantiation depth guard are independent signals,
/// not a bundle. (A prior version of this test asserted both fired
/// together; that was never verified against real tsc and was itself the
/// conformance false positive tracked by this fix — tsz's `TS2589`
/// alongside `TS2615` here was the bug.)
#[test]
fn circular_mapped_type_alias_emits_ts2615_only() {
    let source = r#"
type N<T, K extends string> = T | { [P in K]: N<T, K> }[K];
type M = N<number, "M">;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit TS2589 when only the mapped-type circularity check trips, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 2615),
        "Should emit TS2615 for circular mapped type property, got: {diags:?}"
    );
    let ts2615 = diags.iter().find(|d| d.0 == 2615).unwrap();
    assert!(
        ts2615.1.contains("'M'"),
        "TS2615 message should reference property 'M', got: {}",
        ts2615.1
    );
    assert!(
        ts2615.1.contains(r#"[P in "M"]"#),
        "TS2615 message should include mapped type with quoted key, got: {}",
        ts2615.1
    );
}

/// Same shape as `circular_mapped_type_alias_emits_ts2615_only`, with
/// renamed type-alias/parameter binders and a second concrete-key
/// instantiation, to confirm the fix isn't keyed on the specific names or a
/// single use site. Verified against pinned `typescript@7.0.2`: only TS2615
/// at each instantiation, no TS2589.
#[test]
fn circular_mapped_type_alias_ts2615_only_renamed_binders() {
    let source = r#"
type Circ<Val, Key extends string> = Val | { [P in Key]: Circ<Val, Key> }[Key];
type Result = Circ<boolean, "X">;
type Other = Circ<string, "Y">;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit TS2589 for a renamed-binder circular mapped type, got: {diags:?}"
    );
    assert_eq!(
        diags.iter().filter(|d| d.0 == 2615).count(),
        2,
        "Should emit one TS2615 per concrete-key instantiation, got: {diags:?}"
    );
}

/// Negative control: a genuinely excessively-deep type alias (no mapped-type
/// circularity involved at all) must still report TS2589 on its own —
/// splitting the TS2589/TS2615 emission must not silence the depth guard
/// for the case it was designed for. Mirrors `ts2589_message_text` in
/// `ts2589_tests.rs`.
#[test]
fn non_mapped_recursive_alias_still_reports_ts2589_alone() {
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2589),
        "Should still emit TS2589 for genuine excessive depth, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2615),
        "Should not emit TS2615 when the alias body isn't a self-referencing mapped type, got: {diags:?}"
    );
}

/// TS2615 should NOT be emitted when the mapped type constraint resolves to
/// multiple keys (e.g., `keyof T`). In that case, tsc only emits TS2589.
#[test]
fn circular_mapped_type_alias_no_ts2615_for_keyof_constraint() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type tup = [number, number, number, number];
function foo(arg: Circular<tup>): tup {
    return arg;
}
"#;
    let diags = get_diagnostics(source);
    // tsc does not emit TS2615 for `Circular<tup>` because `keyof tup`
    // doesn't resolve to a single concrete string literal key.
    // (The interface-level TS2615 is a separate check in interface_checks.rs.)
    // Here we only verify the type-alias-application path doesn't false-positive.
    let alias_ts2615 = diags
        .iter()
        .filter(|d| d.0 == 2615 && d.1.contains("'?'"))
        .count();
    assert_eq!(
        alias_ts2615, 0,
        "Should NOT emit TS2615 with '?' placeholder for keyof constraint, got: {diags:?}"
    );
}

/// A recursive mapped type whose template contains the alias itself in a union
/// with a ground type should NOT emit TS2589.  tsc handles this coinductively.
///
/// Regression for: <https://github.com/tsz-org/tsz/issues/6169>
#[test]
fn recursive_mapped_type_with_union_ground_type_no_ts2589() {
    let source = r#"
type RecursiveRecord<K extends string, V> = {
    [P in K]: V | RecursiveRecord<K, V>;
};
const rec: RecursiveRecord<string, number> = {
    a: 1,
    b: { c: 2, d: { e: 3 } },
};
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "RecursiveRecord<K,V> should NOT emit TS2589; got: {diags:?}"
    );
}

/// A mapped type whose template IS purely the self-reference (no ground union)
/// should also not emit TS2589 — tsc uses coinductive handling for it too.
#[test]
fn purely_self_referential_mapped_type_no_ts2589() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
const x: Circular<{ a: string }> = { a: { a: { a: {} as any } } };
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "A direct-body mapped type alias should NOT emit TS2589; got: {diags:?}"
    );
}

/// When a homomorphic self-referential mapped-type alias is applied to a tuple
/// argument and checked against that tuple type, tsc detects the infinite
/// instantiation chain and emits TS2589 instead of TS2322.
///
/// Structural rule: `type A<T> = { [P in keyof T]: A<T> }` applied to a tuple
/// `tup` produces a tuple of `A<tup>` applications; element-by-element comparison
/// against `tup` would recurse infinitely, so tsc emits TS2589.
#[test]
fn homomorphic_self_mapped_tuple_arg_vs_tuple_target_emits_ts2589() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type tup = [number, number, number, number];
function foo(arg: Circular<tup>): tup {
    return arg;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2589 = diags
        .iter()
        .find(|d| d.code == 2589)
        .expect("Should emit TS2589 for Circular<tup> vs tup");
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Should NOT emit TS2322 when TS2589 fires; got: {diags:?}"
    );
    let start = ts2589.start as usize;
    assert_eq!(
        &source[start..start + "Circular<tup>".len()],
        "Circular<tup>",
        "Should emit TS2589 for Circular<tup> vs tup; got: {diags:?}"
    );
}

/// The cycle detection must not depend on the alias spelling.
/// Renaming `Circular` to `Loop` and `tup` to `Pair` must produce the same result.
#[test]
fn homomorphic_self_mapped_tuple_arg_renamed_alias_emits_ts2589() {
    let source = r#"
type Loop<X> = { [K in keyof X]: Loop<X> };
type Pair = [string, boolean];
function bar(arg: Loop<Pair>): Pair {
    return arg;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2589 = diags
        .iter()
        .find(|d| d.code == 2589)
        .expect("Should emit TS2589 for Loop<Pair> vs Pair regardless of alias name");
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Should NOT emit TS2322 when TS2589 fires; got: {diags:?}"
    );
    let start = ts2589.start as usize;
    assert_eq!(
        &source[start..start + "Loop<Pair>".len()],
        "Loop<Pair>",
        "TS2589 should anchor at the explicit source type annotation; got: {ts2589:?}"
    );
}

/// Single-element tuple: the rule applies regardless of tuple length.
#[test]
fn homomorphic_self_mapped_single_element_tuple_emits_ts2589() {
    let source = r#"
type Mirror<T> = { [P in keyof T]: Mirror<T> };
type Solo = [number];
function baz(arg: Mirror<Solo>): Solo {
    return arg;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2589),
        "Should emit TS2589 for Mirror<Solo> vs Solo (single-element tuple); got: {diags:?}"
    );
}

/// Non-tuple object argument: coinductive handling applies, no TS2589.
#[test]
fn homomorphic_self_mapped_object_arg_no_ts2589() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type Obj = { x: number; y: string };
function foo(arg: Circular<Obj>): Obj {
    return arg;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit TS2589 for Circular<Obj> with object arg; got: {diags:?}"
    );
}

/// An alias with union-or-ground template is NOT a self-referential mapped type;
/// it must not emit TS2589 even with a tuple argument.
#[test]
fn recursive_mapped_with_union_ground_and_tuple_no_ts2589() {
    let source = r#"
type DeepMap<T> = { [K in keyof T]: number | DeepMap<T> };
type nums = [number, number];
function foo(arg: DeepMap<nums>): nums {
    return arg;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Union-ground template should NOT emit TS2589; got: {diags:?}"
    );
}
