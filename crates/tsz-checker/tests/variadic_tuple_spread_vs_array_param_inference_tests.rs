//! Checker integration tests for inferring an array parameter's element type
//! from a variadic-tuple argument's spread element.
//!
//! Structural rule: a spread `...X` in an argument tuple contributes the
//! *element* type of `X`, not `X` itself. So calling `head<T>(array: T[]): T`
//! with a variadic tuple `[...End]` (where `End extends string[]`) must infer
//! `T = string` — the element of `End`'s constraint — not `T = End` (whose
//! constraint `string[]` would then leak into the result and trip TS2322).
//!
//! Owner: the call-inference constraint walker
//! (`tsz_solver::operations::constraints`), which matches a `Tuple` source
//! against an `Array` target by relating each source element to the array
//! element. Before the fix, a rest element that was a type parameter (not a
//! bare `Array`) was passed through unchanged.
//!
//! Cases vary binder names and the spread's constraint shape so the rule
//! follows the type structure, not identifier text. #14175.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_only_one_2322(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "{label}: expected exactly one TS2322, got {codes:?}"
    );
}

// =============================================================================
// Positive: the variadic spread's element type drives the array param inference
// =============================================================================

#[test]
fn variadic_tuple_of_string_constrained_param_infers_element() {
    // The reported repro (#14175): tsc accepts; tsz used to infer `T = string[]`.
    assert_no_errors(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const x = head(end);
  const s: string = x;
}
"#,
        "head([...End]) where End extends string[] infers T = string",
    );
}

#[test]
fn variadic_tuple_inference_is_binder_name_independent() {
    // Same shape, different binder names — the rule must be structural.
    assert_no_errors(
        r#"
declare function first<Elem>(seq: Elem[]): Elem;
function pull<Rest extends string[]>(items: [...Rest]) {
  const v = first(items);
  const out: string = v;
}
"#,
        "renamed binders still infer the element type",
    );
}

#[test]
fn spread_of_concrete_array_infers_element() {
    assert_no_errors(
        r#"
declare function head<T>(array: T[]): T;
function probe(end: [...string[]]) {
  const s: string = head(end);
}
"#,
        "[...string[]] infers T = string",
    );
}

#[test]
fn fixed_prefix_then_variadic_infers_element() {
    assert_no_errors(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [string, ...End]) {
  const s: string = head(end);
}
"#,
        "[string, ...End] infers T = string",
    );
}

#[test]
fn variadic_tuple_of_number_constrained_param_infers_element() {
    assert_no_errors(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends number[]>(end: [...End]) {
  const n: number = head(end);
}
"#,
        "head([...End]) where End extends number[] infers T = number",
    );
}

// =============================================================================
// Negative: the inferred element type is still checked, so genuine mismatches
// remain errors (the fix must not over-accept).
// =============================================================================

#[test]
fn inferred_element_type_still_reports_real_mismatch() {
    assert_only_one_2322(
        r#"
declare function head<T>(array: T[]): T;
function probe<End extends number[]>(end: [...End]) {
  const bad: string = head(end);
}
"#,
        "number element is not assignable to string",
    );
}
