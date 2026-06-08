//! Inference into a single naked type parameter that is a member of a union
//! target, from a multi-member source.
//!
//! Structural rule: when a generic parameter is shaped as `T | <concrete>`
//! (so the target union has exactly one naked type variable and no other
//! placeholder-bearing members), and the argument supplies several source
//! members (e.g. the element type `number | string` of an array literal),
//! `tsc` infers `T = number | string` — it forms `getUnionType(unmatched)`
//! and records it as a *single* inference candidate. tsz previously recorded
//! each source member as a separate competing candidate, so common-supertype
//! resolution (notably the array-element "leftmost wins" rule) fixed `T` to a
//! single member and emitted a spurious assignability error against the other
//! members at the argument position.
//!
//! These tests vary the type-parameter and function names to confirm the rule
//! is structural and not keyed to any particular identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn ts2322(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect()
}

/// `Array<T | boolean>` argument `[1, "a"]` must infer `T = string | number`,
/// producing no error at the call argument, and exactly one error when the
/// result is assigned to an incompatible annotation.
#[test]
fn union_target_array_infers_union_of_all_source_members() {
    let source = r#"
declare function pick<T>(xs: Array<T | boolean>): T;
const r = pick([1, "a"]);
const bad: 0 = r;
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 (the bad assignment), got: {diags:?}"
    );
    let msg = &errors[0].message_text;
    assert!(
        msg.contains("string | number") || msg.contains("number | string"),
        "Inferred type must be the union of both element types. Got: {msg}"
    );
}

/// Assigning the inferred union to a compatible annotation must not error: this
/// pins that no spurious error is emitted at the argument position either.
#[test]
fn union_target_array_union_result_is_assignable_to_union_annotation() {
    let source = r#"
declare function pick<T>(xs: Array<T | boolean>): T;
const r = pick([1, "a"]);
const good: string | number = r;
"#;
    let diags = check(source);
    assert!(
        ts2322(&diags).is_empty(),
        "Expected no TS2322 for the union-typed inference and assignment. Got: {diags:?}"
    );
}

/// The rule is independent of the chosen names: renaming the type parameter and
/// the function must not change the outcome.
#[test]
fn union_target_array_infers_union_with_renamed_binders() {
    let source = r#"
declare function gather<Elem>(items: Array<Elem | boolean>): Elem;
const collected = gather([1, "a"]);
const bad: 0 = collected;
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 with renamed binders, got: {diags:?}"
    );
    let msg = &errors[0].message_text;
    assert!(
        msg.contains("string | number") || msg.contains("number | string"),
        "Inferred type must be the union regardless of binder names. Got: {msg}"
    );
}

/// A non-`boolean` fixed arm exercises the same path with a different concrete
/// member, and three distinct source members must all be unioned.
#[test]
fn union_target_array_three_members_all_union() {
    let source = r#"
declare function take<V>(xs: Array<V | null>): V;
const r = take([1, "a", true]);
const bad: 0 = r;
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 (bad assignment), got: {diags:?}"
    );
    let msg = &errors[0].message_text;
    assert!(
        msg.contains("string")
            && msg.contains("number")
            && (msg.contains("true") || msg.contains("boolean")),
        "Inferred type must union all three element types. Got: {msg}"
    );
}

/// A single source member against a union target is unchanged: `T` is inferred
/// from the one source, with no union behavior to apply.
#[test]
fn union_target_single_source_member_unchanged() {
    let source = r#"
declare function one<T>(x: T | boolean): T;
const r = one(1);
const bad: string = r;
"#;
    let diags = check(source);
    let errors = ts2322(&diags);
    assert_eq!(
        errors.len(),
        1,
        "Expected exactly one TS2322 (number not assignable to string), got: {diags:?}"
    );
    assert!(
        errors[0].message_text.contains("number") && errors[0].message_text.contains("string"),
        "Got: {}",
        errors[0].message_text
    );
}
