//! Tests for TS7030 handling when a declared return type includes `undefined`.
//!
//! When `noImplicitReturns` is enabled, TS7030 ("Not all code paths return a value")
//! is suppressed for top-level `undefined`, `void`, `any`, and unions that include
//! `void`. It is not suppressed for `T | undefined`.
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

/// `string | undefined` return type: TypeScript still reports TS7030 for a
/// partial value-returning path.
#[test]
fn ts7030_emitted_for_string_or_undefined_return() {
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
        codes.contains(&7030),
        "TS7030 must be emitted when return type is string | undefined; got: {codes:?}"
    );
}

/// `number | undefined` return type in a regular function: same reporting rule.
#[test]
fn ts7030_emitted_for_number_or_undefined_return() {
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
        codes.contains(&7030),
        "TS7030 must be emitted when return type is number | undefined; got: {codes:?}"
    );
}

#[test]
fn ts7030_suppressed_for_top_level_undefined_return() {
    let source = r#"
declare const cond: boolean;
function f(): undefined {
    if (cond) {
        return undefined;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type is undefined; got: {codes:?}"
    );
}

#[test]
fn ts7030_suppressed_for_void_return() {
    let source = r#"
declare const cond: boolean;
function f(): void {
    if (cond) {
        return undefined;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type is void; got: {codes:?}"
    );
}

#[test]
fn ts7030_suppressed_for_any_return() {
    let source = r#"
declare const cond: boolean;
function f(): any {
    if (cond) {
        return undefined;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type is any; got: {codes:?}"
    );
}

#[test]
fn ts7030_suppressed_for_union_with_void_return() {
    let source = r#"
declare const cond: boolean;
function f(): string | void {
    if (cond) {
        return undefined;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted when return type includes void; got: {codes:?}"
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
fn ts7030_emitted_for_arrow_function_with_undefined_union() {
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
        codes.contains(&7030),
        "TS7030 must be emitted for arrow with string | undefined return; got: {codes:?}"
    );
}

#[test]
fn ts7030_suppressed_for_unannotated_any_return() {
    let source = r#"
declare const cond: boolean;
function f() {
    if (cond) {
        return undefined as any;
    }
}
"#;
    let codes = check_with_no_implicit_returns(source);
    assert!(
        !codes.contains(&7030),
        "TS7030 must not be emitted for unannotated function inferred as any; got: {codes:?}"
    );
}
