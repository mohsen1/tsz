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
use tsz_checker::test_utils::check_source_diagnostics;
use tsz_parser::parser::ParserState;
use tsz_solver::type_queries::get_callable_shape;
use tsz_solver::{TypeId, TypeInterner};

fn check_source(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
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
fn static_method_returning_polymorphic_this_keeps_static_members() {
    let diags = check_source(
        r#"
        class C {
            static fn() { return this; }
            static get x() { return 1; }
            static set x(v) { }
            constructor(public a: number, private b: number) { }
            static foo: string;
        }

        var r = C.fn();
        var r2 = r.x;
        var r3 = r.foo;

        class D extends C {
            bar: string;
        }

        var r = D.fn();
        var r2 = r.x;
        var r3 = r.foo;
        "#,
    );
    let codes = error_codes(&diags);
    assert!(
        !codes.contains(&2339),
        "Static method returning `this` should preserve constructor-side static members, got: {codes:?}"
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

#[test]
fn recursive_static_class_expression_keeps_placeholder_static_member() {
    let source = r#"
        class C {
            static D = class extends C {};
        }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let class_idx = source_file.statements.nodes[0];
    let class_node = arena.get(class_idx).expect("class node");
    let class_decl = arena.get_class(class_node).expect("class decl");
    let static_prop_idx = class_decl.members.nodes[0];
    let static_prop_node = arena.get(static_prop_idx).expect("static property node");
    let static_prop = arena
        .get_property_decl(static_prop_node)
        .expect("static property decl");
    let static_prop_type = checker
        .ctx
        .node_types
        .get(&static_prop.initializer.0)
        .copied()
        .expect("static property initializer type");
    let callable = get_callable_shape(&types, static_prop_type).expect("callable shape");
    let property_names: Vec<_> = callable
        .properties
        .iter()
        .map(|prop| (types.resolve_atom(prop.name), prop.type_id))
        .collect();
    let base_name = arena
        .get_class_at(static_prop.initializer)
        .and_then(|class_expr| class_expr.heritage_clauses.as_ref())
        .and_then(|clauses| clauses.nodes.first().copied())
        .and_then(|clause_idx| arena.get_heritage_clause_at(clause_idx))
        .and_then(|heritage| heritage.types.nodes.first().copied())
        .and_then(|type_idx| {
            arena
                .get_expr_type_args_at(type_idx)
                .map_or(Some(type_idx), |eta| Some(eta.expression))
        })
        .and_then(|expr_idx| arena.get_identifier_at(expr_idx))
        .map(|ident| ident.escaped_text.clone());

    assert!(
        callable
            .properties
            .iter()
            .any(|prop| types.resolve_atom(prop.name) == "D" && prop.type_id == TypeId::ANY),
        "Expected recursive static class expression to preserve inherited D: any placeholder; base={base_name:?}, properties={property_names:#?}, callable={callable:#?}"
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
