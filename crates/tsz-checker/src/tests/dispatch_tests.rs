use crate::context::{CheckerOptions, ScriptTarget};
use crate::test_utils::check_js_source_diagnostics;
use crate::test_utils::check_source;
use crate::test_utils::check_source_diagnostics;
use tsz_common::checker_options::JsxMode;

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

    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Type assertions should not emit nested TS2353 from array elements, got: {diags:?}"
    );
}

#[test]
fn ts2352_array_assertion_with_best_common_type_does_not_emit_ts2353() {
    let diags = check_source_diagnostics(
        r#"
<{ id: number; }[]>[{ foo: "s" }, {}];
"#,
    );

    assert!(
        diags.is_empty(),
        "Expected no diagnostics when array assertion falls back to best common type, got: {diags:?}"
    );
}

#[test]
fn ts2352_merged_class_namespace_record_cast_reports_missing_string_index() {
    let diags = check_source_diagnostics(
        r#"
type Dict = { [key: string]: unknown };
class C1 { foo() {} }
new C1() as Dict;

class C2 { foo() {} }
namespace C2 { export const unrelated = 3; }
new C2() as Dict;

namespace C3 { export const unrelated = 3; }
C3 as Dict;
"#,
    );

    let ts2352: Vec<_> = diags.iter().filter(|d| d.code == 2352).collect();
    assert_eq!(
        ts2352.len(),
        2,
        "Expected exactly two TS2352 diagnostics, got: {diags:?}"
    );
    assert!(
        ts2352
            .iter()
            .all(|diag| diag.message_text.contains("Conversion of type")),
        "Expected TS2352 conversion diagnostics for the class assertions, got: {ts2352:?}"
    );
}

#[test]
fn ts2339_property_access_anchors_property_token() {
    let source = r#"
declare const value: {};
value.missing;
"#;

    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(matching.len(), 1, "Expected one TS2339, got: {diags:?}");

    let missing_pos = source.find("missing").expect("expected property token") as u32;
    assert_eq!(
        matching[0].start, missing_pos,
        "Expected TS2339 to anchor at the property token, got: {matching:?}"
    );
    assert_eq!(
        matching[0].length, 7,
        "Expected TS2339 to cover only the property token"
    );
}

#[test]
fn ts7053_element_access_anchors_full_expression() {
    let source = r#"
declare const key: string;
declare const value: {};
value[key];
"#;

    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 7053).collect();
    assert_eq!(matching.len(), 1, "Expected one TS7053, got: {diags:?}");

    let expr_pos = source.find("value[key]").expect("expected element access") as u32;
    assert_eq!(
        matching[0].start, expr_pos,
        "Expected TS7053 to anchor at the full element access expression, got: {matching:?}"
    );
}

#[test]
fn ts7015_number_index_error_anchors_index_argument() {
    let source = r#"
declare const arr: number[];
arr["name"];
"#;

    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 7015).collect();
    assert_eq!(matching.len(), 1, "Expected one TS7015, got: {diags:?}");

    let index_pos = source.find("\"name\"").expect("expected string index") as u32;
    assert_eq!(
        matching[0].start, index_pos,
        "Expected TS7015 to anchor at the index argument, got: {matching:?}"
    );
}

