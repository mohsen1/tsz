//! Regression coverage: generic type-parameter defaults must stay scoped to
//! their own declaration and instantiation — they must not "bleed" between
//! independent branches (independent aliases, independent instantiations of
//! the same alias, parallel object members, or shadowed inner type params).
//!
//! Tracks the `solver-29-20` "generic defaults bleed between independent
//! branches" family (mohsen1/tsz#11608 and siblings #11589 / #11487).
//!
//! Structural rule under test:
//!
//! > When a generic declaration has a defaulted type parameter, each
//! > instantiation resolves that default against *its own* explicitly
//! > supplied arguments only; a default resolved for one instantiation (or
//! > one declaration that happens to share a type-parameter name) must never
//! > be reused for an independent instantiation/declaration.
//!
//! Per the repo anti-hardcoding directive (§25) the cases below vary the
//! user-chosen type-parameter names (`T`/`K`, `A`/`B`, `P`/`Q`) so a fix that
//! keyed on a specific spelling would fail at least one case.

use tsz_checker::test_utils::{check_source_strict, diagnostic_codes, diagnostics_without_codes};

/// Semantic error codes, dropping TS2318 ("cannot find global type") which is
/// expected noise in the no-stdlib unit harness. Mirrors the convention in
/// `distributive_conditional_default_tests.rs`.
fn errors(source: &str) -> Vec<u32> {
    diagnostic_codes(&diagnostics_without_codes(
        &check_source_strict(source),
        &[2318],
    ))
}

/// Two independent aliases that declare different defaults must each resolve
/// to their own default; the `number` default of one must not leak into the
/// other. The two aliases use *different* type-parameter names (`T` vs `K`)
/// so the invariant is exercised across name choices in a single case (§25).
#[test]
fn independent_aliases_keep_their_own_default() {
    let codes = errors(
        r#"
type FirstT<T = string> = T;
type SecondK<K = number> = K;

const a: FirstT = "ok";   // FirstT resolves to string
const b: SecondK = 1;     // SecondK resolves to number
const bad: SecondK = "x"; // string is not assignable to number
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "only the mismatched `SecondK` assignment should error; got {codes:?}"
    );
}

/// A default that references an earlier type parameter (`B = A`) must be
/// resolved independently for each instantiation. `Pair<string>` must yield
/// `second: string` and `Pair<number>` must yield `second: number` — the
/// resolved default from one instantiation must not be reused for the other.
/// Two parameter spellings (`A`/`B` and `P`/`Q`) prove the rule is not keyed
/// on the names (§25).
#[test]
fn earlier_param_default_resolves_per_instantiation() {
    let codes = errors(
        r#"
type PairAB<A, B = A> = { first: A; second: B };
type PairPQ<P, Q = P> = { first: P; second: Q };

const okAB: PairAB<string> = { first: "a", second: "b" };
const okPQ: PairPQ<number> = { first: 1, second: 2 };

const badAB: PairAB<number> = { first: 1, second: "b" };  // second: string vs number
const badPQ: PairPQ<string> = { first: "a", second: 3 };  // second: number vs string
"#,
    );
    assert_eq!(
        codes,
        vec![2322, 2322],
        "each instantiation's earlier-param default must be independent across names; got {codes:?}"
    );
}

/// A `keyof T` default must be recomputed per instantiation. `Keys<{ x: 1 }>`
/// must be `"x"` and must not retain the key space of any other
/// instantiation.
#[test]
fn keyof_default_recomputed_per_instantiation() {
    let codes = errors(
        r#"
type Keys<T, K extends keyof T = keyof T> = K;

const k1: Keys<{ a: 1; b: 2 }> = "a"; // "a" | "b"
const k2: Keys<{ x: 1 }> = "x";       // "x"
const bad: Keys<{ x: 1 }> = "a";      // "a" not in "x"
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "keyof default must be scoped to its own type argument; got {codes:?}"
    );
}

/// A generic method nested inside a generic object type declares its own
/// type parameter that shadows the outer one of the same name. Instantiating
/// the outer type must not capture (substitute into) the inner, independent
/// type parameter — the inner method stays generic.
#[test]
fn shadowed_inner_type_param_is_not_captured() {
    let codes = errors(
        r#"
type Outer<T> = {
  outer: T;
  inner: <T>(x: T) => T; // independent, shadowing T
};

type O = Outer<number>;
declare const o: O;

const okStr: string = o.inner("hello"); // inner stayed generic <T>(x:T)=>T
o.outer = 5;                             // outer T = number
const bad: number = o.inner("hi");       // string is not assignable to number
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "outer instantiation must not capture the shadowed inner type param; got {codes:?}"
    );
}

/// Defaults on a generic *function* must be resolved independently for each
/// call site. `makeBox(1)` must resolve `U = number` and must not reuse the
/// `U = string` resolved at an earlier call.
#[test]
fn function_generic_default_is_per_call() {
    let codes = errors(
        r#"
function makeBox<T, U = T>(a: T, b?: U): { a: T; b: U } {
  return { a, b: b as U };
}

const sBox = makeBox("s"); // { a: string; b: string }
const nBox = makeBox(1);   // { a: number; b: number }

const okStr: string = sBox.b;
const okNum: number = nBox.b;
const bad: string = nBox.b; // number is not assignable to string
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "function default U must be resolved per call site; got {codes:?}"
    );
}

/// A generic class default must be scoped per instantiation, including when
/// the default is used as the explicit type annotation.
#[test]
fn class_generic_default_is_per_instantiation() {
    let codes = errors(
        r#"
class Container<T = boolean> {
  constructor(public value: T) {}
}

const strC = new Container("x");          // T = string (inferred)
const defC: Container = new Container(true); // T = boolean (default)

const okStr: string = strC.value;
const okBool: boolean = defC.value;
const bad: number = defC.value; // boolean is not assignable to number
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "class default T must be scoped per instantiation; got {codes:?}"
    );
}

/// Negative/fallback guard: a genuinely wrong default-defaulted value must
/// still be rejected. Proves the suite is not vacuously passing by silencing
/// all diagnostics for defaulted generics.
#[test]
fn defaulted_generic_still_rejects_real_mismatch() {
    let codes = errors(
        r#"
type Boxed<T = string> = { value: T };
const wrong: Boxed = { value: 123 }; // number is not assignable to string
"#,
    );
    assert_eq!(
        codes,
        vec![2322],
        "a real mismatch against a defaulted generic must still error; got {codes:?}"
    );
}
