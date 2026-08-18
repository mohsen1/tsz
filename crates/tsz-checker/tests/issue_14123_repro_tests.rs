//! Regression for issue #14123: a recursive conditional alias that extracts an
//! `(infer E)[]` element from a generic-alias array property must converge to a
//! concrete fixpoint instead of stack-overflowing (SIGABRT).
//!
//! Root cause: the concrete recursive-conditional application-fixpoint sharing
//! permit (#13508/#13894) treated a *deferred* self-application — e.g.
//! `D<{id:0}[]>` finalizing to the unevaluated `D<{id:0}>` — as a converged
//! fixpoint, because it carries no free type parameter and no `error`. Caching
//! `(D, args) -> D<…>` poisoned the resolver-independent application-eval cache:
//! a later read returned the deferred self-application, which re-applied `D` on
//! the same input forever. The fix rejects results that still contain an
//! application of a recursive alias (a body that re-references its own `DefId`).
//!
//! These assert convergence (no SIGABRT, no TS2589) AND the exact `tsc` result
//! (`R = { id: 0 }`): the positive assignment must hold and a mismatched target
//! must report TS2322. Binder names are varied across cases so the guard is
//! structural, not identifier-driven.
//!
//! Harness fidelity: these route through the shared-`DefinitionStore` checker
//! construction (`check_source_with_libs_shared_def_store`), the
//! production-driver shape, NOT the plain `check_source_with_libs` path.
//! The recursion here unwraps through a *lib generic* (`Promise`), and the
//! plain harness cannot unify a lib generic's base declaration across the
//! user arena and the lib arena (issue #16125): the alias application then
//! stays an unevaluated self-application in relation position, and the
//! mismatch witness silently loses its `TS2322` (only a `never` target still
//! rejects, via the no-progress `target == NEVER` arm in `check_subtype`) —
//! a harness-only false negative the real CLI does not exhibit (the r3
//! known-failures adjudication, re-confirmed 2026-08-18). Under the shared
//! store the fixpoint converges and the oracle-exact diagnostic fires.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_shared_def_store;

fn code_messages(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    tsz_checker::test_utils::diagnostic_code_messages(check_source_with_libs_shared_def_store(
        source, "test.ts", opts, &libs,
    ))
}

fn codes(source: &str) -> Vec<u32> {
    code_messages(source).into_iter().map(|(c, _)| c).collect()
}

/// The minimal witness from the issue: Promise → `{ payload }` → `(infer E)[]`
/// over a generic alias `Box<T> = Promise<{ payload: T[] }>`.
#[test]
fn issue_14123_alias_array_infer_converges_to_object() {
    let c = codes(
        r#"
type D<T> =
    T extends Promise<infer U> ? D<U> :
    T extends { payload: infer P } ? D<P> :
    T extends (infer E)[] ? D<E> :
    T;
type Box<T> = Promise<{ payload: T[] }>;
type R = D<Box<{ id: 0 }>>;
declare const r: R;
const ok: { id: 0 } = r;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert!(
        !c.contains(&2322),
        "R must equal {{ id: 0 }} (positive assignment holds). Got: {c:?}"
    );
}

/// Same shape, renamed binders, with a mismatched target: the conditional still
/// resolves to `{ id: 0 }`, so assigning it to `{ id: 1 }` reports exactly one
/// TS2322 — proving the recursion converged to a concrete value rather than a
/// deferred placeholder. The rendered source side must name the converged
/// fixpoint (`{ id: 0; }`), not the alias application, matching `tsc`:
/// "Type '{ id: 0; }' is not assignable to type '{ id: 1; }'."
#[test]
fn issue_14123_alias_array_infer_reports_mismatch() {
    let cm = code_messages(
        r#"
type Unwrap<Q> =
    Q extends Promise<infer A> ? Unwrap<A> :
    Q extends { payload: infer B } ? Unwrap<B> :
    Q extends (infer C)[] ? Unwrap<C> :
    Q;
type Wrapper<S> = Promise<{ payload: S[] }>;
type Result = Unwrap<Wrapper<{ id: 0 }>>;
declare const value: Result;
const bad: { id: 1 } = value;
"#,
    );
    assert!(
        !cm.iter().any(|(c, _)| *c == 2589),
        "must converge, no TS2589. Got: {cm:?}"
    );
    let ts2322: Vec<_> = cm.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "mismatched target must report exactly one TS2322. Got: {cm:?}"
    );
    assert!(
        ts2322[0].1.contains("{ id: 0; }"),
        "the TS2322 source side must render the converged fixpoint \
         `{{ id: 0; }}`, not a deferred alias application. Got: {:?}",
        ts2322[0].1
    );
}

/// Second mismatch witness, renamed binders and a *primitive* target: a
/// converged object fixpoint assigned to `string` must also reject. In the
/// low-fidelity (non-shared-`DefinitionStore`) harness this family regressed
/// to "assignable to everything except `never`", so the target-shape spread
/// (object literal above, primitive here) pins the rejection as coming from
/// the converged value itself rather than one lucky target comparison.
#[test]
fn issue_14123_alias_array_infer_rejects_primitive_target() {
    let c = codes(
        r#"
type Peel2<Z> =
    Z extends Promise<infer K> ? Peel2<K> :
    Z extends { payload: infer L } ? Peel2<L> :
    Z extends (infer M)[] ? Peel2<M> :
    Z;
type Carton<W> = Promise<{ payload: W[] }>;
type Got = Peel2<Carton<{ id: 0 }>>;
declare const got: Got;
const bad: string = got;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "primitive mismatched target must report exactly one TS2322. Got: {c:?}"
    );
}

