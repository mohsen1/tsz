//! Regression coverage for TS2574 ("A rest element type must be an array type").
//!
//! tsc emits TS2574 whenever the *resolved* type of a rest tuple element is not
//! array-like (`isArrayLikeType`): primitives (`[...string]`), object types,
//! and — crucially — type parameters whose constraint is not array-like,
//! including bare unconstrained parameters (`[...T]`). Concrete array/tuple
//! rests (`[...string[]]`, `[...[number, number]]`) and type parameters
//! constrained to an array (`<T extends any[]>[...T]`) remain valid.

use crate::test_utils::check_source_codes;

/// `[...string]` — rest element wrapping a primitive.
#[test]
fn rest_primitive_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...string];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...string]`: {codes:?}"
    );
}

/// `[...string[]]` — rest element wrapping an array; valid.
#[test]
fn rest_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...string[]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...string[]]`: {codes:?}"
    );
}

/// `[...T]` where `T` is an unconstrained type parameter — TS2574 (its
/// constraint defaults to `unknown`, which is not array-like). Matches tsc.
#[test]
fn rest_unconstrained_type_parameter_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T> = [...T];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...T]` (unconstrained type-parameter spread): {codes:?}"
    );
}

/// `[...T]` where `T extends any[]` — valid (constraint is array-like).
#[test]
fn rest_array_constrained_type_parameter_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T extends any[]> = [...T];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...T]` with `T extends any[]`: {codes:?}"
    );
}

/// `[...[number, string]]` — rest element wrapping a tuple; valid.
#[test]
fn rest_tuple_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...[number, string]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...[number, string]]`: {codes:?}"
    );
}

/// `[number, ...boolean]` — primitive rest after fixed elements; still TS2574.
#[test]
fn rest_primitive_after_fixed_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [number, ...boolean];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[number, ...boolean]`: {codes:?}"
    );
}

/// `[...rest: string]` — NAMED rest member wrapping a primitive; TS2574.
#[test]
fn named_rest_primitive_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...rest: string];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for named rest `[...rest: string]`: {codes:?}"
    );
}

/// `[...rest: string[]]` — named rest wrapping an array; valid.
#[test]
fn named_rest_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...rest: string[]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for named rest `[...rest: string[]]`: {codes:?}"
    );
}

/// `[...rest: T]` where `T` is unconstrained — TS2574 (matches tsc).
#[test]
fn named_rest_unconstrained_type_parameter_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T> = [...rest: T];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for named rest `[...rest: T]` (unconstrained): {codes:?}"
    );
}

/// `[...rest: T]` where `T extends any[]` — valid (constraint is array-like).
#[test]
fn named_rest_array_constrained_type_parameter_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T extends any[]> = [...rest: T];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for named rest `[...rest: T]` with `T extends any[]`: {codes:?}"
    );
}

/// `[...{ a: 1 }]` — rest element wrapping a (non-array) object type; TS2574.
/// Covers the gap where a previous AST-only heuristic only recognised
/// primitive keywords as "obviously non-array".
#[test]
fn rest_object_type_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...{ a: 1 }];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...{{ a: 1 }}]`: {codes:?}"
    );
}

/// `[...AL]` where `AL = number[]` — alias resolving to an array; valid.
/// Exercises lazy-alias resolution in the array-like check.
#[test]
fn rest_alias_to_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type AL = number[];
type T = [...AL];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for alias-to-array spread `[...AL]`: {codes:?}"
    );
}

/// `[...Cond<number[]>]` where the utility conditional resolves to `number[]`;
/// valid. Exercises application/conditional evaluation in the array-like check.
#[test]
fn rest_utility_resolving_to_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Cond<T> = T extends infer U ? U : never;
type T = [...Cond<number[]>];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for utility-to-array spread `[...Cond<number[]>]`: {codes:?}"
    );
}

/// `[...Cond<number>]` where the utility conditional resolves to `number`;
/// TS2574 (the resolved type is not array-like).
#[test]
fn rest_utility_resolving_to_primitive_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Cond<T> = T extends infer U ? U : never;
type T = [...Cond<number>];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for utility-to-primitive spread `[...Cond<number>]`: {codes:?}"
    );
}

/// `[...string, ...number]` — tsc reports a single TS2574 and stops at the
/// first offending rest element (its `checkTupleType` loop `break`s). We must
/// not over-emit one diagnostic per bad rest element.
#[test]
fn multiple_bad_rests_emit_single_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...string, ...number];
"#,
    );
    let count = codes.iter().filter(|&&c| c == 2574).count();
    assert_eq!(
        count, 1,
        "exactly one TS2574 expected for `[...string, ...number]`: {codes:?}"
    );
}

/// `[...T] extends [infer A] ? A : never` — the check clause `[...T]` of a
/// conditional type must be grammar-checked like a free-standing tuple, so an
/// unconstrained `T` still produces TS2574.
#[test]
fn conditional_check_clause_rest_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Head<T> = [...T] extends [infer A] ? A : never;
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for conditional check clause `[...T] extends ...`: {codes:?}"
    );
}

/// Rest-position `infer` in a conditional extends clause (`[infer A, ...infer B]`)
/// must NOT be flagged: `...infer B` infers the tail as an array.
#[test]
fn extends_clause_rest_infer_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Tail<T> = T extends [infer A, ...infer B] ? B : never;
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for rest-position infer `[infer A, ...infer B]`: {codes:?}"
    );
}

/// Spread of a generic utility application that resolves to a tuple/array but
/// still references free type parameters (`[...Tuple<I, E>]`) must NOT flag:
/// tsc instantiates it to an array; tsz cannot fully resolve it here, so it is
/// treated as indeterminate rather than flagged. Regression for the
/// `largeTupleTypes` / `recursiveConditionalTypes` conformance cases.
#[test]
fn rest_deferred_generic_utility_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Grow<A extends unknown[], N extends number> =
    A['length'] extends N ? A : Grow<[...A, unknown], N>;
type Wrap<I, E extends number> = [...Grow<[I], E>];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for deferred generic utility spread `[...Grow<[I], E>]`: {codes:?}"
    );
}

/// Spread of a type parameter constrained to an array-typed alias
/// (`<S extends Sel[]> [...S]`) must NOT flag. Regression for the
/// `contextualTypeTupleEnd` conformance case.
#[test]
fn rest_type_parameter_with_aliased_array_constraint_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Sel = (x: unknown) => unknown;
type SelTuple = Sel[];
declare function f<S extends SelTuple>(...args: [...selectors: S, last: Sel]): void;
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...S]` with `S extends SelTuple` (array alias): {codes:?}"
    );
}

