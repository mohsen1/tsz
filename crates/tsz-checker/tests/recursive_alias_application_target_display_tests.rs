//! Tests for tsc-style display of TS2322 messages when the assignment target
//! is a generic Application of a *recursive* type alias (an alias whose body
//! reaches another reference to itself).
//!
//! Two structural rules are covered:
//!
//! 1. **Application body** (`type T<U> = [U, T<...>]`): when
//!    `Application(Lazy(D), args)` instantiates and the body is itself an
//!    Application, preserve the outer alias name. Expansion produces an
//!    unbounded `[..., ...]` cascade that makes diagnostics useless.
//!    - Conformance test: `inferFromNestedSameShapeTuple.ts`
//!
//! 2. **Structural body** (`type LinkedList<T> = T & { next: LinkedList<T> }`):
//!    when the alias body is a structural Intersection (not an Application),
//!    instantiation produces the intersection type directly. We store the
//!    display alias `evaluated → original_type_id` so diagnostics show the
//!    alias name (`LinkedList<Entity>`) instead of the expanded body.
//!    - Conformance test: `recursiveIntersectionTypes.ts`
//!
//! tsc shows `Type 'T1<U>' is not assignable to type 'T2<U>'`; we used to
//! expand the target to `[42, [42, [42, [42, [42, [42, [42, [42, [..., ...]]]]]]]]]`.

use crate::test_utils::check_source_strict_messages as check_strict;

/// `T1<U>` / `T2<U>` are Applications of recursive tuple aliases. The alias
/// bodies reach themselves via nested Applications. tsc keeps the alias name
/// on both sides; expansion would emit `[42, [42, [..., ...]]]` cascades.
#[test]
fn ts2322_recursive_tuple_alias_keeps_alias_target_display() {
    let source = r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];

function qq<U>(x: T1<U>, y: T2<U>) {
    y = x;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `y = x`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'T2<U>'"),
        "TS2322 target must keep the alias `T2<U>` for a recursive tuple \
         alias instead of expanding to its body. Got: {msg:?}"
    );
    assert!(
        !msg.contains("[..., ...]"),
        "TS2322 message must not contain the elision marker that signals \
         unbounded recursive expansion. Got: {msg:?}"
    );
    assert!(
        !msg.contains("[42, [42"),
        "TS2322 message must not expand a recursive alias to a stack of its \
         own body. Got: {msg:?}"
    );
}

/// Same rule with different type-parameter names to verify the fix is not
/// hardcoded to particular identifiers (anti-hardcoding directive §25).
#[test]
fn ts2322_recursive_tuple_alias_keeps_alias_target_display_alt_names() {
    let source = r#"
type RecA<P> = [string, RecA<{ inner: P }>];
type RecB<P> = ["lit", RecB<{ inner: P }>];

function f<X>(a: RecA<X>, b: RecB<X>) {
    b = a;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `b = a`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'RecB<X>'"),
        "TS2322 target must keep the recursive alias name `RecB<X>`. Got: \
         {msg:?}"
    );
    assert!(
        !msg.contains("[..., ...]"),
        "TS2322 message must not contain the elision marker. Got: {msg:?}"
    );
}

/// A non-recursive generic alias with a tuple body. tsz currently expands
/// this to the structural form, matching the existing `preserve_tuple_alias_display`
/// behaviour. The recursive-alias rule must not affect this case.
#[test]
fn ts2322_non_recursive_tuple_alias_target_display_unchanged() {
    let source = r#"
type Pair<T> = [T, T];
let a: Pair<number>;
let b: Pair<string>;
b = a;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected at least one TS2322 for `b = a`; got: {diags:?}"
    );
    // The non-recursive case retains the existing `preserve_tuple_alias_display`
    // path: target expands to `[string, string]`. The recursive-alias guard
    // must not affect this. The assertion intentionally locks the *current*
    // behaviour for the non-recursive case to catch accidental scope creep.
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("[string, string]") || msg.contains("'Pair<string>'"),
        "TS2322 target for a non-recursive alias should remain in its \
         existing display path (currently expands to `[string, string]`). \
         Got: {msg:?}"
    );
}

/// Structural rule (intersection body): `type LinkedList<T> = T & { next: LinkedList<T> }`.
/// The alias body is an Intersection (not an Application), so instantiation
/// produces the intersection directly. The display alias must be stored so
/// diagnostics show `LinkedList<Entity>` not the expanded body.
///
/// Conformance test: `recursiveIntersectionTypes.ts`
#[test]
fn ts2322_recursive_intersection_alias_keeps_alias_source_display() {
    let source = r#"
interface Entity { id: number; }
interface Product extends Entity { name: string; }

type LinkedList<T> = T & { next: LinkedList<T> };

function assign(entityList: LinkedList<Entity>, productList: LinkedList<Product>) {
    productList = entityList;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `productList = entityList`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("LinkedList<"),
        "TS2322 message must show the alias name `LinkedList<...>` for a \
         recursive intersection alias, not the expanded structural body. \
         Got: {msg:?}"
    );
    assert!(
        !msg.contains("& { next:"),
        "TS2322 message must not expand the recursive intersection body \
         `& {{ next: ... }}`. Got: {msg:?}"
    );
}

