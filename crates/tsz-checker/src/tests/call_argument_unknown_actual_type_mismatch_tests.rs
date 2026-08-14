//! Tests that a call argument whose type is the real `unknown` type is
//! reported as `TS2345` when the parameter type does not accept `unknown`.
//!
//! Structural rule: `tsc`'s call-argument relation check treats `unknown` as
//! an ordinary (if maximal) type — it fails to relate to any parameter type
//! other than `unknown`/`any`/a bare unconstrained type parameter, exactly
//! like any other concrete-but-mismatched argument type. `handle_call_result`
//! in `types/computation/call_result.rs` previously short-circuited its
//! `CallResult::ArgumentTypeMismatch` arm whenever the *argument's* resolved
//! type was `TypeId::UNKNOWN`, treating it the same as the `TypeId::ERROR`
//! cascading-failure sentinel and returning before the mismatch was ever
//! reported. That conflation silently accepted any `unknown`-typed argument
//! against any parameter type — a full false negative, not merely a missing
//! elaboration note.

use crate::test_utils::check_source_diagnostics;

fn ts2345_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2345)
        .count()
}

#[test]
fn unknown_argument_against_concrete_parameter_reports_ts2345() {
    let source = r#"
function accept(input: string): void {}
declare const value: unknown;
accept(value);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "expected TS2345 for an `unknown` argument against a `string` parameter"
    );
}

#[test]
fn unknown_argument_against_renamed_concrete_parameter_reports_ts2345() {
    // Same shape, different binder names — the fix must not be keyed on
    // any particular identifier.
    let source = r#"
function consumeNumber(quantity: number): void {}
declare const payload: unknown;
consumeNumber(payload);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "expected TS2345 for an `unknown` argument against a `number` parameter"
    );
}

#[test]
fn unknown_argument_through_wrapper_call_reports_ts2345() {
    let source = r#"
function inner(x: string): void {}
function outer(input: unknown): void {
    inner(input);
}
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "expected TS2345 when a wrapper function forwards its own `unknown` parameter"
    );
}

#[test]
fn unknown_argument_against_object_parameter_reports_ts2345() {
    let source = r#"
function accept(input: { field: string }): void {}
declare const value: unknown;
accept(value);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "expected TS2345 for an `unknown` argument against a structural object parameter"
    );
}

#[test]
fn unknown_argument_against_unknown_parameter_is_not_an_error() {
    let source = r#"
function accept(input: unknown): void {}
declare const value: unknown;
accept(value);
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "an `unknown` argument against an `unknown` parameter is always assignable"
    );
}

#[test]
fn unknown_argument_against_any_parameter_is_not_an_error() {
    let source = r#"
function accept(input: any): void {}
declare const value: unknown;
accept(value);
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "an `unknown` argument against an `any` parameter is always assignable"
    );
}

#[test]
fn unknown_argument_against_unconstrained_generic_parameter_is_not_an_error() {
    // `T` is unconstrained, so it widens to accept `unknown` directly —
    // this must stay a positive (no-diagnostic) case after the fix.
    let source = r#"
function identity<T>(input: T): T { return input; }
declare const value: unknown;
identity(value);
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "an `unknown` argument against an unconstrained type parameter is assignable"
    );
}

#[test]
fn unknown_assignment_to_string_still_reports_ts2322() {
    // Non-regression check: the sibling `TS2322` assignment surface (a
    // direct `const: T = value` initializer, not a call argument) already
    // rejected an `unknown` source correctly before this fix and must
    // continue to.
    let source = r#"
declare const value: unknown;
const s: string = value;
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322; got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
