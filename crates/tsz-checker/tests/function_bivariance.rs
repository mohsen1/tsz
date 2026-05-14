//! Tests for Function Bivariance (Lawyer Layer).
//!
//! These tests verify that methods are bivariant while function properties
//! are contravariant, per TypeScript's function variance rules.

use tsz_checker::test_utils::check_source_code_messages;

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

    let diagnostics = check_source_code_messages(&source);
    let error_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == expected_error_code)
        .count();

    assert!(
        error_count >= 1,
        "Expected at least 1 TS{expected_error_code} error, got {error_count}: {diagnostics:?}"
    );
}

fn test_no_errors(source: &str) {
    // Prepend @strictFunctionTypes comment BEFORE GLOBAL_TYPE_MOCKS
    // because the parser stops at the first non-comment line
    // Remove any existing @strictFunctionTypes from source to avoid duplication
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!("// @strictFunctionTypes: true\n{GLOBAL_TYPE_MOCKS}\n{source_clean}");

    let errors: Vec<_> = check_source_code_messages(&source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
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

    let mut codes: Vec<u32> = check_source_code_messages(&source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // ignore "Cannot find global type"
        .map(|(code, _)| code)
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

/// When a callback parameter type is itself a function type and its parameter
/// type is incompatible, tsc keeps the outer TS2322 wrapper and does not emit
/// TS2328 as a separate top-level diagnostic.
#[test]
fn test_ts2328_not_emitted_for_callback_parameter_mismatch() {
    // fc1 has parameter f: (x: Animal) => Animal
    // fc2 has parameter f: (x: Dog) => Dog
    // Assigning fc1 to fc2 should emit TS2322 only because the nested
    // contravariant check fails on the callback's parameter type.
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
    assert!(
        !codes.contains(&2328),
        "TS2328 should not appear for inner parameter mismatch, got {codes:?}"
    );
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

/// Passing a callback with a NARROWER parameter type to a method call must be
/// rejected under --strictFunctionTypes.
///
/// Structural rule: `(dog: Dog) => void` is not assignable to
/// `(animal: Animal) => void` because the contravariant check
/// `Animal <: Dog` fails (Animal is missing `bark`).
#[test]
fn test_method_call_callback_contravariance_narrower_param_errors() {
    test_function_variance(
        r#"
        interface Animal { name: string }
        interface Dog extends Animal { bark(): void }

        interface Handler {
            handle(callback: (animal: Animal) => void): void;
        }

        declare const handler: Handler;
        handler.handle((dog: Dog) => { dog.bark(); });
        "#,
        2345,
    );
}

/// Same rule with different type parameter names to prove it is not hardcoded.
#[test]
fn test_method_call_callback_contravariance_different_names() {
    test_function_variance(
        r#"
        interface Base { x: number }
        interface Derived extends Base { y: number }

        interface Processor {
            process(fn: (input: Base) => void): void;
        }

        declare const p: Processor;
        p.process((d: Derived) => { d.y; });
        "#,
        2345,
    );
}

/// Passing a callback with a WIDER parameter type must succeed (covariant arg is ok).
#[test]
fn test_method_call_callback_wider_param_ok() {
    test_no_errors(
        r#"
        interface Animal { name: string }
        interface Dog extends Animal { bark(): void }

        interface Handler {
            handle(callback: (dog: Dog) => void): void;
        }

        declare const handler: Handler;
        handler.handle((animal: Animal) => {});
        "#,
    );
}

/// Passing an exactly matching callback type must succeed.
#[test]
fn test_method_call_callback_same_param_ok() {
    test_no_errors(
        r#"
        interface Animal { name: string }

        interface Handler {
            handle(callback: (animal: Animal) => void): void;
        }

        declare const handler: Handler;
        handler.handle((a: Animal) => {});
        "#,
    );
}

/// Callback contravariance also applies when calling through element access.
#[test]
fn test_element_access_call_callback_contravariance_errors() {
    test_function_variance(
        r#"
        interface Animal { name: string }
        interface Dog extends Animal { bark(): void }

        interface Handler {
            handle(callback: (animal: Animal) => void): void;
        }

        declare const handler: Handler;
        handler["handle"]((dog: Dog) => { dog.bark(); });
        "#,
        2345,
    );
}

/// A plain function (not called through property access) must also enforce
/// callback contravariance — existing behaviour, not a regression.
#[test]
fn test_plain_function_call_callback_contravariance_errors() {
    test_function_variance(
        r#"
        interface Animal { name: string }
        interface Dog extends Animal { bark(): void }

        declare function handle(callback: (animal: Animal) => void): void;
        handle((dog: Dog) => { dog.bark(); });
        "#,
        2345,
    );
}
