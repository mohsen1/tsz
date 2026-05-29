#[test]
fn jsdoc_type_tag_with_generic_interface_preserves_args_in_diagnostic() {
    // Regression: assignability messages must preserve `Name<Args>` for
    // generic interface/class refs (not just `@typedef`s) referenced from
    // a JSDoc `@type` annotation. See: subclassThisTypeAssignable01
    // conformance test where
    // `/** @type {ClassComponent<any>} */ const test9 = new C();`
    // previously produced "...is not assignable to type 'ClassComponent'."
    // instead of "...is not assignable to type 'ClassComponent<any>'."
    use crate::CheckerOptions;
    use crate::test_utils::check_source;
    let diags = check_source(
        r#"
interface Box<T> { value: T }
class C { constructor() { this.q = 1; } }

/** @type {Box<string>} */
const b = new C();
"#,
        "test.ts",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    // Must mention the instantiated alias name with type arguments.
    let assignability_codes = [2322u32, 2741];
    assert!(
        diags.iter().any(
            |d| assignability_codes.contains(&d.code) && d.message_text.contains("Box<string>")
        ),
        "Expected an assignability message to mention `Box<string>`, got: {diags:?}"
    );
    // Must NOT show the bare `Box` (without type arguments) in any
    // assignability-class diagnostic.
    let has_bare = diags.iter().any(|d| {
        assignability_codes.contains(&d.code)
            && d.message_text.contains(" 'Box'")
            && !d.message_text.contains("Box<")
    });
    assert!(
        !has_bare,
        "Expected no assignability message to show bare `Box`, got: {diags:?}"
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
    let relevant = diagnostics_with_code(&diags, 2345);
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
    let relevant = diagnostics_with_code(&diags, 2345);
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
    let relevant = diagnostics_with_code(&diags, 2345);
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
fn jsdoc_constructor_identifier_argument_uses_typeof_source_display() {
    let diags = check_js_source_diagnostics(
        r#"
/**
 * @param {function(new: { length: number }, number): number} c
 * @return {function(new: { length: number }, number): number}
 */
function id2(c) {
    return c;
}

/**
 * @constructor
 * @param {number} n
 */
var E = function(n) {
  this.not_length_on_purpose = n;
};

id2(E);
"#,
    );
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(ts2345.len(), 1, "Expected one TS2345, got: {diags:?}");
    let message = &ts2345[0].message_text;
    assert!(
        message.contains("Argument of type 'typeof E'"),
        "Expected JS constructor identifier source display to use `typeof E`, got: {message:?}"
    );
    assert!(
        !message.contains("new (n: number)"),
        "Expected diagnostic not to expand the constructor signature, got: {message:?}"
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
    let ts2304 = diagnostics_with_code(&diags, 2304);
    assert_eq!(
        ts2304.len(),
        1,
        "Expected one TS2304 for out-of-scope typedef property template name, got: {diags:?}"
    );
}

#[test]
fn jsdoc_broken_typedef_body_recovers_alias_as_any() {
    let diags = check_js_source_diagnostics(
        r#"
/** @typedef {U} T */
/**
 * @returns {T}
 */
function f() {
    return 1;
}
/** @type {T} */
const x = 3;
"#,
    );
    let ts2304_messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| d.message_text.to_string())
        .collect();
    assert!(
        ts2304_messages.iter().any(|m| m.contains("'U'")),
        "Expected TS2304 for unresolved typedef body name, got: {diags:?}"
    );
    assert!(
        !ts2304_messages.iter().any(|m| m.contains("'T'")),
        "Broken typedef body should not make the alias name unresolved, got: {diags:?}"
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for literal arrow return assignable to union, got: {:?}",
        diagnostic_messages(&ts2322)
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
    let ts2351 = diagnostics_with_code(&diags, 2351);
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for dotted namespace class merge, got: {:?}",
        diagnostic_messages(&ts2351)
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
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        1,
        "Expected 1 TS2540 for readonly property assignment via this in as-const object, got codes: {:?}",
        diagnostic_codes(&diags)
    );
    // Must NOT emit TS2322 — the readonly check takes precedence.
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when TS2540 (readonly) applies, got: {:?}",
        diagnostic_messages(&ts2322)
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
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 for readonly property read, got: {:?}",
        diagnostic_messages(&ts2540)
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
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        2,
        "Expected 2 TS2540 for readonly property assignments in as-const method, got codes: {:?}",
        diagnostic_summaries(&diags)
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
    let ts2540 = diagnostics_with_code(&diags, 2540);
    assert_eq!(
        ts2540.len(),
        0,
        "Expected no TS2540 without as-const, got: {:?}",
        diagnostic_messages(&ts2540)
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for number not assignable to string, got: {:?}",
        diagnostic_messages(&ts2322)
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for reverse-mapped tuple inference through conditional template, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn generic_tuple_rest_argument_infers_union_from_all_rest_elements() {
    let diags = check_source_diagnostics(
        r#"
declare function f0<T, U>(x: [T, ...U[]]): [T, U];
f0([1, "hello", true]);
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 when tuple rest inference merges string | boolean, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_array_return_rejects_mapped_element_to_type_parameter_array() {
    let diags = check_source_diagnostics(
        r#"
interface Stuff {
    field: number;
    anotherField: string;
}
function doStuffWithStuffArr<T extends Stuff>(arr: { [K in keyof T & keyof Stuff]: T[K] }[]): T[] {
    return arr;
}
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.iter().any(|d| {
            d.message_text.contains(
                "Type '{ [K in keyof T & keyof Stuff]: T[K]; }[]' is not assignable to type 'T[]'",
            )
        }),
        "Expected TS2322 for reverse-mapped array return, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_dependent_default_uses_inferred_literal_not_constraint() {
    let diags = check_source_diagnostics(
        r#"
type Record<K extends string, T> = { [P in K]: T };
type StateConfig<TAction extends string> = {
  entry?: TAction;
  states?: Record<string, StateConfig<TAction>>;
};
declare function createMachine<
  TConfig extends StateConfig<TAction>,
  TAction extends string = TConfig["entry"] extends string ? TConfig["entry"] : string,
>(config: { [K in keyof TConfig & keyof StateConfig<any>]: TConfig[K] }): [TAction, TConfig];
createMachine({
  entry: "foo",
  states: {
    a: {
      entry: "bar",
    },
  },
});
"#,
    );

    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert!(
        ts2322.iter().any(|d| {
            d.message_text
                .contains("Type '\"bar\"' is not assignable to type '\"foo\"'")
        }),
        "Expected nested entry to be checked against inferred literal \"foo\", got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn reverse_mapped_excess_property_display_matches_nested_and_asserted_branches() {
    let diags = check_source_diagnostics(
        r#"
interface WithNestedProp {
  prop: string;
  nested: { prop: string; };
  other: { prop: string; };
}
declare function withNestedProp<T extends WithNestedProp>(props: {[K in keyof T & keyof WithNestedProp]: T[K]}): T;
withNestedProp({prop: "foo", nested: { prop: "bar" }, other: { prop: "baz" }, extra: 10 });

type IsLiteralString<T extends string> = string extends T ? false : true;
interface ProvidedActor {
  src: string;
  logic: () => unknown;
}
type DistributeActors<TActor> = TActor extends { src: infer TSrc } ? { src: TSrc; } : never;
interface MachineConfig<TActor extends ProvidedActor> {
  types?: { actors?: TActor; };
  invoke: IsLiteralString<TActor["src"]> extends true ? DistributeActors<TActor> : { src: string; };
}
declare function createXMachine<
  const TConfig extends MachineConfig<TActor>,
  TActor extends ProvidedActor = TConfig extends { types: { actors: ProvidedActor} } ? TConfig["types"]["actors"] : ProvidedActor,
>(config: {[K in keyof MachineConfig<any> & keyof TConfig]: TConfig[K]}): TConfig;
const child = () => "foo";
createXMachine({
  types: {} as {
    actors: {
      src: "str";
      logic: typeof child;
    };
  },
  invoke: {
    src: "str",
  },
  extra: 10
});
"#,
    );

    let ts2353 = diagnostics_with_code(&diags, 2353);
    assert!(
        ts2353.iter().any(|d| {
            d.message_text.contains(
                "type '{ prop: \"foo\"; nested: { prop: string; }; other: { prop: string; }; }'",
            )
        }),
        "Expected anonymous nested object excess display to preserve top literal and structurally widen nested props, got: {:?}",
        diagnostic_messages(&ts2353)
    );
    assert!(
        ts2353.iter().any(|d| {
            d.message_text.contains(
                "types: { actors: { src: \"str\"; logic: () => string; }; }; invoke: { readonly src: \"str\"; };",
            )
        }),
        "Expected asserted types branch to strip readonly while invoke remains readonly, got: {:?}",
        diagnostic_messages(&ts2353)
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
    let ts7006 = diagnostics_with_code(&diags, 7006);
    assert_eq!(
        ts7006.len(),
        1,
        "Expected exactly 1 TS7006 for 'x' in fn2's arrow (intra-binding ref), got: {:?}",
        diagnostic_messages(&ts7006)
    );
}

#[test]
fn ts2352_tuple_different_length_assertion() {
    // Same-length tuples with incompatible element types
    let diags = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, number];"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags, 2352),
        1,
        "Expected TS2352 for [number, string] as [number, number]"
    );

    // Different-length tuples (shorter to longer)
    let diags2 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = x as [number, string, boolean];"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags2, 2352),
        1,
        "Expected TS2352 for [number, string] as [number, string, boolean]"
    );

    // Angle bracket syntax
    let diags3 = check_source_diagnostics(
        r#"var x: [number, string] = [1, "a"]; var y = <[number, string, boolean]>x;"#,
    );
    assert_eq!(
        diagnostic_count_with_code(&diags3, 2352),
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.no = this.no` inside equality guard, got: {:?}",
        diagnostic_messages(&ts2322)
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
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for `typeof this.this = this.this` inside equality guard, got: {:?}",
        diagnostic_messages(&ts2322)
    );
}

#[test]
fn regex_named_groups_emit_target_and_missing_backreference_diagnostics() {
    let diags = check_source(
        r#"
const regex = /(?<foo>)\k<Foo>/;
"#,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&1503),
        "Expected TS1503 for named capture groups under ES2015, got {codes:?}"
    );
    assert!(
        codes.contains(&1532),
        "Expected TS1532 for unknown named backreference, got {codes:?}"
    );
}

#[test]
fn ts2416_interface_class_merge_method_override_incompatible() {
    // When a class and interface share the same name (declaration merging),
    // the derived class override check must see interface members from the base.
    // Here Bar.method returns string | undefined (from optionalProperty?)
    // but interface Foo declares method(a: number): string — TS2416 should fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    method(a: number): string;
    optionalMethod?(a: number): string;
    property: string;
    optionalProperty?: string;
}

class Foo {
    additionalProperty!: string;

    additionalMethod(a: number): string {
        return this.method(0);
    }
}

class Bar extends Foo {
    method(a: number) {
        return this.optionalProperty;
    }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        1,
        "Expected TS2416 for Bar.method incompatible with merged interface Foo.method, got: {:?}",
        diagnostic_messages(&ts2416)
    );
    assert!(
        ts2416[0].message_text.contains("method"),
        "TS2416 should reference the 'method' property, got: {}",
        ts2416[0].message_text
    );
}

#[test]
fn ts2416_interface_class_merge_property_override_incompatible() {
    // Property signatures from merged interfaces should also be visible
    // in the base chain summary. Here Bar.prop is number but interface
    // Foo declares prop: string — TS2416 should fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    prop: string;
}

class Foo {
    extra!: number;
}

class Bar extends Foo {
    prop: number = 42;
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        1,
        "Expected TS2416 for Bar.prop incompatible with merged interface Foo.prop, got: {:?}",
        diagnostic_messages(&ts2416)
    );
    assert!(
        ts2416[0].message_text.contains("prop"),
        "TS2416 should reference the 'prop' property, got: {}",
        ts2416[0].message_text
    );
}

#[test]
fn no_false_ts2416_interface_class_merge_compatible_override() {
    // When the derived override IS compatible with the merged interface member,
    // TS2416 should NOT fire.
    let diags = check_source_diagnostics(
        r#"
interface Foo {
    method(a: number): string;
}

class Foo {
    extra!: string;
}

class Bar extends Foo {
    method(a: number): string {
        return "hello";
    }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    assert_eq!(
        ts2416.len(),
        0,
        "Expected no TS2416 for compatible override, got: {:?}",
        diagnostic_messages(&ts2416)
    );
}

#[test]
fn ts2416_this_predicate_inheritance_not_suppressed() {
    // Regression for typePredicateInherit.ts: tsc never infers `this is T`
    // predicates from a method body, so a class method without an explicit
    // return type annotation that happens to return `boolean` must NOT be
    // suppressed when the interface (or base class) it satisfies declares a
    // `this is X` predicate. tsc reports TS2416 for each such mismatch.
    let diags = check_source_diagnostics(
        r#"
interface A {
  method1(): this is { a: 1 };
  method2(): boolean;
  method3(): this is { a: 1 };
}
class B implements A {
  method1() { }
  method2() { }
  method3() { return true; }
}
class C {
  method1(): this is { a: 1 } { return true; }
  method3(): this is { a: 1 } { return true; }
}
class D extends C {
  method1(): void { }
  method3(): boolean { return true; }
}
"#,
    );
    let ts2416 = diagnostics_with_code(&diags, 2416);
    let messages = diagnostic_messages(&ts2416);
    assert_eq!(
        ts2416.len(),
        5,
        "Expected 5 TS2416 (B.method1/2/3 + D.method1/3), got: {messages:?}"
    );
    for name in ["method1", "method2", "method3"] {
        assert!(
            ts2416
                .iter()
                .any(|d| d.message_text.contains(&format!("Property '{name}'"))),
            "Expected TS2416 mentioning Property '{name}', got: {messages:?}"
        );
    }
}

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
    let ts2352 = diagnostics_with_code(&diags, 2352);
    assert_eq!(
        ts2352.len(),
        0,
        "Expected no TS2352 for string enum comparable assertion, got: {:?}",
        diagnostic_messages(&ts2352)
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

    let ts2571 = diagnostics_with_code(&diags, 2571);
    assert_eq!(
        ts2571.len(),
        1,
        "Expected exactly one TS2571 for unknown array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
    );

    let empty_start = source.find("[]").expect("expected empty array pattern") as u32;
    assert_eq!(
        ts2571[0].start, empty_start,
        "TS2571 should anchor at the empty array pattern"
    );

    let ts2488 = diagnostics_with_code(&diags, 2488);
    assert_eq!(
        ts2488.len(),
        2,
        "Expected TS2488 on both unknown array destructuring patterns, got: {:?}",
        diagnostic_code_starts(&diags)
    );
}

#[test]
fn catch_array_destructuring_unknown_suppresses_ts2571() {
    let diags = check_source_diagnostics(
        r#"
try {} catch ([x]) {}
"#,
    );

    let ts2571 = diagnostics_with_code(&diags, 2571);
    assert_eq!(
        ts2571.len(),
        0,
        "Expected no TS2571 for catch-clause array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
    );
    let ts2488 = diagnostics_with_code(&diags, 2488);
    assert_eq!(
        ts2488.len(),
        1,
        "Expected TS2488 for catch-clause array destructuring, got: {:?}",
        diagnostic_code_starts(&diags)
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
    let ts2351 = diagnostics_with_code(&diags, 2351);
    assert_eq!(
        ts2351.len(),
        0,
        "Expected no TS2351 for interface with construct signature, got: {:?}",
        diagnostic_messages(&ts2351)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on generic class self-reference, got: {:?}",
        diagnostic_messages(&ts2339)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for f.x where f: Foo<string>, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn no_false_ts2339_on_self_cast_in_generic_class_property_initializer() {
    let diags = check_source_diagnostics(
        r#"
class Bar<T> {
    num!: number;
    Field: number = (this as Bar<any>).num;
    Value = (this as Bar<any>).num;
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for self-cast property initializer, got: {:?}",
        diagnostic_messages(&ts2339)
    );

    let missing_diags = check_source_diagnostics(
        r#"
class Bar<T> {
    Value = (this as Bar<any>).missing;
}
"#,
    );
    assert!(
        missing_diags.iter().any(|d| d.code == 2339),
        "Expected TS2339 for genuinely missing self-cast member, got: {:?}",
        diagnostic_summaries(&missing_diags)
    );
}

#[test]
fn inherited_generic_class_field_this_method_aliases_use_declared_base_chain() {
    // A generic base class field initializer can be checked while constructing
    // a derived class instance whose inheritance graph edge is not populated yet.
    // The initializer's `this.method` lookup still needs to recover members
    // declared on the base class through the declared `extends` chain.
    let diags = check_source_diagnostics(
        r#"
interface BaseDef { tag?: string }
type BaseAny = Base<any, any, any>;
interface WrapperDef<T extends BaseAny> extends BaseDef { schema: T }

abstract class Base<Output, Def extends BaseDef = BaseDef, Input = Output> {
    readonly _output!: Output;
    readonly _input!: Input;
    readonly _def!: Def;

    abstract parse(data: unknown): Output;

    first(value: unknown): Output {
        return this.parse(value);
    }
    firstAlias = this.first;

    second<Func extends (arg: Output) => unknown>(check: Func): Wrapper<BaseAny> {
        return null as any;
    }
    secondAlias = this.second;
}

class Wrapper<T extends BaseAny> extends Base<unknown, WrapperDef<T>, unknown> {
    parse(data: unknown): unknown {
        return data;
    }
}

class Text extends Base<string> {
    parse(data: unknown): string {
        return String(data);
    }
}

type UseWrapper = Wrapper<Text>;
type UseText = Text;
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for inherited field aliases, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn base_class_field_this_does_not_recover_derived_only_members() {
    let diags = check_source_diagnostics(
        r#"
class Base {
    x = this.derivedOnly;
}

class Derived extends Base {
    derivedOnly() {
        return 1;
    }
}
"#,
    );
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        1,
        "Expected TS2339 for derived-only member in base initializer, got: {:?}",
        diagnostic_summaries(&diags)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter returning this, got: {:?}",
        diagnostic_messages(&ts2339)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for r2.y where r2 = c.y, got: {:?}",
        diagnostic_messages(&ts2339)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for getter `this` return type on class with getter after constructor, got: {:?}",
        diagnostic_messages(&ts2339)
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for enum member access through typeof namespace, got: {:?}",
        diagnostic_messages(&ts2339)
    );
    // TS2403: Subsequent variable declarations must have the same type
    let ts2403 = diagnostics_with_code(&diags, 2403);
    assert_eq!(
        ts2403.len(),
        0,
        "Expected no TS2403 for enum typeof mismatch, got: {:?}",
        diagnostic_messages(&ts2403)
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
    let matching = diagnostics_with_code(&diags, 2345);
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
    let ts2339 = diagnostics_with_code(&diags, 2339);
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for property access on class with circular computed property, got: {:?}",
        diagnostic_messages(&ts2339)
    );
}

#[test]
fn satisfies_preserves_literal_type_for_direct_literal() {
    // `1 satisfies number` should have type `1` (preserved), not `number` (widened).
    // tsc: `checkSatisfiesExpressionWorker` calls `checkExpression` which returns
    // fresh literal types from `checkNumericLiteral` regardless of contextual type.
    // Assignment to a literal target `true` then shows source `'1'`, not `'number'`.
    let diags = check_source_diagnostics(
        r#"
const a: true = 1 satisfies number;
const b: true = "foo" satisfies string;
const c: 2 = 1 satisfies number;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        3,
        "Expected 3 TS2322 errors for satisfies literal assignments, got: {:?}",
        diagnostic_messages(&ts2322)
    );
    // All three should preserve the source literal in the diagnostic (not widen).
    assert!(
        ts2322[0].message_text.contains("Type '1'"),
        "Expected `Type '1'` preserved for `1 satisfies number`, got: {}",
        ts2322[0].message_text
    );
    assert!(
        ts2322[1].message_text.contains("Type '\"foo\"'"),
        "Expected `Type '\"foo\"'` preserved for `\"foo\" satisfies string`, got: {}",
        ts2322[1].message_text
    );
    assert!(
        ts2322[2].message_text.contains("Type '1'"),
        "Expected `Type '1'` preserved for `1 satisfies number` assigned to `2`, got: {}",
        ts2322[2].message_text
    );
}

#[test]
fn satisfies_widens_source_for_ts1360_when_target_is_primitive() {
    // For TS1360 (`Type X does not satisfy the expected type Y`), when Y is not a
    // literal-sensitive type (e.g. `boolean`, `number`), tsc widens a bare literal
    // source for display: `Type 'number' does not satisfy the expected type 'boolean'.`
    // This preserves our existing match with tsc even though the internal type
    // of `1 satisfies boolean` is now `1` (preserved literal) rather than `number`.
    let diags = check_source_diagnostics(
        r#"
const x = 1 satisfies boolean;
"#,
    );
    let ts1360 = diagnostics_with_code(&diags, 1360);
    assert_eq!(
        ts1360.len(),
        1,
        "Expected 1 TS1360 error for `1 satisfies boolean`, got: {:?}",
        diagnostic_messages(&ts1360)
    );
    assert!(
        ts1360[0].message_text.contains("Type 'number'"),
        "Expected source widened to 'number' in TS1360 message (target is non-literal `boolean`), got: {}",
        ts1360[0].message_text
    );
    assert!(
        ts1360[0].message_text.contains("'boolean'"),
        "Expected target `boolean` in TS1360 message, got: {}",
        ts1360[0].message_text
    );
}

#[test]
fn satisfies_array_literal_elaborates_per_element() {
    // `[10, "20"] satisfies number[]` should elaborate per-element rather than
    // emitting a generic TS1360 on the whole expression. tsc emits TS2322 at
    // the offending `"20"` element with `Type 'string' is not assignable to
    // type 'number'.`, matching its `elaborateElementwise` behavior.
    //
    // Iteration variable / property names are deliberately varied across
    // assertions to avoid fingerprinting a specific spelling — the rule is
    // structural over array literal sources, not over specific identifiers.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([10, "20"] satisfies number[]));
take(10, ...([1, 2, "x", 4] satisfies number[]));
take(10, ...(([1, "wrapped"]) satisfies number[]));
take(10, ...(([1, "asserted"] as (number | string)[]) satisfies number[]));
"#,
    );

    // First satisfies has one bad element: "20" (string).
    // Second satisfies has one bad element: "x" (string).
    // The wrapped cases prove source unwrapping reaches the same array-literal
    // element path for parenthesized and asserted array sources.
    // Each source should emit TS2322 at the bad element, NOT TS1360 on the whole satisfies.
    let ts2322 = diagnostics_with_code(&diags, 2322);
    let ts1360 = diagnostics_with_code(&diags, 1360);

    assert_eq!(
        ts1360.len(),
        0,
        "Expected NO TS1360 generic-satisfies error; expected per-element TS2322 instead, got TS1360s: {:?}",
        diagnostic_messages(&ts1360)
    );
    assert_eq!(
        ts2322.len(),
        4,
        "Expected exactly 4 TS2322 elaborations (one per bad element), got: {:?}",
        diagnostic_messages(&ts2322)
    );
    for diag in &ts2322 {
        assert!(
            diag.message_text.contains("'string'") && diag.message_text.contains("'number'"),
            "Expected TS2322 message about string -> number, got: {}",
            diag.message_text
        );
    }
}

#[test]
fn satisfies_array_literal_all_elements_compatible_no_diagnostic() {
    // Sanity check: when every element of an array literal satisfies the
    // target's element type, no diagnostic should be reported. This guards
    // against the new array-elaboration path firing on assignable sources.
    let diags = check_source_diagnostics(
        r#"
declare function take(...args: unknown[]): void;
take(10, ...([1, 2, 3] satisfies number[]));
"#,
    );
    assert_eq!(
        diags.len(),
        0,
        "Expected no diagnostics for fully-compatible array literal, got: {:?}",
        diagnostic_summaries(&diags)
    );
}

#[test]
fn satisfies_result_type_is_assignable_to_target_literal_union() {
    // `"A" satisfies string` should have type `"A"` so it remains assignable to
    // a parameter of type `"A" | "B"`. Widening to `string` (the previous
    // behavior) would produce a false TS2345.
    let diags = check_source_diagnostics(
        r#"
declare function fn(s: "A" | "B"): void;
fn("A" satisfies string);
fn("C" satisfies string);
"#,
    );
    // First call should succeed; second should fail with TS2345 (string literal
    // "C" is not assignable to "A" | "B").
    let ts2345 = diagnostics_with_code(&diags, 2345);
    assert_eq!(
        ts2345.len(),
        1,
        "Expected exactly 1 TS2345 for the `\"C\"` call (not the `\"A\"` call), got: {:?}",
        diagnostic_messages(&ts2345)
    );
}

#[test]
fn ts2322_nested_generic_alias_two_levels() {
    // Box<Box<number>> should not be assignable to Box<Box<string>>
    let diags = check_source_diagnostics(
        r#"
type Box<T> = { value: T };
declare const x: Box<Box<number>>;
declare let y: Box<Box<string>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Box<Box<number>> vs Box<Box<string>>, got: {:?}",
        diagnostic_codes(&diags)
    );
}

#[test]
fn ts2322_nested_fn_alias_four_levels() {
    // Cb<Cb<Cb<Cb<number>>>> should not be assignable to Cb<Cb<Cb<Cb<string>>>>
    // where Cb<T> = {noAlias: () => T}["noAlias"]
    let diags = check_source_diagnostics(
        r#"
type Cb<T> = {noAlias: () => T}["noAlias"];
declare const x: Cb<Cb<Cb<Cb<number>>>>;
declare let y: Cb<Cb<Cb<Cb<string>>>>;
y = x;
"#,
    );
    let ts2322 = diagnostics_with_code(&diags, 2322);
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322 for Cb<Cb<Cb<Cb<number>>>> vs Cb<Cb<Cb<Cb<string>>>>, got: {:?}",
        diagnostic_codes(&diags)
    );
    // Both source and target must be shown in structurally-expanded form.
    // tsc does not preserve alias names when the alias body is an IndexedAccess type.
    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("() => () => () => () => number"),
        "Expected source to expand to '() => () => () => () => number', got: {msg}"
    );
    assert!(
        msg.contains("() => () => () => () => string"),
        "Expected target to expand to '() => () => () => () => string', got: {msg}"
    );
}

// Regression: a property-name identifier that happens to share a name with the
// enclosing variable must not be treated as a self-reference for TS7023.
//
// Rule: when a function-like initializer scans its body for self-references
// to detect circular return-type inference, identifiers in non-value name
// positions (property access RHS, qualified-name RHS, property/method/accessor
// names) are property keys, not lexical references — they must not match
// the enclosing variable's symbol.
#[test]
fn ts7023_no_false_positive_on_property_name_collision_assign() {
    // `Object.assign` inside an arrow body is a property name on the right of
    // a property access. The lexical `assign` variable is not referenced.
    let diags = check_source_diagnostics(
        r#"
const assign = <T, U>(a: T, b: U) => Object.assign(a, b);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for property-name collision with enclosing variable, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_no_false_positive_on_property_name_collision_alt_name() {
    // Same rule with a different variable name to prove the fix is structural,
    // not name-specific.
    let diags = check_source_diagnostics(
        r#"
const merge = <T, U>(a: T, b: U) => Object.merge(a, b);
declare namespace Object { function merge<A, B>(a: A, b: B): A & B; }
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert!(
        ts7023.is_empty(),
        "Expected no TS7023 for `merge` colliding with property name, got: {:?}",
        diagnostic_messages(&ts7023)
    );
}

#[test]
fn ts7023_still_fires_on_genuine_self_reference() {
    // Sanity: a real recursive call inside a function-like initializer
    // without a return type annotation must still produce TS7023.
    let diags = check_source_diagnostics(
        r#"
const recur = (n: number) => recur(n);
"#,
    );
    let ts7023 = diagnostics_with_code(&diags, 7023);
    assert_eq!(
        ts7023.len(),
        1,
        "Expected TS7023 for genuine recursive arrow without return annotation, got: {:?}",
        diagnostic_codes(&diags)
    );
}

