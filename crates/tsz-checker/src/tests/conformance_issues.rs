//! Unit tests documenting known conformance test failures
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance test investigation (2026-02-08). They serve as:
//! - Documentation of expected vs actual behavior
//! - Easy verification when fixes are implemented
//! - Minimal reproduction cases for debugging
//!
//! See docs/conformance-*.md for full context.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to compile TypeScript and get diagnostics
fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

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
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser didn't consume the invalid token after emitting error
/// Fix: Added next_token() call in state_statements.rs after reserved word error
#[test]
fn test_parser_cascading_error_suppression() {
    let source = r#"
// classes cannot use predefined types as names
class void {}
        "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {:#?}",
        parser_diagnostics
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {}",
        ts1005_count
    );
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {:#?}",
        parser_diagnostics
    );
}

/// Issue: Interface with reserved word name
///
/// Expected: TS1005 only (no cascading errors)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Similar to class declarations, interfaces need to reject reserved words
/// Fix: Added reserved word check in state_declarations.rs parse_interface_declaration
#[test]
fn test_interface_reserved_word_error_suppression() {
    let source = r#"
interface void {}
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    // Should only emit TS1005 '{' expected
    let ts1005_count = parser_diagnostics
        .iter()
        .filter(|(c, _)| *c == 1005)
        .count();

    assert!(
        has_error(&parser_diagnostics, 1005),
        "Should emit TS1005 for syntax error.\nActual errors: {:#?}",
        parser_diagnostics
    );
    assert_eq!(
        ts1005_count, 1,
        "Should only emit one TS1005, got {}",
        ts1005_count
    );
    // Check for common cascading errors
    assert!(
        !has_error(&parser_diagnostics, 1068),
        "Should NOT emit cascading TS1068 error.\nActual errors: {:#?}",
        parser_diagnostics
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

/// Issue: Private identifiers as parameters
///
/// Expected: TS18009 (private identifiers cannot be used as parameters)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage as parameters
/// Fix: Added validation in state_statements.rs parse_parameter
#[test]
fn test_private_identifier_as_parameter() {
    // TS18009 is a PARSER error
    let source = r#"
class Foo {
    method(#param: any) {}
}
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18009),
        "Should emit TS18009 for private identifier as parameter.\nActual errors: {:#?}",
        parser_diagnostics
    );
}

/// Issue: Private identifiers in variable declarations
///
/// Expected: TS18029 (private identifiers not allowed in variable declarations)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in variable declarations
/// Fix: Added validation in state_statements.rs parse_variable_declaration_with_flags
#[test]
fn test_private_identifier_in_variable_declaration() {
    // TS18029 is a PARSER error
    let source = r#"
const #x = 1;
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18029),
        "Should emit TS18029 for private identifier in variable declaration.\nActual errors: {:#?}",
        parser_diagnostics
    );
}

/// Issue: Optional chain with private identifiers
///
/// Expected: TS18030 (optional chain cannot contain private identifiers)
/// Status: FIXED (2026-02-09)
///
/// Root cause: Parser wasn't validating private identifier usage in optional chains
/// Fix: Added validation in state_expressions.rs when handling QuestionDotToken
#[test]
fn test_private_identifier_in_optional_chain() {
    // TS18030 is a PARSER error
    let source = r#"
class Bar {
    #prop = 42;
    test() {
        return this?.#prop;
    }
}
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let parser_diagnostics: Vec<(u32, String)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect();

    assert!(
        parser_diagnostics.iter().any(|(c, _)| *c == 18030),
        "Should emit TS18030 for private identifier in optional chain.\nActual errors: {:#?}",
        parser_diagnostics
    );
}

/// Issue: TS18016 checker validation - private identifier outside class
///
/// For property access expressions (`obj.#bar`), TSC only emits TS18013 (semantic:
/// can't access private member) — NOT TS18016 (grammar: private identifier outside class).
/// TS18016 is only emitted for truly invalid syntax positions (object literals, etc.)
/// because `obj.#bar` is valid syntax even outside a class body.
///
/// Status: FIXED (2026-02-10) - corrected to match TSC behavior
#[test]
fn test_ts18016_private_identifier_outside_class() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Foo {
    #bar: number;
}

