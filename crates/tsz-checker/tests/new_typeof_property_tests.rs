//! Tests for `TypeQuery` (typeof) resolution in new expressions.
//!
//! When a class constructor is accessed through an interface/object property
//! typed as `typeof ClassName`, the resulting `TypeQuery` must be resolved before
//! attempting constructor resolution. Without this, `new obj.prop(args)` would
//! produce a false TS2351 ("This expression is not constructable").

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// When an interface property is typed as `typeof SomeClass`, calling `new obj.prop()`
/// should successfully construct the class — no TS2351 error.
#[test]
fn test_new_typeof_property_no_false_ts2351() {
    let codes = get_error_codes(
        r#"
class B {
    constructor(public x: number) {}
}

interface C {
    prop: typeof B;
}

declare var c: C;
let instance = new c.prop(1);
"#,
    );
    assert!(
        !codes.contains(&2351),
        "Should not emit TS2351 for `new c.prop(1)` when prop is `typeof B`, got: {codes:?}"
    );
}

/// Same pattern with a no-arg constructor.
#[test]
fn test_new_typeof_property_no_arg_constructor() {
    let codes = get_error_codes(
        r#"
class A {}

interface Holder {
    ctor: typeof A;
}

declare var h: Holder;
let a = new h.ctor();
"#,
    );
    assert!(
        !codes.contains(&2351),
        "Should not emit TS2351 for `new h.ctor()` when ctor is `typeof A`, got: {codes:?}"
    );
}

/// Typeof through a plain object type (not just an interface).
#[test]
fn test_new_typeof_in_object_type() {
    let codes = get_error_codes(
        r#"
class Foo {
    constructor(public value: string) {}
}

declare var obj: { factory: typeof Foo };
let f = new obj.factory("hello");
"#,
    );
    assert!(
        !codes.contains(&2351),
        "Should not emit TS2351 for new through object type property, got: {codes:?}"
    );
}

/// Wrong argument count should still produce TS2554, not TS2351.
#[test]
fn test_new_typeof_property_wrong_arg_count() {
    let codes = get_error_codes(
        r#"
class B {
    constructor(public x: number) {}
}

interface C {
    prop: typeof B;
}

declare var c: C;
let instance = new c.prop();
"#,
    );
    // Should NOT produce TS2351 (not constructable) — the type IS constructable.
    assert!(
        !codes.contains(&2351),
        "Should not emit TS2351 — the typeof property is constructable, got: {codes:?}"
    );
    // Should produce TS2554 (wrong argument count) since constructor expects 1 arg.
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 for wrong arg count on typeof constructor, got: {codes:?}"
    );
}
