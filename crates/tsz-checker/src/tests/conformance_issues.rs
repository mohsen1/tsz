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
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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
        options,
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

#[test]
fn test_class_extends_primitive_reports_ts2863() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C extends number {}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2863),
        "Expected TS2863 when class extends primitive type. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_class_implements_primitive_reports_ts2864() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C implements number {}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2864),
        "Expected TS2864 when class implements primitive type. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_indirect_class_cycle_reports_all_ts2506_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C extends E { foo: string; }
class D extends C { bar: string; }
class E extends D { baz: number; }

class C2<T> extends E2<T> { foo: T; }
class D2<T> extends C2<T> { bar: T; }
class E2<T> extends D2<T> { baz: T; }
        "#,
    );

    let ts2506_count = diagnostics.iter().filter(|(code, _)| *code == 2506).count();
    assert_eq!(
        ts2506_count, 6,
        "Expected TS2506 on all six classes in the two cycles. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_interface_extends_primitive_reports_ts2840() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I extends number {}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2840),
        "Expected TS2840 when interface extends primitive type. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_instance_member_initializer_constructor_param_capture_reports_ts2301() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare var console: {
    log(msg?: any): void;
};
var field1: string;

class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2301),
        "Expected TS2301 for constructor parameter capture in instance initializer. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_instance_member_initializer_missing_name_reports_ts2663() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare var console: {
    log(msg?: any): void;
};

export class Test1 {
    constructor(private field1: string) {
    }
    messageHandler = () => {
        console.log(field1);
    };
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2663),
        "Expected TS2663 for missing free name in module instance initializer. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_instance_member_initializer_local_shadow_does_not_report_ts2301() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare var console: {
    log(msg?: any): void;
};

class Test {
    constructor(private field: string) {
    }
    messageHandler = () => {
        var field = this.field;
        console.log(field);
    };
}
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2301),
        "Did not expect TS2301 for locally shadowed identifier in initializer. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_unresolved_import_namespace_access_suppresses_ts2708() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
import { alias } from "foo";
let x = new alias.Class();
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2708),
        "Should not emit cascading TS2708 for unresolved imported namespace access. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_super_call_args_match_instantiated_generic_base_ctor() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    constructor(public value: T) {}
}

class Derived extends Base<number> {
    constructor() {
        super("hi");
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for super argument type mismatch against instantiated base ctor. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_derived_constructor_without_super_reports_ts2377() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {}

class Derived extends Base {
    constructor() {}
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2377),
        "Expected TS2377 for derived constructor missing super() call. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_super_property_before_super_call_reports_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    method() {}
}

class Derived extends Base {
    constructor() {
        super.method();
        super();
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() call. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_super_property_access_reports_ts2340() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    value = 1;
}

class Derived extends Base {
    method() {
        return super.value;
    }
}
        "#,
    );

    assert!(
        has_error(&diagnostics, 2340),
        "Expected TS2340 for super property access to non-method member. Actual diagnostics: {:#?}",
        diagnostics
    );
}

#[test]
fn test_super_in_constructor_parameter_reports_ts2336_and_ts17011() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class B {
    public foo(): number {
        return 0;
    }
}

class C extends B {
    constructor(a = super.foo()) {
    }
}
                "#,
    );

    assert!(
        has_error(&diagnostics, 2336),
        "Expected TS2336 for super in constructor argument context. Actual diagnostics: {:#?}",
        diagnostics
    );
    assert!(
        has_error(&diagnostics, 17011),
        "Expected TS17011 for super property access before super() in constructor context. Actual diagnostics: {:#?}",
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

/// Issue: false-positive assignability errors with contextual generic outer type parameters.
///
/// Mirrors: contextualOuterTypeParameters.ts
/// Expected: no TS2322/TS2345 errors
#[test]
fn test_contextual_outer_type_parameters_no_false_assignability_errors() {
    let source = r#"
declare function f(fun: <T>(t: T) => void): void

f(t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
});

const fn1: <T>(x: T) => void = t => {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};

const fn2: <T>(x: T) => void = function test(t) {
    type isArray = (typeof t)[] extends string[] ? true : false;
    type IsObject = { x: typeof t } extends { x: string } ? true : false;
};
"#;

    let mut options = CheckerOptions::default();
    options.strict = true;
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2322),
        "Should NOT emit TS2322 for contextual generic outer type parameters.\nActual errors: {:#?}",
        relevant
    );
    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual generic outer type parameters.\nActual errors: {:#?}",
        relevant
    );
}

/// Issue: false-positive TS2345 in contextual signature instantiation chain.
///
/// Mirrors: contextualSignatureInstantiation2.ts
/// Expected: no TS2345
#[test]
fn test_contextual_signature_instantiation_chain_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
var dot: <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T) => (_: U) => S;
dot = <T, S>(f: (_: T) => S) => <U>(g: (_: U) => T): (r:U) => S => (x) => f(g(x));
var id: <T>(x:T) => T;
var r23 = dot(id)(id);
        "#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 for contextual signature instantiation chain.\nActual errors: {:#?}",
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

