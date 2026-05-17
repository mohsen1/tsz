//! Tests for issue #6486: `infer K` in a template-literal-type span must carry
//! an implicit `string` constraint so that `{ [P in K]: V }` passed as a type
//! argument does not produce a false TS2322.
//!
//! Structural rule: when `${infer X}` appears as a span expression in a
//! template-literal-type pattern in a conditional `extends` clause, `X` is
//! implicitly constrained to `string`. That constraint must be propagated into
//! the checker's type-parameter scope for the true branch so that `X` is
//! accepted as a valid mapped-type key even when the containing mapped type is
//! a type argument to another generic.

use tsz_checker::diagnostics::Diagnostic;

fn check(src: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_strict(src)
}

fn ts2322(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2322).collect()
}

fn assert_no_ts2322(src: &str, label: &str) {
    let diags = check(src);
    let errs = ts2322(&diags);
    assert!(
        errs.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        errs.iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

fn assert_has_ts2322(src: &str, label: &str) {
    let diags = check(src);
    let errs = ts2322(&diags);
    assert!(
        !errs.is_empty(),
        "[{label}] expected TS2322 but got no errors"
    );
}

// ---------------------------------------------------------------------------
// Original reproduction from issue #6486
// ---------------------------------------------------------------------------

#[test]
fn original_repro_parse_query_string() {
    assert_no_ts2322(
        r#"
type MergeParams<A, B> = {
  [K in keyof A | keyof B]: K extends keyof A
    ? K extends keyof B ? A[K] | B[K] : A[K]
    : K extends keyof B ? B[K] : never;
};
type ParseQS<S extends string> = S extends `${infer K}=${infer V}&${infer Rest}`
  ? MergeParams<{ [P in K]: V }, ParseQS<Rest>>
  : S extends `${infer K}=${infer V}`
  ? { [P in K]: V }
  : {};
type R = ParseQS<"a=1&b=2&c=3">;
const ok: R = { a: "1", b: "2", c: "3" };
export {};
"#,
        "original_repro",
    );
}

// ---------------------------------------------------------------------------
// Renamed variable — proves rule is not keyed on identifier spelling
// ---------------------------------------------------------------------------

#[test]
fn renamed_variable_key() {
    assert_no_ts2322(
        r#"
type Wrap<T> = { value: T };
type SingleKey<S extends string> = S extends `${infer Key}` ? Wrap<{ [P in Key]: string }> : never;
type R = SingleKey<"hello">;
export {};
"#,
        "renamed Key",
    );
}

#[test]
fn renamed_variable_segment() {
    assert_no_ts2322(
        r#"
type Wrap<T> = { value: T };
type Seg<S extends string> = S extends `${infer Segment}` ? Wrap<{ [P in Segment]: number }> : never;
type R = Seg<"world">;
export {};
"#,
        "renamed Segment",
    );
}

// ---------------------------------------------------------------------------
// Multiple spans — both infers get string constraint
// ---------------------------------------------------------------------------

#[test]
fn multiple_spans_both_in_type_args() {
    assert_no_ts2322(
        r#"
type Pair<A, B> = { first: A; second: B };
type Multi<S extends string> = S extends `${infer K}-${infer V}`
  ? Pair<{ [P in K]: string }, { [P in V]: number }>
  : never;
type R = Multi<"foo-bar">;
export {};
"#,
        "multiple spans both in type args",
    );
}

#[test]
fn three_spans() {
    assert_no_ts2322(
        r#"
type Triple<A, B, C> = { a: A; b: B; c: C };
type ThreeSeg<S extends string> = S extends `${infer A}/${infer B}/${infer C}`
  ? Triple<{ [P in A]: number }, { [P in B]: string }, { [P in C]: boolean }>
  : never;
type R = ThreeSeg<"x/y/z">;
export {};
"#,
        "three spans",
    );
}

// ---------------------------------------------------------------------------
// Inline (non-type-arg) form — regression: must still work
// ---------------------------------------------------------------------------

#[test]
fn inline_mapped_type_still_works() {
    assert_no_ts2322(
        r#"
type ParseSimple<S extends string> = S extends `${infer K}=${infer V}&${infer Rest}`
  ? { [P in K]: V } & ParseSimple<Rest>
  : S extends `${infer K}=${infer V}`
  ? { [P in K]: V }
  : {};
type R = ParseSimple<"a=1&b=2">;
export {};
"#,
        "inline mapped type regression",
    );
}

// ---------------------------------------------------------------------------
// Single-span in type-arg: simplest shape
// ---------------------------------------------------------------------------

#[test]
fn single_span_in_type_arg() {
    assert_no_ts2322(
        r#"
type Box<T> = { v: T };
type Single<S extends string> = S extends `${infer K}` ? Box<{ [P in K]: string }> : never;
type R = Single<"foo">;
export {};
"#,
        "single span in type arg",
    );
}

// ---------------------------------------------------------------------------
// Nested generic type arg with multiple levels
// ---------------------------------------------------------------------------

#[test]
fn nested_type_arg_levels() {
    assert_no_ts2322(
        r#"
type Outer<T> = { outer: T };
type Inner<T> = { inner: T };
type Nested<S extends string> = S extends `${infer K}`
  ? Outer<Inner<{ [P in K]: boolean }>>
  : never;
type R = Nested<"x">;
export {};
"#,
        "nested type arg levels",
    );
}

// ---------------------------------------------------------------------------
// Distinct independent names — one in template span, one not
// Each mapped type uses only its own variable
// ---------------------------------------------------------------------------

#[test]
fn distinct_names_remain_independent() {
    // K is from a template span (gets string constraint)
    // V is also from a template span (gets string constraint)
    // Both can be used as mapped keys independently
    assert_no_ts2322(
        r#"
type Dict<T> = { [key: string]: T };
type Param<S extends string> = S extends `${infer K}=${infer V}`
  ? Dict<{ [P in K]: string }> & Dict<{ [P in V]: number }>
  : never;
type R = Param<"key=val">;
export {};
"#,
        "distinct independent names",
    );
}

// ---------------------------------------------------------------------------
// Negative: infer NOT from a template span has no implicit string constraint
// and still errors when used as a mapped type key in a type arg
// ---------------------------------------------------------------------------

#[test]
fn non_template_infer_still_errors_as_mapped_key_in_type_arg() {
    // K is inferred from `{ key: infer K }` — no template-literal position,
    // no implicit string constraint. Using K as a mapped key in a type arg
    // should still emit TS2322 (K is not assignable to string | number | symbol).
    assert_has_ts2322(
        r#"
type Box<T> = { v: T };
type FromObject<T> = T extends { key: infer K } ? Box<{ [P in K]: string }> : never;
export {};
"#,
        "non-template infer errors in type arg",
    );
}
