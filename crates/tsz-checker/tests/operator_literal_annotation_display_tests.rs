//! Regression tests for #8031: operator error messages must show the widened
//! primitive type (`number`, `bigint`) when a parameter is annotated with a
//! numeric or bigint **literal** type, not the raw annotation text (`2`, `1n`).
//!
//! Structural rule: `operator_type_parameter_annotation_text_for_expression`
//! exists to preserve type-parameter annotation names (e.g. `T`, `K`) in
//! operator error messages. TypeScript identifier names always start with a
//! letter, `_`, or `$`; numeric/bigint literals start with a digit and must
//! not match the identifier heuristic.

use tsz_checker::test_utils::check_source_code_messages;

/// `false < two` where `two: 2` — primary repro from #8031.
/// The error message must say `'number'`, not `'2'`.
#[test]
fn numeric_literal_param_annotation_shows_widened_number() {
    let msgs = check_source_code_messages(
        r"
function f(two: 2) {
    let a = false < two;
}
",
    );
    let op_errors: Vec<_> = msgs.iter().filter(|(c, _)| *c == 2365).collect();
    assert!(
        !op_errors.is_empty(),
        "Expected TS2365 for `false < two`; got: {msgs:?}"
    );
    for (_, msg) in &op_errors {
        assert!(
            msg.contains("number"),
            "Expected error to mention 'number' not '2'; got: {msg}"
        );
        assert!(
            !msg.contains("'2'"),
            "Error must not display raw literal annotation '2'; got: {msg}"
        );
    }
}

/// `false < one` where `one: 1` — different numeric literal.
/// Proves the fix is not tied to the specific literal value `2`.
#[test]
fn different_numeric_literal_param_shows_widened_number() {
    let msgs = check_source_code_messages(
        r"
function g(one: 1) {
    let a = false < one;
}
",
    );
    let op_errors: Vec<_> = msgs.iter().filter(|(c, _)| *c == 2365).collect();
    assert!(
        !op_errors.is_empty(),
        "Expected TS2365 for `false < one: 1`; got: {msgs:?}"
    );
    for (_, msg) in &op_errors {
        assert!(
            msg.contains("number"),
            "Expected 'number' not '1'; got: {msg}"
        );
        assert!(
            !msg.contains("'1'"),
            "Must not show raw literal '1'; got: {msg}"
        );
    }
}

/// `false + big` where `big: 1n` — bigint literal parameter.
/// The message must say `'bigint'`, not `'1n'`.
#[test]
fn bigint_literal_param_annotation_shows_widened_bigint() {
    let msgs = check_source_code_messages(
        r"
function h(big: 1n) {
    let a = false + big;
}
",
    );
    let op_errors: Vec<_> = msgs
        .iter()
        .filter(|(c, _)| *c == 2365 || *c == 2362 || *c == 2363)
        .collect();
    assert!(
        !op_errors.is_empty(),
        "Expected operator error for `false + big: 1n`; got: {msgs:?}"
    );
    for (_, msg) in &op_errors {
        assert!(
            !msg.contains("'1n'"),
            "Must not show raw bigint literal '1n'; got: {msg}"
        );
    }
}

/// Type parameter annotation `T` must still use the identifier text.
/// Proves the fix does not regress the intended type-param display behavior.
#[test]
fn type_param_annotation_still_shows_identifier() {
    let msgs = check_source_code_messages(
        r"
function f<T extends string | bigint>(a: T) {
    return false < a;
}
",
    );
    let op_errors: Vec<_> = msgs
        .iter()
        .filter(|(c, _)| *c == 2365 || *c == 2362 || *c == 2363)
        .collect();
    assert!(
        !op_errors.is_empty(),
        "Expected an operator error for `false < a: T`; got: {msgs:?}"
    );
    for (_, msg) in &op_errors {
        assert!(
            msg.contains('T'),
            "Expected identifier annotation 'T' in error message; got: {msg}"
        );
    }
}

/// A short identifier-like annotation that resolves to a type alias is not a
/// type parameter. The operator message should render the widened operand type.
#[test]
fn short_type_alias_annotation_shows_widened_number() {
    let msgs = check_source_code_messages(
        r"
type N = number;
function f(a: N) {
    return false < a;
}
type Num = number;
function g(a: Num) {
    return false < a;
}
",
    );
    let op_errors: Vec<_> = msgs.iter().filter(|(c, _)| *c == 2365).collect();
    assert_eq!(
        op_errors.len(),
        2,
        "Expected two TS2365 errors for alias-annotated parameters; got: {msgs:?}"
    );
    for (_, msg) in &op_errors {
        assert!(
            msg.contains("number"),
            "Expected alias annotation to render widened 'number'; got: {msg}"
        );
        assert!(
            !msg.contains("'N'") && !msg.contains("'Num'"),
            "Alias annotation names must not be preserved as type-parameter text; got: {msg}"
        );
    }
}
