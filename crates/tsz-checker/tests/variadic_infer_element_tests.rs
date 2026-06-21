//! Regression tests for issue #14175.
//!
//! Inferring `T` from a parameter `T[]` given an argument of variadic-tuple
//! type `[...End extends string[]]` must infer `T = string` (the element type),
//! not `T = string[]` (the spread's constraint array). The wrong inference made
//! `head(end)` resolve to `string[]` and a downstream `const s: string = x`
//! failed with a spurious TS2322.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

/// Type-check `source` with the default lib bundle wired in (array/tuple
/// element semantics depend on the `Array<T>` global), returning every
/// diagnostic code. The empty-lib fast path cannot resolve `End[number]`.
fn diag_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// The exact issue repro: `head<T>(a: T[])` called with a `[...End]` argument
/// where `End extends string[]`. tsc infers `x: string`, so `const s: string =
/// x` type-checks. Before the fix tsz inferred `x: string[]` and reported a
/// spurious TS2322.
#[test]
fn variadic_tuple_spread_infers_element_type_not_constraint_array() {
    let source = r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const x = head(end);
  const s: string = x;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 (x should be inferred as string); got {codes:?}"
    );
}

/// Adjacent binder-name variation with a different constraint element type:
/// `Tail extends number[]`, so the spread element is `number` and a downstream
/// `const m: number = y` must type-check.
#[test]
fn variadic_tuple_spread_infers_element_type_binder_variation() {
    let source = r#"
declare function take<U>(items: U[]): U;
function gather<Tail extends number[]>(tail: [...Tail]) {
  const y = take(tail);
  const m: number = y;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 (y should be inferred as number); got {codes:?}"
    );
}

/// Negative control: a concrete array argument still infers the element type as
/// before. `head([1, 2, 3])` must infer `T = number`, so assigning the result
/// to a `string` must still report TS2322 (the fix must not blanket-suppress).
#[test]
fn concrete_array_still_infers_element_type() {
    let source = r#"
declare function head<T>(array: T[]): T;
const n = head([1, 2, 3]);
const bad: string = n;
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 assigning number element to string; got {codes:?}"
    );
}

/// Negative control: a genuine nested `T[][]` vs `T[]` parameter still infers
/// the inner element-array correctly. `flat<T>(a: T[][]): T[]` called with
/// `[[1],[2]]` infers `T = number`, returns `number[]`, and assigning that to a
/// `string[]` must report TS2322 (the element-extraction rule must not collapse
/// the nesting one level too far).
#[test]
fn nested_array_parameter_still_infers_one_level() {
    let source = r#"
declare function flat<T>(a: T[][]): T[];
const ff = flat([[1], [2]]);
const bad: string[] = ff;
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 assigning number[] to string[]; got {codes:?}"
    );
}
