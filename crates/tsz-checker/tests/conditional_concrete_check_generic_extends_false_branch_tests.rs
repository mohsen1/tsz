//! Checker integration tests for resolving a conditional with a *concrete*
//! check against a *generic* extends type to its false branch.
//!
//! Structural rule: when a conditional's relation fails and the check type is
//! concrete while the extends type carries type parameters, tsc resolves
//! eagerly via the permissive instantiation (every extends type parameter →
//! `any`): if the relation still fails under `any`, take the false branch;
//! otherwise defer. tsz used to defer unconditionally whenever the extends type
//! had a type parameter, leaving the conditional opaque so the false-branch
//! literal didn't match (spurious TS2322).
//!
//! Owner: `tsz_solver::evaluation::evaluate_rules::conditional::evaluate_conditional`
//! — the post-relation deferral gate now consults
//! `permissive_false_branch_is_definitive` for the `extends_has_type_params`
//! operand (it previously short-circuited and never reached it for a concrete
//! check). #14232 (ts-essentials).

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_has_2322(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "{label}: expected a TS2322, got {codes:?}"
    );
}

// =============================================================================
// Positive: concrete check vs generic extends resolves the false branch
// =============================================================================

#[test]
fn empty_tuple_vs_generic_nonempty_tuple_takes_false_branch() {
    // The reported repro (#14232): `[]` is not `[any, ...any[]]` regardless of T.
    assert_no_errors(
        r#"
function f<T>() {
  type A = [] extends [T, ...T[]] ? "yes" : "no";
  const a: A = "no";
}
export {};
"#,
        "[] extends [T, ...T[]] resolves to \"no\"",
    );
}

#[test]
fn empty_tuple_vs_single_generic_element_takes_false_branch() {
    assert_no_errors(
        r#"
function g<T>() {
  type B = [] extends [T] ? "yes" : "no";
  const b: B = "no";
}
export {};
"#,
        "[] extends [T] resolves to \"no\"",
    );
}

#[test]
fn false_branch_resolution_is_binder_name_independent() {
    assert_no_errors(
        r#"
function pull<Elem>() {
  type R = [] extends [Elem, ...Elem[]] ? 1 : 0;
  const r: R = 0;
}
export {};
"#,
        "renamed binder still resolves the false branch",
    );
}

// =============================================================================
// Negative / must-still-defer: a genuinely indeterminate conditional stays
// deferred (the fix must not over-resolve).
// =============================================================================

#[test]
fn generic_check_stays_deferred() {
    // `T extends string` is indeterminate until T is known: under the permissive
    // instantiation `any extends string` holds, so the relation is NOT definitive
    // false and the conditional must stay deferred — assigning a concrete to the
    // deferred result still errors.
    assert_has_2322(
        r#"
function h<T>() {
  type C = T extends string ? "yes" : "no";
  const c: C = "yes";
}
export {};
"#,
        "T extends string stays deferred (not eagerly resolved)",
    );
}
