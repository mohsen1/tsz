//! TS2352 assertion comparability for thenable-shaped casts whose generic
//! method signatures mint opaque-reference RETURN pairs during erasure.
//!
//! Structural rule: when every shared-arity parameter pair of a signature
//! pair is assertion-comparable and both erased return types are opaque
//! references (`Application`/`Lazy`) the resolver-free query layer cannot
//! materialize, the signatures are treated as overlapping — the same
//! conservative both-opaque policy the Lazy/Lazy rule applies. Scoped to the
//! signature RETURN leg: an opaque pair in PROPERTY position still
//! decomposes strictly (`{ p: Promise<void> } as { p: Map<string, number> }`
//! must keep failing). Witness: zustand persist.ts
//! `hydrate() as Promise<void>` where `hydrate(): Thenable<undefined> |
//! undefined`.

use crate::test_utils::check_source_diagnostics;

fn ts2352_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2352)
        .count()
}

/// A local mirror of lib `Promise` / the zustand `Thenable`: the unit-test
/// harness runs a cut-down lib where the real `Promise` resolves differently
/// than under the CLI (the canary + CLI fixtures pin the lib form), so the
/// suite pins the structural rule against local shapes with the same
/// signature anatomy — optional nullable callback parameters and defaulted
/// method type parameters.
const THENABLE: &str = r#"
interface Prom<T> {
  then<TResult1 = T, TResult2 = never>(
    onfulfilled?: ((value: T) => TResult1 | Prom<TResult1>) | undefined | null,
    onrejected?: ((reason: any) => TResult2 | Prom<TResult2>) | undefined | null
  ): Prom<TResult1 | TResult2>
}
type Thenable<Value> = {
  then<V>(onFulfilled: (value: Value) => V | Prom<V> | Thenable<V>): Thenable<V>
  catch<V>(onRejected: (reason: Error) => V | Prom<V> | Thenable<V>): Thenable<V>
}
"#;

/// The zustand witness: a possibly-undefined thenable cast to `Promise<void>`.
#[test]
fn thenable_union_to_promise_cast_is_accepted() {
    let source = format!(
        "{THENABLE}
declare function hydrate(): Thenable<undefined> | undefined
const rehydrate = () => hydrate() as Prom<void>
"
    );
    assert_eq!(
        ts2352_count(&source),
        0,
        "tsc accepts the thenable-to-promise cast"
    );
}

/// Non-union source with a void/undefined value mismatch.
#[test]
fn thenable_undefined_to_promise_void_cast_is_accepted() {
    let source = format!(
        "{THENABLE}
declare const pending: Thenable<undefined>
const settled = pending as Prom<void>
"
    );
    assert_eq!(ts2352_count(&source), 0, "tsc accepts the direct cast");
}

/// Optional callback parameter (implicit `| undefined`) on a local generic
/// promise-like target — the parameter shape lib `Promise.then` uses.
/// (Binder names varied from the lib shapes.)
#[test]
fn optional_callback_param_generic_target_cast_is_accepted() {
    let source = r#"
interface Emitter<V> { then<W>(cb: (value: V) => W): Emitter<W> }
interface Sink<T> { then<R1>(onDone?: (value: T) => R1): Sink<R1> }
declare const pipe: Emitter<undefined>
const drained = pipe as Sink<void>
"#;
    assert_eq!(
        ts2352_count(source),
        0,
        "optional-callback generic target must overlap"
    );
}

/// Nullable-union callback parameter on the target.
#[test]
fn nullable_callback_param_generic_target_cast_is_accepted() {
    let source = r#"
interface Emitter<V> { then<W>(cb: (value: V) => W): Emitter<W> }
interface Sink<T> { then<R1>(onDone: ((value: T) => R1) | null): Sink<R1> }
declare const pipe: Emitter<undefined>
const drained = pipe as Sink<void>
"#;
    assert_eq!(
        ts2352_count(source),
        0,
        "nullable-callback generic target must overlap"
    );
}

/// Non-generic source `then` against a generic target `then`.
#[test]
fn non_generic_source_then_generic_target_cast_is_accepted() {
    let source = r#"
interface Once<V> { then(cb: (value: V) => number): Once<number> }
interface Sink<T> { then<R1>(onDone: (value: T) => R1): Sink<R1> }
declare const single: Once<undefined>
const drained = single as Sink<void>
"#;
    assert_eq!(
        ts2352_count(source),
        0,
        "non-generic source signature must overlap a generic target"
    );
}

/// Negative control: an opaque application pair in PROPERTY position must
/// still decompose strictly — the return-leg leniency must not leak there.
#[test]
fn nested_application_property_mismatch_still_reports_ts2352() {
    let source = r#"
interface Prom2<T> { then<R1>(cb: (v: T) => R1): Prom2<R1> }
interface Dict<K, V> { get(key: K): V | undefined; set(key: K, value: V): Dict<K, V> }
declare const holder: { p: Prom2<void> }
const swapped = holder as { p: Dict<string, number> }
"#;
    assert_eq!(
        ts2352_count(source),
        1,
        "property-position application pairs must keep failing"
    );
}

/// Negative control: incomparable non-thenable object casts keep failing.
#[test]
fn plain_property_type_mismatch_still_reports_ts2352() {
    let source = r#"
declare const named: { x: string }
const renumbered = named as { x: number }
"#;
    assert_eq!(
        ts2352_count(source),
        1,
        "distinct shared-property primitives must keep failing"
    );
}

/// Negative control: a thenable cast to a shape with NO shared members keeps
/// failing.
#[test]
fn thenable_to_unrelated_shape_still_reports_ts2352() {
    let source = r#"
interface Emitter<V> { then<W>(cb: (value: V) => W): Emitter<W> }
interface Registry { size: number; clear(): void }
declare const pipe: Emitter<undefined>
const misfiled = pipe as Registry
"#;
    assert_eq!(
        ts2352_count(source),
        1,
        "no-shared-member casts must keep failing"
    );
}