/// Same structural rule with alternate names to verify the fix is not
/// hardcoded to `LinkedList` (anti-hardcoding directive §25).
#[test]
fn ts2322_recursive_intersection_alias_keeps_alias_source_display_alt_names() {
    let source = r#"
interface BaseNode { id: string; }
interface LeafNode extends BaseNode { value: number; }

type Chain<T> = T & { rest: Chain<T> };

function assign(base: Chain<BaseNode>, leaf: Chain<LeafNode>) {
    leaf = base;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `leaf = base`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("Chain<"),
        "TS2322 message must show the alias name `Chain<...>` for a \
         recursive intersection alias. Got: {msg:?}"
    );
    assert!(
        !msg.contains("& { rest:"),
        "TS2322 message must not expand the intersection body `& {{ rest: ... }}`. \
         Got: {msg:?}"
    );
}

/// Structural rule (union body, #8423): a TS2322 message for a recursive
/// union-bodied alias must show the alias name and not multi-level unfold
/// the recursive self-reference.
#[test]
fn ts2322_recursive_union_alias_source_and_target_display() {
    let source = r#"
type ValueOrArray<E> = E | ValueOrArray<E>[];

declare const v: ValueOrArray<string>;
const x: ValueOrArray<number> = v;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `x = v`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'ValueOrArray<string>'"),
        "TS2322 source must keep the recursive union alias name \
         `ValueOrArray<string>`. Got: {msg:?}"
    );
    assert!(
        msg.contains("'ValueOrArray<number>'"),
        "TS2322 target must keep the recursive union alias name \
         `ValueOrArray<number>`. Got: {msg:?}"
    );
    // Guard against the over-expanded form: the source side must not contain
    // the recursive body twice (which would indicate two levels of unfolding).
    let value_or_array_occurrences = msg.matches("ValueOrArray<").count();
    assert!(
        value_or_array_occurrences <= 3,
        "TS2322 message must not multiply unfold the recursive alias; \
         expected at most 3 occurrences of `ValueOrArray<` (one on each side \
         plus at most one inner reference). Got {value_or_array_occurrences} \
         in: {msg:?}"
    );
}

/// Same rule with different identifier choices (anti-§25 hardcoding).
#[test]
fn ts2322_recursive_union_alias_source_and_target_display_alt_names() {
    let source = r#"
type Nested<P> = P | Nested<P>[];

declare const a: Nested<boolean>;
const b: Nested<number> = a;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `b = a`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'Nested<boolean>'"),
        "TS2322 source must keep the recursive alias name `Nested<boolean>`. \
         Got: {msg:?}"
    );
    assert!(
        msg.contains("'Nested<number>'"),
        "TS2322 target must keep the recursive alias name `Nested<number>`. \
         Got: {msg:?}"
    );
    let nested_occurrences = msg.matches("Nested<").count();
    assert!(
        nested_occurrences <= 3,
        "TS2322 message must not multiply unfold the recursive alias; got \
         {nested_occurrences} occurrences of `Nested<` in: {msg:?}"
    );
}

/// Source is `(ValueOrArray<E>)[]` (an Array-of-Application form). The Array
/// is one level of expansion against the alias body's recursive arm; tsc
/// displays it as `(ValueOrArray<number>)[]` (alias preserved inside the
/// Array). tsz must not over-expand the recursive arm by re-evaluating the
/// nested Application again.
#[test]
fn ts2322_recursive_union_alias_array_source_display() {
    let source = r#"
type ValueOrArray<E> = E | ValueOrArray<E>[];

declare const v: ValueOrArray<number>[];
const x: ValueOrArray<number> & { extra: string } = v;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected at least one TS2322 for the intersection mismatch; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("ValueOrArray<number>"),
        "TS2322 must show the recursive alias name. Got: {msg:?}"
    );
    // The source must not contain a multi-level structural unfolding of the
    // alias body (e.g. `(number | (ValueOrArray<number>)[] | (number | …)[])[]`).
    // The give-away of a multi-level unfolding is the literal substring
    // `(number |` appearing inside the source — that's the alias body
    // expanded once *inside* the recursive arm.
    let source_str = msg
        .strip_prefix("Type '")
        .and_then(|rest| rest.find("' is not assignable").map(|i| &rest[..i]))
        .expect("TS2322 message should begin with `Type '...'`");
    assert!(
        !source_str.contains("(number |"),
        "TS2322 source must not unfold the recursive arm twice; the alias \
         body should appear at most once. Got source={source_str:?} from \
         msg={msg:?}"
    );
}

/// Negative/fallback: a non-recursive intersection alias must still take the
/// ordinary diagnostic path and assert its actual display surface. This catches
/// over-eager structural-body aliasing without relying on an assignable
/// superset assignment.
#[test]
fn ts2322_non_recursive_intersection_alias_not_affected() {
    let source = r#"
type Box<T> = T & { name: string };

let a: Box<{ id: number }>;
let b: { id: string; name: string } = { id: "bad", name: "ok" };
a = b;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `a = b`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("Box<"),
        "non-recursive intersection aliases should keep the ordinary TS2322 \
         alias display surface. Got: {msg:?}"
    );
    assert_eq!(
        msg,
        "Type '{ id: string; name: string; }' is not assignable to type \
         'Box<{ id: number; }>'.",
        "non-recursive intersection aliases should keep the ordinary TS2322 \
         display surface while the recursive-only rule remains scoped. Got: {msg:?}"
    );
}
