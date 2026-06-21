//! Tests for conditional types whose check type is concrete but whose extends
//! type carries a type parameter.
//!
//! Issue #14232 (mined from ts-essentials): when a conditional's relation fails
//! and the extends type is generic but the check type is concrete, tsc resolves
//! eagerly — it takes the false branch as soon as the relation *also* fails
//! under the permissive instantiation (every type parameter replaced by `any`,
//! tsc's `getPermissiveInstantiation` gate). tsz previously deferred
//! unconditionally whenever the extends type carried a type parameter, leaving
//! the conditional opaque and reporting a spurious TS2322 at the use site.
//!
//! The structural rule:
//!   When `Concrete extends GenericExtends ? X : Y` fails the relation and
//!   `Concrete` has no free type parameters, take `Y` iff the relation also
//!   fails for `GenericExtends[params -> any]`; otherwise defer.
//!
//! These cases vary the binder name and the shape (tuple, object, array) so the
//! behavior is structural, not tied to any particular identifier.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count, diagnostics_without_codes};

fn ts2322_count(source: &str) -> usize {
    diagnostic_count(&check_source_strict(source), 2322)
}

/// No diagnostics other than TS2318 (missing global lib types in the no-stdlib
/// unit harness).
fn has_no_errors(source: &str) -> bool {
    diagnostics_without_codes(&check_source_strict(source), &[2318]).is_empty()
}

// ── The canonical repro: empty tuple vs one-or-more generic tuple ─────────

#[test]
fn empty_tuple_never_satisfies_one_or_more_generic_tuple() {
    // `[] extends [T, ...T[]]` can never hold for any `T`: the empty tuple has
    // length 0 while `[T, ...T[]]` requires at least one element. The false
    // branch is permissively definitive, so `A` resolves to `"no"`.
    assert!(
        has_no_errors(
            r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "no";
}
export {};
"#
        ),
        "empty tuple vs `[T, ...T[]]` must resolve to the false branch"
    );
}

#[test]
fn empty_tuple_one_or_more_generic_tuple_rejects_true_branch_literal() {
    // The conditional resolved to `"no"`, so assigning `"yes"` must still error.
    assert_eq!(
        ts2322_count(
            r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "yes";
}
export {};
"#
        ),
        1,
        "`\"yes\"` is not assignable to the resolved false branch `\"no\"`"
    );
}

// Binder name must not matter — same structure, different type-parameter name.
#[test]
fn empty_tuple_one_or_more_generic_tuple_renamed_param() {
    assert!(
        has_no_errors(
            r#"
function f<Element>() {
  type A = [] extends [Element, ...Element[]] ? "yes" : "no";
  const a: A = "no";
}
export {};
"#
        ),
        "renamed type parameter must behave identically"
    );
}

// ── Object shape: a missing required key is permissively definitive ───────

#[test]
fn object_missing_required_key_takes_false_branch() {
    // `{ a: 1 }` is missing `b`, so it can never satisfy `{ a: 1; b: T }` for
    // any `T`. The false branch is definitive.
    assert!(
        has_no_errors(
            r#"
function f<T>() {
  type A = { a: 1 } extends { a: 1; b: T } ? "yes" : "no";
  const a: A = "no";
}
export {};
"#
        ),
        "object missing a required key must resolve to the false branch"
    );
}

// ── Deferral must be preserved when the permissive form could still match ──

#[test]
fn concrete_tuple_vs_single_generic_element_stays_deferred() {
    // `[string] extends [T]` could be true (`T = string`) or false, so tsc keeps
    // the conditional deferred. Neither `"yes"` nor `"no"` is assignable to the
    // opaque conditional, so both assignments error (two TS2322).
    assert_eq!(
        ts2322_count(
            r#"
function f<T>() {
  type A = [string] extends [T] ? "yes" : "no";
  const a: A = "yes";
  const b: A = "no";
}
export {};
"#
        ),
        2,
        "permissively-matchable conditional must stay deferred (both assignments error)"
    );
}

// ── True branch when the relation holds regardless of instantiation ───────

#[test]
fn identical_generic_tuple_takes_true_branch() {
    // `[T] extends [T]` holds for every `T`, so `A` resolves to `"yes"`.
    assert_eq!(
        ts2322_count(
            r#"
function f<T>() {
  type A = [T] extends [T] ? "yes" : "no";
  const a: A = "no";
}
export {};
"#
        ),
        1,
        "`[T] extends [T]` resolves to the true branch `\"yes\"`, so `\"no\"` errors"
    );
}
