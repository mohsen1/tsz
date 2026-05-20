//! Tests for `TypeQuery` (typeof) resolution in new expressions.
//!
//! When a class constructor is accessed through an interface/object property
//! typed as `typeof ClassName`, the resulting `TypeQuery` must be resolved before
//! attempting constructor resolution. Without this, `new obj.prop(args)` would
//! produce a false TS2351 ("This expression is not constructable").

use crate::test_utils::check_source_codes as get_error_codes;

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

/// `typeof x.p` inside an if-block where `x.p` has been narrowed by an equality
/// check should resolve to the narrowed type, not the declared union type.
/// This matches TypeScript's control-flow–aware typeof type queries.
#[test]
fn test_typeof_qualified_name_narrowed_by_equality_check() {
    let codes = get_error_codes(
        r#"
interface I<T> {
  p: T;
}
function e(x: I<"A" | "B">) {
    if (x.p === "A") {
        let a: "A" = (null as unknown as typeof x.p)
    }
}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Should not emit TS2322 when typeof x.p is narrowed by x.p === \"A\", got: {codes:?}"
    );
}

/// `typeof x.p` outside of any narrowing block should still resolve to
/// the full declared type (no spurious narrowing).
#[test]
fn test_typeof_qualified_name_no_narrowing_outside_guard() {
    let codes = get_error_codes(
        r#"
interface I<T> {
  p: T;
}
function e(x: I<"A" | "B">) {
    let a: "A" = (null as unknown as typeof x.p)
}
"#,
    );
    // "A" | "B" is not assignable to "A", so TS2322 should fire
    assert!(
        codes.contains(&2322),
        "Should emit TS2322 when typeof x.p is not narrowed, got: {codes:?}"
    );
}

/// `typeof c` in a type alias inside a narrowed branch should see the narrowed type.
#[test]
fn test_typeof_identifier_narrowed_by_typeof_guard_in_type_alias() {
    let codes = get_error_codes(
        r#"
declare let c: string | number;
if (typeof c === "string") {
    type C = typeof c;
    const bad: C = 1;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "Should emit TS2322 when narrowed typeof c is string inside type alias, got: {codes:?}"
    );
}

/// `typeof c` in a variable annotation inside a narrowed branch should also see the narrowed type.
#[test]
fn test_typeof_identifier_narrowed_by_typeof_guard_in_variable_annotation() {
    let codes = get_error_codes(
        r#"
declare let c: string | number;
if (typeof c === "string") {
    const bad: typeof c = 1;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "Should emit TS2322 when narrowed typeof c is string in variable annotation, got: {codes:?}"
    );
}