#[test]
fn ts2345_never_parameter_uses_non_contextual_object_literal_display() {
    let diags = check_source_diagnostics(
        r#"
declare function fn(x: never): void;
fn({ a: 1, b: 2 });
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(matching.len(), 1, "Expected one TS2345, got: {diags:?}");

    let msg = &matching[0].message_text;
    // tsc widens literal types in object literal display for diagnostics
    assert!(
        msg.contains("Argument of type '{ a: number; b: number; }'"),
        "Expected widened object literal display (matching tsc), got: {msg}"
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
fn contextual_request_does_not_leak_between_sibling_properties() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: {
    left: (s: string) => void;
    right: (n: number) => void;
}): void;

takes({
    left: s => s.toUpperCase(),
    right: n => n.toFixed(),
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected no contextual leak diagnostics, got: {relevant:?}"
    );
}

#[test]
fn write_context_access_does_not_reuse_read_cache() {
    let diags = check_source_diagnostics(
        r#"
declare const access: {
    get value(): undefined;
    set value(v: number);
};

const read1: undefined = access.value;
access.value = 1;
const read2: undefined = access["value"];
access["value"] = 1;
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2540)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected write-context accesses to use setter/write types, got: {relevant:?}"
    );
}

#[test]
fn assertion_origin_does_not_leak_outside_asserted_expression() {
    let diags = check_source_diagnostics(
        r#"
const asserted = ((x) => 1) as (x: string) => string;
const assigned: (x: string) => string = (x) => 1;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one assignment-context TS2322, got: {diags:?}"
    );
    assert_eq!(
        ts7006.len(),
        0,
        "Expected asserted expression parameters to stay contextually typed, got: {diags:?}"
    );
}

#[test]
fn speculative_overload_check_does_not_poison_successful_candidate() {
    let diags = check_source_diagnostics(
        r#"
declare function fn(cb: (s: number) => void): void;
declare function fn(cb: (s: string) => void): void;

fn(s => s.toUpperCase());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected speculative overload rollback to avoid poisoning the successful candidate, got: {relevant:?}"
    );
}

#[test]
fn nested_object_literal_context_is_preserved_without_ambient_restore() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: {
    outer: {
        onText: (s: string) => void;
        nested: { onNumber: (n: number) => void };
    };
}): void;

takes({
    outer: {
        onText: s => s.toUpperCase(),
        nested: {
            onNumber: n => n.toFixed(),
        },
    },
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected nested object literal contextual typing to stay isolated, got: {relevant:?}"
    );
}

#[test]
fn iife_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(arg: { cb: (s: string) => void }): void;

takes((() => ({
    cb: s => s.toUpperCase(),
}))());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected no IIFE contextual-typing regressions, got: {relevant:?}"
    );
}

#[test]
fn jsx_children_and_props_use_request_path() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
}

declare function Comp(props: { render: (s: string) => JSX.Element }): JSX.Element;

<Comp render={s => { s.toUpperCase(); return <div />; }} />;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSX request-path contextual typing to work, got: {relevant:?}"
    );
}

#[test]
fn destructuring_request_path_stays_stable_in_switch_parameter_and_variable_positions() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(cb: (arg: { value: string }) => string): void;

switch (0) {
    case 0: {
        const inferred = ({ value = "ok" } = {}) => value;
        const annotated: typeof inferred = ({ value = "ok" } = {}) => value;
        takes(({ value = "x" }) => value.toUpperCase());
        break;
    }
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected destructuring request transport to survive switch/parameter/variable paths, got: {relevant:?}"
    );
}

#[test]
fn destructuring_parameter_declaration_preserves_nested_binding_context() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(fn: ([a, b, [[c]], ...x]: [number, number, [[string]], boolean, boolean]) => void): void;

takes(([a, b, [[c]], ...x]) => {
    a.toFixed();
    b.toFixed();
    c.toUpperCase();
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339 | 7031))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected destructuring parameter bindings to stay request-aware, got: {relevant:?}"
    );
}

#[test]
fn catch_finally_and_logical_assignment_preserve_request_intent() {
    let diags = check_source_diagnostics(
        r#"
let box: { text?: string } = {};

try {
    box.text ||= "x";
} catch ({ message = "err" }) {
    message.toUpperCase();
} finally {
    box.text &&= box.text.trim();
}

box.text = box.text || "ok";
box.text!.toUpperCase();
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected catch/finally and logical assignment request flow to stay stable, got: {relevant:?}"
    );
}

#[test]
fn nonnull_assertion_context_stays_local_to_asserted_expression() {
    let diags = check_source_diagnostics(
        r#"
const ok: (s: string) => string = ((x) => x)!;
const bad: (s: string) => number = x => x;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one non-null-containment TS2322, got: {diags:?}"
    );
    assert_eq!(
        ts7006.len(),
        0,
        "Expected non-null assertion contextual typing to stay local, got: {diags:?}"
    );
}

#[test]
fn generic_contextual_function_inference_uses_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function mapValue<T, U>(value: T, fn: (x: T) => U): U;

