//! Regression tests for intersection modifier merging through evaluated
//! (aliased) intersections, including homomorphic mapped types over them.
//!
//! tsc merges a shared property's `readonly`/optional modifiers across
//! intersection constituents with AND semantics: the property is readonly
//! (optional) only when *all* contributors are readonly (optional). A
//! structural subtype constituent (`{ readonly a: number }` is a subtype of
//! `{ a?: number }`) must not let intersection simplification drop the
//! writable/optional constituent and silently keep the more-restrictive one.
//!
//! See issue #10863: "mapped key operations can lose optional/readonly
//! modifier intent after merges."

use tsz_checker::test_utils::check_source_strict_messages_without_missing_libs as check_strict;

fn codes(source: &str) -> Vec<u32> {
    check_strict(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// `{ readonly a } & { a? }` merges to a writable, required `a`. Writing to it
/// must not raise TS2540 (read-only) and reading must not require an undefined
/// check (the property is required, not optional).
#[test]
fn aliased_readonly_required_and_optional_intersection_is_writable_required() {
    let source = r#"
type A = { readonly a: number };
type B = { a?: number };
declare const m: A & B;
m.a = 1;
const r: number = m.a;
"#;
    assert!(
        !codes(source).contains(&2540),
        "writable+optional intersection member must keep `a` writable, got: {:?}",
        check_strict(source)
    );
    assert!(
        !codes(source).contains(&2322),
        "intersection of required and optional `a` must stay required, got: {:?}",
        check_strict(source)
    );
}

/// The same merge must survive a homomorphic mapped copy with an identity
/// `as K` key remap — the witness from issue #10863.
#[test]
fn homomorphic_mapped_over_intersection_preserves_writable_required() {
    let source = r#"
type Id<T> = { [K in keyof T as K]: T[K] };
type A = { readonly a: number };
type B = { a?: number };
declare const m: Id<A & B>;
m.a = 1;
const r: number = m.a;
"#;
    assert!(
        !codes(source).contains(&2540),
        "mapped copy of writable+optional intersection must keep `a` writable, got: {:?}",
        check_strict(source)
    );
    assert!(
        !codes(source).contains(&2322),
        "mapped copy must keep `a` required, got: {:?}",
        check_strict(source)
    );
}

/// A plain (no `as`) homomorphic mapped type must merge modifiers identically.
#[test]
fn plain_homomorphic_mapped_over_intersection_preserves_writable() {
    let source = r#"
type Id<T> = { [K in keyof T]: T[K] };
type A = { readonly a: number };
type B = { a?: number };
declare const m: Id<A & B>;
m.a = 1;
"#;
    assert!(
        !codes(source).contains(&2540),
        "plain mapped copy must keep `a` writable, got: {:?}",
        check_strict(source)
    );
}

/// When *all* constituents agree on readonly, the merged property stays
/// readonly — the guard must not over-relax legitimately readonly properties.
#[test]
fn all_readonly_intersection_stays_readonly() {
    let source = r#"
type Id<T> = { [K in keyof T as K]: T[K] };
type A = { readonly a: number };
type B = { readonly a: number };
declare const m: Id<A & B>;
m.a = 1;
"#;
    assert!(
        codes(source).contains(&2540),
        "all-readonly intersection must remain readonly, got: {:?}",
        check_strict(source)
    );
}

/// Renaming key remaps (non-identity) still merge modifiers homomorphically.
#[test]
fn rename_mapped_over_intersection_merges_readonly() {
    let source = r#"
type Prefix<T> = { [K in keyof T as `p_${string & K}`]: T[K] };
type A = { readonly a: number };
type B = { a?: number };
declare const m: Prefix<A & B>;
m.p_a = 1;
"#;
    assert!(
        !codes(source).contains(&2540),
        "renamed mapped copy must keep `p_a` writable, got: {:?}",
        check_strict(source)
    );
}

/// Larger intersection mixing modifiers per property: `a` writable+required,
/// `b` and `c` carried through. Only the genuinely readonly cases stay readonly.
#[test]
fn mixed_modifier_intersection_merges_per_property() {
    let source = r#"
type Id<T> = { [K in keyof T as K]: T[K] };
type A = { readonly a?: number; b: string };
type B = { a: number; c: boolean };
declare const m: Id<A & B>;
m.a = 1;        // a: readonly? AND writable -> writable (ok)
m.b = "x";      // b: writable (ok)
m.c = true;     // c: writable (ok)
const z: number = m.a; // a: optional? AND required -> required (ok)
"#;
    let result = codes(source);
    assert!(
        !result.contains(&2540),
        "mixed intersection must keep `a` writable, got: {:?}",
        check_strict(source)
    );
    assert!(
        !result.contains(&2322),
        "mixed intersection must keep `a` required, got: {:?}",
        check_strict(source)
    );
}
