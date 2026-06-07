//! Nested excess-property (TS2353) checking through a *concrete* homomorphic
//! mapped target whose evaluated property values are themselves object /
//! intersection shapes — the recursive-utility-over-intersection family from
//! `excessPropertyCheckIntersectionWithRecursiveType` (TypeScript #44750).
//!
//! Structural rule: when a fresh object literal is checked against a concrete
//! homomorphic mapped target `{ [K in keyof T]: F<T[K]> }` and every top-level
//! key is present, tsc still descends into each present property's nested
//! object literal and reports a deeper excess key. tsz's
//! `report_concrete_mapped_target_excess_property` used to check excess only at
//! the mapped level and then claim the literal was fully handled, which skipped
//! that descent and silently dropped the nested TS2353. The fix defers to the
//! recursion-capable simple-object / intersection branches when there is no
//! excess at the mapped level.
//!
//! The witness needs two structural ingredients together: (1) the mapped target
//! stays a *raw* mapped type at the property position (it does not get evaluated
//! to a plain object before the excess walk), which happens when a recursive
//! conditional alias places its `& Example<T>` *inside* the conditional branches
//! rather than outside; and (2) at least two levels of nesting, so the excess
//! sits below the mapped level. Binder names are varied so the behaviour is
//! structural, not spelling specific.

use crate::test_utils::check_source_strict_messages_without_missing_libs;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_messages_without_missing_libs(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// Conditional alias with `& Example<T>` *inside* both branches (the Schema2
/// shape). The inner `props` mapped target stays a raw mapped type, so the
/// nested excess key two levels down was previously dropped.
#[test]
fn intersection_inside_branch_recursive_mapped_flags_nested_excess() {
    let diags = check_source_strict_messages_without_missing_libs(
        "type Req = { l1: { l2: boolean } };\n\
         type Example<T> = { ex?: T | null };\n\
         type Schema<T> = (T extends boolean\n\
            ? { type: 'boolean' } & Example<T>\n\
            : { props: { [P in keyof T]: Schema<T[P]> } } & Example<T>);\n\
         const o: Schema<Req> = { props: { l1: { props: { l2: { type: 'boolean' }, invalid: false } } } };",
    );
    let cs: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        cs.contains(&2353),
        "nested excess key 'invalid' two levels under a recursive mapped target must be TS2353; got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2353 && m.contains("invalid")),
        "the TS2353 must name the excess key 'invalid'; got {diags:?}"
    );
}

/// The sibling shape with `& Example<T>` *outside* the conditional (Schema1):
/// the property value is evaluated to a plain object before the excess walk, so
/// this already worked. Kept as a control that the fix did not disturb it.
#[test]
fn intersection_outside_branch_recursive_mapped_flags_nested_excess() {
    let cs = codes(
        "type Req = { l1: { l2: boolean } };\n\
         type Example<T> = { ex?: T | null };\n\
         type Schema<T> = (T extends boolean\n\
            ? { type: 'boolean' }\n\
            : { props: { [P in keyof T]: Schema<T[P]> } }) & Example<T>;\n\
         const o: Schema<Req> = { props: { l1: { props: { l2: { type: 'boolean' }, invalid: false } } } };",
    );
    assert!(
        cs.contains(&2353),
        "nested excess key must be TS2353 for the outside-intersection shape too; got {cs:?}"
    );
}

/// `Example<T>` placed first in the intersection (Schema3) — order independence.
#[test]
fn intersection_leading_example_recursive_mapped_flags_nested_excess() {
    let cs = codes(
        "type Req = { l1: { l2: boolean } };\n\
         type Example<T> = { ex?: T | null };\n\
         type Schema<T> = Example<T> & (T extends boolean\n\
            ? { type: 'boolean' }\n\
            : { props: { [P in keyof T]: Schema<T[P]> } });\n\
         const o: Schema<Req> = { props: { l1: { props: { l2: { type: 'boolean' }, invalid: false } } } };",
    );
    assert!(
        cs.contains(&2353),
        "nested excess key must be TS2353 with leading Example<T>; got {cs:?}"
    );
}

/// Renamed binders prove the rule is structural rather than spelling specific.
#[test]
fn renamed_binders_recursive_mapped_flags_nested_excess() {
    let diags = check_source_strict_messages_without_missing_libs(
        "type Tree = { branch: { leaf: boolean } };\n\
         type Wrap<U> = { tag?: U | null };\n\
         type Node<U> = (U extends boolean\n\
            ? { kind: 'boolean' } & Wrap<U>\n\
            : { members: { [Q in keyof U]: Node<U[Q]> } } & Wrap<U>);\n\
         const tree: Node<Tree> = { members: { branch: { members: { leaf: { kind: 'boolean' }, bogus: 1 } } } };",
    );
    let cs: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        cs.contains(&2353),
        "renamed-binder recursive mapped target must still flag nested excess; got {diags:?}"
    );
    assert!(
        diags.iter().any(|(c, m)| *c == 2353 && m.contains("bogus")),
        "the TS2353 must name the excess key 'bogus'; got {diags:?}"
    );
}

/// Excess sitting three levels deep is still reported (the descent is not
/// bounded to a single recursion step).
#[test]
fn deep_nested_excess_under_recursive_mapped_is_reported() {
    let cs = codes(
        "type Req = { a: { b: { c: boolean } } };\n\
         type Example<T> = { ex?: T | null };\n\
         type Schema<T> = (T extends boolean\n\
            ? { type: 'boolean' } & Example<T>\n\
            : { props: { [P in keyof T]: Schema<T[P]> } } & Example<T>);\n\
         const o: Schema<Req> = { props: { a: { props: { b: { props: { c: { type: 'boolean' }, nope: 0 } } } } } };",
    );
    assert!(
        cs.contains(&2353),
        "excess key three levels deep must be TS2353; got {cs:?}"
    );
}

/// Negative control: a structurally valid literal (no extra key) must NOT
/// produce a spurious TS2353. Guards against the fix over-firing on present
/// properties.
#[test]
fn valid_recursive_mapped_literal_has_no_excess_error() {
    let cs = codes(
        "type Req = { l1: { l2: boolean } };\n\
         type Example<T> = { ex?: T | null };\n\
         type Schema<T> = (T extends boolean\n\
            ? { type: 'boolean' } & Example<T>\n\
            : { props: { [P in keyof T]: Schema<T[P]> } } & Example<T>);\n\
         const o: Schema<Req> = { props: { l1: { props: { l2: { type: 'boolean' } } } } };",
    );
    assert!(
        !cs.contains(&2353),
        "a valid recursive-mapped literal must not report excess; got {cs:?}"
    );
}
