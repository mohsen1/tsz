//! Regression tests for issue #9683 — a context-sensitive callback argument
//! must be contextually typed using the inference contributed by *all*
//! non-context-sensitive sibling arguments, regardless of argument order.
//!
//! For `f<U>(fn: (acc: U) => U, init: U): U` called `f((acc) => acc + 1, 0)`,
//! tsc combines the `number` candidate from the callback return with the
//! literal `0` candidate from `init` and widens, inferring `U = number`. tsz
//! previously fixed `U = 0` because the callback (positioned before `init`)
//! was contextually typed with `acc: any` — the `init` argument had not yet
//! contributed to the Round 2 substitution. The wrong inferred type masked a
//! `TS2322`.
//!
//! The fix pre-seeds the Round 2 contextual substitution from every
//! non-context-sensitive argument before typing any sensitive callback, so the
//! callback's contextual parameter type is order-independent.
use crate::test_utils::{check_source_diagnostics, diagnostics_with_code};

/// Reported repro: callback first, literal `init` last. `U` must widen to
/// `number`, so assigning the result to `0` is a `TS2322`.
#[test]
fn callback_before_literal_widens_type_param_to_number() {
    let diags = check_source_diagnostics(
        r#"
declare function f<U>(fn: (acc: U) => U, init: U): U;
const r = f((acc) => acc + 1, 0);
const widened: number = r; // OK — U inferred as number
const narrowed: 0 = r;     // TS2322 — number is not assignable to 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (on `const narrowed: 0 = r`), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Control: literal `init` first, callback last. This ordering already
/// inferred `number`; it must keep doing so.
#[test]
fn literal_before_callback_keeps_type_param_number() {
    let diags = check_source_diagnostics(
        r#"
declare function f<U>(init: U, fn: (acc: U) => U): U;
const r = f(0, (acc) => acc + 1);
const widened: number = r; // OK
const narrowed: 0 = r;     // TS2322
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (on `const narrowed: 0 = r`), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Renamed type parameter and parameter names: the fix is structural, not
/// keyed to the spelling `U`/`acc`.
#[test]
fn renamed_type_param_widens_to_number() {
    let diags = check_source_diagnostics(
        r#"
declare function g<Acc>(step: (a: Acc) => Acc, seed: Acc): Acc;
const r = g((a) => a + 1, 0);
const narrowed: 0 = r; // TS2322 — number not assignable to 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 with renamed type parameter, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Real-world `reduce` shape: a third trailing literal `init` still feeds the
/// callback's contextual type so `U` widens to `number`.
#[test]
fn reduce_shape_widens_to_number() {
    let diags = check_source_diagnostics(
        r#"
declare function reduce<T, U>(arr: T[], fn: (acc: U, x: T) => U, init: U): U;
const r = reduce([1, 2, 3], (acc, x) => acc + x, 0);
const narrowed: 0 = r; // TS2322 — number not assignable to 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for reduce shape, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Repeated calls in one checker run must start independent inference
/// transactions. The first call widens a numeric literal through its callback;
/// the second call must recompute from the string seed/callback instead of
/// reusing the earlier numeric candidate or contextual substitution.
#[test]
fn repeated_contextual_calls_do_not_leak_inference_state() {
    let diags = check_source_diagnostics(
        r#"
declare function f<U>(fn: (acc: U) => U, init: U): U;

const numeric = f((acc) => acc + 1, 0);
const textual = f((acc) => acc + "!", "");

const numericOk: number = numeric;
const textualOk: string = textual;
const numericTooNarrow: 0 = numeric;   // TS2322
const textualTooNarrow: "" = textual; // TS2322
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        2,
        "expected independent number/string repeated-call inference, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Control: the callback return does not depend on `U` (`() => 5`), but the
/// callback-return candidate (`5`) still combines with `init`'s `0` and widens
/// to `number`.
#[test]
fn callback_return_independent_of_type_param_widens() {
    let diags = check_source_diagnostics(
        r#"
declare function h<U>(fn: () => U, init: U): U;
const r = h(() => 5, 0);
const narrowed: 5 = r; // TS2322 — number not assignable to 5
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative control: with no callback to add a widened candidate, a lone
/// literal argument keeps its literal type (no widening).
#[test]
fn single_literal_argument_preserves_literal() {
    let diags = check_source_diagnostics(
        r#"
declare function k<U>(init: U): U;
const r = k(0);
const stays: 0 = r; // OK — U stays 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.is_empty(),
        "expected no TS2322 (U should stay the literal 0), got: {:?}",
        ts2322
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative control: a `const` type parameter preserves the literal even when
/// a callback is present, so the result remains assignable to `0`.
#[test]
fn const_type_param_preserves_literal_with_callback() {
    let diags = check_source_diagnostics(
        r#"
declare function c<const U>(fn: (acc: U) => U, init: U): U;
const r = c((acc) => acc, 0);
const stays: 0 = r; // OK — const U preserves 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.is_empty(),
        "expected no TS2322 (const U preserves literal 0), got: {:?}",
        ts2322
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Overload-resolution variant of the same widening rule (kysely #10663 TS2769
// family). When the generic accumulator overload is NOT the unique arity match
// — `Array.prototype.reduce` ships a competing non-generic `(cb, init: T): T`
// overload of the same arity — the competing overload contextually types the
// callback first, so `U` acquires a *usable* contravariant candidate. Direct
// parameter inference then wrongly narrowed the already-widened accumulator
// (`number`) back to the fresh literal seed (`0`), and the callback's widened
// body no longer matched the fixed `0`, producing a false TS2769. The widened
// `inferred` must own the type parameter regardless of the competing overload.
// ---------------------------------------------------------------------------

/// `Array.prototype.reduce`-shaped overload set (two non-generic overloads plus
/// the generic accumulator overload). Summing an object field into a fresh
/// literal `0` seed — accumulator type `number` differs from element type `Row`,
/// so overload #2 (`init: T = Row`) cannot match and the generic overload is
/// forced — must infer `U = number`. The positive assignment (`: number`) must
/// be clean; the negative one (`: 0`) must be the sole TS2322, proving `U`
/// widened to `number` rather than staying the fixed seed `0`, even with the
/// competing non-generic overload present.
#[test]
fn reduce_overload_set_seed_widens_to_number() {
    let diags = check_source_diagnostics(
        r#"
interface Row { id: number }
interface MyArr<T> {
  reduce(cb: (p: T, c: T, i: number, a: T[]) => T): T;
  reduce(cb: (p: T, c: T, i: number, a: T[]) => T, init: T): T;
  reduce<U>(cb: (p: U, c: T, i: number, a: T[]) => U, init: U): U;
}
declare const rows: MyArr<Row>;
const total = rows.reduce((acc, x) => acc + x.id, 0);
const widened: number = total; // OK — U widened to number
const narrowed: 0 = total;     // TS2322 — number not assignable to 0
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (on `const narrowed: 0`, proving U widened to number), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Renamed binders: the fix is structural, not keyed to `reduce`/`U`/`acc`.
#[test]
fn reduce_overload_set_renamed_binders_widen() {
    let diags = check_source_diagnostics(
        r#"
interface Item { value: number }
interface Seq<E> {
  fold(step: (a: E, e: E, n: number, all: E[]) => E): E;
  fold(step: (a: E, e: E, n: number, all: E[]) => E, seed: E): E;
  fold<Acc>(step: (a: Acc, e: E, n: number, all: E[]) => Acc, seed: Acc): Acc;
}
declare const items: Seq<Item>;
const sum = items.fold((a, e) => a + e.value, 0);
const ok: number = sum; // OK — Acc widened to number
"#,
    );

    assert!(
        diags.is_empty(),
        "expected no diagnostics with renamed binders, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The seed's primitive family is not special-cased: a fresh *string* literal
/// seed widens to `string` through the same path.
#[test]
fn reduce_overload_set_string_seed_widens() {
    let diags = check_source_diagnostics(
        r#"
interface Row { id: number }
interface MyArr<T> {
  reduce(cb: (p: T, c: T, i: number, a: T[]) => T): T;
  reduce(cb: (p: T, c: T, i: number, a: T[]) => T, init: T): T;
  reduce<U>(cb: (p: U, c: T, i: number, a: T[]) => U, init: U): U;
}
declare const rows: MyArr<Row>;
const joined = rows.reduce((acc, x) => acc + x.id, "");
const ok: string = joined; // OK — U widened to string
"#,
    );

    assert!(
        diags.is_empty(),
        "expected no diagnostics for the string-seed reduce, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative control for the widening guard above (independent of overloads):
/// heterogeneous direct arguments with conflicting literal bases must still
/// surface a genuine mismatch. The `"don't narrow below inferred"` guard must
/// not swallow the real `"x"` vs `1` error — this path is gated out by the
/// `has_conflicting_literal_bases` check, so the mismatch is preserved.
#[test]
fn conflicting_direct_literals_still_error() {
    let diags = check_source_diagnostics(
        r#"
declare function pair<T>(cb: (t: T) => void, a: T, b: T): T;
const r = pair((t) => {}, 1, "x"); // TS2345 — "x" not assignable to 1
"#,
    );

    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "expected the genuine conflicting-literal TS2345 to survive, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
