//! Regression coverage for #14141's `inferenceShouldFailOnEvolvingArrays`
//! sub-fixture: the native fix that replaced the source-text-gated
//! `align_evolving_array_inference_diagnostics` rewrite.
//!
//! Structural rule:
//!
//! > An evolving/auto array assignment target (`let acc = []`, or `let acc;
//! > acc = []`) has the provisional element type `any`; its finalized `any[]`
//! > is not a stable declared type. It must NOT be sourced as the contextual
//! > type for the assignment RHS. When the RHS is a generic call, using that
//! > `any[]` as a contextual return type fixes the call's type parameter to
//! > `any` and suppresses the argument-position error the same call reports
//! > with no context. So `acc = f([42])` for
//! > `f<T extends string[], U extends string>(arg: { [K in U]: T }[U]): T` must
//! > still check the `[42]` element against `string` — exactly as the bare
//! > `f([42])` statement does.
//!
//! `tsc` (pinned 7.0.2) reports `TS2322 Type 'number' is not assignable to type
//! 'string'.` at both the bare call and the evolving-array assignment. Every
//! case here uses distinct binder / type-parameter names so the behavior
//! follows the structural shape, not any identifier spelling (CLAUDE.md
//! anti-hardcoding gate) — the deleted rewrite matched the literal fixture text
//! `zz`/`logFirstLength`, which these names deliberately avoid.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

const TS2322_NOT_ASSIGNABLE: u32 = 2322;
const NUMBER_NOT_STRING: &str = "Type 'number' is not assignable to type 'string'.";

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn count_number_not_string(diags: &[(u32, String)]) -> usize {
    diags
        .iter()
        .filter(|(code, msg)| *code == TS2322_NOT_ASSIGNABLE && msg == NUMBER_NOT_STRING)
        .count()
}

/// The generic whose parameter type blocks argument inference but simplifies to
/// `T`, so a non-`string[]` argument surfaces the element mismatch (the shape
/// from TS issue #25675). Named `firstLen`/`A`/`B` to avoid the deleted
/// rewrite's `logFirstLength`/`T`/`U` literals.
const GENERIC: &str = "function firstLen<A extends string[], B extends string>(arg: { [K in B]: A }[B]): A {\n\
     \x20   return arg;\n\
     }\n";

#[test]
fn evolving_empty_array_target_still_reports_argument_error() {
    // Both the bare call and the evolving-array-assigned call must error.
    let src = format!(
        "{GENERIC}\
         firstLen([42]);\n\
         let acc = [];\n\
         acc = firstLen([42]);\n"
    );
    let diags = diagnostics(&src);
    assert_eq!(
        count_number_not_string(&diags),
        2,
        "evolving-array assignment must not suppress the generic-call argument \
         error; expected TS2322 at both the bare and assigned call, got: {diags:?}"
    );
}

#[test]
fn control_flow_any_null_then_empty_array_target_reports_argument_error() {
    // `let a; a = []` is the control-flow-typed-any evolving-array form; it must
    // behave like the direct `let a = []` initializer.
    let src = format!(
        "{GENERIC}\
         let bucket;\n\
         bucket = [];\n\
         bucket = firstLen([42]);\n"
    );
    let diags = diagnostics(&src);
    assert_eq!(
        count_number_not_string(&diags),
        1,
        "control-flow-any evolving array assignment must still report the \
         generic-call argument error; got: {diags:?}"
    );
}

#[test]
fn explicit_array_annotation_target_still_reports_argument_error() {
    // A real annotation is a stable declared type and still contextually types
    // the RHS — the fix is scoped to evolving/auto arrays, not to every `[]`.
    let src = format!(
        "{GENERIC}\
         let named: string[] = [];\n\
         named = firstLen([42]);\n"
    );
    let diags = diagnostics(&src);
    assert_eq!(
        count_number_not_string(&diags),
        1,
        "annotated array target must still report the generic-call argument \
         error; got: {diags:?}"
    );
}

#[test]
fn evolving_array_assigned_well_typed_generic_call_is_clean() {
    // Negative control: a `string[]` argument satisfies the constraint, so the
    // fix must not manufacture a spurious error on the evolving-array path.
    let src = format!(
        "{GENERIC}\
         let store = [];\n\
         store = firstLen([\"ok\"]);\n"
    );
    let diags = diagnostics(&src);
    assert_eq!(
        count_number_not_string(&diags),
        0,
        "a constraint-satisfying argument must not error on the evolving-array \
         path; got: {diags:?}"
    );
}

const TS6133_DECLARED_NEVER_READ: u32 = 6133;

fn diagnostics_no_unused(source: &str) -> Vec<(u32, String)> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_locals: true,
            no_unused_parameters: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

#[test]
fn write_only_assignment_target_still_reports_unused() {
    // The evolving-array gate resolves the assignment target, so it must NOT
    // mark the target as "read" — otherwise a write-only variable is silently
    // treated as used and its TS6133 disappears (the `unusedMultipleParameter*`
    // / `noUnusedLocals_writeOnly` regression family). Mirrors
    // `unusedMultipleParameter1InFunctionExpression`: `slot` is written only.
    let src = "var run = function (slot: string) {\n    slot = \"dummy\";\n};\n";
    let count = diagnostics_no_unused(src)
        .iter()
        .filter(|(code, msg)| *code == TS6133_DECLARED_NEVER_READ && msg.contains("'slot'"))
        .count();
    assert_eq!(
        count, 1,
        "a write-only assignment target must still report TS6133"
    );
}

#[test]
fn read_then_written_target_reports_no_unused() {
    // Negative control: once the variable is genuinely read, no TS6133 — the
    // gate must not manufacture a spurious unused diagnostic either.
    let src =
        "var run = function (slot: string) {\n    console.log(slot);\n    slot = \"dummy\";\n};\n";
    let has = diagnostics_no_unused(src)
        .iter()
        .any(|(code, msg)| *code == TS6133_DECLARED_NEVER_READ && msg.contains("'slot'"));
    assert!(!has, "a read variable must not report TS6133");
}

#[test]
fn evolving_array_assigned_plain_array_literal_is_clean() {
    // Withholding the contextual type must not break ordinary evolving-array
    // growth: assigning a concrete array literal remains error-free.
    let src = "let list = [];\nlist = [1, 2, 3];\nlist = [\"a\"];\n";
    let diags = diagnostics(src);
    assert!(
        diags.is_empty(),
        "plain evolving-array reassignment must stay clean; got: {diags:?}"
    );
}
