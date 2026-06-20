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

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

fn codes(source: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
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
/// deferred placeholder.
#[test]
fn issue_14123_alias_array_infer_reports_mismatch() {
    let c = codes(
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
    assert!(!c.contains(&2589), "must converge, no TS2589. Got: {c:?}");
    assert_eq!(
        c.iter().filter(|&&x| x == 2322).count(),
        1,
        "mismatched target must report exactly one TS2322. Got: {c:?}"
    );
}
