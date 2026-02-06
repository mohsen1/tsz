//! Tests for TS2300 emission ("Duplicate identifier")
//!
//! These tests verify that duplicate identifier errors are correctly emitted
//! for class members based on declaration order, matching tsc behavior.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper function to check source and return diagnostics.
fn check(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let lib_files = crate::test_fixtures::load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Test that method followed by property emits TS2300 only on the property.
/// Regression test for fix where both were being reported.
#[test]
fn test_duplicate_method_then_property() {
    let source = r#"
class C {
    a(): number { return 0; }
    a: number;
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        ts2300_errors.len(),
        1,
        "Should have exactly 1 TS2300 error (on property, not method), got: {:?}",
        ts2300_errors
    );
}

/// Test that property followed by method emits TS2300 on BOTH declarations.
/// This is the special case where tsc reports both as duplicates.
#[test]
fn test_duplicate_property_then_method() {
    let source = r#"
class K {
    b: number;
    b(): number { return 0; }
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        ts2300_errors.len(),
        2,
        "Should have 2 TS2300 errors (both property and method), got: {:?}",
        ts2300_errors
    );
}

/// Test that property followed by property emits TS2300 only on the second property.
#[test]
fn test_duplicate_property_then_property() {
    let source = r#"
class D {
    c: number;
    c: string;
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        ts2300_errors.len(),
        1,
        "Should have exactly 1 TS2300 error (on second property only), got: {:?}",
        ts2300_errors
    );
}

/// Test that method overloads are allowed (no TS2300).
#[test]
fn test_method_overloads_allowed() {
    let source = r#"
class C {
    foo(x: number): void;
    foo(x: string): void;
    foo(x: any) { }
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert!(
        ts2300_errors.is_empty(),
        "Should NOT have TS2300 for method overloads, got: {:?}",
        ts2300_errors
    );
}

/// Test that duplicate properties in interfaces emit TS2300 only on subsequent declarations.
#[test]
fn test_duplicate_interface_properties() {
    let source = r#"
interface Foo {
    x: number;
    x: string;
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert_eq!(
        ts2300_errors.len(),
        1,
        "Should have exactly 1 TS2300 error (on second property), got: {:?}",
        ts2300_errors
    );
}

/// Test that interface merging is allowed (no TS2300).
#[test]
fn test_interface_merging_allowed() {
    let source = r#"
interface Foo {
    x: number;
}
interface Foo {
    y: string;
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    assert!(
        ts2300_errors.is_empty(),
        "Should NOT have TS2300 for interface merging, got: {:?}",
        ts2300_errors
    );
}

/// Test that duplicate function implementations emit TS2393, not TS2300.
#[test]
fn test_duplicate_function_implementations() {
    let source = r#"
class C {
    foo(x: number) { }
    foo(x: string) { }
}
"#;
    let diagnostics = check(source);

    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();
    let ts2393_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2393).collect();

    assert!(
        ts2300_errors.is_empty(),
        "Should NOT have TS2300 for duplicate implementations, got: {:?}",
        ts2300_errors
    );
    assert_eq!(
        ts2393_errors.len(),
        2,
        "Should have 2 TS2393 errors for both implementations, got: {:?}",
        ts2393_errors
    );
}
