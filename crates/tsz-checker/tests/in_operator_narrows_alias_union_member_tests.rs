//! Regression coverage for #14153: the `in` operator must narrow a *type alias /
//! generic application* union to the member declaring the key.
//!
//! Structural rule: when the `in` receiver is a union reached through a type
//! alias instantiation or generic application (e.g. `Enumerable<T> = ArrayLike<T>
//! | Iterable<T>`), `tsc`'s `narrowTypeByInKeyword` filters the constituents of
//! the *resolved* union — true branch keeps members whose apparent type has the
//! (string-literal) key, false branch keeps the complement. tsz now resolves the
//! alias/application to its structural union before the union-vs-non-union
//! decision in the solver `in`-narrowing path, so `"length" in items` narrows to
//! the `ArrayLike<T>` member and `items.length` is `number`.
//!
//! Before the fix the alias instantiation reached narrowing as a
//! `TypeData::Application`, `union_list_id` returned `None`, the union-filtering
//! path was skipped, and the receiver fell through to the non-union branch
//! (`union & Record<prop, unknown>`), so the property read back as `unknown` —
//! remeda's `length.ts:35` emitted a false `TS2322` (`unknown` not assignable to
//! `number`).
//!
//! A non-literal computed key (`k in x`, `k: string`) must still NOT narrow
//! (negative control). Verified against `tsc` 5.8.3.
//!
//! The remeda witness uses `ArrayLike<T> | Iterable<T>`; these cases use
//! lib-free generic aliases of the same shape (a generic application union where
//! one member declares the key) so the checker test harness resolves every name
//! while still exercising the `TypeData::Application` resolution path.

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2322_count(diagnostics: &[(u32, String)]) -> usize {
    diagnostics.iter().filter(|(code, _)| *code == 2322).count()
}

#[test]
fn in_operator_narrows_generic_alias_application_union_to_member_with_key() {
    // remeda length.ts shape, lib-free so the harness resolves all names: a
    // *generic* alias instantiation `Enumerable<T>` reaches narrowing as a
    // `TypeData::Application` (not a raw `Union`), which is exactly the path the
    // fix resolves. Only `WithLen<T>` has `length: number`; `"length" in items`
    // narrows to it so `items.length` reads back as `number`.
    let diagnostics = check_source_strict_messages(
        r#"
type WithLen<T> = { length: number; item: T };
type WithoutLen<T> = { item: T; tag: string };
type Enumerable<T> = WithLen<T> | WithoutLen<T>;
export const inLen = <T>(items: Enumerable<T>): number =>
  "length" in items ? items.length : 0;
"#,
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "expected `in` to narrow the generic-alias-application union member and read `length` as number, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_narrows_renamed_generic_alias_application_union_to_member_with_key() {
    // Binder names vary; the structural rule must not depend on identifiers.
    let diagnostics = check_source_strict_messages(
        r#"
type Sized<Item> = { length: number; first: Item };
type Unsized<Item> = { first: Item; note: string };
type Collection<Item> = Sized<Item> | Unsized<Item>;
export const size = <Item>(coll: Collection<Item>): number =>
  "length" in coll ? coll.length : 0;
"#,
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "renamed generic-alias-application union should still narrow via `in`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_narrows_object_alias_union_true_branch() {
    // Alias over a concrete object union: the key exists on one member only.
    let diagnostics = check_source_strict_messages(
        r#"
type AB = { a: number } | { b: string };
export const readA = (x: AB): number => ("a" in x ? x.a : 0);
"#,
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "object alias union true branch should narrow to the key-bearing member, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_narrows_object_alias_union_false_branch() {
    // The false branch keeps the complement member.
    let diagnostics = check_source_strict_messages(
        r#"
type AB = { a: number } | { b: string };
export const readB = (x: AB): string => ("a" in x ? "" : x.b);
"#,
    );
    assert_eq!(
        ts2322_count(&diagnostics),
        0,
        "object alias union false branch should narrow to the complement member, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_does_not_narrow_with_non_literal_key() {
    // Negative control: a non-literal computed key cannot identify a member, so
    // `x.a` stays an error (tsc: TS2339). The fix must not over-narrow here.
    let diagnostics = check_source_strict_messages(
        r#"
type AB = { a: number } | { b: string };
export const bad = (x: AB, k: string): number => (k in x ? x.a : 0);
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2339),
        "non-literal `in` key must not narrow; expected TS2339 on `x.a`, got {diagnostics:#?}"
    );
}
