//! Regression tests for issue #14175 and the broader variadic-tuple
//! spread-element-type family.
//!
//! The element type contributed by a spread `...X` inside a tuple is `X`'s
//! number-indexed element type (tsc's `getElementTypeOfArrayType`). For a
//! variadic spread of a type parameter constrained to an array — `[...End]`
//! with `End extends string[]` — that element type is `End[number]` (`string`),
//! NOT `End` itself (`string[]`). The previous ad-hoc helpers returned the
//! spread type unchanged, so:
//!
//! * inferring `T` from a `T[]` parameter against a `[...End]` argument bound
//!   `T = string[]` instead of `string` (the reported #14175 false positive),
//! * indexing `[...End][number]` / `[...End][0]` produced `string[]`, and
//! * `[...End]` (or `[...P]` with a tuple-constrained `P`) was wrongly rejected
//!   as assignable to `(string | number)[]`.
//!
//! All three now resolve through the shared `rest_spread_element_type` helper.
//! Tests vary the binder names so the fix is structural, not name-keyed.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2322_count(source: &str) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(code, _)| *code == 2322)
        .count()
}

#[test]
fn infer_element_from_variadic_tuple_argument_against_array_param() {
    // Reported repro: `head(end)` must infer `T = End[number]` (`string`), so
    // `const s: string = x` is accepted.
    let source = r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const x = head(end);
  const s: string = x;
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "T must infer the element type End[number] (string), not the constraint array string[]"
    );
}

#[test]
fn infer_element_from_variadic_tuple_argument_renamed_binders() {
    // Same rule, different binder spellings and a readonly array constraint —
    // the behaviour is structural, not tied to `head`/`T`/`End`.
    let source = r#"
declare function first<Elem>(xs: readonly Elem[]): Elem;
function probe<Acc extends readonly string[]>(acc: readonly [...Acc]) {
  const value = first(acc);
  const s: string = value;
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "renamed binders and readonly arrays must infer the element type too"
    );
}

#[test]
fn variadic_tuple_indexed_access_resolves_element_type() {
    // `[...End][0]` and `(typeof end)[number]` are `string`, not `string[]`.
    let source = r#"
function probe<End extends string[]>(end: [...End]) {
  const head: string = end[0];
  type N = (typeof end)[number];
  const member: string = (0 as unknown) as N;
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "indexing a variadic tuple spread yields the spread's element type"
    );
}

#[test]
fn variadic_tuple_assignable_to_array_of_element_union() {
    // `[...P]` with `P extends [string, number]` is assignable to
    // `(string | number)[]` — the relation must use the constraint's element
    // union, not the tuple constraint itself.
    let source = r#"
function probe<P extends [string, number]>(end: [...P]) {
  const a: (string | number)[] = end;
}
function probe2<End extends string[]>(end: [...End]) {
  const a: string[] = end;
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "a variadic tuple spread is assignable to an array of its element union"
    );
}

#[test]
fn variadic_tuple_element_mismatch_still_reported() {
    // Negative control: `[...End]` with `End extends string[]` is NOT assignable
    // to `number[]`, and the inferred element must reject a `number`-typed sink.
    let source = r#"
declare function head<T>(array: T[]): T;
function probe<End extends string[]>(end: [...End]) {
  const bad: number[] = end;
  const x = head(end);
  const alsoBad: number = x;
}
"#;
    assert_eq!(
        ts2322_count(source),
        2,
        "genuine element-type mismatches must still be reported"
    );
}

#[test]
fn concrete_array_argument_still_infers_element_type() {
    // Negative control / non-generic form: a plain `string[]` argument still
    // fixes `T = string`, and a number sink is rejected.
    let source = r#"
declare function head<T>(array: T[]): T;
function probe(end: string[]) {
  const x = head(end);
  const s: string = x;
  const bad: number = x;
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "concrete array arguments keep inferring the element type"
    );
}