const result = mapValue({ text: "ok" }, ({ text }) => text.toUpperCase());
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 7031 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic contextual inference to remain request-aware, got: {relevant:?}"
    );
}

#[test]
fn generic_mapped_method_contextual_typing_uses_request_path() {
    let diags = check_source(
        r#"
declare function f<T extends object>(
    data: T,
    handlers: { [P in keyof T]: (value: T[P], prop: P) => void },
): void;

f({ data: 0 }, {
    data(value, key) {
        value.toFixed();
        key.toUpperCase();
    },
});
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic mapped method shorthand to stay contextually typed, got: {relevant:?}"
    );
}

#[test]
fn computed_mapped_callback_context_uses_callable_fallback() {
    let diags = check_source(
        r#"
declare function tag(): "d";

declare function forceMatch<T>(matched: {
    [K in keyof T]: ({ key }: { key: K }) => void;
}): void;

forceMatch({
    [tag()]: ({ key }) => {
        const exact: "d" = key;
    },
});
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7031 | 7006 | 2322))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected computed mapped callbacks to keep callable context, got: {relevant:?}"
    );
}

#[test]
fn return_context_substitution_preserves_rest_tuple_callback_args() {
    let diags = check_source(
        r#"
interface Generator<Y, R, N> {}
type Covariant<A> = (_: never) => A;
interface Effect<out A> {
    readonly _A: Covariant<A>;
}

declare function lift<AEff, Args extends Array<any>>(
    body: (...args: Args) => Generator<never, AEff, never>,
): (...args: Args) => Effect<AEff>;

declare function takes(handler: (a: string) => Effect<void>): void;

takes(lift(function* (a) {
    a.toUpperCase();
}));
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected return-context substitution to preserve rest-tuple callback args, got: {relevant:?}"
    );
}

#[test]
fn contextual_this_for_class_expression_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function takes(ctor: new () => { value: string; read(): string }): void;

takes(class {
    value = "ok";
    read() {
        return this.value.toUpperCase();
    }
});
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2339 | 2683))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected contextual `this` in class expressions to use request transport, got: {relevant:?}"
    );
}

#[test]
fn jsx_children_contextual_typing_uses_request_path() {
    let diags = check_source(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
    interface ElementChildrenAttribute {
        children: {};
    }
}

declare function Panel(props: { children: (s: string) => JSX.Element }): JSX.Element;

<Panel>{s => { s.toUpperCase(); return <div />; }}</Panel>;
"#,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSX children contextual typing to stay on the request path, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_template_and_param_resolution_stay_stable_through_request_path() {
    let diags = check_source(
        r#"
/** @template T
 * @param {(value: T) => T} fn
 * @param {T} value
 */
function apply(fn, value) {
    return fn(value);
}

/** @template T */
class Box {
    /** @param {T} value */
    constructor(value) {
        this.value = value;
    }
}

/** @param {{ text: string }} value */
const useText = (value) => value.text.toUpperCase();

apply(useText, { text: "ok" });
new Box("ok");
"#,
        "test.js",
        CheckerOptions::default(),
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 7006 | 7031 | 2304 | 2314 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JSDoc template/param resolution to stay stable, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_generic_callback_typedef_type_tag_resolves_as_callable() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @template T
 * @callback B
 * @returns {T}
 */

/** @type {B<string>} */
let b = {};

b();
b(1);
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for assigning {{}} to generic callback typedef, got: {codes:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2322 && d.message_text.contains("B<string>")),
        "Expected TS2322 to preserve the instantiated JSDoc callback alias in the message, got: {diags:?}"
    );
    assert!(
        codes.contains(&2554),
        "Expected TS2554 for calling instantiated callback typedef with an extra arg, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2349),
        "Expected instantiated callback typedef to stay callable, got: {codes:?}"
    );
}

#[test]
fn jsdoc_callback_nested_params_build_one_object_parameter() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @callback WorksWithPeopleCallback
 * @param {Object} person
 * @param {string} person.name
 * @param {number} [person.age]
 * @returns {void}
 */