let f: Foo;
let x = f.#bar;  // Outside class - should error TS18013 only (not TS18016)
        "#,
    );

    // Filter out TS2318 (missing global types) which are noise for this test
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS18016 for property access — TSC doesn't emit it here.
    // TS18016 is only for truly invalid positions (object literals, standalone expressions).
    assert!(
        !has_error(&relevant_diagnostics, 18016),
        "Should NOT emit TS18016 for property access outside class (TSC doesn't).\nActual errors: {:#?}",
        relevant_diagnostics
    );

    // Should emit TS18013 (semantic error - property not accessible)
    assert!(
        has_error(&relevant_diagnostics, 18013),
        "Should emit TS18013 for private identifier access outside class.\nActual errors: {:#?}",
        relevant_diagnostics
    );
}

/// Issue: TS2416 false positive for private field "overrides"
///
/// Expected: Private fields with same name in child class should NOT emit TS2416
/// Status: FIXED (2026-02-09)
///
/// Root cause: Override checking didn't skip private identifiers
/// Fix: Added check in class_checker.rs to skip override validation for names starting with '#'
#[test]
fn test_private_field_no_override_error() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Parent {
    #foo: number;
}

class Child extends Parent {
    #foo: string;  // Should NOT emit TS2416 - private fields don't participate in inheritance
}
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    // Should NOT emit TS2416 (incompatible override) for private fields
    assert!(
        !has_error(&relevant_diagnostics, 2416),
        "Should NOT emit TS2416 for private field with same name in child class.\nActual errors: {:#?}",
        relevant_diagnostics
    );
}

/// Issue: Computed property destructuring produces false TS2349
///
/// From: computed-property-destructuring.md
/// Expected: No TS2349 errors
/// Actual: TS2349 "This expression is not callable" errors
///
/// Root cause: Computed property name expression in destructuring binding
/// may be incorrectly treated or the type resolution fails.
#[test]
fn test_computed_property_destructuring_no_false_ts2349() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
let foo = "bar";
let {[foo]: bar} = {bar: "baz"};
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2349),
        "Should NOT emit TS2349 for computed property destructuring.\nActual errors: {:#?}",
        relevant
    );
}

/// Issue: Contextual typing for generic function parameters
///
/// From: contextual-typing-generics.md
/// Expected: No TS7006 errors (parameter gets contextual type from generic function type)
/// Actual: TS7006 "Parameter implicitly has 'any' type"
///
/// Root cause: When a function expression/arrow is assigned to a generic function type
/// like `<T>(x: T) => void`, the parameter should get its type from contextual typing.
/// Currently, the parameter type is not inferred from the contextual type.
#[test]
fn test_contextual_typing_generic_function_param() {
    // Enable noImplicitAny to trigger TS7006
    let source = r#"
// @noImplicitAny: true
const fn2: <T>(x: T) => void = function test(t) { };
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_implicit_any = true;
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed as T.\nActual errors: {:#?}",
        relevant
    );
}

/// Issue: Contextual typing for arrow function assigned to generic type
#[test]
fn test_contextual_typing_generic_arrow_param() {
    let source = r#"
// @noImplicitAny: true
declare function f(fun: <T>(t: T) => void): void;
f(t => { });
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_implicit_any = true;
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 't' should be contextually typed from generic.\nActual errors: {:#?}",
        relevant
    );
}

/// Regression test: TS7006 SHOULD still fire for closures without any contextual type
#[test]
fn test_ts7006_still_fires_without_contextual_type() {
    let source = r#"
// @noImplicitAny: true
var f = function(x) { };
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_implicit_any = true;
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        has_error(&relevant, 7006),
        "SHOULD emit TS7006 - parameter 'x' has no contextual type.\nActual errors: {:#?}",
        relevant
    );
}
