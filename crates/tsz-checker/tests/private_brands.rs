//! Tests for private/protected member nominal typing (Lawyer Layer).
//!
//! These tests verify that classes with private/protected members behave nominally,
//! not structurally. This implements TypeScript's "brand checking" where private
//! members create a nominal identity that overrides structural compatibility.

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, diagnostics::Diagnostic, state::CheckerState};
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Check both parser and checker diagnostics
    let in_parser = parser.get_diagnostics().iter().any(|d| d.code == code);
    let in_checker = checker.ctx.diagnostics.iter().any(|d| d.code == code);
    in_parser || in_checker
}

fn test_private_brands_with_codes(source: &str, expected_errors: usize, error_codes: &[u32]) {
    let diagnostics = collect_private_brand_diagnostics(source);

    let error_count = diagnostics
        .iter()
        .filter(|d| error_codes.contains(&d.code))
        .count();

    assert_eq!(
        error_count, expected_errors,
        "Expected {expected_errors} errors with codes {error_codes:?}, got {error_count}: {diagnostics:?}"
    );
}

fn collect_private_brand_diagnostics(source: &str) -> Vec<Diagnostic> {
    collect_private_brand_diagnostics_with_options(source, "test.ts", CheckerOptions::default())
}

fn collect_private_brand_diagnostics_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.clone()
}

/// Test that private members are nominal - different classes with same private member shape
/// are NOT compatible, even though structurally they match.
#[test]
fn test_private_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a private property 'x'.
    let source = r"
        class A { private x: number = 1; }
        class B { private x: number = 1; }
        let a: A = new B();
        ";

    test_private_brands(source, 1);
    let diagnostics = collect_private_brand_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for private nominal mismatch");
    assert!(
        ts2322.related_information.iter().any(|info| info
            .message_text
            .contains("Types have separate declarations of a private property 'x'.")),
        "Expected nominal private-property detail in TS2322 related info, got: {ts2322:?}"
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
    let diagnostics = collect_private_brand_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for private-brand structural assignment");
    assert!(
        !has_error_code(source, 2741),
        "Private-brand structural assignment should report TS2322, not TS2741"
    );
    // TODO: Improve TS2322 elaboration to include "Property 'x' is private in type 'A'"
    assert!(
        ts2322.message_text.contains("not assignable to type"),
        "Expected TS2322 for private-brand structural assignment, got: {ts2322:?}"
    );
}

/// Test that protected members are also nominal.
/// Protected members create a brand just like private members.
#[test]
fn test_protected_members_are_nominal() {
    // TS2322: Type 'B' is not assignable to type 'A'.
    //   Types have separate declarations of a protected property 'x'.
    let source = r"
        class A { protected x: number = 1; }
        class B { protected x: number = 1; }
        let a: A = new B();
        ";

    test_private_brands(source, 1);
    let diagnostics = collect_private_brand_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for protected nominal mismatch");
    assert!(
        ts2322.related_information.iter().any(|info| info
            .message_text
            .contains("Types have separate declarations of a protected property 'x'.")),
        "Expected nominal protected-property detail in TS2322 related info, got: {ts2322:?}"
    );
}

