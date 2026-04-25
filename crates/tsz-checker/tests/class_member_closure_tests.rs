//! Regression tests for the `ClassMemberClosure` / `OwnMemberSummary` boundary.
//!
//! These tests verify that class/member summary extraction and routing
//! through the boundary correctly handles:
//!   - Strict property initialization (TS2564)
//!   - Parameter properties
//!   - Override visibility/type checks (TS4112-TS4115, TS2416)
//!   - Base/member closure consistency

use crate::context::CheckerOptions;
use crate::test_utils::check_source;
use tsz_common::diagnostics::Diagnostic;

fn check_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_with_no_implicit_override(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_default(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, CheckerOptions::default())
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

// =============================================================================
// Strict Property Initialization (TS2564)
// =============================================================================

#[test]
fn strict_property_init_basic_ts2564() {
    let source = r#"
        class C {
            x: number;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        has_code(&diags, 2564),
        "Expected TS2564 for uninitialized property, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_with_initializer_no_error() {
    let source = r#"
        class C {
            x: number = 0;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 for initialized property, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_constructor_assigns_no_error() {
    let source = r#"
        class C {
            x: number;
            constructor() {
                this.x = 0;
            }
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 when constructor assigns, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_optional_no_error() {
    let source = r#"
        class C {
            x?: number;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 for optional property, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_definite_assignment_no_error() {
    let source = r#"
        class C {
            x!: number;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 for definite assignment assertion, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_any_type_no_error() {
    let source = r#"
        class C {
            x: any;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 for 'any' typed property, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_union_with_undefined_no_error() {
    let source = r#"
        class C {
            x: number | undefined;
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Should not emit TS2564 for union including undefined, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn strict_property_init_multiple_properties() {
    let source = r#"
        class C {
            x: number;
            y: string;
            z: boolean = true;
        }
    "#;
    let diags = check_strict(source);
    assert_eq!(
        count_code(&diags, 2564),
        2,
        "Expected 2 TS2564 errors (x and y), got: {:?}",
        codes(&diags)
    );
}

// =============================================================================
// Parameter Properties
// =============================================================================

#[test]
fn parameter_property_public() {
    let source = r#"
        class C {
            constructor(public x: number) {}
        }
        let c = new C(1);
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 2564),
        "Parameter property should not need separate initialization: {:?}",
        codes(&diags)
    );
}

#[test]
fn parameter_property_private() {
    let source = r#"
        class C {
            constructor(private x: number) {}
        }
    "#;
    let diags = check_default(source);
    // No errors expected — private parameter property is valid
    let non_trivial: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2564 || d.code == 2416)
        .collect();
    assert!(
        non_trivial.is_empty(),
        "No TS2564/TS2416 expected for private param property: {:?}",
        codes(&diags)
    );
}

#[test]
fn parameter_property_protected() {
    let source = r#"
        class C {
            constructor(protected x: number) {}
        }
    "#;
    let diags = check_default(source);
    let non_trivial: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2564 || d.code == 2416)
        .collect();
    assert!(
        non_trivial.is_empty(),
        "No TS2564/TS2416 expected for protected param property: {:?}",
        codes(&diags)
    );
}

#[test]
fn parameter_property_readonly() {
    let source = r#"
        class C {
            constructor(readonly x: number) {}
        }
    "#;
    let diags = check_default(source);
    let non_trivial: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2564 || d.code == 2416)
        .collect();
    assert!(
        non_trivial.is_empty(),
        "No TS2564/TS2416 expected for readonly param property: {:?}",
        codes(&diags)
    );
}

#[test]
fn parameter_property_strict_init_no_error() {
    // Parameter properties are always assigned — should not trigger TS2564
    let source = r#"
        class C {
            constructor(public x: number) {}
        }
    "#;
    let diags = check_strict(source);
    assert!(
        !has_code(&diags, 2564),
        "Parameter property should not trigger TS2564: {:?}",
        codes(&diags)
    );
}

// =============================================================================
// Override Visibility / Type Checks
// =============================================================================

#[test]
fn override_no_base_class_ts4112() {
    // TS4112: override modifier on member without base class
    let source = r#"
        class C {
            override x: number = 0;
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 4112),
        "Expected TS4112 for override without base class, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn override_member_not_in_base_ts4113() {
    // TS4113: override on member that doesn't exist in base
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            override y: number = 0;
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 4113),
        "Expected TS4113 for override not in base, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn override_valid_no_error() {
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            override x: number = 1;
        }
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 4112) && !has_code(&diags, 4113),
        "Valid override should not emit TS4112/TS4113, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn no_implicit_override_missing_ts4114() {
    // TS4114: member overrides base without 'override' keyword
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            x: number = 1;
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    assert!(
        has_code(&diags, 4114),
        "Expected TS4114 for missing override keyword, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn no_implicit_override_with_keyword_no_error() {
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            override x: number = 1;
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    assert!(
        !has_code(&diags, 4114),
        "Override keyword should satisfy noImplicitOverride, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn override_parameter_property_ts4115() {
    // TS4115: parameter property must have override when it overrides a base member
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            constructor(public x: number) { super(); }
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    assert!(
        has_code(&diags, 4115),
        "Expected TS4115 for parameter property overriding base without override, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn override_parameter_property_with_override_no_error() {
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            constructor(override public x: number) { super(); }
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    assert!(
        !has_code(&diags, 4115),
        "Override keyword on parameter property should satisfy noImplicitOverride, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn property_type_incompatible_with_base_ts2416() {
    // TS2416: property type not assignable to same property in base type
    let source = r#"
        class Base {
            x: number = 0;
        }
        class Derived extends Base {
            x: string = "";
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 2416),
        "Expected TS2416 for incompatible property type, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn property_type_compatible_with_base_no_error() {
    // Subtype is OK
    let source = r#"
        class Base {
            x: number | string = 0;
        }
        class Derived extends Base {
            x: number = 0;
        }
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 2416),
        "Compatible property type should not emit TS2416, got: {:?}",
        codes(&diags)
    );
}

// =============================================================================
// Base/Member Closure Consistency
// =============================================================================

#[test]
fn deep_inheritance_override_valid() {
    // Override through multiple inheritance levels
    let source = r#"
        class A {
            x: number = 0;
        }
        class B extends A {}
        class C extends B {
            override x: number = 1;
        }
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 4113),
        "Override should resolve through deep inheritance chain, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn deep_inheritance_override_not_in_chain_ts4113() {
    let source = r#"
        class A {
            x: number = 0;
        }
        class B extends A {}
        class C extends B {
            override y: number = 1;
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 4113),
        "Override should fail when member not in inheritance chain, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn override_method_valid() {
    let source = r#"
        class Base {
            foo(): void {}
        }
        class Derived extends Base {
            override foo(): void {}
        }
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 4113),
        "Method override should be valid, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn method_type_incompatible_with_base_ts2416() {
    let source = r#"
        class Base {
            foo(): number { return 0; }
        }
        class Derived extends Base {
            foo(): string { return ""; }
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 2416),
        "Incompatible method return type should emit TS2416, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn private_member_not_in_override_scope() {
    // Private members should NOT appear in the base chain for override checking.
    // A derived class can declare a member with the same name as a private base member
    // without needing `override`.
    let source = r#"
        class Base {
            private x: number = 0;
        }
        class Derived extends Base {
            x: number = 1;
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    // With noImplicitOverride, this should NOT require 'override' because
    // private members are not inherited and can't be overridden.
    // tsc does emit TS2415 for this pattern (class incorrectly extends base class)
    // but NOT TS4114.
    assert!(
        !has_code(&diags, 4114),
        "Private base member should not trigger noImplicitOverride check, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn static_member_override_valid() {
    let source = r#"
        class Base {
            static x: number = 0;
        }
        class Derived extends Base {
            override static x: number = 1;
        }
    "#;
    let diags = check_default(source);
    assert!(
        !has_code(&diags, 4113),
        "Static member override should be valid, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn abstract_member_implementation_no_override_needed() {
    // Implementing an abstract method should not require `override` keyword
    // even with noImplicitOverride.
    let source = r#"
        abstract class Base {
            abstract foo(): void;
        }
        class Derived extends Base {
            foo(): void {}
        }
    "#;
    let diags = check_with_no_implicit_override(source);
    assert!(
        !has_code(&diags, 4114),
        "Implementing abstract method should not need override keyword, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn accessor_property_kind_mismatch_ts2610() {
    // TS2610: property overrides accessor in base
    let source = r#"
        class Base {
            get x(): number { return 0; }
        }
        class Derived extends Base {
            x: number = 0;
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 2610),
        "Property overriding accessor should emit TS2610, got: {:?}",
        codes(&diags)
    );
}

#[test]
fn accessor_method_kind_mismatch_ts2423() {
    // TS2423: base has method, derived has accessor
    let source = r#"
        class Base {
            foo(): void {}
        }
        class Derived extends Base {
            get foo(): number { return 0; }
        }
    "#;
    let diags = check_default(source);
    assert!(
        has_code(&diags, 2423),
        "Accessor overriding method should emit TS2423, got: {:?}",
        codes(&diags)
    );
}