/// Issue: Contextual typing for mapped type generic parameters
///
/// When a generic function has a mapped type parameter like `{ [K in keyof P]: P[K] }`,
/// and P has a constraint (e.g. `P extends Props`), the lambda parameters inside the
/// object literal argument should be contextually typed from the constraint.
///
/// For example:
/// ```typescript
/// interface Props { when: (value: string) => boolean; }
/// function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
/// good2({ when: value => false }); // `value` should be typed as `string`
/// ```
///
/// Root cause was two-fold:
/// 1. During two-pass generic inference, when all args are context-sensitive,
///    type parameters had no candidates. Fixed by using upper bounds (constraints)
///    in `get_current_substitution` instead of UNKNOWN.
/// 2. The instantiated mapped type contained Lazy references that the solver's
///    NoopResolver couldn't resolve. Fixed by evaluating the contextual type
///    with the checker's Judge (which has the full TypeEnvironment resolver)
///    before extracting property types.
#[test]
fn test_contextual_typing_mapped_type_generic_param() {
    let source = r#"
// @noImplicitAny: true
interface Props {
    when: (value: string) => boolean;
}
function good2<P extends Props>(attrs: { [K in keyof P]: P[K] }) { }
good2({ when: value => false });
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
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - parameter 'value' should be contextually typed as string \
         from the mapped type constraint Props.\nActual errors: {:#?}",
        relevant
    );
}

/// Issue: TS2344 reported twice for the same type argument
///
/// When `get_type_from_type_node` re-resolves a type reference (e.g., because
/// `type_parameter_scope` changes between type environment building and statement
/// checking), `validate_type_reference_type_arguments` was called twice for the
/// same node, producing duplicate TS2344 errors.
///
/// Fix: Use `emitted_diagnostics` deduplication in `error_type_constraint_not_satisfied`
/// to prevent emitting the same TS2344 at the same source position twice.
#[test]
fn test_ts2344_no_duplicate_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Box<T extends string> {
    value: T;
}
type BadBox = Box<number>;
type IsString<T extends string> = T extends string ? true : false;
type Test2 = IsString<number>;
type Keys<T extends object> = keyof T;
type Test4 = Keys<string>;
        "#,
    );

    // Count TS2344 errors - each should appear exactly once
    let ts2344_count = diagnostics.iter().filter(|(code, _)| *code == 2344).count();
    assert_eq!(
        ts2344_count, 3,
        "Should emit exactly 3 TS2344 errors (one per bad type arg), not duplicates.\nActual errors: {:#?}",
        diagnostics
    );
}

/// TS2339: Property access on `this` in static methods should use constructor type
///
/// In static methods, `this` refers to `typeof C` (the constructor type), not an
/// instance of C. Accessing instance properties on `this` in a static method should
/// emit TS2339 because instance properties don't exist on the constructor type.
#[test]
fn test_ts2339_this_in_static_method() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C {
    public p = 0;
    static s = 0;
    static b() {
        this.p = 1; // TS2339 - 'p' is instance, doesn't exist on typeof C
        this.s = 2; // OK - 's' is static
    }
}
        "#,
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        1,
        "Should emit exactly 1 TS2339 for 'this.p' in static method.\nActual errors: {:#?}",
        diagnostics
    );
    assert!(
        ts2339_errors[0].1.contains("'p'") || ts2339_errors[0].1.contains("\"p\""),
        "TS2339 should mention property 'p'. Got: {}",
        ts2339_errors[0].1
    );
}

#[test]
fn test_interface_accessor_declarations() {
    // Interface accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Test {
    get foo(): string;
    set foo(s: string | number);
}
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        "#,
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Interface accessors should be recognized as properties. Got TS2339 errors: {:#?}",
        ts2339_errors
    );
}

#[test]
fn test_type_literal_accessor_declarations() {
    // Type literal accessor declarations (get/set) should be recognized as properties
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Test = {
    get foo(): string;
    set foo(s: number);
};
const t = {} as Test;
let m: string = t.foo;   // OK - getter returns string
        "#,
    );

    let ts2339_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Type literal accessors should be recognized as properties. Got TS2339 errors: {:#?}",
        ts2339_errors
    );
}

/// Issue: False-positive TS2345 when interface extends another and adds call signatures
///
/// From: addMoreCallSignaturesToBaseSignature2.ts
/// Expected: No errors - `a(1)` should match inherited `(bar: number): string` signature
/// Actual: TS2345 (falsely claims argument type mismatch)
///
/// When interface Bar extends Foo (which has `(bar: number): string`),
/// and Bar adds `(key: string): string`, calling `a(1)` with a numeric
/// argument should match the inherited signature without error.
#[test]
fn test_interface_inherited_call_signature_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Foo {
    (bar:number): string;
}

interface Bar extends Foo {
    (key: string): string;
}

var a: Bar;
var kitty = a(1);
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - a(1) should match inherited (bar: number) => string.\nActual errors: {:#?}",
        relevant
    );
}

/// Issue: False-positive TS2345 with mixin pattern (class extends function return)
///
/// From: anonClassDeclarationEmitIsAnon.ts
/// Expected: No errors - `Timestamped(User)` should work as a valid base class
/// Actual: TS2345 (falsely claims User is not assignable to Constructor parameter)
///
/// The mixin pattern `function Timestamped<TBase extends Constructor>(Base: TBase)`
/// with `Constructor<T = {}> = new (...args: any[]) => T` should accept any class.
#[test]
fn test_mixin_pattern_no_false_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Constructor<T = {}> = new (...args: any[]) => T;

function Timestamped<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
        timestamp = 0;
    };
}

class User {
    name = '';
}

class TimestampedUser extends Timestamped(User) {
    constructor() {
        super();
    }
}
        "#,
    );

    // Filter out TS2318 (missing global types)
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    assert!(
        !has_error(&relevant, 2345),
        "Should NOT emit TS2345 - User should be assignable to Constructor<{{}}>.\nActual errors: {:#?}",
        relevant
    );
}
