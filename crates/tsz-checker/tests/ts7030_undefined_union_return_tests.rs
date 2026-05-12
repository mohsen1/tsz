//! Tests for TS7030 suppression when the declared return type includes `undefined`.
//!
//! When `noImplicitReturns` is enabled, TS7030 ("Not all code paths return a value")
//! must be suppressed for functions/methods whose annotated return type contains
//! `undefined`. An implicit fall-through returns `undefined`, which is type-safe
//! when `undefined` is part of the declared return type.
//!
//! Related issue: #5949

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn check_with_no_implicit_returns(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: true,
        no_implicit_returns: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// `string | undefined` return type: falling through returns implicit `undefined`,
/// which is type-safe. TS7030 must not be emitted.
#[test]
fn ts7030_suppressed_for_string_or_undefined_return() {
    let source = r#"
function maybeReturn(x: string | null): string | undefined {
    if (x !== null) {
        return x;
    }
    // Falls through: implicit undefined, valid for string | undefined
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type is string | undefined; got: {codes:?}"
    );
}

/// `number | undefined` return type in a regular function: same suppression rule.
#[test]
fn ts7030_suppressed_for_number_or_undefined_return() {
    let source = r#"
declare const cond: boolean;
function f(): number | undefined {
    if (cond) {
        return 42;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type is number | undefined; got: {codes:?}"
    );
}

/// Unannotated function with partial returns: TS7030 must still be emitted.
/// (TS7030 fires for unannotated functions; TS2366 fires for annotated ones.)
#[test]
fn ts7030_still_emitted_for_unannotated_function_with_partial_return() {
    let source = r#"
declare const cond: boolean;
function f() {
    if (cond) {
        return "yes";
    }
    // Falls through without returning
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        codes.contains(&7030),
        "TS7030 must be emitted for unannotated functions with partial returns; got: {codes:?}"
    );
}

/// Arrow function with `string | undefined` return type annotation.
#[test]
fn ts7030_suppressed_for_arrow_function_with_undefined_union() {
    let source = r#"
declare const cond: boolean;
const f = (): string | undefined => {
    if (cond) {
        return "yes";
    }
};
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted for arrow with string | undefined return; got: {codes:?}"
    );
}
