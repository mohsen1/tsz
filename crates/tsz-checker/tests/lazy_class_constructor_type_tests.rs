//! Tests for resolving Lazy class types to constructor types in value position.
//!
//! When a class method returns `this` (the class itself) during class construction,
//! the type system creates a `Lazy(DefId)` placeholder to break circular references.
//! This placeholder must resolve to the constructor type (not the instance type) when
//! used in value position — e.g., as a return value or when accessing static members.
//!
//! Root cause: `TypeEnvironment::resolve_lazy` unconditionally returns the instance type
//! for class `DefIds`, but value-position class references need the constructor type.
//! The fix applies `resolve_lazy_class_to_constructor` at two sites:
//!   1. After `infer_return_type_from_body` (return type inference)
//!   2. In `get_type_of_private_property_access` (private member access on class ref)

use tsz_binder::BinderState;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_source(source: &str) -> Vec<Diagnostic> {
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
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn error_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

// ── Static method returning self-reference ──────────────────────────────

#[test]
fn static_method_returning_class_ref_no_false_ts2339() {
    // A static method that returns the class itself should not produce TS2339
    // when the result is used to access static members.
    let diags = check_source(
        r#"
        class A {
            static #x = 1;
            static getClass() { return A; }
        }
        A.getClass().#x;
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for private static field on class ref; got: {codes:?}"
    );
}

#[test]
fn static_method_in_static_field_init() {
    // `static s = C.#method()` — C is Lazy(DefId) inside a static field initializer.
    // The fix in computed_helpers.rs resolves this to constructor type so #method is found.
    let diags = check_source(
        r#"
        class C {
            static s = C.#method();
            static #method() { return 42; }
        }
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for #method on C in static field init; got: {codes:?}"
    );
}

#[test]
fn static_field_assignment_via_class_ref() {
    // Static private field assignment through class self-reference in method body.
    let diags = check_source(
        r#"
        class A {
            static #field = 0;
            static setField(val: number) {
                A.#field = val;
            }
        }
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for static private field assignment; got: {codes:?}"
    );
}

#[test]
fn static_private_field_destructured_binding() {
    // Destructured binding from static private field should work.
    let diags = check_source(
        r#"
        class A {
            static #field = { x: 1, y: 2 };
            static getCoords() {
                const { x, y } = A.#field;
                return x + y;
            }
        }
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Should not emit TS2339 for destructured static private field; got: {codes:?}"
    );
}

// ── Instance members should NOT be affected ─────────────────────────────

#[test]
fn instance_private_field_still_works() {
    // Instance private field access should remain unaffected by the fix.
    let diags = check_source(
        r#"
        class A {
            #value = 10;
            getValue() { return this.#value; }
        }
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Instance private field access should work; got: {codes:?}"
    );
}

// ── Negative test: TS2339 should still fire when appropriate ────────────

#[test]
fn ts2339_still_emitted_for_nonexistent_private_member() {
    // Accessing a private member that does not exist should still emit TS2339.
    let diags = check_source(
        r#"
        class A {
            static #existing = 1;
        }
        A.#nonExisting;
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        codes.contains(&2339) || codes.contains(&18016),
        "Should emit TS2339 or TS18016 for non-existing private member; got: {codes:?}"
    );
}
