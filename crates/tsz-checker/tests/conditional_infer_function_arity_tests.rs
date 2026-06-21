//! A conditional type's `extends` check against a function `infer` pattern must
//! respect parameter arity: a source function with MORE required parameters than
//! a fixed-arity pattern (no trailing rest) does not match, so the conditional
//! falls to its next branch.
//!
//! Regression for #14323 (type-zoo): `F extends (p0: infer P0) => any ? [P0] :
//! F extends (p0, p1) => any ? [P0, P1] : never` for `F = (a: string, b: number)
//! => void` must pick `[P0, P1]`. tsz matched the 1-arity pattern (ignoring the
//! extra param) and produced `[P0]`, yielding a spurious TS2322.

use tsz_checker::test_utils::check_source_code_messages;

fn error_count(source: &str) -> usize {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code != 0)
        .count()
}

#[test]
fn two_arity_source_does_not_match_one_arity_infer_pattern() {
    let src = r#"
type ParamTypes<F> =
  F extends (p0: infer P0) => any ? [P0]
  : F extends (p0: infer P0, p1: infer P1) => any ? [P0, P1] : never;
const fn = (a: string, b: number) => {};
const pts: [string, number] = (null as any as ParamTypes<typeof fn>);
export {};
"#;
    assert_eq!(
        error_count(src),
        0,
        "a 2-arity function must fall through the 1-arity infer pattern to the 2-arity branch"
    );
}

// Binder-name variation: the rule is structural (parameter arity), not keyed on
// any identifier.
#[test]
fn renamed_two_arity_source_does_not_match_one_arity_pattern() {
    let src = r#"
type Args<Func> =
  Func extends (first: infer A) => any ? [A]
  : Func extends (first: infer A, second: infer B) => any ? [A, B] : never;
const handler = (x: boolean, y: string) => {};
const got: [boolean, string] = (null as any as Args<typeof handler>);
export {};
"#;
    assert_eq!(
        error_count(src),
        0,
        "renamed pattern: 2-arity source must not match the 1-arity infer branch"
    );
}

// Negative control 1: a rest infer pattern (`(...args: infer P) => any`,
// `Parameters`-style) absorbs any arity and must still match — the arity guard is
// exempt for trailing-rest patterns. Uses a user-defined alias to avoid depending
// on lib utility types in the unit harness.
#[test]
fn rest_infer_pattern_still_matches_any_arity() {
    let src = r#"
type Args<F> = F extends (...args: infer P) => any ? P : never;
type Two = Args<(a: string, b: number) => void>;
const two: [string, number] = null as any as Two;
type Zero = Args<() => void>;
const zero: [] = null as any as Zero;
export {};
"#;
    assert_eq!(
        error_count(src),
        0,
        "a rest infer pattern must keep matching every arity (2-arity and 0-arity)"
    );
}

// Negative control 2: a source with FEWER parameters still matches the wider
// pattern, defaulting the unmatched infer slots to `unknown`.
#[test]
fn fewer_arity_source_still_matches_wider_pattern() {
    let src = r#"
type Pair<F> = F extends (a: infer A, b: infer B) => any ? [A, B] : never;
type T = Pair<(x: string) => void>;
const t: [string, unknown] = null as any as T;
export {};
"#;
    assert_eq!(
        error_count(src),
        0,
        "a 1-arity source must still match a 2-arity infer pattern (B defaults to unknown)"
    );
}
