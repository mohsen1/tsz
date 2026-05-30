//! Tests for unbound `infer` defaults when an *optional* property of a
//! conditional's extends pattern is absent in the check type.
//!
//! Structural rule: when matching a check type against a conditional's `infer`
//! pattern, an absent source property at an *optional* pattern position
//! contributes no inference candidate (tsc's `inferFromProperties` skips it).
//! The infer variable stays unbound and defaults to its constraint (or
//! `unknown`) — tsc's `getInferredType` — and the conditional takes its **true**
//! branch. Previously tsz injected a spurious `undefined` candidate, which made
//! a plain `infer R` resolve to `undefined` (instead of `unknown`) and made a
//! constrained `infer R extends C` fail its constraint and collapse the
//! conditional to its false branch.
//!
//! The assertions use deliberate assignments that error under TS2322 iff the
//! inferred type is correct, so they distinguish `unknown` / the constraint from
//! the buggy `undefined` / false-branch results. Each case uses its own alias
//! names (including renamed infer variables) to prove the behavior is not keyed
//! to a particular identifier spelling.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn count_2322(source: &str) -> usize {
    check_source(source, "test.ts", strict_options())
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

#[test]
fn plain_unbound_infer_over_absent_optional_defaults_to_unknown() {
    // R has no candidate (`value` absent) and no constraint -> tsc: `unknown`,
    // true branch. `unknown` accepts any assignment, so there is no TS2322.
    // A buggy `undefined` (or the `"NONE"` false branch) would reject `123`.
    let source = r#"
        type Unwrap<T> = T extends { value?: infer R } ? R : "NONE";
        type Res = Unwrap<{ other: 1 }>;
        const accepted: Res = 123;
    "#;
    assert_eq!(count_2322(source), 0);
}

#[test]
fn constrained_unbound_infer_over_absent_optional_defaults_to_constraint() {
    // R has no candidate (`value` absent) but is constrained to `string` ->
    // tsc: R = `string`, true branch. Only `bad` (a number) must error; the
    // false-branch bug would instead reject `good`.
    let source = r#"
        type Unwrap<T> = T extends { value?: infer R extends string } ? R : "NONE";
        type Res = Unwrap<{ other: 1 }>;
        const good: Res = "hello";
        const bad: Res = 123;
    "#;
    assert_eq!(count_2322(source), 1);
}

#[test]
fn constrained_unbound_infer_default_is_not_name_sensitive() {
    // Same as above with the infer variable renamed `Q`; the fix must be keyed to
    // the structural shape, not the identifier.
    let source = r#"
        type Unwrap<T> = T extends { value?: infer Q extends string } ? Q : "NONE";
        type Res = Unwrap<{ other: 1 }>;
        const good: Res = "world";
        const bad: Res = 123;
    "#;
    assert_eq!(count_2322(source), 1);
}

#[test]
fn present_optional_property_still_infers_the_value_type() {
    // When the optional property is present, inference is unchanged: R = the
    // property type (`number`), not `number | undefined`. `bad` must error.
    let source = r#"
        type Unwrap<T> = T extends { value?: infer R } ? R : "NONE";
        type Res = Unwrap<{ value: number }>;
        const ok: Res = 3;
        const bad: Res = "x";
    "#;
    assert_eq!(count_2322(source), 1);
}

#[test]
fn multi_prop_pattern_defaults_only_the_absent_optional_slot() {
    // `a` is present (X = "hi"); `b` is an absent optional constrained to
    // `number` (Y = `number`). Only the `["hi", "no"]` assignment must error.
    let source = r#"
        type Pick2<T> = T extends { a: infer X; b?: infer Y extends number }
            ? [X, Y]
            : "NONE";
        type Res = Pick2<{ a: "hi" }>;
        const good: Res = ["hi", 7];
        const bad: Res = ["hi", "no"];
    "#;
    assert_eq!(count_2322(source), 1);
}

#[test]
fn required_absent_property_still_takes_the_false_branch() {
    // Regression guard: an absent *required* property is a hard no-match, so the
    // conditional takes its false branch (`"FALSE"`). `bad` must error.
    let source = r#"
        type Get<T> = T extends { req: infer X } ? X : "FALSE";
        type Res = Get<{ other: 1 }>;
        const good: Res = "FALSE";
        const bad: Res = 5;
    "#;
    assert_eq!(count_2322(source), 1);
}

#[test]
fn present_optional_violating_constraint_still_takes_the_false_branch() {
    // Regression guard: a *present* candidate that violates the constraint
    // (`number` is not assignable to `string`) takes the false branch, distinct
    // from the absent-optional case which defaults to the constraint.
    let source = r#"
        type Get<T> = T extends { v?: infer X extends string } ? X : "FALSE";
        type Res = Get<{ v: number }>;
        const good: Res = "FALSE";
        const bad: Res = 5;
    "#;
    assert_eq!(count_2322(source), 1);
}
