//! Tests that `.bind(thisArg)` on an overloaded function preserves the whole
//! overload set (immer `produceWithPatches.bind(immer)` false-positive).
//!
//! Structural rule: when a function value's call signatures declare no `this`
//! parameter, `tsc` types `x.bind(thisArg)` as the first `CallableFunction.bind`
//! overload `bind<T>(this: T, thisArg: ThisParameterType<T>): OmitThisParameter<T>`.
//! Because `ThisParameterType<T>` collapses to `unknown` (no `this` to strip),
//! `OmitThisParameter<T>` is the identity `T`, so the bound value keeps *every*
//! call signature of the receiver.
//!
//! tsz synthesizes the `strictBindCallApply` `.bind` method one bound function
//! per receiver call signature. With more than one call signature, overload
//! resolution of `.bind(thisArg)` picked the first synthesized method, so the
//! bound value collapsed to the receiver's first call signature — dropping the
//! rest. Assigning it back to the overloaded interface then wrongly reported
//! TS2322 (immer `src/immer.ts` line 54). The fix emits a single identity
//! `.bind` method that carries all of the receiver's call signatures.
//!
//! Binder names are varied across cases so no fix can key on an identifier.

use crate::test_utils::check_source_diagnostics;

/// The immer witness, reduced: an overloaded call interface bound with only a
/// `thisArg` must stay assignable to itself. No TS2322.
#[test]
fn bind_on_overloaded_function_preserves_all_signatures() {
    let diags = check_source_diagnostics(
        r#"
interface IProduceWithPatches {
  <Recipe extends (...a: any[]) => any>(recipe: Recipe): [Recipe];
  <State, Recipe>(recipe: (draft: State) => Recipe, initialState: State): {
    s: State;
    r: Recipe;
  };
}
declare const impl: IProduceWithPatches;
const produceWithPatches: IProduceWithPatches = impl.bind(undefined as any);
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "`.bind(thisArg)` on an overloaded receiver must preserve every call \
         signature; expected no TS2322, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Renamed-binder variant: identical structure, completely different
/// identifiers for the interface, type parameters, and members. Locks out any
/// name-based fast path.
#[test]
fn bind_on_overloaded_function_preserves_all_signatures_renamed_binders() {
    let diags = check_source_diagnostics(
        r#"
interface Widget {
  <Quux extends (...zs: any[]) => any>(head: Quux): [Quux];
  <Elem, Ret>(head: (node: Elem) => Ret, seed: Elem): { e: Elem; r: Ret };
}
declare const gadget: Widget;
const bound: Widget = gadget.bind(0 as any);
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "renamed-binder overloaded `.bind` must preserve every call signature; \
         expected no TS2322, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Adjacent negative case: the bound value is still the receiver's real type, so
/// assigning it to a genuinely incompatible target must still report TS2322.
/// This proves the fix preserves soundness rather than blanket-suppressing the
/// override check.
#[test]
fn bind_on_overloaded_function_still_reports_real_mismatch() {
    let diags = check_source_diagnostics(
        r#"
interface Multi {
  <A extends (...a: any[]) => any>(recipe: A): [A];
  <B, C>(recipe: (draft: B) => C, initialState: B): { b: B; c: C };
}
declare const impl: Multi;
// The bound value keeps Multi's signatures, none of which accept a bare
// `number` first argument, so this assignment is a genuine mismatch.
const bad: (n: number) => number = impl.bind(undefined as any);
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "binding an overloaded function then assigning it to an incompatible \
         target must still report TS2322"
    );
}

/// Adjacent case: a single-signature function's `.bind(thisArg)` keeps working
/// (the identity fast-path only triggers for multi-signature receivers, but the
/// single-signature per-signature path must remain correct).
#[test]
fn bind_on_single_signature_function_preserves_signature() {
    let diags = check_source_diagnostics(
        r#"
type Fn = (recipe: (x: number) => number) => [number];
declare const impl: Fn;
const bound: Fn = impl.bind(undefined as any);
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "single-signature `.bind` must preserve its signature; expected no \
         TS2322, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
