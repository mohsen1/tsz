//! Regression coverage for defaulted recursive aliases whose recursive branch
//! is detected as excessively deep.
//!
//! Tracks the solver-29-20 issue family. The structural rule is:
//! when a defaulted generic alias has already emitted TS2589 at the recursive
//! alias boundary, later aliases that merely reference the failed instantiation
//! should recover through the error type instead of cascading another TS2589 at
//! the reference site.

use tsz_checker::test_utils::{check_source_strict, diagnostic_codes, diagnostics_without_codes};

fn semantic_codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&diagnostics_without_codes(
        &check_source_strict(source),
        &[2318],
    ))
}

#[test]
fn defaulted_recursive_alias_does_not_cascade_ts2589_to_use_alias() {
    let codes = semantic_codes(
        r#"
type Link29<T, D extends number = 4> = [D] extends [0] ? T : Link29<T[]>;
type Solve29<T> = Link29<T> extends infer U ? U : never;
type A = Solve29<string>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "tsc reports the recursive alias boundary and wrapper alias only; got {codes:?}"
    );
}

#[test]
fn renamed_defaulted_recursive_alias_does_not_cascade_ts2589_to_use_alias() {
    let codes = semantic_codes(
        r#"
type Route<X, N extends number = 4> = [N] extends [0] ? X : Route<X[]>;
type Project<X> = Route<X> extends infer Y ? Y : never;
type Q = Project<number>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589, 2589],
        "renamed aliases and params should follow the same TS2589 recovery rule; got {codes:?}"
    );
}

#[test]
fn direct_recursive_alias_use_still_reports_ts2589() {
    let codes = semantic_codes(
        r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589],
        "direct recursive alias instantiation still owns its use-site TS2589; got {codes:?}"
    );
}

#[test]
fn ordinary_conditional_tuple_builder_still_reports_depth_at_deep_use() {
    let codes = semantic_codes(
        r#"
type BuildTuple<T, N extends number> = N extends N
  ? number extends N
    ? T[]
    : _BuildTuple<T, N, []>
  : never;
type _BuildTuple<T, N extends number, R extends unknown[]> =
  R["length"] extends N ? R : _BuildTuple<T, N, [T, ...R]>;
type Thousand = BuildTuple<number, 1000>;
"#,
    );

    assert_eq!(
        codes,
        vec![2589],
        "ordinary nonrecursive conditional wrappers should still report deep concrete instantiations; got {codes:?}"
    );
}