/// Reopened witness #1 (issue comment, 2026-06-20): a recursive conditional that
/// unwraps `infer` through *nested generic-alias containers* — `Box<T> =
/// Promise<Set<T>>` — used to SIGABRT because the `infer V` extraction from the
/// alias-`Application` source minted a fresh `Application` each step and never
/// recognized convergence. It must converge to `{ id: 0 }`. Binders renamed so
/// the guard is structural.
#[test]
fn issue_14123_nested_alias_container_infer_converges() {
    let c = codes(
        r#"
type Peel<K> =
    K extends Promise<infer M> ? Peel<M> :
    K extends Set<infer N> ? Peel<N> :
    K;
type Nested<W> = Promise<Set<W>>;
type Out = Peel<Nested<{ id: 0 }>>;
declare const out: Out;
const ok: { id: 0 } = out;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert!(
        !c.contains(&2322),
        "Out must equal {{ id: 0 }} (positive assignment holds). Got: {c:?}"
    );
}

/// Reopened witness #2 (issue comment / #14330, 2026-06-21): the still-failing
/// path was an un-guarded self-recursion in `match_infer_object_pattern` when
/// matching an object `{ payload: infer P }` pattern against an alias-
/// `Application` source — a *single* application `DeepUnwrap<AsyncBox<{id:0}>>`
/// was enough to abort. The object-pattern `Application` arm now re-enters the
/// guarded `match_infer_pattern`, so it must converge to `{ id: 0 }`.
#[test]
fn issue_14123_object_pattern_over_alias_application_converges() {
    let c = codes(
        r#"
type DeepUnwrap<G> =
    G extends Promise<infer H> ? DeepUnwrap<H> :
    G extends { payload: infer J } ? DeepUnwrap<J> :
    G;
type AsyncBox<V> = Promise<{ payload: V }>;
type Settled = DeepUnwrap<AsyncBox<{ id: 0 }>>;
declare const settled: Settled;
const ok: { id: 0 } = settled;
"#,
    );
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert!(
        !c.contains(&2322),
        "Settled must equal {{ id: 0 }} (positive assignment holds). Got: {c:?}"
    );
}

/// Mutually-recursive deferred conditionals (`A` evaluates to `B`, `B` to `A`)
/// over an alias source must terminate through the `(source, pattern)` cycle
/// guard rather than unwinding `A -> B -> A -> ...` until the stack aborts. This
/// is the cycle the `match_infer_unwrapped_application` re-routing was added to
/// break; the result resolves to the inner `{ id: 0 }`.
#[test]
fn issue_14123_mutually_recursive_alias_conditionals_terminate() {
    let c = codes(
        r#"
type StepA<P> = P extends { a: infer X } ? StepB<X> : P;
type StepB<Q> = Q extends { b: infer Y } ? StepA<Y> : Q;
type Wrap<T> = { a: { b: T } };
type Done = StepA<Wrap<{ id: 0 }>>;
declare const done: Done;
const ok: { id: 0 } = done;
"#,
    );
    assert!(!c.contains(&2589), "must terminate, no TS2589. Got: {c:?}");
    assert!(
        !c.contains(&2322),
        "Done must equal {{ id: 0 }} (positive assignment holds). Got: {c:?}"
    );
}

/// Safety-net guard (issue #14123, fix direction 2): a genuinely non-convergent
/// recursive conditional — each step *grows* the type, so no cycle is ever hit —
/// must degrade to a bounded TS2589 diagnostic, never a process-aborting stack
/// overflow. This exercises the depth budgets plus the `stacker::maybe_grow`
/// stack-growth guard on the infer-match recursion: reaching this assertion at
/// all proves the process did not SIGABRT.
#[test]
fn issue_14123_non_convergent_conditional_degrades_to_ts2589() {
    let c = codes(
        r#"
type Grow<T> =
    T extends (infer E)[] ? Grow<[E, E]> :
    Grow<T[]>;
type Y = Grow<{ id: 0 }>;
declare const y: Y;
"#,
    );
    assert!(
        c.contains(&2589),
        "non-convergent recursion must report TS2589 (and not crash). Got: {c:?}"
    );
}

/// Safety-net generalization (issue #14123, fix direction #2): a genuinely
/// non-convergent recursive conditional — one whose recursive call *grows* its
/// argument every step (`Grow<{ p: P }>` -> `Grow<{ p: P[] }>` -> …) so it never
/// reaches a fixpoint — must degrade to a bounded `TS2589` diagnostic rather than
/// exhausting the native stack with a `SIGABRT`. This locks the property that the
/// conditional/`infer` evaluation recursion is depth-bounded: a future loss of
/// convergence on this path can never again crash the process, only surface
/// "Type instantiation is excessively deep and possibly infinite". Binder names
/// differ from the converging witnesses above so the guard is structural.
#[test]
fn issue_14123_nonconvergent_growth_bounds_to_ts2589_not_sigabrt() {
    let c = codes(
        r#"
type Grow<V> =
    V extends { p: infer Q } ? Grow<{ p: Q[] }> :
    V;
type Diverge = Grow<{ p: 0 }>;
declare const d: Diverge;
"#,
    );
    assert!(
        c.contains(&2589),
        "non-convergent growth must surface bounded TS2589 (no SIGABRT). Got: {c:?}"
    );
}
