//! Repro + adjacent matrix for the false-negative where a generic type's
//! type-parameter constraint is a CONDITIONAL that references a SIBLING type
//! parameter which is OMITTED at the use site (relies on its default). The
//! constraint must be evaluated against the sibling's resolved default so the
//! conditional reduces and the type-argument constraint check (TS2344) runs.
//! See issue #14754 (`TanStack` Query `OmitKeyof`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn count_code(diags: &[u32], expected: u32) -> usize {
    diags.iter().filter(|&&c| c == expected).count()
}

/// Core witness (issue root-cause-direct reduction): the sibling `S` is omitted
/// and defaults to `'narrow'`, so the conditional constraint
/// `S extends 'wide' ? keyof O | string : keyof O` reduces to `keyof O`. The bad
/// key `"z"` violates `keyof A`, so tsc reports TS2344. tsz used to skip the
/// check because the defaulted sibling was never substituted.
#[test]
fn defaulted_sibling_conditional_constraint_emits_ts2344() {
    let diags = codes(
        r#"
type G<
  O,
  K extends S extends 'wide' ? keyof O | string : keyof O,
  S extends 'narrow' | 'wide' = 'narrow',
> = [O, K, S]
type A = { x: 1; y: 2 }
type T = G<A, "z">
export type _ = T
"#,
    );
    assert_eq!(
        count_code(&diags, 2344),
        1,
        "defaulted-sibling conditional constraint must still emit TS2344; got {diags:#?}"
    );
}

/// Anti-hardcoding: rename every binder. The rule is structural, not name-driven.
#[test]
fn defaulted_sibling_conditional_constraint_is_binder_name_independent() {
    let diags = codes(
        r#"
type Pick2<
  Obj,
  Key extends Mode extends 'loose' ? keyof Obj | string : keyof Obj,
  Mode extends 'tight' | 'loose' = 'tight',
> = [Obj, Key, Mode]
type Rec = { a: 1; b: 2 }
type Out = Pick2<Rec, "c">
export type _ = Out
"#,
    );
    assert_eq!(
        count_code(&diags, 2344),
        1,
        "renamed-binder form must also emit TS2344; got {diags:#?}"
    );
}

/// Positive control: a valid key satisfies the reduced constraint, so no TS2344.
#[test]
fn defaulted_sibling_conditional_constraint_valid_key_is_clean() {
    let diags = codes(
        r#"
type G<
  O,
  K extends S extends 'wide' ? keyof O | string : keyof O,
  S extends 'narrow' | 'wide' = 'narrow',
> = [O, K, S]
type A = { x: 1; y: 2 }
type T = G<A, "x">
export type _ = T
"#,
    );
    assert_eq!(
        count_code(&diags, 2344),
        0,
        "a valid key must satisfy the reduced constraint; got {diags:#?}"
    );
}

/// The `'wide'`/`'loose'` true branch widens the constraint to also accept a
/// `string`, so the same `"z"` that fails under the default is accepted when the
/// sibling is explicitly the wide mode.
#[test]
fn explicit_wide_sibling_accepts_extra_key() {
    let diags = codes(
        r#"
type G<
  O,
  K extends S extends 'wide' ? keyof O | string : keyof O,
  S extends 'narrow' | 'wide' = 'narrow',
> = [O, K, S]
type A = { x: 1; y: 2 }
type T = G<A, "z", 'wide'>
export type _ = T
"#,
    );
    assert_eq!(
        count_code(&diags, 2344),
        0,
        "the wide branch accepts an extra string key; got {diags:#?}"
    );
}

/// Explicit narrow sibling, bad key: still TS2344 (the conditional reduces to the
/// false branch `keyof O`).
#[test]
fn explicit_narrow_sibling_bad_key_emits_ts2344() {
    let diags = codes(
        r#"
type G<
  O,
  K extends S extends 'wide' ? keyof O | string : keyof O,
  S extends 'narrow' | 'wide' = 'narrow',
> = [O, K, S]
type A = { x: 1; y: 2 }
type T = G<A, "z", 'narrow'>
export type _ = T
"#,
    );
    assert_eq!(
        count_code(&diags, 2344),
        1,
        "explicit narrow sibling with a bad key must emit TS2344; got {diags:#?}"
    );
}

/// `@ts-expect-error` regression mirroring `OmitKeyof.test-d.ts`: the directive
/// is consumed by the TS2344, so there must be no TS2578 (unused directive).
#[test]
fn ts_expect_error_on_constraint_violation_is_consumed() {
    let diags = codes(
        r#"
type OmitKeyof<
  TObject,
  TKey extends TStrictly extends 'safely'
    ? keyof TObject | (string & Record<never, never>)
    : keyof TObject,
  TStrictly extends 'strictly' | 'safely' = 'strictly',
> = [TObject, TKey, TStrictly]
type A = { x: string; y: number }
// @ts-expect-error 'z' is not in keyof A
type T = OmitKeyof<A, 'z'>
export type _ = T
"#,
    );
    assert_eq!(
        count_code(&diags, 2578),
        0,
        "the @ts-expect-error must be consumed by TS2344, not reported as unused; got {diags:#?}"
    );
}