// ── Deferred conditional indexed-access bases ────────────────────────────────
// A spread of `Cond<T>[k]` where `Cond<T>` is a deferred conditional indexes the
// conditional's branch-union constraint by `k` (tsc's
// `getConstraintOfIndexedAccessType`) and classifies array-like-ness from that
// apparent type — tuple-valued branches are accepted, non-array branches still
// flag. The binder names vary so no fixture name drives the decision.

/// `[...Cond<T>["suffix"]]` where both branches give a tuple at `suffix` — the
/// apparent type (`[] | [string]`) is array-like, so no TS2574. Matches tsc.
#[test]
fn rest_deferred_conditional_indexed_tuple_branch_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Shape<T extends ReadonlyArray<unknown>> = T extends readonly []
  ? { suffix: [] }
  : { suffix: [string] };
type Spread<T extends ReadonlyArray<unknown>> = [...Shape<T>["suffix"]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...Shape<T>[\"suffix\"]]` with tuple branches: {codes:?}"
    );
}

/// Inline deferred conditional (no alias indirection) — same acceptance.
#[test]
fn rest_inline_deferred_conditional_indexed_tuple_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Spread<U extends ReadonlyArray<unknown>> =
  [...(U extends readonly [] ? { rest: [1] } : { rest: [2] })["rest"]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for inline deferred conditional indexed tuple spread: {codes:?}"
    );
}

/// Negative: `[...Cond<T>[k]]` where the indexed branch is a *non-array* (string
/// literals) must STILL flag TS2574 (the apparent type `"x" | "y"` is not
/// array-like). Guards against over-suppression.
#[test]
fn rest_deferred_conditional_indexed_nonarray_branch_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Shape<T extends ReadonlyArray<unknown>> = T extends readonly []
  ? { val: "x" }
  : { val: "y" };
type Spread<T extends ReadonlyArray<unknown>> = [...Shape<T>["val"]];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...Shape<T>[\"val\"]]` with string-literal branches: {codes:?}"
    );
}

// ── Indexed-access of a type parameter constrained by a generic-alias tuple ──
// A spread of `T[K]` (K a concrete numeric-literal index) where `T`'s constraint
// resolves through a generic type alias to a tuple looks through the constraint
// to element `K` and classifies array-like-ness from it (tsc's
// `getBaseConstraintOfType`). Array elements are accepted; non-array elements
// still flag. Binder names vary so no fixture name drives the decision.

/// `[...T[0]]` where `T extends Pair<unknown[], unknown[]>` and `Pair` is a
/// generic-alias tuple — element `0` is `unknown[]`, so no TS2574. Matches tsc.
#[test]
fn rest_type_param_alias_tuple_array_element_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Pair<A, B> = [A, B];
type SpreadFirst<T extends Pair<unknown[], unknown[]>> = [...T[0]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...T[0]]` with alias-tuple array element: {codes:?}"
    );
}

/// Both indices spread (`[...T[0], ...T[1]]`) — each element is an array. The
/// binder name (`Couple`) differs from the case above so no fixture name drives
/// the decision.
#[test]
fn rest_type_param_alias_tuple_both_indices_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Couple<A, B> = [A, B];
type SpreadBoth<S extends Couple<string[], number[]>> = [...S[0], ...S[1]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...S[0], ...S[1]]` with alias-tuple array elements: {codes:?}"
    );
}

/// Alias-of-alias constraint (`Wrap<number[]>` → `Pair<number[], number[][]>`)
/// — element `1` resolves through both aliases to an array; no TS2574.
#[test]
fn rest_type_param_nested_alias_tuple_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Pair<A, B> = [A, B];
type Wrap<X> = Pair<X, X[]>;
type Spread<T extends Wrap<number[]>> = [...T[1]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for nested-alias tuple array element spread: {codes:?}"
    );
}

/// `readonly` alias-tuple constraint — element `0` is still an array; no TS2574.
#[test]
fn rest_type_param_readonly_alias_tuple_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Spread<T extends Readonly<[string[], number[]]>> = [...T[0]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for readonly alias-tuple array element spread: {codes:?}"
    );
}

/// Negative: `[...T[0]]` where element `0` of the alias tuple is a *non-array*
/// (`string`) must STILL flag TS2574. Guards against over-suppression.
#[test]
fn rest_type_param_alias_tuple_nonarray_element_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type Pair<A, B> = [A, B];
type SpreadBad<T extends Pair<string, unknown[]>> = [...T[0]];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...T[0]]` with non-array alias-tuple element: {codes:?}"
    );
}
