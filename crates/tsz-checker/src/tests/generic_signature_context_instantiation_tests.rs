//! Regression tests for issue #13232 — relating two same-arity generic
//! signatures whose target expresses a source type parameter through a type
//! *function* (a conditional / indexed-access / mapped alias application).
//!
//! When an un-annotated, contextually-typed generic function expression is
//! assigned to a generic signature whose return type wraps the function's own
//! type parameter in a deferred alias application, tsc relates the two via
//! `instantiateSignatureInContextOf`: the source's type parameter is inferred
//! from the target so the two signatures become identical. tsz previously
//! alpha-renamed the parameters and compared `Box<T>` against
//! `Box<MappedResponseType<R, T>>` directly, producing a false `TS2322`:
//!
//! ```ts
//! type MRT<R extends RT, J = any> = R extends keyof RM ? RM[R] : J;
//! interface Box<T> { _data?: T; }
//! interface Api { raw<T = any, R extends RT = "json">(): Box<MRT<R, T>>; }
//! // assigning `() => Box<T>` here is clean in tsc; tsz reported TS2322
//! const impl: Api["raw"] = function <T, R extends RT>() { /* return Box<T> */ };
//! ```
//!
//! The fix adds a tsc-style contextual-instantiation fallback to the generic
//! signature relation (see
//! `relations/subtype/rules/functions/checking/context_instantiation.rs`). It
//! only relaxes a comparison that already failed and is gated to ordinary
//! assignability, so genuine mismatches (concrete sources, explicitly annotated
//! returns) still report `TS2322`.
//!
//! Real-world witness: the `ofetch` `$fetchRaw` assignment to
//! `$Fetch["raw"]` (`MappedResponseType` mapped over `ResponseType`).
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/13232>

use crate::test_utils::check_source_codes;

fn assert_no(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&code),
        "unexpected TS{code} (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

fn assert_has(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&code),
        "expected TS{code}, got none. Got: {codes:?}\nSource:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Core witness: un-annotated generic function expression whose inferred return
// `Box<T>` must relate to the contextual `Box<MRT<R, T>>`. Clean in tsc.
// ---------------------------------------------------------------------------

