use crate::test_utils::check_source_diagnostics;

#[test]
fn ts7006_false_positive_arrow_in_generic_call() {
    // Arrow functions in object literal properties within generic indexed-access
    // calls should receive contextual typing from the inferred type parameter.
    // This tests that TS7006 is NOT falsely emitted for `r` in `callback: (r) => {}`.
    let diags = check_source_diagnostics(
        r#"
type Events = {
    a: { callback: (r: string) => void }
};
declare function emit<T extends keyof Events>(type: T, data: Events[T]): void;
emit('a', {
    callback: (r) => {},
});
"#,
    );
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        0,
        "Expected no TS7006 for contextually-typed arrow param, got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2352_this_type_assertion_in_class() {
    let diags = check_source_diagnostics(
        r#"
class C5 {
    bar() {
        let x1 = <this>undefined;
        let x2 = undefined as this;
    }
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        matching.len(),
        2,
        "Expected 2 TS2352 for this type assertions, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2352_angle_bracket_type_display_no_trailing_gt() {
    // For `<T>expr`, the type node span may include `>` — verify it's stripped
    let diags = check_source_diagnostics(
        r#"
class A { foo() { return ""; } }
class B extends A { bar() { return 1; } }
function foo2<T extends A>(x: T) {
    var y = x;
    y = <T>new B();
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2352, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    // Verify message says "type 'T'" not "type 'T>'"
    let msg = &matching[0].message_text;
    assert!(
        msg.contains("to type 'T'"),
        "Expected 'to type 'T'' in message, got: {msg}"
    );
}

#[test]
fn ts2352_this_type_assertion_static_no_error() {
    // In static context, `this` is invalid (TS2526), so TS2352 should not fire
    let diags = check_source_diagnostics(
        r#"
class C2 {
    static y = <this>undefined;
}
"#,
    );
    let ts2352: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        ts2352.len(),
        0,
        "Expected no TS2352 in static context, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2352_structured_target_with_type_parameter_still_reports() {
    let diags = check_source_diagnostics(
        r#"
function f<T>() {
    const x = <T[]>null;
}
"#,
    );
    // Filter out TS2318 "Cannot find global type" from missing lib declarations.
    let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
    let matching: Vec<_> = relevant.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected one TS2352 for `null as T[]`, got: {relevant:?}"
    );
    assert!(
        matching[0].message_text.contains("type 'T[]'"),
        "Expected TS2352 target display to preserve `T[]`, got: {:?}",
        matching[0]
    );
}

#[test]
fn ts2352_array_assertion_anchors_first_excess_property() {
    let source = r#"
<{ id: number; }[]>[{ foo: "s" }];
"#;
    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(matching.len(), 1, "Expected one TS2352, got: {diags:?}");

    let foo_pos = source.find("foo").expect("expected foo property") as u32;
    assert_eq!(
        matching[0].start, foo_pos,
        "Expected TS2352 to anchor at the excess property name, got: {matching:?}"
    );
}

#[test]
fn ts2345_never_parameter_uses_widened_object_literal_display() {
    let diags = check_source_diagnostics(
        r#"
declare function fn(x: never): void;
fn({ a: 1, b: 2 });
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(matching.len(), 1, "Expected one TS2345, got: {diags:?}");

    let msg = &matching[0].message_text;
    // tsc widens literal types in assignability error messages:
    // `{ a: 1, b: 2 }` displays as `{ a: number; b: number; }`
    assert!(
        msg.contains("Argument of type '{ a: number; b: number; }'"),
        "Expected widened object literal display (tsc behavior), got: {msg}"
    );
    assert!(
        msg.contains("parameter of type 'never'"),
        "Expected never parameter display, got: {msg}"
    );
}

#[test]
fn type_params_in_object_literal_methods_no_ts2304() {
    // Type parameters in object literal method shorthands must be in scope
    // for parameter types, return types, and body type references.
    let diags = check_source_diagnostics(
        r#"
let a = {
    test<K>(x: K): K { return x; }
};
interface Bar { bar: number; }
let b = {
    test<K extends keyof Bar>(a: K, b: Bar[K]) { }
};
"#,
    );
    let ts2304: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert_eq!(
        ts2304.len(),
        0,
        "Expected no TS2304 for type params in object literal methods, got: {:?}",
        ts2304.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn class_namespace_merge_same_file_no_ts2351() {
    // Same-file class+namespace merge: `new A()` inside `namespace A` should
    // resolve to the class constructor, not produce TS2351.
    let diags = check_source_diagnostics(
        r#"
class A {
    id: string;
}
namespace A {
    export var Instance = new A();
}
"#,
    );
    let ts2351: Vec<_> = diags.iter().filter(|d| d.code == 2351).collect();
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for class+namespace merge, got: {:?}",
        ts2351.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn arrow_expression_body_literal_union_return_no_false_ts2322() {
    // Concise arrow `() => "bar"` assigned to a variable with type `() => "foo" | "bar"`
    // should NOT emit TS2322 — "bar" is a member of the union "foo" | "bar".
    let diags = check_source_diagnostics(
        r#"
type FnType = () => "foo" | "bar";
const f2: FnType = () => "bar";
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for literal arrow return assignable to union, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn dotted_namespace_class_merge_same_file_no_ts2351() {
    // Dotted namespace `X.Y` with class+namespace merge in same file.
    let diags = check_source_diagnostics(
        r#"
namespace X.Y {
    export class Point {
        constructor(x: number, y: number) {
            this.x = x;
            this.y = y;
        }
        x: number;
        y: number;
    }
}
namespace X.Y {
    export namespace Point {
        export var Origin = new Point(0, 0);
    }
}
"#,
    );
    let ts2351: Vec<_> = diags.iter().filter(|d| d.code == 2351).collect();
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for dotted namespace class merge, got: {:?}",
        ts2351.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2540_as_const_object_method_this_readonly() {
    // When an object literal is declared `as const`, `this` inside methods
    // should see readonly properties.  Assigning to `this.x` must produce
    // TS2540 ("Cannot assign to 'x' because it is a read-only property"),
    // not TS2322 ("Type '20' is not assignable to type '10'").
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { this.x = 20 } } as const;
"#,
    );
    let ts2540: Vec<_> = diags.iter().filter(|d| d.code == 2540).collect();
    assert_eq!(
        ts2540.len(),
        1,
        "Expected 1 TS2540 for readonly property assignment via this in as-const object, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    // Must NOT emit TS2322 — the readonly check takes precedence.
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when TS2540 (readonly) applies, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2540_as_const_object_method_this_readonly_no_false_positive() {
    // Reading from `this.x` inside an as-const method should NOT produce
    // any error — only writes should trigger TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { return this.x } } as const;
"#,
    );
    let ts2540: Vec<_> = diags.iter().filter(|d| d.code == 2540).collect();
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 for readonly property read, got: {:?}",
        ts2540.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2540_as_const_nested_method_this_readonly() {
    // Multiple properties in an as-const object with a method that assigns
    // to different properties should all produce TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = {
    x: 10,
    y: "hello",
    foo() {
        this.x = 20;
        this.y = "world";
    }
} as const;
"#,
    );
    let ts2540: Vec<_> = diags.iter().filter(|d| d.code == 2540).collect();
    assert_eq!(
        ts2540.len(),
        2,
        "Expected 2 TS2540 for readonly property assignments in as-const method, got codes: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_ts2540_without_const_assertion() {
    // Without `as const`, properties are mutable, so `this.x = 20` should
    // NOT produce TS2540.
    let diags = check_source_diagnostics(
        r#"
let o = { x: 10, foo() { this.x = 20 } };
"#,
    );
    let ts2540: Vec<_> = diags.iter().filter(|d| d.code == 2540).collect();
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 without as-const, got: {:?}",
        ts2540.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
