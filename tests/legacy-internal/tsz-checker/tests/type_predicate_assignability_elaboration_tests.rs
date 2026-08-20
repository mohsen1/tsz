//! Regression tests for the assignability elaboration of a type-predicate
//! return mismatch — TS1224 (`Signature '{0}' must be a type predicate.`) and
//! TS1226 (`Type predicate '{0}' is not assignable to '{1}'.`).
//!
//! Structural rule: `are_type_predicates_compatible`
//! (`crates/tsz-solver/src/relations/subtype/rules/functions/mod.rs`) already
//! correctly rejects these assignments — `x is T` vs. a plain
//! boolean-returning function, or two incompatible predicates — so TS2322
//! already fires. What was missing is the elaboration `tsc` always attaches
//! under that TS2322 header; tsz owns it through the same
//! `relation -> reason -> diagnostic` chain as every other structural
//! assignability failure, via a new `SubtypeFailureReason::TypePredicateMismatch`
//! explained in `explain_function.rs` and rendered in
//! `render_type_predicate_mismatch`.
//!
//! The rule is structural, so cases vary binder/interface names where a name
//! reaches the rendered output (CLAUDE.md anti-hardcoding gate).

use crate::test_utils::{check_with_options, strict_checker_options};

/// Full elaboration text (primary message plus every related-information
/// line, in order) of the single diagnostic with `code` in `source`.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_with_options(source, strict_checker_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

/// TS1224: the target demands a type guard (`x is string`) and the source is
/// a plain boolean-returning function with no predicate at all.
#[test]
fn plain_boolean_function_to_type_guard_reports_ts1224() {
    let text = elaboration(
        r#"
function isString(x: unknown): boolean {
    return typeof x === "string";
}
let guard: (x: unknown) => x is string;
guard = isString;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: unknown) => boolean' is not assignable to type '(x: unknown) => x is string'.\n\
         Signature '(x: unknown): boolean' must be a type predicate.",
    );
}

/// Same rule, renamed binders/parameter — locks the shape as structural, not
/// a fixture spelling.
#[test]
fn plain_boolean_function_to_type_guard_reports_ts1224_renamed_binders() {
    let text = elaboration(
        r#"
function checkThing(value: unknown): boolean {
    return typeof value === "number";
}
let predicate: (value: unknown) => value is number;
predicate = checkThing;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(value: unknown) => boolean' is not assignable to type '(value: unknown) => value is number'.\n\
         Signature '(value: unknown): boolean' must be a type predicate.",
    );
}

/// TS1226: both sides declare a type guard but narrow to incompatible types.
/// tsc nests the inner `Type 'S' is not assignable to type 'T'.` leaf beneath
/// the predicate-specific line.
#[test]
fn incompatible_type_guards_reports_ts1226() {
    let text = elaboration(
        r#"
declare function isNumber(x: unknown): x is number;
let guard: (x: unknown) => x is string;
guard = isNumber;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(x: unknown) => x is number' is not assignable to type '(x: unknown) => x is string'.\n\
         Type predicate 'x is number' is not assignable to 'x is string'.\n\
         Type 'number' is not assignable to type 'string'.",
    );
}

/// Same rule, renamed binders — the predicate subject and interface names
/// must not leak into the structural decision.
#[test]
/// The trailing `'meow' is declared here.` is tsc's `TS2728` pointer, folded in
/// by `elaboration` with every other related-information line — not an extra
/// chain frame. tsc emits it on this nested missing-property frame; tsz dropped
/// it at `depth > 0` until #16443's nested-elaboration fix. Oracled on
/// `typescript@7.0.2`: `pred.ts:1:17` on `meow`.
fn incompatible_type_guards_reports_ts1226_renamed_binders() {
    let text = elaboration(
        r#"
interface Cat { meow(): void; }
interface Dog { bark(): void; }
declare function isDog(value: unknown): value is Dog;
let guard: (value: unknown) => value is Cat;
guard = isDog;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type '(value: unknown) => value is Dog' is not assignable to type '(value: unknown) => value is Cat'.\n\
         Type predicate 'value is Dog' is not assignable to 'value is Cat'.\n\
         Property 'meow' is missing in type 'Dog' but required in type 'Cat'.\n\
         'meow' is declared here.",
    );
}

/// Negative control: a target with an ASSERTION predicate (`asserts x`, no
/// narrowed type) accepts a plain function with no predicate at all — the
/// assertion is a call-site annotation, not a runtime contract the
/// implementation must satisfy. Must stay clean (no TS2322/TS1224).
#[test]
fn plain_function_to_assertion_only_target_is_compatible() {
    let codes = diagnostic_codes(
        r#"
function noop(x: unknown): void {}
let assertFn: (x: unknown) => asserts x;
assertFn = noop;
"#,
    );
    assert!(
        codes.is_empty(),
        "assigning a plain void function to an assertion-only target must be clean, got {codes:?}"
    );
}

/// Negative control: a source with a type guard is assignable to a plain
/// boolean-returning target (the predicate only narrows the caller's view;
/// tsc treats it as strictly more specific). Must stay clean.
#[test]
fn type_guard_source_to_plain_boolean_target_is_compatible() {
    let codes = diagnostic_codes(
        r#"
declare function isString(x: unknown): x is string;
let plain: (x: unknown) => boolean;
plain = isString;
"#,
    );
    assert!(
        codes.is_empty(),
        "assigning a type-guard source to a plain boolean target must be clean, got {codes:?}"
    );
}

/// Negative control: matching type guards on both sides are compatible.
#[test]
fn matching_type_guards_are_compatible() {
    let codes = diagnostic_codes(
        r#"
declare function isString(x: unknown): x is string;
let guard: (x: unknown) => x is string;
guard = isString;
"#,
    );
    assert!(
        codes.is_empty(),
        "identical type guards must be compatible, got {codes:?}"
    );
}
