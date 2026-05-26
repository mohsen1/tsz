//! Regression tests for #9673: identity-style conditional comparisons over
//! unresolved conditional aliases must preserve uncertainty.
//!
//! Structural rule: when the compared function-return types contain deferred
//! conditional aliases, `tsc` cannot prove identity or non-identity for a free
//! type parameter. The identity conditional therefore remains `boolean`, and
//! constraining it to `false` reports TS2344.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count};

fn assert_one_ts2344(source: &str, label: &str) {
    let diagnostics = check_source_strict(source);
    let count = diagnostic_count(&diagnostics, 2344);
    assert_eq!(
        count, 1,
        "[{label}] expected one TS2344 from boolean not satisfying false, got {count}: {diagnostics:#?}"
    );
}

#[test]
fn identity_extends_of_deferred_conditionals_reports_boolean_constraint_error() {
    assert_one_ts2344(
        r#"
type Eq<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;

type Left<T> = T extends string ? 1 : 2;
type Right<T> = T extends string ? 1 : 3;
type AssertFalse<X extends false> = X;

type Bad<T> = AssertFalse<Eq<Left<T>, Right<T>>>;
"#,
        "issue repro",
    );
}

#[test]
fn identity_extends_of_deferred_conditionals_is_name_invariant() {
    assert_one_ts2344(
        r#"
type Same<X, Y> =
  (<P>() => P extends X ? "yes" : "no") extends
  (<P>() => P extends Y ? "yes" : "no") ? true : false;

type One<Q> = Q extends number ? "yes" : "no";
type Two<Q> = Q extends number ? "yes" : "maybe";
type ExpectFalse<V extends false> = V;

type Bad<Q> = ExpectFalse<Same<One<Q>, Two<Q>>>;
"#,
        "renamed params",
    );
}