/**
 * @param {WorksWithPeopleCallback} callback
 * @returns {void}
 */
function eachPerson(callback) {
    callback({ name: "Empty" });
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2554 || d.code == 2345)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected nested callback params to shape a single object parameter, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_optional_properties_stay_optional_in_param_tags() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} Opts
 * @property {string} x
 * @property {string=} y
 * @property {string} [z]
 * @property {string} [w="hi"]
 *
 * @param {Opts} opts
 */
function foo(opts) {
    opts.x;
}

foo({ x: "abc" });
"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected optional typedef properties to stay optional at param-tag call sites, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_property_name_then_type_syntax_stays_optional_in_param_tags() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} AnotherOpts
 * @property anotherX {string}
 * @property anotherY {string=}
 *
 * @param {AnotherOpts} opts
 */
function foo(opts) {
    opts.anotherX;
}

foo({ anotherX: "world" });
"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected alternate @property name {{type}} syntax to preserve optionality at param-tag call sites, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_prop_alias_uses_same_property_parser() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @typedef {Object} AliasOpts
 * @prop aliasX {string}
 * @prop [aliasY="hi"] {string}
 *
 * @param {AliasOpts} opts
 */
function foo(opts) {
    opts.aliasX;
}

foo({ aliasX: "world" });
"#,
    );
    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected @prop alias tags to share typedef property parsing semantics, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_constructor_template_scope_flows_to_prototype_methods() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @constructor
 * @template {string} K
 * @template V
 */
function Multimap() {
    /** @type {Object<string, V>} */
    this._map = {};
}

Multimap.prototype = {
    /**
     * @param {K} key
     * @returns {V}
     */
    get(key) {
        return this._map[key + ""];
    }
};

/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    /** @type {T} */
    this.u;
    this.t = t;
}

/**
 * @param {T} v
 * @param {object} o
 * @param {T} o.nested
 */
Zet.prototype.add = function(v, o) {
    this.u = v || o.nested;
    return this.u;
};

/** @type {number} */
let answer = new Zet(1).add(3, { nested: 4 });
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2339 | 7006 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected constructor @template scope to flow to prototype methods, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_generic_constructor_prototype_object_literal_methods_use_instance_this() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @class
 * @template T
 * @param {T} t
 */
function Cp(t) {
    this.x = 1;
    this.y = t;
}
Cp.prototype = {
    m1() { return this.x; },
    m2() { this.z = this.x + 1; return this.y; }
};
var cp = new Cp(1);

/** @type {number} */
var n = cp.x;
/** @type {number} */
var n = cp.y;
/** @type {number} */
var n = cp.m1();
/** @type {number} */
var n = cp.m2();
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected generic JS constructor prototype object literal methods to use instance `this`, got: {relevant:?}"
    );
}

#[test]
fn jsdoc_typedef_property_unknown_template_name_emits_ts2304() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    this.t = t;
}

/**
 * @typedef {Object} A
 * @property {T} value
 */
/** @type {A} */
const options = { value: null };
"#,
    );
    let ts2304: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert_eq!(
        ts2304.len(),
        1,
        "Expected one TS2304 for out-of-scope typedef property template name, got: {diags:?}"
    );
}

#[test]
fn tagged_template_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
declare function tag(strs: TemplateStringsArray, f: (n: number) => void): void;

tag`${n => n.toFixed()}`;
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected tagged-template contextual typing to stay on the request path, got: {relevant:?}"
    );
}

