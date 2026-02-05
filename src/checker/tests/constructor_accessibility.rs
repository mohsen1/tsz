//! Tests for constructor accessibility (Lawyer Layer).
//!
//! These tests verify that classes with private/protected constructors
//! cannot be instantiated from invalid scopes.

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

fn test_constructor_accessibility(source: &str, expected_error_code: u32) {
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
        crate::checker::context::CheckerOptions::default(),
    );

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
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.category == crate::checker::types::DiagnosticCategory::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors, got {}: {:?}",
        errors.len(),
        errors
    );
}

/// Test that private constructors cannot be accessed outside the class.
#[test]
fn test_private_constructor_instantiation() {
    // TS2673: Constructor of class 'A' is private and only accessible within the class declaration.
    test_constructor_accessibility(
        r#"
        class A { private constructor() {} }
        let a = new A();
        "#,
        2673,
    );
}

/// Test that protected constructors cannot be accessed outside the class hierarchy.
#[test]
fn test_protected_constructor_instantiation() {
    // TS2674: Constructor of class 'A' is protected and only accessible within the class declaration.
    test_constructor_accessibility(
        r#"
        class A { protected constructor() {} }
        let a = new A();
        "#,
        2674,
    );
}

/// Test that private constructors CAN be accessed inside the class (static factory pattern).
#[test]
fn test_private_constructor_inside_class() {
    // Should pass
    test_no_errors(
        r#"
        class A {
            private constructor() {}
            static create() { return new A(); }
        }
        "#,
    );
}

/// Test that protected constructors CAN be accessed in subclasses.
#[test]
fn test_protected_constructor_in_subclass() {
    // Should pass (super call allowed)
    test_no_errors(
        r#"
        class A { protected constructor() {} }
        class B extends A {
            constructor() { super(); }
        }
        "#,
    );
}

/// Test that private constructors fail in subclasses (subclass can't call super).
#[test]
fn test_private_constructor_in_subclass() {
    // TS2673: Cannot extend a class with a private constructor
    test_constructor_accessibility(
        r#"
        class A { private constructor() {} }
        class B extends A {
            constructor() { super(); }
        }
        "#,
        2673,
    );
}

/// Test that protected constructor can't be called from unrelated class.
#[test]
fn test_protected_constructor_cross_class() {
    // TS2674: Constructor is protected
    test_constructor_accessibility(
        r#"
        class A { protected constructor() {} }
        class B {
            foo() { return new A(); }
        }
        "#,
        2674,
    );
}

/// Test that public constructor has no restrictions (baseline).
#[test]
fn test_public_constructor_no_restrictions() {
    // Should pass
    test_no_errors(
        r#"
        class A { public constructor() {} }
        let a = new A();
        "#,
    );
}

/// Test that default constructor (no accessibility modifier) is public.
#[test]
fn test_default_constructor_is_public() {
    // Should pass
    test_no_errors(
        r#"
        class A {}
        let a = new A();
        "#,
    );
}

/// Test that class with private constructor can be used as a type annotation.
/// Type annotation doesn't require instantiation.
#[test]
fn test_private_constructor_type_annotation() {
    // Should pass (using as type, not constructing)
    test_no_errors(
        r#"
        class A { private constructor() {} private x: number = 1; }
        function foo(a: A) {}
        foo(null as any);
        "#,
    );
}

/// Test that abstract classes can't be instantiated directly.
#[test]
fn test_abstract_class_instantiation() {
    // TS2511: Cannot create an instance of an abstract class
    test_constructor_accessibility(
        r#"
        abstract class A {}
        let a = new A();
        "#,
        2511,
    );
}

/// Test that abstract classes can be extended and subclass instantiated.
#[test]
fn test_abstract_class_subclass_instantiation() {
    // Should pass
    test_no_errors(
        r#"
        abstract class A {}
        class B extends A {}
        let b = new B();
        "#,
    );
}
