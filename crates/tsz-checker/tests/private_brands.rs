//! Tests for private/protected member nominal typing (Lawyer Layer).
//!
//! These tests verify that classes with private/protected members behave nominally,
//! not structurally. This implements TypeScript's "brand checking" where private
//! members create a nominal identity that overrides structural compatibility.

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn test_private_brands(source: &str, expected_errors: usize) {
    test_private_brands_with_codes(source, expected_errors, &[2322])
}

fn has_error_code(source: &str, code: u32) -> bool {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Check both parser and checker diagnostics
    let in_parser = parser.get_diagnostics().iter().any(|d| d.code == code);
    let in_checker = checker.ctx.diagnostics.iter().any(|d| d.code == code);
    in_parser || in_checker
}

fn test_private_brands_with_codes(source: &str, expected_errors: usize, error_codes: &[u32]) {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| error_codes.contains(&d.code))
        .count();

    assert_eq!(
        error_count, expected_errors,
        "Expected {} errors with codes {:?}, got {}: {:?}",
        expected_errors, error_codes, error_count, checker.ctx.diagnostics
    );
}

/// Test that private members are nominal - different classes with same private member shape
/// are NOT compatible, even though structurally they match.
#[test]
fn test_private_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a private property 'x'.
    test_private_brands(
        r"
        class A { private x: number = 1; }
        class B { private x: number = 1; }
        let a: A = new B();
        ",
        1,
    );
}

/// Test that private members prevent structural assignment to object literals.
/// Even if the object literal has the same shape, the private brand is missing.
#[test]
fn test_private_member_prevents_structural_assignment() {
    let source = r"
        class A { private x: number = 1; }
        let a: A = { x: 1 };
        ";

    test_private_brands(source, 1);
    assert!(
        !has_error_code(source, 2741),
        "Private-brand structural assignment should report TS2322, not TS2741"
    );
}

/// Test that protected members are also nominal.
/// Protected members create a brand just like private members.
#[test]
fn test_protected_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a protected property 'x'.
    test_private_brands(
        r"
        class A { protected x: number = 1; }
        class B { protected x: number = 1; }
        let a: A = new B();
        ",
        1,
    );
}

/// Test that subclasses ARE compatible with their base classes.
/// The subclass inherits the private brand from the parent.
#[test]
fn test_subclass_compatibility() {
    // Should pass (subclass shares the private brand)
    test_private_brands(
        r"
        class A { private x: number = 1; }
        class B extends A {}
        let a: A = new B();
        ",
        0,
    );
}

/// Test that public members are structural (default TypeScript behavior).
/// Public members don't create a nominal brand.
#[test]
fn test_public_members_are_structural() {
    // Should pass (public members are structural)
    test_private_brands(
        r"
        class A { public x: number = 1; }
        class B { public x: number = 1; }
        let a: A = new B();
        ",
        0,
    );
}

/// Test that multiple private members create a stronger brand.
/// Classes must match ALL private members to be compatible.
#[test]
fn test_multiple_private_members() {
    // TS2322: Types have separate declarations of private properties 'x' and 'y'.
    test_private_brands(
        r"
        class A { private x: number = 1; private y: number = 2; }
        class B { private x: number = 1; private y: number = 2; }
        let a: A = new B();
        ",
        1,
    );
}

/// Test that classes with different private member sets are incompatible.
#[test]
fn test_different_private_members() {
    // TS2322: Types with different private members are incompatible
    test_private_brands(
        r"
        class A { private x: number = 1; }
        class B { private y: number = 1; }
        let a: A = new B();
        ",
        1,
    );
}

/// Test that private methods also create nominal brands.
#[test]
fn test_private_methods_create_brands() {
    // TS2322: Types have separate declarations of a private method 'foo'
    test_private_brands(
        r"
        class A { private foo() {} }
        class B { private foo() {} }
        let a: A = new B();
        ",
        1,
    );
}

/// Generic instantiations with the same ECMAScript private brand are still
/// incompatible when the private member types differ.
#[test]
fn test_generic_private_identifiers_still_check_member_types() {
    test_private_brands(
        r#"
        class C<T> {
            #foo: T;
            constructor(t: T) {
                this.#foo = t;
            }
        }

        let a = new C(3);
        let b = new C("hello");
        a = b;
        b = a;
        "#,
        2,
    );
}

/// Test that assigning object literal with extra property to class with private member.
/// Object literals can't have private members, so this should fail with TS2353 (excess property).
#[test]
fn test_object_literal_extra_property_to_class_with_private() {
    // TSC emits TS2353: "Object literal may only specify known properties, and 'y' does not exist in type 'A'."
    test_private_brands_with_codes(
        r"
        class A { private x: number = 1; }
        let a: A = { x: 1, y: 2 };
        ",
        1,
        &[2353],
    );
}

/// Test that same class is assignable to itself (trivial case).
#[test]
fn test_same_class_compatibility() {
    // Should pass
    test_private_brands(
        r"
        class A { private x: number = 1; }
        let a1: A = new A();
        let a2: A = a1;
        ",
        0,
    );
}

// ============================================================
// TS18016: Private identifiers not allowed outside class bodies
// ============================================================

/// TS18016 should be emitted for private identifiers in type literal property signatures.
#[test]
fn test_ts18016_private_id_in_type_literal() {
    assert!(
        has_error_code(r"type A = { #foo: string; };", 18016,),
        "Should emit TS18016 for private identifier in type literal"
    );
}

/// TS18016 should be emitted for private identifiers in interface property signatures.
#[test]
fn test_ts18016_private_id_in_interface() {
    assert!(
        has_error_code(r"interface B { #bar: number; }", 18016,),
        "Should emit TS18016 for private identifier in interface"
    );
}

/// Private identifier on `any` type outside class body → TS18016.
#[test]
fn test_ts18016_private_id_on_any_outside_class() {
    assert!(
        has_error_code(
            r"
            declare var x: any;
            x.#nope;
            ",
            18016,
        ),
        "Should emit TS18016 for undeclared private name on any outside class"
    );
}

/// Private identifier on `any` type INSIDE class body (but undeclared) → TS2339.
#[test]
fn test_ts2339_private_id_on_any_inside_class() {
    assert!(
        has_error_code(
            r"
            class C {
                #foo = 1;
                m(x: any) { x.#unknown; }
            }
            ",
            2339,
        ),
        "Should emit TS2339 for undeclared private name on any inside class"
    );
    // Should NOT emit TS18013 for this case
    assert!(
        !has_error_code(
            r"
            class C {
                #foo = 1;
                m(x: any) { x.#unknown; }
            }
            ",
            18013,
        ),
        "Should NOT emit TS18013 for undeclared private name on any inside class"
    );
}
