//! Generic-call results must keep a *free enclosing* type parameter free.
//!
//! When a generic call `f<R>(...)` infers `R` to a type parameter that is bound
//! by an *enclosing* generic scope (e.g. the caller's own `<V>`), and `R` occurs
//! in a **function-typed** result position, tsc keeps it as a free reference
//! (`() => V`) rather than re-quantifying the result into a fresh generic
//! signature (`<V>() => V`). tsz used to re-generalize any resolved type
//! parameter reachable through a function-typed result, so the returned callback
//! became generic and a later call re-inferred it to `unknown` — a spurious
//! TS2322 downstream (the jotai `useResetAtom` / `atomWithRefresh` M14 family).
//!
//! Only synthetic higher-order inference placeholders (a generic *function
//! argument's* own quantifier, TS 3.4 HOFI) may be re-generalized into the
//! result; a free enclosing `User` type parameter may not.

use crate::test_utils::check_source_strict_codes;

#[test]
fn enclosing_type_param_in_function_result_binds_not_regeneralized() {
    // `doSet` must be `() => V`, so `doSet()` is `V` and `return doSet()` is
    // fine. If it re-generalized to `<V>() => V`, `doSet()` would infer
    // `unknown` and the return would raise TS2322.
    let codes = check_source_strict_codes(
        r#"
interface Cell<R> { commit: () => R }
declare function useCommit<R>(cell: Cell<R>): () => R
function makeSetter<V>(cell: Cell<V>): V {
  const doSet = useCommit(cell)
  return doSet()
}
"#,
    );
    assert!(
        codes.is_empty(),
        "enclosing type param in a function result must bind, not re-generalize; got {codes:?}"
    );
}

#[test]
fn enclosing_type_param_in_function_result_renamed_binders() {
    // Same shape, unrelated identifiers — not keyed on any binder name.
    let codes = check_source_strict_codes(
        r#"
interface Slot<Out> { pull: () => Out }
declare function bindPull<Out>(slot: Slot<Out>): () => Out
function attach<Payload>(slot: Slot<Payload>): Payload {
  const puller = bindPull(slot)
  return puller()
}
"#,
    );
    assert!(
        codes.is_empty(),
        "renamed enclosing type param must bind through the function result; got {codes:?}"
    );
}

#[test]
fn enclosing_type_param_non_function_results_still_bind() {
    // Guard: array / object / bare result positions already bound correctly and
    // must keep doing so (the fix is scoped to function-typed results).
    let codes = check_source_strict_codes(
        r#"
interface Cell<R> { commit: () => R }
declare function pluck<R>(cell: Cell<R>): R
declare function pluckArr<R>(cell: Cell<R>): R[]
declare function pluckObj<R>(cell: Cell<R>): { v: R }
function useAll<V>(cell: Cell<V>): void {
  const a: V = pluck(cell)
  const b: V[] = pluckArr(cell)
  const c: { v: V } = pluckObj(cell)
}
"#,
    );
    assert!(
        codes.is_empty(),
        "non-function result positions must still bind the enclosing type param; got {codes:?}"
    );
}

#[test]
fn generic_function_argument_result_stays_generic() {
    // Negative guard for the *legitimate* HOFI re-generalization: when the
    // argument is itself a generic function, its own quantifier flows into the
    // result and the result stays callable at multiple instantiations. This must
    // keep working (the fix only excludes *free enclosing* type params).
    let codes = check_source_strict_codes(
        r#"
declare function makeApplier<A, B>(f: (a: A) => B): (a: A) => B
function twice<T>(x: T): T { return x }
const applyTwice = makeApplier(twice)
const r: number = applyTwice(1)
const s: string = applyTwice("x")
"#,
    );
    assert!(
        codes.is_empty(),
        "a generic function argument's result must remain generic/callable; got {codes:?}"
    );
}
