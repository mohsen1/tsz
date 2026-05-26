//! Regression test for #9730: type-parameter inference from a `readonly`
//! array/tuple element position widened the element literals.
//!
//! ## Structural rule
//!
//! When a type parameter `T` is inferred from an element position of a
//! `readonly` array or tuple source (e.g. an `as const` argument, or any
//! `readonly T[]` / `readonly [..]` value), tsc treats the element literals
//! as **non-fresh** and does NOT widen them. So `new Set([1, 2] as const)`
//! infers `Set<1 | 2>`, not `Set<number>`. A *mutable* array source still
//! widens its fresh element literals (`new Set([1, 2])` -> `Set<number>`),
//! because only fresh literals widen.
//!
//! This is keyed on the structural readonly-ness of the source, not on any
//! identifier spelling or on `Set`/`Map` specifically — it holds for any
//! generic constructor or function whose parameter is a readonly array/tuple
//! (see §25 ANTI-HARDCODING DIRECTIVE and §26 GENERALIZATION GATE). The tests
//! below vary the type-parameter names (`T`, `E`, `K`, `V`) and the carrier
//! shape (class constructor, free function, nested readonly tuple) to prove
//! the rule is structural.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn ts2322_diags(source: &str) -> Vec<String> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.clone())
        .collect()
}

/// Reported repro shape: a generic constructor taking `readonly T[]` called
/// with an `as const` array preserves the element literals.
#[test]
fn readonly_ctor_param_preserves_const_array_literals() {
    let source = r#"
declare class MySet<T> { constructor(values: readonly T[]); val: T; }
const ms = new MySet([1, 2] as const);
const bad: 1 = ms.val;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        !ds[0].contains("number") && ds[0].contains('1') && ds[0].contains('2'),
        "element literals should be preserved as `1 | 2`, not widened: {ds:?}",
    );
}

/// Renamed type parameter + string literals: the rule must not depend on the
/// type-parameter name or the literal kind.
#[test]
fn readonly_ctor_param_preserves_const_array_literals_renamed() {
    let source = r#"
declare class MyBag<E> { constructor(items: readonly E[]); item: E; }
const mb = new MyBag(["x", "y"] as const);
const bad: "x" = mb.item;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        ds[0].contains("\"x\"") && ds[0].contains("\"y\""),
        "string literals should be preserved as `\"x\" | \"y\"`: {ds:?}",
    );
}

/// Nested readonly tuple (the `Map<K, V>` shape): a `readonly (readonly
/// [K, V])[]` parameter against `[['a', 1]] as const` preserves both `K` and
/// `V` element literals.
#[test]
fn readonly_nested_tuple_param_preserves_key_and_value_literals() {
    let source = r#"
declare class MyMap<K, V> { constructor(entries: readonly (readonly [K, V])[]); k: K; v: V; }
const mm = new MyMap([["a", 1]] as const);
const badKey: "z" = mm.k;
const badVal: 9 = mm.v;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(
        ds.len(),
        2,
        "expected two TS2322 (key + value), got: {ds:?}"
    );
    assert!(
        ds.iter().any(|m| m.contains("\"a\"")),
        "key literal `\"a\"` should be preserved: {ds:?}",
    );
    assert!(
        ds.iter().any(|m| m.contains("Type '1'")),
        "value literal `1` should be preserved: {ds:?}",
    );
}

/// Free function with a `readonly T[]` parameter — same rule, different
/// carrier shape (no constructor / no class involved).
#[test]
fn readonly_function_param_preserves_const_array_literals() {
    let source = r#"
declare function pick<T>(xs: readonly T[]): T;
const p = pick([10, 20] as const);
const bad: 10 = p;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        !ds[0].contains("number") && ds[0].contains("10") && ds[0].contains("20"),
        "element literals should be preserved as `10 | 20`: {ds:?}",
    );
}

/// Negative control: a *mutable* array argument to a `readonly T[]` parameter
/// still widens its fresh element literals, matching tsc.
#[test]
fn mutable_array_into_readonly_param_still_widens() {
    let source = r#"
declare function pick<T>(xs: readonly T[]): T;
const p = pick([10, 20]);
const bad: 10 = p;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        ds[0].contains("number"),
        "mutable-array element literals should widen to `number`: {ds:?}",
    );
}

/// Negative control: a fully mutable array argument to a mutable `T[]`
/// parameter widens (the pre-existing, correct behavior).
#[test]
fn mutable_array_into_mutable_param_widens() {
    let source = r#"
declare class MySet<T> { constructor(values: readonly T[]); val: T; }
const ms = new MySet([1, 2]);
const bad: 1 = ms.val;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        ds[0].contains("number"),
        "fresh element literals should widen to `number`: {ds:?}",
    );
}

/// `Iterable<T>` carrier: an `as const` array is iterable, and inference from
/// the iterator element position preserves the readonly element literals too.
#[test]
fn iterable_param_preserves_const_array_literals() {
    let source = r#"
interface Iterator2<T> { next(): { value: T; done: boolean }; }
interface Iterable2<T> { [Symbol.iterator](): Iterator2<T>; }
declare function takeIter<T>(x: Iterable2<T>): T;
const it = takeIter([1, 2] as const);
const bad: 1 = it;
"#;
    let ds = ts2322_diags(source);
    assert_eq!(ds.len(), 1, "expected one TS2322, got: {ds:?}");
    assert!(
        !ds[0].contains("number") && ds[0].contains('1') && ds[0].contains('2'),
        "iterable element literals should be preserved as `1 | 2`: {ds:?}",
    );
}
