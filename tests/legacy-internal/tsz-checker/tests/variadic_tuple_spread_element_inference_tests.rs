//! Tests for issue #14175: inferring `T` from a parameter `T[]` against a
//! variadic-tuple argument `[...End]` must infer the element type, not the whole
//! constraint array.
//!
//! Structural rule: a variadic/rest spread `...End` (where `End extends string[]`)
//! distributes the type obtained by number-indexing its operand — `string` — never
//! the operand `End` itself. The element-type query
//! (`rest_spread_element_type`) is the single owner of that rule, so indexed
//! access (`[...End][number]`), generic-call inference (`head([...End])`), and
//! best-common-type all agree.
//!
//! Binder names are varied across the cases to prove the fix is structural and not
//! keyed on any identifier.

use crate::test_utils::check_source_diagnostics;

fn ts2322_2345_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .filter(|c| *c == 2322 || *c == 2345)
        .collect()
}

/// The exact witness from the issue: `head([...End])` infers `T = string`, so
/// `const s: string = x` is clean.
#[test]
fn head_of_variadic_spread_infers_element_type() {
    let codes = ts2322_2345_codes(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const x = head(end);
  const s: string = x;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no assignability errors, got: {codes:?}"
    );
}

/// The element type of a variadic spread is observable directly through
/// number-indexed access, independent of inference.
#[test]
fn number_index_of_variadic_spread_is_element_type() {
    let codes = ts2322_2345_codes(
        r#"
function probe<Items extends string[]>(items: [...Items]) {
  type E = (typeof items)[number];
  const e: E = "ok";
}
"#,
    );
    assert!(codes.is_empty(), "expected E = string, got: {codes:?}");
}

/// Alias-wrapped array parameter (`Arr<T> = T[]`) reaches the same element rule.
#[test]
fn head_through_array_alias_infers_element_type() {
    let codes = ts2322_2345_codes(
        r#"
type Arr<U> = U[];
declare function head<T>(a: Arr<T>): T;
function probe<Rest extends string[]>(rest: [...Rest]) {
  const x = head(rest);
  const s: string = x;
}
"#,
    );
    assert!(codes.is_empty(), "expected T = string, got: {codes:?}");
}

/// Negative control: a genuine mismatch (`number` target) must still be reported,
/// proving the fix narrows the element type rather than blanket-suppressing.
#[test]
fn head_of_variadic_spread_still_rejects_real_mismatch() {
    let codes = ts2322_2345_codes(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const x = head(end);
  const n: number = x;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "expected TS2322 for string assigned to number, got: {codes:?}"
    );
}
