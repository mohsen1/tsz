//! Tests for issue #14794: spreading a non-tuple array (`E[]`) into a rest type
//! parameter `...args: T` must infer `T = E[]` (open, variadic), never the
//! length-1 tuple `[E]`.
//!
//! Structural rule: when a spread argument's value is a non-tuple array/iterable
//! and it lands on a rest parameter whose open-ended length feeds an inference
//! variable (a bare type-parameter rest `...args: T`, or a rest tuple), the spread
//! must contribute an open-ended rest element (`...E[]`) to the synthetic argument
//! tuple used for rest-parameter inference. Collapsing it into a single
//! representative element makes `T` infer as a fixed-length `[E]`, so any
//! arity-dependent read — out-of-bounds indexing (TS2493), `.length`, or
//! fixed-tuple assignment — then diverges from `tsc`. Owner: the call-checker
//! argument collector (`candidate_collection`), reusing the same
//! spread-argument-marker mechanism already used for open-ended tuple-spread
//! tails.
//!
//! Binder names are varied across the cases to prove the fix is structural and
//! not keyed on any identifier.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn filtered(source: &str, wanted: &[u32]) -> Vec<u32> {
    codes(source)
        .into_iter()
        .filter(|c| wanted.contains(c))
        .collect()
}

/// The exact witness from the issue: indexing the result past element 0 must not
/// produce TS2493 — `T` is `number[]`, not `[number]`.
#[test]
fn array_spread_into_rest_type_param_indexes_without_ts2493() {
    let found = filtered(
        r#"
declare function apply<Args extends unknown[]>(...args: Args): Args;
declare const xs: number[];
const out = apply(...xs);
const a = out[2];
const b = out[5];
"#,
        &[2493],
    );
    assert!(
        found.is_empty(),
        "expected no TS2493 (result is number[], not [number]), got: {found:?}"
    );
}

/// The inferred result is genuinely the open array `number[]`, not a length-1
/// tuple: it is assignable to `number[]` but NOT to the fixed tuple `[number]`
/// (which a length-1 tuple would satisfy). The TS2322 on the fixed-tuple target
/// is the discriminator that proves the open-array shape.
#[test]
fn array_spread_into_rest_type_param_infers_open_array_not_fixed_tuple() {
    let open = filtered(
        r#"
declare function collect<List extends unknown[]>(...items: List): List;
declare const values: string[];
const gathered = collect(...values);
const asArray: string[] = gathered;
"#,
        &[2322],
    );
    assert!(
        open.is_empty(),
        "expected number[] result assignable to string[], got: {open:?}"
    );

    let fixed = filtered(
        r#"
declare function collect<List extends unknown[]>(...items: List): List;
declare const values: string[];
const gathered = collect(...values);
const asFixed: [string] = gathered;
"#,
        &[2322],
    );
    assert!(
        fixed.contains(&2322),
        "expected open string[] result NOT assignable to fixed [string], got: {fixed:?}"
    );
}

/// The `const`/`readonly` form (`...args: T`, `T extends readonly unknown[]`)
/// behaves the same: the readonly array spread stays open, so out-of-bounds
/// indexing is clean.
#[test]
fn readonly_array_spread_into_rest_type_param_indexes_without_ts2493() {
    let found = filtered(
        r#"
declare function freeze<Tup extends readonly unknown[]>(...parts: Tup): Tup;
declare const ro: readonly number[];
const frozen = freeze(...ro);
const x = frozen[3];
"#,
        &[2493],
    );
    assert!(
        found.is_empty(),
        "expected no TS2493 for readonly-array spread, got: {found:?}"
    );
}

/// Regression: a *tuple* value spread still expands positionally, so `T` is the
/// fixed tuple and per-position element types are preserved. The fixed-tuple
/// target is assignable (no TS2322) and a real out-of-bounds index still errors.
#[test]
fn tuple_spread_still_expands_to_fixed_tuple() {
    let assignable = filtered(
        r#"
declare function pack<Slots extends unknown[]>(...slots: Slots): Slots;
declare const pair: [number, string];
const packed = pack(...pair);
const same: [number, string] = packed;
"#,
        &[2322],
    );
    assert!(
        assignable.is_empty(),
        "expected tuple spread to infer the fixed [number, string], got: {assignable:?}"
    );

    let oob = filtered(
        r#"
declare function pack<Slots extends unknown[]>(...slots: Slots): Slots;
declare const pair: [number, string];
const packed = pack(...pair);
const beyond = packed[2];
"#,
        &[2493],
    );
    assert!(
        oob.contains(&2493),
        "expected TS2493 for a genuine out-of-bounds index on a fixed 2-tuple, got: {oob:?}"
    );
}

/// Regression / negative control: a plain array rest parameter (`...rest: E[]`)
/// carries no inference variable that depends on the spread's exact length, so a
/// non-tuple array spread is accepted with no TS2556 and no TS2493 — the
/// historical materialization is unchanged.
#[test]
fn plain_array_rest_parameter_accepts_array_spread() {
    let found = filtered(
        r#"
declare function sink(...rest: number[]): void;
declare const ns: number[];
sink(...ns);
"#,
        &[2556, 2493, 2345],
    );
    assert!(
        found.is_empty(),
        "expected a plain array rest parameter to accept the array spread cleanly, got: {found:?}"
    );
}

/// Regression: a generic *element* rest parameter (`...rest: E[]`) infers the
/// element type `E` from the array element, independent of arity — the array
/// spread must not be remarked here.
#[test]
fn generic_element_rest_parameter_infers_element_type() {
    let found = filtered(
        r#"
declare function firstKind<E>(...rest: E[]): E;
declare const ds: number[];
const k = firstKind(...ds);
const asNumber: number = k;
const asString: string = k;
"#,
        &[2322],
    );
    // `E = number`: `number` target is clean, `string` target errors. Exactly one
    // TS2322 proves the element type was inferred (not collapsed to a tuple).
    assert_eq!(
        found,
        vec![2322],
        "expected E inferred as number (one TS2322 on the string target), got: {found:?}"
    );
}
