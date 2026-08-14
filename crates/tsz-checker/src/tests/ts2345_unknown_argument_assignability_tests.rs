//! `unknown` as a call *argument's* type must be checked like any other
//! type, not treated as an `any`/`error` escape hatch.
//!
//! Structural rule: `tsc` only accepts `unknown` where the target itself is
//! `any` or `unknown` (or the source and target are otherwise related); a
//! concrete or bare-type-parameter target still reports `TS2345`. Two
//! call-argument code paths independently special-cased `actual ==
//! TypeId::UNKNOWN` as if it were `TypeId::ERROR` (an unresolved-type
//! cascade guard) and silently dropped the diagnostic:
//! `handle_call_result`'s `ArgumentTypeMismatch` arm
//! (`types/computation/call_result.rs`) and
//! `should_suppress_argument_not_assignable_diagnostic`
//! (`error_reporter/call_errors/error_emission.rs`). The equivalent `TS2322`
//! assignment surface (`const s: string = v` where `v: unknown`) already
//! rejected this correctly, so the defect was isolated to call arguments.

use crate::test_utils::check_source_diagnostics;

fn has_ts2345(source: &str) -> bool {
    check_source_diagnostics(source)
        .iter()
        .any(|d| d.code == 2345)
}

#[test]
fn unknown_argument_against_concrete_target_reports_ts2345() {
    assert!(has_ts2345(
        r#"
function helper(x: string) {}
declare const v: unknown;
helper(v);
"#
    ));
}

#[test]
fn unknown_argument_from_catch_clause_reports_ts2345() {
    assert!(has_ts2345(
        r#"
function helper(x: string) {}
try {} catch (e) {
    helper(e);
}
"#
    ));
}

#[test]
fn unknown_argument_against_nested_function_outer_type_parameter_reports_ts2345() {
    assert!(has_ts2345(
        r#"
function outer<Q>(v: unknown) {
    function helper(x: Q) {}
    helper(v);
}
"#
    ));
}

#[test]
fn unknown_argument_against_explicit_type_argument_target_reports_ts2345() {
    assert!(has_ts2345(
        r#"
declare function takesQ<Q>(x: Q): void;
function c<Q>(v: unknown) { takesQ<Q>(v); }
"#
    ));
}

#[test]
fn unknown_argument_against_class_method_type_parameter_reports_ts2345() {
    assert!(has_ts2345(
        r#"
class Box<Q> {
    helper(x: Q) {}
    m(v: unknown) {
        this.helper(v);
    }
}
"#
    ));
}

#[test]
fn unknown_argument_against_any_target_is_clean() {
    assert!(!has_ts2345(
        r#"
function acceptsAny(x: any) {}
declare const v: unknown;
acceptsAny(v);
"#
    ));
}

#[test]
fn unknown_argument_against_unknown_target_is_clean() {
    assert!(!has_ts2345(
        r#"
function acceptsUnknown(x: unknown) {}
declare const v: unknown;
acceptsUnknown(v);
"#
    ));
}

#[test]
fn unknown_argument_reports_at_the_correct_index_among_multiple_arguments() {
    let diags = check_source_diagnostics(
        r#"
function two(a: number, b: string) {}
declare const v: unknown;
two(1, v);
"#,
    );
    let diag = diags
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected a TS2345; got: {diags:?}"));
    assert!(
        diag.message_text.contains("'unknown'") && diag.message_text.contains("'string'"),
        "expected mismatch against the second (string) parameter; got: {}",
        diag.message_text
    );
}

#[test]
fn number_argument_against_concrete_target_still_reports_ts2345_regression_guard() {
    // Regression guard: the ERROR/UNKNOWN-target legs of the suppression
    // checks stay intact, and ordinary non-`unknown` mismatches are
    // unaffected by removing the `actual == TypeId::UNKNOWN` legs.
    assert!(has_ts2345(
        r#"
function helper(x: string) {}
helper(42);
"#
    ));
}

#[test]
fn any_argument_against_concrete_target_stays_clean_regression_guard() {
    assert!(!has_ts2345(
        r#"
function helper(x: string) {}
declare const v: any;
helper(v);
"#
    ));
}
