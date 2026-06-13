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
