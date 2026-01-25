//! Tests for global type error detection (TS2318, TS2583)
//!
//! These tests verify that missing global types emit appropriate errors:
//! - TS2318: Cannot find global type (for @noLib or pre-ES2015 types)
//! - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)
//!
//! Note: These tests use TestContext::new_without_lib() to simulate missing lib.d.ts

use crate::test_fixtures::TestContext;

/// Helper function to create a checker without lib.d.ts and check source code
fn check_without_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let mut ctx = TestContext::new_without_lib();
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_missing_promise_emits_ts2304_without_lib() {
    // Without lib.d.ts, Promise should emit TS2304 (Cannot find name)
    let diagnostics = check_without_lib("const p = new Promise<void>();");

    // Should emit TS2304 for Promise when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Promise without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_map_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib("const m = new Map<string, number>();");

    // Should emit TS2304 for Map when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Map without lib.d.ts"
    );
}

#[test]
fn test_missing_set_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib("const s = new Set<number>();");

    // Should emit TS2304 for Set when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Set without lib.d.ts"
    );
}

#[test]
fn test_missing_symbol_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(r#"const s = Symbol("foo");"#);

    // Should emit TS2304 for Symbol when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Symbol without lib.d.ts"
    );
}

#[test]
fn test_missing_date_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib("const d = new Date();");

    // Should emit TS2304 for Date when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Date without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_regexp_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(r#"const r = new RegExp("foo");"#);

    // Should emit TS2304 for RegExp when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for RegExp without lib.d.ts"
    );
}

#[test]
fn test_promise_type_reference_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(
        r#"
function foo(): Promise<void> {
    return Promise.resolve();
}
"#,
    );

    // Should emit TS2304 for Promise in both type position and expression
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 errors for Promise without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_console_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(r#"console.log("hello");"#);

    // Should emit TS2304 for console when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for console without lib.d.ts"
    );
}

// Tests with lib.d.ts loaded - these should NOT emit errors

/// Helper function to create a checker WITH lib.d.ts and check source code
fn check_with_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let mut ctx = TestContext::new();
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder
        .bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_console_no_error_with_lib() {
    let diagnostics = check_with_lib(r#"console.log("hello");"#);

    // With lib.d.ts, console should not emit TS2304
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "console should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}

#[test]
fn test_array_no_error_with_lib() {
    let diagnostics = check_with_lib("const arr: Array<number> = [1, 2, 3];");

    // Array is a built-in type that should be available with lib.d.ts
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "Array should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}

#[test]
fn test_object_no_error_with_lib() {
    let diagnostics = check_with_lib("const obj: Object = {};");

    // Object is a built-in type that should be available with lib.d.ts
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "Object should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}
