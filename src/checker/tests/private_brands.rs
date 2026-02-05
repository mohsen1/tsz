//! Tests for private/protected member nominal typing (Lawyer Layer).
//!
//! These tests verify that classes with private/protected members behave nominally,
//! not structurally. This implements TypeScript's "brand checking" where private
//! members create a nominal identity that overrides structural compatibility.

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

fn test_private_brands(source: &str, expected_errors: usize) {
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
        .filter(|d| d.code == 2322)
        .count();

    assert_eq!(
        error_count, expected_errors,
        "Expected {} TS2322 errors, got {}: {:?}",
        expected_errors, error_count, checker.ctx.diagnostics
    );
}

/// Test that private members are nominal - different classes with same private member shape
/// are NOT compatible, even though structurally they match.
#[test]
fn test_private_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a private property 'x'.
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        class B { private x: number = 1; }
        let a: A = new B();
        "#,
        1,
    );
}

/// Test that private members prevent structural assignment to object literals.
/// Even if the object literal has the same shape, the private brand is missing.
#[test]
fn test_private_member_prevents_structural_assignment() {
    // TS2322: Property 'x' is private in type 'A' but not in type '{ x: number; }'.
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        let a: A = { x: 1 };
        "#,
        1,
    );
}

/// Test that protected members are also nominal.
/// Protected members create a brand just like private members.
#[test]
fn test_protected_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a protected property 'x'.
    test_private_brands(
        r#"
        class A { protected x: number = 1; }
        class B { protected x: number = 1; }
        let a: A = new B();
        "#,
        1,
    );
}

/// Test that subclasses ARE compatible with their base classes.
/// The subclass inherits the private brand from the parent.
#[test]
fn test_subclass_compatibility() {
    // Should pass (subclass shares the private brand)
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        class B extends A {}
        let a: A = new B();
        "#,
        0,
    );
}

/// Test that public members are structural (default TypeScript behavior).
/// Public members don't create a nominal brand.
#[test]
fn test_public_members_are_structural() {
    // Should pass (public members are structural)
    test_private_brands(
        r#"
        class A { public x: number = 1; }
        class B { public x: number = 1; }
        let a: A = new B();
        "#,
        0,
    );
}

/// Test that multiple private members create a stronger brand.
/// Classes must match ALL private members to be compatible.
#[test]
fn test_multiple_private_members() {
    // TS2322: Types have separate declarations of private properties 'x' and 'y'.
    test_private_brands(
        r#"
        class A { private x: number = 1; private y: number = 2; }
        class B { private x: number = 1; private y: number = 2; }
        let a: A = new B();
        "#,
        1,
    );
}

/// Test that classes with different private member sets are incompatible.
#[test]
fn test_different_private_members() {
    // TS2322: Types with different private members are incompatible
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        class B { private y: number = 1; }
        let a: A = new B();
        "#,
        1,
    );
}

/// Test that private methods also create nominal brands.
#[test]
fn test_private_methods_create_brands() {
    // TS2322: Types have separate declarations of a private method 'foo'
    test_private_brands(
        r#"
        class A { private foo() {} }
        class B { private foo() {} }
        let a: A = new B();
        "#,
        1,
    );
}

/// Test that assigning object literal with extra property to class with private member.
/// Object literals can't have private members, so this should fail.
#[test]
fn test_object_literal_extra_property_to_class_with_private() {
    // TS2322: Object literal can't match private member
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        let a: A = { x: 1, y: 2 };
        "#,
        1,
    );
}

/// Test that same class is assignable to itself (trivial case).
#[test]
fn test_same_class_compatibility() {
    // Should pass
    test_private_brands(
        r#"
        class A { private x: number = 1; }
        let a1: A = new A();
        let a2: A = a1;
        "#,
        0,
    );
}
