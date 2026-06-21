//! Regression: under an ES5 target, iterating/spreading a **readonly** array
//! (`readonly T[]` / `ReadonlyArray<T>`) or a generic type parameter constrained
//! to an array/tuple/readonly-array must not emit TS2802 — these iterate as
//! arrays in ES5 (no `--downlevelIteration` needed), matching tsc. The ES5
//! array-like iterability checks recognized neither readonly arrays nor a type
//! parameter's apparent type.
//!
//! Owner: `crates/tsz-checker/src/checkers/iterable_checker.rs`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn es5_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES5,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn es5_iterate_generic_constrained_to_readonly_array_no_ts2802() {
    let codes = es5_codes(
        r#"
function iterate<A extends ReadonlyArray<number>>(a: A): number[] {
  for (const v of a) { v.toFixed(); }
  return [...a];
}
"#,
    );
    assert!(
        !codes.contains(&2802),
        "iterating a generic constrained to ReadonlyArray must not emit TS2802 in ES5; got {codes:?}"
    );
}

#[test]
fn es5_iterate_concrete_readonly_array_no_ts2802() {
    let codes = es5_codes(
        r#"
function f(a: readonly number[], b: ReadonlyArray<string>) {
  for (const x of a) { x.toFixed(); }
  return [...b];
}
"#,
    );
    assert!(
        !codes.contains(&2802),
        "iterating a concrete readonly array must not emit TS2802 in ES5; got {codes:?}"
    );
}

#[test]
fn es5_iterate_generic_constrained_to_tuple_no_ts2802() {
    let codes = es5_codes(
        r#"
function g<Items extends [number, string]>(items: Items) {
  for (const v of items) { void v; }
  return [...items];
}
"#,
    );
    assert!(
        !codes.contains(&2802),
        "iterating a generic constrained to a tuple must not emit TS2802 in ES5; got {codes:?}"
    );
}
