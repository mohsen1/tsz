//! Tests for intersection type signature resolution.
//!
//! Verifies that call/construct signatures are correctly collected from
//! intersection type members, enabling mixin patterns and similar TypeScript
//! idioms where an intersection of callable types should be callable.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

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

    let in_parser = parser.get_diagnostics().iter().any(|d| d.code == code);
    let in_checker = checker.ctx.diagnostics.iter().any(|d| d.code == code);
    in_parser || in_checker
}

/// Mixin pattern: `class Derived extends Mixin(Base)<string>` should not
/// emit TS2315 ("Type is not generic") when the mixin function returns
/// an intersection with a generic constructor.
#[test]
fn mixin_extends_intersection_no_ts2315() {
    let source = r#"
        type Constructor<T> = new (...args: any[]) => T;

        class Base<T> {
            value!: T;
            constructor(v: T) {}
        }

        interface Mixed { mixed: boolean; }

        function Mixin<T extends Constructor<Base<any>>>(base: T): T & Constructor<Mixed> {
            return base as any;
        }

        class Derived extends Mixin(Base)<string> {
            prop!: number;
        }
    "#;
    assert!(
        !has_error_code(source, 2315),
        "Mixin pattern with generic intersection should not emit TS2315"
    );
}

/// Mixin pattern: `new Derived('hello')` should not emit TS2554
/// ("Expected 0 arguments") when the intersection constructor has
/// inherited params from the base class.
#[test]
fn mixin_extends_intersection_no_ts2554() {
    let source = r#"
        type Constructor<T> = new (...args: any[]) => T;

        class Base<T> {
            value!: T;
            constructor(v: T) {}
        }

        interface Mixed { mixed: boolean; }

        function Mixin<T extends Constructor<Base<any>>>(base: T): T & Constructor<Mixed> {
            return base as any;
        }

        class Derived extends Mixin(Base)<string> {
            prop!: number;
        }

        const d = new Derived('hello');
    "#;
    assert!(
        !has_error_code(source, 2554),
        "new Derived('hello') should not emit TS2554 when constructor is inherited through intersection"
    );
}

/// Simple intersection of two constructor types should be constructable
/// without TS2349 ("not callable").
#[test]
fn intersection_constructor_callable_no_ts2349() {
    let source = r#"
        type A = new (x: string) => { a: string };
        type B = new (x: string) => { b: number };
        type AB = A & B;
        declare const ctor: AB;
        const instance = new ctor('test');
    "#;
    assert!(
        !has_error_code(source, 2349),
        "Intersection of constructors should be callable"
    );
}

#[test]
fn discriminated_union_intersection_property_access() {
    // When an intersection narrows a discriminated union with a literal discriminant,
    // properties from the matching branch should be accessible.
    // Regression test for typeVariableConstraintIntersections.ts conformance.
    let source = r#"
        type OptionOne = { kind: "one"; s: string; };
        type OptionTwo = { kind: "two"; x: number; y: number; };
        type Options = OptionOne | OptionTwo;

        type OptionHandlers = {
            [K in Options['kind']]: (option: Options & { kind: K }) => string;
        }

        const optionHandlers: OptionHandlers = {
            "one": option => option.s,
            "two": option => option.x + "," + option.y,
        };
    "#;
    assert!(
        !has_error_code(source, 2339),
        "Property access on narrowed discriminated union intersection should succeed"
    );
}

#[test]
fn discriminated_union_intersection_simple() {
    // Simple case: direct intersection of a union with a discriminant literal object.
    let source = r#"
        type A = { tag: "a"; valA: string; };
        type B = { tag: "b"; valB: number; };
        type AB = A | B;

        declare const x: AB & { tag: "a" };
        const v: string = x.valA;
    "#;
    assert!(
        !has_error_code(source, 2339),
        "Property 'valA' should be accessible on (A | B) & {{tag: 'a'}}"
    );
}