#[test]
fn test_protected_member_visibility_mismatch_elaborates_ts2322() {
    let source = r"
        class A { protected x: number = 1; }
        class B { public x: number = 1; }
        let a: A = new B();
        ";

    test_private_brands(source, 1);
    let diagnostics = collect_private_brand_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for protected/public visibility mismatch");
    // TODO: Improve TS2322 elaboration to include visibility detail
    assert!(
        ts2322.message_text.contains("not assignable to type"),
        "Expected TS2322 for protected/public visibility mismatch, got: {ts2322:?}"
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
    let source = r"
        class A { private x: number = 1; }
        class B { private y: number = 1; }
        let a: A = new B();
        ";

    test_private_brands(source, 1);
    let diagnostics = collect_private_brand_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for different private members");
    assert!(
        ts2322.related_information.iter().any(|info| {
            info.message_text.contains("Property 'y' in type 'B'")
                && !info.message_text.contains("[private field]")
        }),
        "Expected nominal mismatch related info to mention the concrete private member, got: {ts2322:?}"
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

/// Private identifier on a function prototype assignment outside class body -> TS18016.
#[test]
fn test_ts18016_private_id_on_js_prototype_assignment_outside_class() {
    let diagnostics = collect_private_brand_diagnostics_with_options(
        r"
        function A() {}
        A.prototype.#no = 2;
        ",
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let ts18016_count = diagnostics.iter().filter(|d| d.code == 18016).count();
    assert_eq!(
        ts18016_count, 1,
        "Should emit TS18016 for private name assignment on a JS prototype outside class, got: {diagnostics:?}"
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

/// Private identifier brand check in nested static block should resolve to outer class.
/// `#field in obj` inside a nested class static block should find the outer class's private field.
#[test]
fn test_private_identifier_in_nested_static_block() {
    // This tests the autoAccessor10.ts conformance case
    // The private field is declared on C3, and accessed via `#a2_accessor_storage in C3`
    // inside a nested class's static block.
    let source = r#"
class C3 {
    static #a2_accessor_storage = 1;
    static {
        class C3_Inner {
            static {
                #a2_accessor_storage in C3;
            }
        }
    }
}
"#;

    let diagnostics = collect_private_brand_diagnostics(source);

    // Filter for TS2339 errors
    let ts2339_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();

    assert!(
        ts2339_errors.is_empty(),
        "Should NOT emit TS2339 for private identifier in nested static block brand check. \
        The private field #a2_accessor_storage should be resolved from the outer class C3. \
        Got {} errors: {:?}",
        ts2339_errors.len(),
        ts2339_errors
    );
}

/// Test that TS2420 fires when an interface inherits a private member from a
/// base class and the implementing class declares that member with wider
/// (public) visibility. The class widens visibility, which is a structural
/// mismatch regardless of whether the class extends the same base.
#[test]
fn test_ts2420_for_public_class_member_vs_private_interface_member() {
    let source = r"
        class Foo {
            private x!: string;
        }
        interface I extends Foo {
            y: number;
        }
        class Bar2 extends Foo implements I {
            x!: string;
            y!: number;
        }
    ";

    let diagnostics = collect_private_brand_diagnostics(source);

    let ts2420_for_bar2 = diagnostics.iter().find(|d| {
        d.code == 2420
            && d.message_text.contains("Bar2")
            && d.message_text.contains("interface 'I'")
    });
    assert!(
        ts2420_for_bar2.is_some(),
        "expected TS2420 for Bar2 implementing I: class declares public 'x' but I inherits a private 'x' from Foo. Got diagnostics: {diagnostics:?}"
    );
    let d = ts2420_for_bar2.unwrap();
    assert!(
        d.message_text
            .contains("Property 'x' is private in type 'I' but not in type 'Bar2'")
            || d.related_information.iter().any(|r| r
                .message_text
                .contains("Property 'x' is private in type 'I' but not in type 'Bar2'")),
        "expected visibility-widening elaboration for TS2420 on Bar2, got: {d:?}"
    );
}

// ============================================================
// Ergonomic brand check (`#field in expr`) diagnostics
// ============================================================

/// TS1451: `(#field) in v` — private identifier in a parenthesized expression is a
/// standalone expression (not the direct LHS of `in`).
#[test]
fn test_ts1451_parenthesized_private_identifier_in_expression() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
    check(v: any) {
        return (#field) in v; // TS1451
    }
}
"#,
    );
    let ts1451_count = diagnostics.iter().filter(|d| d.code == 1451).count();
    assert_eq!(
        ts1451_count, 1,
        "Expected exactly 1 TS1451 for parenthesized private identifier in `in` expression. Got: {diagnostics:?}"
    );
}

/// TS18016: `#field in v` used outside any class body.
#[test]
fn test_ts18016_private_in_expression_outside_class() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
}
function check(v: C) {
    return #field in v; // TS18016 - outside class body
}
"#,
    );
    let ts18016_count = diagnostics.iter().filter(|d| d.code == 18016).count();
    assert_eq!(
        ts18016_count, 1,
        "Expected TS18016 for #field in expression outside class body. Got: {diagnostics:?}"
    );
}

/// TS2339: typo in private identifier name (`#fiel` vs `#field`) — error even when RHS is `any`.
#[test]
fn test_ts2339_typo_private_identifier_in_expression_any_rhs() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
    check(v: any) {
        return #fiel in v; // TS2339 - typo, even though v is any
    }
}
"#,
    );
    let ts2339_count = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339_count, 1,
        "Expected TS2339 for undeclared private identifier #fiel even when RHS is any. Got: {diagnostics:?}"
    );
}

/// TS2406: `for (#field in v)` — private identifier as for-in LHS.
#[test]
fn test_ts2406_private_identifier_as_for_in_lhs() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
    check(v: any) {
        for (#field in v) {} // TS2406
    }
}
"#,
    );
    let ts2406_count = diagnostics.iter().filter(|d| d.code == 2406).count();
    let ts2405_count = diagnostics.iter().filter(|d| d.code == 2405).count();
    assert_eq!(
        ts2406_count, 1,
        "Expected TS2406 (not TS2405) for private identifier as for-in LHS. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts2405_count, 0,
        "Should NOT emit TS2405 for private identifier as for-in LHS. Got: {diagnostics:?}"
    );
}

/// TS18047: `#field in u` where `u: object | null` — null is not valid RHS.
#[test]
fn test_ts18047_possibly_null_rhs_in_private_in_expression() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
    check(u: object | null) {
        return #field in u; // TS18047 - u is possibly null
    }
}
"#,
    );
    let ts18047_count = diagnostics.iter().filter(|d| d.code == 18047).count();
    assert_eq!(
        ts18047_count, 1,
        "Expected TS18047 for possibly-null RHS in #field in expr. Got: {diagnostics:?}"
    );
    // Must NOT emit TS2719 (spurious "two different types" error)
    let ts2719_count = diagnostics.iter().filter(|d| d.code == 2719).count();
    assert_eq!(
        ts2719_count, 0,
        "Should NOT emit TS2719 for possibly-null RHS. Got: {diagnostics:?}"
    );
}

/// No errors for valid `#field in expr` with non-class RHS types.
/// tsc does NOT require the RHS to be assignable to the declaring class type.
#[test]
fn test_no_error_private_in_expression_non_class_rhs() {
    let diagnostics = collect_private_brand_diagnostics(
        r#"
class C {
    #field = 1;
    check(v: any) {
        const a = #field in v;             // ok - any
        const b = #field in {};            // ok - object literal
        const c = #field in (v as object); // ok - object type
        const d = #field in new C();       // ok - instance of C
    }
}
"#,
    );
    // Should emit no errors for these valid uses
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| [2339, 1451, 18016, 18047, 2719, 2322].contains(&d.code))
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no errors for valid #field in expressions. Got: {errors:?}"
    );
}
