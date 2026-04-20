// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_ts2339_intersection_property_access() {
    use crate::parser::ParserState;

    // Test property access on intersection types
    let source = r#"
type A = { a: string };
type B = { b: number };
type AB = A & B;

function test(obj: AB) {
    // These should NOT produce TS2339 - intersection has both properties
    const x = obj.a;
    const y = obj.b;
}

function test2(obj: A & { c: boolean }) {
    // These should NOT produce TS2339
    const x = obj.a;
    const y = obj.c;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for intersection property access, got {}: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_overload_arg_count_exceeds_all_only_ts2554_not_ts2769() {
    use crate::parser::ParserState;

    // Regression test for overload calls where argument count exceeds ALL signatures
    // When all overloads fail due to argument count mismatch, should emit TS2554 only, not TS2769
    let code = r#"
declare function mixed(x: string): void;
declare function mixed(x: number, y: number): void;

// This call has 3 arguments, which exceeds both overloads (1 param and 2 params)
// Should emit TS2554 (argument count mismatch) only, not TS2769
mixed(42, 99, 100);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();
    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();

    // Should have TS2554 (argument count mismatch)
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 for argument count mismatch when all overloads fail due to arg count"
    );

    // Should NOT have TS2769 (No overload matches)
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 when all overloads fail due to argument count mismatch, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // Verify TS2554 message
    let first_error_msg = &ts2554_errors[0].message_text;
    assert!(
        first_error_msg.contains("Expected") && first_error_msg.contains("arguments"),
        "TS2554 message should mention expected arguments, got: {first_error_msg}"
    );
}

#[test]
fn test_ts2555_expected_at_least_arguments() {
    use crate::parser::ParserState;

    // Test TS2554 vs TS2555: tsc uses TS2554 ("Expected N-M arguments") for
    // functions with optional params, and TS2555 ("Expected at least N") only
    // for rest params. This test verifies that behavior.

    // Case 1: Optional params → TS2554
    let code = r#"
function foo(a: number, b: string, c?: boolean): void {}

// Too few arguments - should emit TS2554 (not TS2555) because tsc uses
// TS2554 with range format for optional params
foo(1);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();

    // Should have TS2554 (not TS2555) for optional params
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 for too few args with optional params, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Should NOT have TS2555
    let ts2555_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2555)
        .collect();
    assert!(
        ts2555_errors.is_empty(),
        "Should NOT emit TS2555 for optional params (only for rest params), got: {:?}",
        ts2555_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2554_expected_exact_arguments() {
    use crate::parser::ParserState;

    // Test TS2554: Expected N arguments, but got M.
    // This error should be emitted when a function has no optional parameters
    // and the wrong number of arguments are provided.
    let code = r#"
function bar(a: number, b: string): void {}

// Wrong number of arguments - should emit TS2554 (not TS2555)
bar(1);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();

    // Should have TS2554 (exact count expected)
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 when wrong number of arguments for function without optional params, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Verify TS2554 message format (should NOT say "at least")
    let first_error_msg = &ts2554_errors[0].message_text;
    assert!(
        first_error_msg.contains("Expected") && first_error_msg.contains("arguments"),
        "TS2554 message should mention expected arguments, got: {first_error_msg}"
    );
    assert!(
        !first_error_msg.contains("at least"),
        "TS2554 message should NOT say 'at least', got: {first_error_msg}"
    );
}

#[test]
fn test_ts2345_argument_type_mismatch() {
    use crate::parser::ParserState;

    // Test TS2345: Argument of type 'X' is not assignable to parameter of type 'Y'.
    let code = r#"
function baz(a: number): void {}

// Type mismatch - should emit TS2345
baz("hello");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2345_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .collect();

    // Should have TS2345 (argument type mismatch)
    assert!(
        !ts2345_errors.is_empty(),
        "Should emit TS2345 when argument type doesn't match parameter type, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Verify TS2345 message format
    let first_error_msg = &ts2345_errors[0].message_text;
    assert!(
        first_error_msg.contains("not assignable") || first_error_msg.contains("Argument"),
        "TS2345 message should mention 'not assignable' or 'Argument', got: {first_error_msg}"
    );
}

#[test]
fn test_ts2366_arrow_function_missing_return() {
    use crate::parser::ParserState;

    // Test error 2366 for arrow functions with explicit return type
    let source = r#"
// Arrow function with number return type that can fall through
const missingReturn = (): number => {
    if (Math.random() > 0.5) {
        return 1;
    }
};

// Arrow function that returns on all paths - no error
const allPathsReturn = (flag: boolean): number => {
    if (flag) {
        return 1;
    }
    return 2;
};

// Arrow function with void return - no error
const voidReturn = (): void => {
    console.log("ok");
};

// Arrow function without return type annotation - no error
const noAnnotation = () => {
    return 1;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for missingReturn
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for arrow function missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_function_expression_missing_return() {
    use crate::parser::ParserState;

    // Test error 2366 for function expressions with explicit return type
    let source = r#"
// Function expression with string return type that can fall through
const missingReturn = function(): string {
    if (Math.random() > 0.5) {
        return "yes";
    }
};

// Function expression that returns on all paths - no error
const allPathsReturn = function(flag: boolean): string {
    if (flag) {
        return "yes";
    }
    return "no";
};

// Function expression without return type annotation - no error
const noAnnotation = function() {
    return 1;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for missingReturn
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for function expression missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_nested_arrow_functions() {
    use crate::parser::ParserState;

    // Test error 2366 for nested arrow functions
    let source = r#"
function outer(): (x: number) => string {
    // Inner arrow function with return type that can fall through
    return (x: number): string => {
        if (x > 0) {
            return "positive";
        }
    };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for inner arrow function
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for nested arrow function missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_arrow_function_switch_statement() {
    use crate::parser::ParserState;

    // Test error 2366 for arrow functions with switch statements
    let source = r#"
// Arrow function with switch missing default case
const switchNoDefault = (value: number): string => {
    switch (value) {
        case 1:
            return "one";
        case 2:
            return "two";
    }
};

// Arrow function with switch and default - no error
const switchWithDefault = (value: number): string => {
    switch (value) {
        case 1:
            return "one";
        default:
            return "other";
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for switchNoDefault
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for arrow function with switch missing default, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_arrow_function_switch_grouped_cases() {
    use crate::parser::ParserState;

    // Regression: grouped switch cases should not trigger TS2366 when all paths return.
    let source = r#"
const groupedSwitchReturns = (value: string | number): number => {
    switch (typeof value) {
        case "string":
        case "number":
            return 1;
        default:
            return 2;
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors for grouped switch cases with full returns, got: {codes:?}"
    );
}
