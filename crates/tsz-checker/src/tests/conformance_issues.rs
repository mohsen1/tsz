//! Unit tests documenting known conformance test failures
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance test investigation (2026-02-08). They serve as:
//! - Documentation of expected vs actual behavior
//! - Easy verification when fixes are implemented
//! - Minimal reproduction cases for debugging
//!
//! See docs/conformance-*.md for full context.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::test_fixtures::TestContext;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Global type mocks to avoid TS2318 errors
const GLOBAL_TYPE_MOCKS: &str = r#"
interface Array<T> {}
interface String {}
interface Boolean {}
interface Number {}
interface Object {}
interface Function {}
interface Promise<T> {}
interface Error { message?: string; }
declare var console: { log: any };
"#;

/// Helper to compile TypeScript and get diagnostics
fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let source = format!("{}\n{}", GLOBAL_TYPE_MOCKS, source);

    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Set lib contexts
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::checker::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper to check if specific error codes are present
fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

/// Issue: Flow analysis applies narrowing from invalid assignments
///
/// From: derivedClassTransitivity3.ts
/// Expected: TS2322 only (assignment incompatibility)
/// Actual: TS2322 + TS2345 (also reports wrong parameter type on subsequent call)
///
/// Root cause: Flow analyzer treats invalid assignment as if it succeeded,
/// narrowing the variable type to the assigned type.
///
/// Complexity: HIGH - requires binder/checker coordination
/// See: docs/conformance-work-session-summary.md
#[test]
#[ignore = "Flow analysis from invalid assignment - HIGH complexity"]
fn test_flow_narrowing_from_invalid_assignment() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C<T> {
    foo(x: T, y: T) { }
}

class D<T> extends C<T> {
    foo(x: T) { } // ok to drop parameters
}

class E<T> extends D<T> {
    foo(x: T, y?: number) { } // ok to add optional parameters
}

declare var c: C<string>;
declare var e: E<string>;
c = e;                      // Should error: TS2322
var r = c.foo('', '');      // Should NOT error (c is still C<string>)
        "#,
    );

    // Should only have TS2322 on the assignment
    assert!(
        has_error(&diagnostics, 2322),
        "Should emit TS2322 for assignment incompatibility"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit TS2345 - c.foo should use C's signature, not E's.\nActual errors: {:#?}",
        diagnostics
    );
}

/// Issue: Parser emitting cascading error after syntax error
///
/// From: classWithPredefinedTypesAsNames2.ts
/// Expected: TS1005 only
/// Actual: TS1005 + TS1068 (cascading "unexpected token" error)
///
/// Root cause: Parser recovery emitting secondary errors
///
/// Complexity: MEDIUM - requires parser recovery improvements
/// See: docs/conformance-reality-check.md
#[test]
#[ignore = "Parser cascading errors - MEDIUM complexity"]
fn test_parser_cascading_error_suppression() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// classes cannot use predefined types as names
class void {}
        "#,
    );

    // Should only emit TS1005 '{' expected
    let ts1005_count = diagnostics.iter().filter(|(c, _)| *c == 1005).count();

    assert!(
        has_error(&diagnostics, 1005),
        "Should emit TS1005 for syntax error"
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {}",
        ts1005_count
    );
    assert!(
        !has_error(&diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {:#?}",
        diagnostics
    );
}

/// Issue: Overly aggressive strict null checking
///
/// From: neverReturningFunctions1.ts
/// Expected: No errors (control flow eliminates null/undefined)
/// Actual: TS18048 (possibly undefined)
///
/// Root cause: Control flow analysis not recognizing never-returning patterns
///
/// Complexity: HIGH - requires improving control flow analysis
/// See: docs/conformance-analysis-slice3.md
#[test]
#[ignore = "Strict null checking with never-returning functions - HIGH complexity"]
fn test_narrowing_after_never_returning_function() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @strict: true
function fail(message?: string): never {
    throw new Error(message);
}

function f01(x: string | undefined) {
    if (x === undefined) fail("undefined argument");
    x.length;  // Should NOT error - x is string after never-returning call
}
        "#,
    );

    // Should emit no errors
    assert!(
        diagnostics.is_empty(),
        "Should emit no errors - x is narrowed to string after never-returning call.\nActual errors: {:#?}",
        diagnostics
    );
}

/// Issue: Private identifiers in object literals
///
/// Expected: TS18016 (private identifiers not allowed outside class bodies)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in object literals
/// Fix: Added validation in state_expressions.rs parse_property_assignment
#[test]
fn test_private_identifier_in_object_literal() {
    // TS18016 is a PARSER error, so we need to check parser diagnostics
    let source = r#"
const obj = {
    #x: 1
};
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18016),
        "Should emit TS18016 for private identifier in object literal.\nActual errors: {:#?}",
        parser_diagnostics
    );
}

/// Issue: Private identifier access outside class
///
/// Expected: TS18013 (property not accessible outside class)
/// Status: FIXED (2026-02-09)
///
/// Root cause: get_type_of_private_property_access didn't check class scope
/// Fix: Added check in state_type_analysis.rs to emit TS18013 when !saw_class_scope
#[test]
fn test_private_identifier_access_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Foo {
    #bar = 42;
}
const f = new Foo();
const x = f.#bar;  // Should error TS18013
        "#,
    );

    assert!(
        has_error(&diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {:#?}",
        diagnostics
    );
}

/// Issue: Private identifier access from within class should work
///
/// Expected: No errors
/// Status: VERIFIED (2026-02-09)
#[test]
fn test_private_identifier_access_inside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Foo {
    #bar = 42;
    getBar() {
        return this.#bar;  // Should NOT error
    }
}
        "#,
    );

    assert!(
        !has_error(&diagnostics, 18013),
        "Should NOT emit TS18013 when accessing private identifier inside class.\nActual errors: {:#?}",
        diagnostics
    );
}
