//! Tests for global type error detection (TS2318, TS2583)
//!
//! These tests verify that missing global types emit appropriate errors:
//! - TS2318: Cannot find global type (for @noLib or pre-ES2015 types)
//! - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)

use crate::checker::state::CheckerState;
use crate::interner::Atom;
use crate::interner::Interner;
use crate::parser::ParserState;

#[test]
fn test_missing_promise_emits_ts2583() {
    let source = r#"
const p = new Promise<void>();
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583: Cannot find name 'Promise'. Do you need to change your target library?
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    // At least one TS2583 for Promise
    assert!(
        !ts2583_errors.is_empty(),
        "Expected at least one TS2583 error for Promise, got: {:?}",
        diagnostics
    );

    // Verify error message mentions changing target library
    let promise_error = ts2583_errors
        .iter()
        .find(|d| d.message_text.contains("Promise"));
    assert!(
        promise_error.is_some(),
        "Expected TS2583 error to mention Promise"
    );

    let error = promise_error.unwrap();
    assert!(
        error.message_text.contains("target library") || error.message_text.contains("lib"),
        "Expected error message to suggest changing target library, got: {}",
        error.message_text
    );
}

#[test]
fn test_missing_map_emits_ts2583() {
    let source = r#"
const m = new Map<string, number>();
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583 for Map
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(!ts2583_errors.is_empty(), "Expected TS2583 error for Map");
}

#[test]
fn test_missing_set_emits_ts2583() {
    let source = r#"
const s = new Set<number>();
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583 for Set
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(!ts2583_errors.is_empty(), "Expected TS2583 error for Set");
}

#[test]
fn test_missing_symbol_emits_ts2583() {
    let source = r#"
const s = Symbol("foo");
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583 for Symbol
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 error for Symbol"
    );
}

#[test]
fn test_missing_date_emits_ts2318() {
    let source = r#"
const d = new Date();
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2318 for Date (pre-ES2015 global type)
    let diagnostics = checker.get_diagnostics();
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();

    assert!(
        !ts2318_errors.is_empty(),
        "Expected TS2318 error for Date, got: {:?}",
        diagnostics
    );

    // Verify error message
    let date_error = ts2318_errors
        .iter()
        .find(|d| d.message_text.contains("Date"));
    assert!(
        date_error.is_some(),
        "Expected TS2318 error to mention Date"
    );
}

#[test]
fn test_missing_regexp_emits_ts2318() {
    let source = r#"
const r = new RegExp("foo");
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2318 for RegExp
    let diagnostics = checker.get_diagnostics();
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();

    assert!(
        !ts2318_errors.is_empty(),
        "Expected TS2318 error for RegExp"
    );
}

#[test]
fn test_promise_type_reference_emits_ts2583() {
    let source = r#"
function foo(): Promise<void> {
    return Promise.resolve();
}
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583 for Promise in type position
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    // Should have at least 2 errors (one in return type, one in body)
    assert!(
        ts2583_errors.len() >= 2,
        "Expected at least 2 TS2583 errors for Promise, got {}",
        ts2583_errors.len()
    );
}

#[test]
fn test_map_type_reference_emits_ts2583() {
    let source = r#"
function foo(): Map<string, number> {
    return new Map();
}
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Should emit TS2583 for Map in type position
    let diagnostics = checker.get_diagnostics();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 error for Map type reference"
    );
}

#[test]
fn test_array_should_not_emit_error() {
    let source = r#"
const arr: Array<number> = [1, 2, 3];
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Array is a built-in type that should be available
    let diagnostics = checker.get_diagnostics();
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        ts2318_errors.is_empty() && ts2583_errors.is_empty(),
        "Array should not emit TS2318 or TS2583 errors, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_object_should_not_emit_error() {
    let source = r#"
const obj: Object = {};
"#
    .to_string();

    let mut checker = CheckerState::new_for_test(source);
    checker.check();

    // Object is a built-in type that should be available
    let diagnostics = checker.get_diagnostics();
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        ts2318_errors.is_empty() && ts2583_errors.is_empty(),
        "Object should not emit TS2318 or TS2583 errors, got: {:?}",
        diagnostics
    );
}