const WITNESS: &str = "
interface RM { blob: number; text: string; }
type RT = keyof RM | \"json\";
type MRT<R extends RT, J = any> = R extends keyof RM ? RM[R] : J;
interface Box<T> { _data?: T; }
interface Ctx<T = any, R extends RT = RT> { box?: Box<T>; }
interface Api { raw<T = any, R extends RT = \"json\">(): Box<MRT<R, T>>; }
const impl: Api[\"raw\"] = function <T = any, R extends RT = \"json\">() {
  const ctx: Ctx<T, R> = undefined as any;
  return ctx.box!;
};
";

#[test]
fn generic_signature_context_instantiation_no_false_2322() {
    assert_no(2322, WITNESS);
}

// The rule is structural, not identifier-keyed: renaming every binder must not
// change the outcome.
#[test]
fn generic_signature_context_instantiation_no_false_2322_renamed() {
    let renamed = "
interface Kinds { a: number; b: string; }
type Sel = keyof Kinds | \"json\";
type Pick<S extends Sel, D = any> = S extends keyof Kinds ? Kinds[S] : D;
interface Cell<V> { _value?: V; }
interface Holder<V = any, S extends Sel = Sel> { cell?: Cell<V>; }
interface Provider { load<V = any, S extends Sel = \"json\">(): Cell<Pick<S, V>>; }
const provider: Provider[\"load\"] = function <V = any, S extends Sel = \"json\">() {
  const holder: Holder<V, S> = undefined as any;
  return holder.cell!;
};
";
    assert_no(2322, renamed);
}

// ---------------------------------------------------------------------------
// Guardrails — the fallback only relaxes a comparison that should succeed in
// tsc. These mismatches must still report TS2322.
// ---------------------------------------------------------------------------

// A *concrete* source return (`Box<string>`) does not relate to the deferred
// `Box<MRT<R, T>>`; there is no source type parameter to infer.
#[test]
fn concrete_source_return_still_reports_2322() {
    assert_has(
        2322,
        "
interface RM { blob: number; text: string; }
type RT = keyof RM | \"json\";
type MRT<R extends RT, J = any> = R extends keyof RM ? RM[R] : J;
interface Box<T> { _data?: T; }
declare const make: <T, R extends RT>() => Box<string>;
const bad: <T, R extends RT>() => Box<MRT<R, T>> = make;
",
    );
}

// With an *explicit* return-type annotation, tsc checks the return against the
// annotation directly (`T` is not assignable to `MRT<R, T>`), so TS2322 stays.
#[test]
fn explicit_return_annotation_still_reports_2322() {
    assert_has(
        2322,
        "
interface RM { blob: number; text: string; }
type RT = keyof RM | \"json\";
type MRT<R extends RT, J = any> = R extends keyof RM ? RM[R] : J;
interface Box<T> { _data?: T; }
interface Ctx<T = any, R extends RT = RT> { box?: Box<T>; }
interface Api { raw<T = any, R extends RT = \"json\">(): Box<MRT<R, T>>; }
const impl: Api[\"raw\"] = function <T = any, R extends RT = \"json\">(): Box<MRT<R, T>> {
  const ctx: Ctx<T, R> = undefined as any;
  return ctx.box!;
};
",
    );
}

// ---------------------------------------------------------------------------
// Residual of #13232 after #13467: the source return is itself an *explicit*
// `Box<T>` annotation that stays a deferred `Application` (not the materialized
// object shape the `ctx.box!` indirection produced above). Contextual
// instantiation must evaluate BOTH signatures to the same structural form before
// inferring; evaluating only the target left the source's `Box<T>` `Application`
// unmatched, so `T` defaulted to `unknown` and the re-comparison still failed.
// tsc is clean on all of these. See the gate matrix in the issue.
// ---------------------------------------------------------------------------

// Single-conditional alias, source return annotated `Box<T>` (deferred form).
#[test]
fn deferred_application_source_return_no_false_2322() {
    assert_no(
        2322,
        "
type M<R extends string, J = any> = R extends \"a\" ? number : J;
interface Box<T> { data?: T; }
type Raw = <T = any, R extends string = \"a\">() => Box<M<R, T>>;
const f: Raw = <T = any, R extends string = \"a\">(): Box<T> => (undefined as any);
",
    );
}

// Structural, not identifier-keyed: renaming every binder keeps it clean.
#[test]
fn deferred_application_source_return_no_false_2322_renamed() {
    assert_no(
        2322,
        "
type Pick<S extends string, D = any> = S extends \"a\" ? number : D;
interface Cell<V> { value?: V; }
type Load = <V = any, S extends string = \"a\">() => Cell<Pick<S, V>>;
const g: Load = <V = any, S extends string = \"a\">(): Cell<V> => (undefined as any);
",
    );
}

// The target conditional need not even mention the source type parameter: tsc
// still infers the (free/covariant) source `T` to the target's demanded type.
#[test]
fn target_result_without_source_type_param_no_false_2322() {
    assert_no(
        2322,
        "
type M<R extends string, J = any> = R extends \"a\" ? number : string;
interface Box<T> { data?: T; }
type Raw = <T = any, R extends string = \"a\">() => Box<M<R, T>>;
const f: Raw = <T = any, R extends string = \"a\">(): Box<T> => (undefined as any);
",
    );
}

// Guardrail: a *concrete* source return (`Box<number>`) has nothing to infer and
// must still report TS2322 (matches tsc).
#[test]
fn concrete_source_return_deferred_target_still_reports_2322() {
    assert_has(
        2322,
        "
type M<R extends string, J = any> = R extends \"a\" ? number : J;
interface Box<T> { data?: T; }
type Raw = <T = any, R extends string = \"a\">() => Box<M<R, T>>;
const f: Raw = <T = any, R extends string = \"a\">(): Box<number> => (undefined as any);
",
    );
}

// Guardrail: when the source type parameter also occurs *contravariantly* (in a
// parameter), inference is pinned back to the bare parameter and the relation
// must still fail, exactly as tsc reports it.
#[test]
fn invariant_source_type_param_occurrence_still_reports_2322() {
    assert_has(
        2322,
        "
type M<R extends string, J = any> = R extends \"a\" ? number : J;
interface Box<T> { data?: T; }
type Raw = <T = any, R extends string = \"a\">(p: Box<T>) => Box<M<R, T>>;
const f: Raw = <T = any, R extends string = \"a\">(p: Box<T>): Box<T> => (undefined as any);
",
    );
}
