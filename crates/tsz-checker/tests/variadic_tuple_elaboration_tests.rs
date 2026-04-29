//! Tests for variadic-rest tuple elaboration: element-level errors should only
//! be reported for leading fixed elements; variadic/trailing failures defer to
//! tuple-level diagnostics.
//!
//! Regression for: variadicTuples2.ts fingerprint parity

use tsz_checker::test_utils::check_source_diagnostics;

/// When assigning an array literal to a variadic-rest tuple with trailing fixed
/// elements, and the leading element is wrong, exactly one element-level TS2322
/// should be emitted (not extra errors for the variadic/trailing sections).
#[test]
fn variadic_rest_tuple_leading_mismatch_reports_single_element_error() {
    let diags = check_source_diagnostics(
        r#"
type V03 = [number, ...string[], number];
declare let v03: V03;
v03 = [true, 'abc', 'def', 1];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for leading element mismatch. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    // Should only be 1 element-level error (at index 0), not multiple
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 (element 0 mismatch), got {}: {:?}",
        ts2322.len(),
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Assigning an array literal with wrong trailing element to a variadic-rest
/// tuple should produce exactly one TS2322 (tuple-level, not element-level for
/// both the trailing and other sections).
#[test]
fn variadic_rest_tuple_trailing_mismatch_reports_single_error() {
    let diags = check_source_diagnostics(
        r#"
type V03 = [number, ...string[], number];
declare let v03: V03;
v03 = [1, 'abc', 'def', true];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 for trailing element mismatch, got {}: {:?}",
        ts2322.len(),
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Assigning an array literal with mismatched variadic element type to a
/// variadic-rest tuple should produce exactly one TS2322 (no duplicate/extra
/// errors at element level for trailing section).
#[test]
fn variadic_rest_tuple_variadic_mismatch_no_extra_errors() {
    let diags = check_source_diagnostics(
        r#"
type V03 = [number, ...string[], number];
declare let v03: V03;
v03 = [1, 'abc', 42, 3];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 for variadic section mismatch, got {}: {:?}",
        ts2322.len(),
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// A valid assignment to a variadic-rest tuple should produce no errors.
#[test]
fn variadic_rest_tuple_valid_assignment_no_errors() {
    let diags = check_source_diagnostics(
        r#"
type V03 = [number, ...string[], number];
declare let v03: V03;
v03 = [1, 'a', 'b', 2];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for valid variadic-rest tuple assignment. Got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// For a tuple with a rest element but NO trailing fixed elements,
/// element-level errors should still be reported normally for leading elements.
#[test]
fn plain_variadic_tuple_element_error_still_reported() {
    let diags = check_source_diagnostics(
        r#"
type V = [number, ...string[]];
declare let v: V;
v = [true, 'a', 'b'];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for leading element mismatch in plain variadic tuple. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Tuple with only trailing rest and one fixed element (no leading fixed):
/// wrong element in the fixed position should produce a single error.
#[test]
fn trailing_only_variadic_tuple_fixed_element_mismatch_single_error() {
    let diags = check_source_diagnostics(
        r#"
type V01 = [...string[], number];
declare let v01: V01;
v01 = ['abc', 'def', 5, 6];
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 for trailing+rest tuple trailing mismatch, got {}: {:?}",
        ts2322.len(),
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
