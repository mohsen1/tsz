//! Tests for Function Bivariance (Lawyer Layer).
//!
//! These tests verify that methods are bivariant while function properties
//! are contravariant, per TypeScript's function variance rules.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::test_fixtures::TestContext;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Workaround for TS2318 (Cannot find global type) errors in test infrastructure.
const GLOBAL_TYPE_MOCKS: &str = r#"
interface Array<T> {}
interface String {}
interface Boolean {}
interface Number {}
interface Object {}
interface Function {}
interface RegExp {}
interface IArguments {}
"#;

fn test_function_variance(source: &str, expected_error_code: u32) {
    // Prepend @strictFunctionTypes comment BEFORE GLOBAL_TYPE_MOCKS
    // because the parser stops at the first non-comment line
    // Remove any existing @strictFunctionTypes from source to avoid duplication
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!("// @strictFunctionTypes: true\n{GLOBAL_TYPE_MOCKS}\n{source_clean}");

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

    // Set lib contexts for global symbol resolution
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

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == expected_error_code)
        .count();

    assert!(
        error_count >= 1,
        "Expected at least 1 TS{} error, got {}: {:?}",
        expected_error_code,
        error_count,
        checker.ctx.diagnostics
    );
}

fn test_no_errors(source: &str) {
    // Prepend @strictFunctionTypes comment BEFORE GLOBAL_TYPE_MOCKS
    // because the parser stops at the first non-comment line
    // Remove any existing @strictFunctionTypes from source to avoid duplication
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!("// @strictFunctionTypes: true\n{GLOBAL_TYPE_MOCKS}\n{source_clean}");

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

    // Set lib contexts for global symbol resolution
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

    let errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.category == crate::checker::diagnostics::DiagnosticCategory::Error && d.code != 2318
        })
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors, got {}: {:?}",
        errors.len(),
        errors
    );
}

/// Test that methods are bivariant (same parameter types work both ways).
#[test]
fn test_method_bivariance_same_params() {
    // Should pass - methods are bivariant
    test_no_errors(
        r#"
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number): void;
        }
        let a: A = { method: (x: number) => {} };
        let b: B = a;
        "#,
    );
}

/// Test that method parameters accept wider types (bivariance).
#[test]
fn test_method_bivariance_wider_param() {
    // Should pass - methods are bivariant (accept wider in either direction)
    test_no_errors(
        r#"
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let a: A = { method: (x: number | string) => {} };
        let b: B = a;
        "#,
    );
}

/// Test that function properties are contravariant (not bivariant).
#[test]
fn test_function_property_contravariance() {
    // Should fail - function properties are contravariant
    test_function_variance(
        r#"
        // @strictFunctionTypes: true: true
        interface A {
            prop: (x: number | string) => void;
        }
        interface B {
            prop: (x: number) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type 'number' is not assignable to 'number | string'
    );
}

/// Test arrow function properties are contravariant (not bivariant).
#[test]
fn test_arrow_function_property_contravariance() {
    // Should fail - arrow functions are properties, not methods
    test_function_variance(
        r#"
        // @strictFunctionTypes: true: true
        interface A {
            prop: (x: number) => void;
        }
        interface B {
            prop: (x: number | string) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type error
    );
}

/// Test method shorthand syntax is bivariant.
#[test]
fn test_method_shorthand_bivariant() {
    // Should pass - method shorthand is bivariant
    test_no_errors(
        r#"
        // @strictFunctionTypes: true
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let b: B = { method: (x: number | string) => {} };
        let a: A = b;
        "#,
    );
}

/// Test that strictFunctionTypes doesn't affect methods.
#[test]
fn test_method_bivariance_strict_mode() {
    // Should pass - methods are bivariant even in strict mode
    test_no_errors(
        r#"
        // @strictFunctionTypes: true
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let b: B = { method: (x: number | string) => {} };
        let a: A = b;
        "#,
    );
}

/// Test that strictFunctionTypes enforces contravariance for function properties.
#[test]
fn test_function_property_contravariance_strict_mode() {
    // Should fail - function properties are contravariant in strict mode
    test_function_variance(
        r#"
        // @strictFunctionTypes: true
        interface A {
            prop: (x: number | string) => void;
        }
        interface B {
            prop: (x: number) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type error
    );
}

/// Helper: collect all error codes from checking a strict-function-types source.
fn collect_error_codes(source: &str) -> Vec<u32> {
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!("// @strictFunctionTypes: true\n{GLOBAL_TYPE_MOCKS}\n{source_clean}");

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

    let mut codes: Vec<u32> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318) // ignore "Cannot find global type"
        .map(|d| d.code)
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

/// TS2328: When a callback parameter type is itself a function type and is
/// incompatible, tsc emits TS2328 ("Types of parameters 'X' and 'Y' are
/// incompatible") as a separate diagnostic alongside TS2322.
#[test]
fn test_ts2328_emitted_for_callback_parameter_mismatch() {
    // fc1 has parameter f: (x: Animal) => Animal
    // fc2 has parameter f: (x: Dog) => Dog
    // Assigning fc1 to fc2 should emit both TS2322 and TS2328
    // because the parameter types (f) are themselves callable and incompatible.
    let codes = collect_error_codes(
        r#"
        interface Animal { animal: void }
        interface Dog extends Animal { dog: void }

        declare let fc1: (f: (x: Animal) => Animal) => void;
        declare let fc2: (f: (x: Dog) => Dog) => void;
        fc2 = fc1;
        "#,
    );
    assert!(codes.contains(&2322), "Expected TS2322 in {codes:?}");
    assert!(codes.contains(&2328), "Expected TS2328 in {codes:?}");
}

/// TS2328 should NOT be emitted when the outer types are generic type alias
/// applications (like Func<T,U>), even if the underlying parameter types are
/// callable.  tsc reports such failures via type-argument elaboration, not
/// TS2328.
#[test]
fn test_ts2328_not_emitted_for_type_alias_applications() {
    let codes = collect_error_codes(
        r#"
        type Func<T, U> = (x: T) => U;

        declare let h1: Func<Func<Object, void>, Object>;
        declare let h3: Func<Func<string, void>, Object>;
        h3 = h1;
        "#,
    );
    assert!(codes.contains(&2322), "Expected TS2322 in {codes:?}");
    assert!(
        !codes.contains(&2328),
        "TS2328 should NOT appear for type alias applications, got {codes:?}"
    );
}

/// TS2328 should NOT be emitted when callback parameter types contain
/// generic type parameters (tsc skips elaboration for generic signatures).
#[test]
fn test_ts2328_not_emitted_for_generic_callback_params() {
    let codes = collect_error_codes(
        r#"
        function assignmentWithComplexRest2<T extends any[]>() {
            const fn1: (cb: (x: string, ...rest: T) => void) => void = (cb) => {};
            const fn2: (cb: (...args: never) => void) => void = fn1;
        }
        "#,
    );
    assert!(codes.contains(&2322), "Expected TS2322 in {codes:?}");
    assert!(
        !codes.contains(&2328),
        "TS2328 should NOT appear for generic callback params, got {codes:?}"
    );
}
