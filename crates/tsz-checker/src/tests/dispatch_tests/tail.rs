use crate::test_utils::check_source_diagnostics;

#[test]
fn ts2352_string_enum_comparable_in_nested_assertion() {
    // Repro from comparableRelationBidirectional.ts:
    // When asserting an object literal `as UserSettings` where a nested property
    // has a string enum type, the comparable relation should recognize overlap
    // between the string literal `""` and the string enum `AutomationMode` (which
    // has NONE = ""). TS2352 should NOT fire because the types overlap at the
    // property level even though direct assignability fails (string enums are
    // nominally strict for assignments but comparable for type assertions).
    let diags = check_source_diagnostics(
        r#"
enum AutomationMode {
    NONE = "",
    TIME = "time",
    SYSTEM = "system",
    LOCATION = "location",
}
interface Automation {
    mode: AutomationMode;
}
interface UserSettings {
    presets: string[];
    automation: Automation;
}
const x = {
    presets: [],
    automation: {
        mode: "",
    },
} as UserSettings;
"#,
    );
    let ts2352: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        ts2352.len(),
        0,
        "Expected no TS2352 for string enum comparable assertion, got: {:?}",
        ts2352.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn unknown_array_destructuring_ts2571_anchors_only_empty_pattern() {
    let source = r#"
declare function f<T>(): T;
const [] = f();
const [e1, e2] = f();
"#;
    let diags = check_source_diagnostics(source);

    let ts2571: Vec<_> = diags.iter().filter(|d| d.code == 2571).collect();
    assert_eq!(
        ts2571.len(),
        1,
        "Expected exactly one TS2571 for unknown array destructuring, got: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );

    let empty_start = source.find("[]").expect("expected empty array pattern") as u32;
    assert_eq!(
        ts2571[0].start, empty_start,
        "TS2571 should anchor at the empty array pattern"
    );

    let ts2488: Vec<_> = diags.iter().filter(|d| d.code == 2488).collect();
    assert_eq!(
        ts2488.len(),
        2,
        "Expected TS2488 on both unknown array destructuring patterns, got: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

#[test]
fn catch_array_destructuring_unknown_suppresses_ts2571() {
    let diags = check_source_diagnostics(
        r#"
try {} catch ([x]) {}
"#,
    );

    let ts2571: Vec<_> = diags.iter().filter(|d| d.code == 2571).collect();
    assert_eq!(
        ts2571.len(),
        0,
        "Expected no TS2571 for catch-clause array destructuring, got: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
    let ts2488: Vec<_> = diags.iter().filter(|d| d.code == 2488).collect();
    assert_eq!(
        ts2488.len(),
        1,
        "Expected TS2488 for catch-clause array destructuring, got: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

#[test]
fn interface_with_construct_signature_no_ts2351() {
    // An interface with a construct signature (like ProxyConstructor) should
    // be constructable via `new` without TS2351.
    let diags = check_source_diagnostics(
        r#"
interface MyHandler<T extends object> {
    get?(target: T, p: string): any;
}
interface MyConstructor {
    new <T extends object>(target: T, handler: MyHandler<T>): T;
}
declare var MyProxy: MyConstructor;
var t: object = {};
var p = new MyProxy(t, {});
"#,
    );
    let ts2351: Vec<_> = diags.iter().filter(|d| d.code == 2351).collect();
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for interface with construct signature, got: {:?}",
        ts2351.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn no_false_ts2339_on_generic_class_self_referencing_parameter() {
    // Regression test: property access on a generic class type used as a
    // parameter type within the same class's method should not produce false
    // TS2339 errors. The class instance type cache must not be corrupted by
    // ERROR values during re-entrant class checking.
    //
    // Matches tsc behavior for genericClasses4.ts: no errors expected.
    let diags = check_source_diagnostics(
        r#"
class Vec2_T<A> {
    constructor(public x: A, public y: A) { }
    fmap<B>(f: (a: A) => B): Vec2_T<B> {
        var x:B = f(this.x);
        var y:B = f(this.y);
        var retval: Vec2_T<B> = new Vec2_T(x, y);
        return retval;
    }
    apply<B>(f: Vec2_T<(a: A) => B>): Vec2_T<B> {
        var x:B = f.x(this.x);
        var y:B = f.y(this.y);
        var retval: Vec2_T<B> = new Vec2_T(x, y);
        return retval;
    }
}
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on generic class self-reference, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn no_false_ts2339_on_class_param_with_same_class_type() {
    // A method that takes a parameter of the same class type should be able to
    // access properties on that parameter, even when another method returns
    // the same class type (triggering class instance type cache invalidation).
    let diags = check_source_diagnostics(
        r#"
class Foo<A> {
    constructor(public x: A) {}
    bar(): Foo<any> { return this; }
    test(f: Foo<string>): void {
        let v = f.x;
    }
}
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for f.x where f: Foo<string>, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn getter_returning_this_no_false_ts2339() {
    // When a class getter returns `this` without an explicit type annotation,
    // the inferred return type must be the polymorphic `ThisType` — not the
    // partial class instance type. Without the syntactic `returns_only_this`
    // fallback, return-type widening (ObjectWithIndex → Object) can produce
    // a TypeId mismatch, causing the getter property to be omitted from the
    // final class instance type and triggering false TS2339 errors.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    constructor() {}
    get y() { return this; }
    z: T;
}
declare var c: C<string>;
var r = c.y;
r.y;
r.z;
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter returning this, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn no_false_ts2339_for_getter_this_type_after_constructor() {
    // Getter returning `this` declared after constructor should not produce
    // false TS2339 when the getter's return type is accessed through a variable.
    // Previously, the cached_instance_this_type in enclosing_class was stale
    // (set to the Phase 0 prescan type), causing `this` in the getter body to
    // resolve to a partial type missing the getter property itself.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    x = this;
    constructor(x: T) {}
    get y() { return this; }
    z: T;
}

declare var c: C<string>;
var r2 = c.y;
r2.y;
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for r2.y where r2 = c.y, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn getter_returning_this_after_constructor_resolves_to_this_type() {
    // When a getter that returns `this` is declared after the constructor,
    // the inferred return type might not match the Phase 3 partial type by
    // TypeId equality. The syntactic `method_body_returns_only_this` fallback
    // ensures the getter still gets polymorphic `ThisType`, so that accessing
    // getter properties on the result works correctly.
    let diags = check_source_diagnostics(
        r#"
class C<T> {
    foo() { return this; }
    constructor(x: T) {
        this.z = x;
    }
    get y() { return this; }
    z: T;
}

var c: C<string> = new C("hello");
// Getter result should have all class members including y itself
var result = c.y;
result.y;
result.foo;
result.z;

// Method result should also have getter y
var r2 = c.foo();
r2.y;
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter `this` return type on class with getter after constructor, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn enum_in_namespace_typeof_property_access() {
    // When accessing an enum export through a typeof namespace variable,
    // the enum should resolve to its namespace type (with member properties)
    // not the enum instance type (the union of enum values).
    // This is the pattern from conformance test `instantiatedModule.ts`.
    let diags = check_source_diagnostics(
        r#"
namespace M3 {
    export enum Color { Blue, Red }
}
var m3: typeof M3;
var m3 = M3;
var a3: typeof M3.Color;
var a3 = m3.Color;
var a3 = M3.Color;
var blue: M3.Color = a3.Blue;
var p3: M3.Color;
var p3 = M3.Color.Red;
var p3 = m3.Color.Blue;
"#,
    );
    // TS2339: Property does not exist on type
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for enum member access through typeof namespace, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
    // TS2403: Subsequent variable declarations must have the same type
    let ts2403: Vec<_> = diags.iter().filter(|d| d.code == 2403).collect();
    assert_eq!(
        ts2403.len(),
        0,
        "Expected no TS2403 for enum typeof mismatch, got: {:?}",
        ts2403.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2345_readonly_array_preserves_readonly_in_message() {
    // When a readonly array is passed where a mutable array is expected,
    // the TS2345 message should display 'readonly number[]' not 'number[]'.
    let diags = check_source_diagnostics(
        r#"
declare const a: readonly number[];
declare function fn(x: number[]): void;
fn(a);
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(matching.len(), 1, "Expected one TS2345, got: {diags:?}");

    let msg = &matching[0].message_text;
    assert!(
        msg.contains("'readonly number[]'"),
        "Expected 'readonly number[]' in TS2345 message, got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'number[]'"),
        "Expected 'number[]' as target type, got: {msg}"
    );
}

#[test]
fn no_ts2339_for_computed_property_with_circular_class_reference() {
    let diags = check_source_diagnostics(
        r#"
declare const rC: RC<"a">;
rC.x;
declare class RC<T extends "a" | "b"> {
    x: T;
    [rC.x]: "b";
}
"#,
    );
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on class with circular computed property, got: {:?}",
        ts2339.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