#[test]
fn yield_contextual_typing_flows_through_request_path() {
    let diags = check_source_diagnostics(
        r#"
interface Generator<Y, R, N> {}

function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 7006 || d.code == 2339)
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected yield contextual typing to use request path, got: {relevant:?}"
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

#[test]
fn ts2322_typeof_in_type_alias_respects_control_flow_narrowing() {
    // When `typeof c` appears inside a type alias within a narrowed scope,
    // the flow-narrowed type should be used (string, not string | number).
    // This ensures `{ bar: 1 }` is rejected when assigned to type C which
    // has `[key: string]: typeof c` where c has been narrowed to string.
    let diags = check_source_diagnostics(
        r#"
declare let c: string | number;
if (typeof c === 'string') {
    type C = { [key: string]: typeof c };
    const boo1: C = { bar: 'works' };
    const boo2: C = { bar: 1 };
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for number not assignable to string, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn reverse_mapped_tuple_inference_through_conditional_template() {
    // When a mapped type's template is a conditional type like
    // `Tuple[Key] extends Tuple[number] ? MyMappedType<Tuple[Key]> : never`,
    // reverse-mapped inference should be able to reverse through the
    // conditional's true branch to infer Tuple from the argument types.
    // Regression test: previously, reverse_infer_through_template returned
    // None for conditional templates, causing Tuple to default to any[].
    let diags = check_source_diagnostics(
        r#"
type MyMappedType<Primitive extends any> = {
    primitive: Primitive;
};
type TupleMapper<Tuple extends any[]> = {
    [Key in keyof Tuple]: Tuple[Key] extends Tuple[number] ? MyMappedType<Tuple[Key]> : never;
};
declare function extractPrimitives<Tuple extends any[]>(...mappedTypes: TupleMapper<Tuple>): Tuple;
const result: [string, number] = extractPrimitives({ primitive: "" }, { primitive: 0 });
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for reverse-mapped tuple inference through conditional template, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts7006_emitted_for_intra_binding_pattern_reference() {
    // When a destructuring binding element's default references another binding in the
    // same pattern (intra-binding-pattern reference), the contextual type for that
    // property should not flow to the RHS object literal. This matches tsc behavior
    // (TypeScript#59177): `fn2 = fn1` references `fn1` from the same pattern, so the
    // contextual type for `fn2: x => x + 2` is absent and TS7006 fires for `x`.
    let diags = check_source_diagnostics(
        r#"
const { fn1 = (x: number) => 0, fn2 = fn1 } = { fn1: x => x + 1, fn2: x => x + 2 };
"#,
    );
    let ts7006: Vec<_> = diags.iter().filter(|d| d.code == 7006).collect();
    assert_eq!(
        ts7006.len(),
        1,
        "Expected exactly 1 TS7006 for 'x' in fn2's arrow (intra-binding ref), got: {:?}",
        ts7006.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn ts2352_tuple_different_length_assertion() {
    // Same-length tuples with incompatible element types
    let diags = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, number];"#,
    );
    assert_eq!(
        diags.iter().filter(|d| d.code == 2352).count(),
        1,
        "Expected TS2352 for [number, string] as [number, number]"
    );

    // Different-length tuples (shorter to longer)
    let diags2 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, string, boolean];"#,
    );
    assert_eq!(
        diags2.iter().filter(|d| d.code == 2352).count(),
        1,
        "Expected TS2352 for [number, string] as [number, string, boolean]"
    );

    // Angle bracket syntax
    let diags3 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = <[number, string, boolean]>x;"#,
    );
    assert_eq!(
        diags3.iter().filter(|d| d.code == 2352).count(),
        1,
        "Expected TS2352 for <[number, string, boolean]>x"
    );
}

// =============================================================================
// Property access narrowing (this.X after equality checks)
// =============================================================================

#[test]
fn no_false_ts2322_typeof_this_property_after_equality_narrowing() {
    // After `if (this.no === 1)`, both `typeof this.no` and `this.no` in value
    // position should be narrowed to `1`. Without property access narrowing,
    // `typeof this.no` resolves to `1` but `this.no` stays `number`, causing
    // a spurious TS2322: "Type 'number' is not assignable to type '1'".
    let diags = check_source(
        r#"
class Test9 {
    no = 0;

    g() {
        if (this.no === 1) {
            const no: typeof this.no = this.no;
        }
    }
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.no = this.no` inside equality guard, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn no_false_ts2322_typeof_this_property_named_this_after_equality_narrowing() {
    // Same test but for a property literally named `this` — the property access
    // `this.this` should also be narrowed after `if (this.this === 1)`.
    let diags = check_source(
        r#"
class Test9 {
    this = 0;

    g() {
        if (this.this === 1) {
            const no: typeof this.this = this.this;
        }
    }
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.this = this.this` inside equality guard, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}
