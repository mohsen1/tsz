//! Regression guard for the remeda error-type-contagion fix (#13512).
//!
//! The fix suppresses spurious TS2536/TS2574 when an indexed-access object is
//! rooted at the application of an *unresolved imported alias* (e.g. `Simplify<…>`
//! / `TupleParts<…>` from a module that failed to resolve — TS2307), which `tsc`
//! gives the permissive `error` apparent type. That import-failure precondition
//! requires the full module-resolution pipeline and is validated end-to-end by
//! the CLI repros and the `remeda` project-compile delta (TS2536 cluster 54→0),
//! not reproducible through the single-file checker unit harness (which does not
//! run module resolution, so a missing-module import is not flagged unresolved
//! here).
//!
//! What this file *does* guard is the gating that keeps the fix from
//! over-suppressing: a *well-formed* conditional (no unresolved import) must keep
//! its per-branch key-space restriction — a key shared by both branches is
//! accepted, a key present in only one branch still emits TS2536. The fix routes
//! these cases through the unchanged strict path, so they must stay correct.
//! Binder names vary per case so no identifier string drives the decision.

use crate::test_utils::check_source_codes;

/// A key shared by both well-formed branches is accepted (no TS2536). The
/// branch-union keyof is `keyof A ∩ keyof B`, which keeps shared keys; the
/// error-contagion path must not fire for a conditional with no unresolved
/// import.
#[test]
fn shared_key_across_well_formed_conditional_branches_is_accepted() {
    let codes = check_source_codes(
        r#"
type Wf<T> = T extends number ? { p: 1; q: 2 } : { p: 3; r: 4 };
type Shared<T> = Wf<T>["p"];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "a key shared by both well-formed branches must be accepted: {codes:?}"
    );
}

/// A key present in only one well-formed branch still emits TS2536 — the
/// per-branch restriction must survive. This is the adjacency the prototype
/// regressed and the fix is gated to preserve.
#[test]
fn solo_branch_key_in_well_formed_conditional_still_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Wf<U> = U extends number ? { p: 1; q: 2 } : { p: 3; r: 4 };
type Solo<U> = Wf<U>["q"];
"#,
    );
    assert!(
        codes.contains(&2536),
        "a key present in only one well-formed branch must still emit TS2536: {codes:?}"
    );
}

/// A renamed-binder variant of the shared-key case: acceptance must be
/// structural, not keyed to any identifier.
#[test]
fn shared_key_across_well_formed_conditional_branches_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Cond<Elem> = Elem extends string ? { key: 1; only_true: 2 } : { key: 3; only_false: 4 };
type Access<Elem> = Cond<Elem>["key"];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "shared-key acceptance must be structural across renamed binders: {codes:?}"
    );
}

/// A missing key (in neither well-formed branch) still emits TS2536.
#[test]
fn missing_key_in_well_formed_conditional_still_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Cond<X> = X extends string ? { a: 1 } : { a: 2 };
type Bad<X> = Cond<X>["zzz"];
"#,
    );
    assert!(
        codes.contains(&2536),
        "a key absent from both branches must still emit TS2536: {codes:?}"
    );
}
