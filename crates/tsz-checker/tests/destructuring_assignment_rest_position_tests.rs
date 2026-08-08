//! TS2462 ("A rest element must be last in a destructuring pattern") for
//! destructuring *assignment* targets.
//!
//! The parser enforces the rule for binding-pattern forms (`let { ...a, b } = x`);
//! assignment targets parse as plain array/object literals, so the checker must
//! enforce it. Previously only top-level *array* assignment targets were checked;
//! object targets and every nested target were silently accepted. These tests
//! pin parity with `tsc` (verified against `typescript@7.0.2`).

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes, diagnostic_count};

fn ts2462_count(source: &str) -> usize {
    let diagnostics = check_source_diagnostics(source);
    diagnostic_count(&diagnostics, 2462)
}

/// The reported conformance witness (`objectRestPropertyMustBeLast.ts`): an
/// object rest that is not the last property in an assignment target errors.
#[test]
fn object_rest_not_last_in_assignment_errors() {
    let source = r#"
var a: any, x: any;
({ ...a, x } = { x: 1 });
"#;
    assert_eq!(
        ts2462_count(source),
        1,
        "object rest before another property in an assignment target must emit TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// Two out-of-place rests (`{ ...a, x, ...b }`): only the non-last `...a`
/// errors, matching `tsc` (the trailing `...b` is a valid last rest).
#[test]
fn object_rest_only_non_last_errors_in_assignment() {
    let source = r#"
var a: any, b: any, x: any;
({ ...a, x, ...b } = { x: 1 });
"#;
    assert_eq!(
        ts2462_count(source),
        1,
        "only the non-last object rest must emit TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// A rest that IS last is legal in both object and array assignment targets.
#[test]
fn rest_last_in_assignment_is_ok() {
    let source = r#"
var a: any, b: any, p: any, x: any, z: any;
({ p, ...a } = z);
[x, ...b] = z;
"#;
    assert_eq!(
        ts2462_count(source),
        0,
        "a trailing rest must not emit TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// The check recurses into nested targets: object rest inside an array target,
/// object rest inside an object target, and an array rest inside an object
/// target all error, matching `tsc`.
#[test]
fn nested_out_of_place_rest_in_assignment_errors() {
    let source = r#"
var a: any, b: any, c: any, x: any, y: any, p: any, z: any;
[x, { ...a, y }] = z;
({ p: { ...b, y } } = z);
({ p: [...c, y] } = z);
"#;
    assert_eq!(
        ts2462_count(source),
        3,
        "each nested out-of-place rest must emit TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// Regression: a top-level array rest that is not last still errors (behavior
/// preserved from the previous array-only implementation), and a nested array
/// rest now errors too.
#[test]
fn array_rest_not_last_in_assignment_errors_including_nested() {
    let source = r#"
var a: any, b: any, x: any, z: any;
[...a, x] = z;
[a, [...b, x]] = z;
"#;
    assert_eq!(
        ts2462_count(source),
        2,
        "top-level and nested non-last array rests must each emit TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// Anti-hardcoding: the rule is structural (spread position), not keyed on any
/// particular identifier. Renaming the binders keeps exactly one diagnostic.
#[test]
fn rest_position_rule_is_name_independent() {
    let source = r#"
var head: any, tail: any, mid: any;
({ ...head, mid } = { mid: 1 });
"#;
    assert_eq!(
        ts2462_count(source),
        1,
        "renaming binders must not change the structural TS2462 outcome, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}

/// The declaration binding-pattern path is unchanged: `var { ...a, x } = ...`
/// still emits exactly one TS2462 (enforced separately from assignment targets).
#[test]
fn declaration_binding_pattern_rest_still_errors_once() {
    let source = r#"
var { ...a, x } = { x: 1 };
"#;
    assert_eq!(
        ts2462_count(source),
        1,
        "declaration binding-pattern rest-not-last must still emit exactly one TS2462, got {:?}",
        diagnostic_codes(&check_source_diagnostics(source))
    );
}
