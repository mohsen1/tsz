//! Regression tests for #4027.
//!
//! `arg_is_callback_with_unannotated_params` returned `true` for any
//! arrow/function expression whose parameters lacked annotations, and
//! several diagnostic suppression sites used that bare check to silence
//! TS2345 on call arguments. That over-suppressed cases where the target
//! callback signature simply did not have enough parameters to
//! contextually type the source: contextual typing cannot supply a type
//! for a parameter the target signature does not declare, so the
//! parameter-count mismatch must still surface.
//!
//! These tests pin the structural rule:
//!
//!   When a callback argument has unannotated parameters, suppress TS2345
//!   only if the target callable signature has at least as many fixed
//!   parameters as the source callback (or has a rest parameter).

use tsz_checker::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check_non_strict(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: false,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn unannotated_callback_param_count_mismatch_emits_ts2345() {
    let diagnostics = check_non_strict(
        r#"
declare function takesNoArgs(cb: () => void): void;

takesNoArgs(value => {});
"#,
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected TS2345 because callback has more parameters than target signature, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn unannotated_callback_with_matching_target_arity_is_accepted() {
    let diagnostics = check_non_strict(
        r#"
declare function takesOneArg(cb: (value: number) => void): void;

takesOneArg(value => {});
"#,
    );

    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "expected no TS2345 when target supplies a parameter for the callback, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn unannotated_callback_with_rest_target_is_accepted() {
    let diagnostics = check_non_strict(
        r#"
declare function takesRest(cb: (...args: number[]) => void): void;

takesRest((a, b, c) => {});
"#,
    );

    assert!(
        !diagnostics.iter().any(|d| d.code == 2345),
        "expected no TS2345 when target has a rest parameter, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Different bound-variable name ensures the fix is structural, not
/// hardcoded against the exact identifier used in the issue's repro.
#[test]
fn unannotated_callback_param_count_mismatch_independent_of_param_name() {
    let diagnostics = check_non_strict(
        r#"
declare function takesNoArgs(cb: () => void): void;

takesNoArgs(somethingElse => {});
"#,
    );

    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected TS2345 regardless of source parameter name, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
