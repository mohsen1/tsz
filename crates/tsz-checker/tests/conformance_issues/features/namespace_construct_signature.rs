use super::super::core::*;

#[test]
fn test_create_element_inference_keeps_namespace_local_construct_signature() {
    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "Generic createElement inference should accept the namespace-local construct signature for InferFunctionTypes. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_create_element_inference_keeps_namespace_local_construct_signature_in_conformance_mode() {
    if !lib_files_available() {
        return;
    }

    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "Conformance-mode createElement inference should accept the namespace-local construct signature for InferFunctionTypes. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_factory_scoped_jsx_library_managed_attributes_alias_preserves_added_props() {
    let source = r#"
// @jsx: react
// @jsxFactory: jsx
declare namespace React {
    function createElement(type: any, props: any, ...children: any[]): any;
}

declare const React: {
    createElement: typeof React.createElement;
};

declare const jsx: typeof React.createElement;

namespace jsx {
    export namespace JSX {
        export interface Element {}
        export interface ElementClass {}
        export interface ElementAttributesProperty {}
        export interface ElementChildrenAttribute {}
        export interface IntrinsicAttributes {}
        export interface IntrinsicClassAttributes<T> {}
        export type IntrinsicElements = {
            div: { className: string }
        };

        export type WithCSSProp<P> = P & { css: string };
        export type LibraryManagedAttributes<C, P> = WithCSSProp<P>;
    }
}

declare const Comp: (p: { className?: string }) => null;

;<Comp css="color:hotpink;" />;
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(&diagnostics, 2322),
        "Factory-scoped JSX LibraryManagedAttributes alias should preserve added props, got: {diagnostics:#?}"
    );
}

#[test]
fn test_create_element_inference_keeps_namespace_local_construct_signature_with_merged_lib_contexts()
 {
    if !lib_files_available() {
        return;
    }

    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "Merged-lib createElement inference should accept the namespace-local construct signature for InferFunctionTypes. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_create_element_inference_keeps_namespace_local_construct_signature_with_shared_lib_cache() {
    if !lib_files_available() {
        return;
    }

    let source = r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#;

    let diagnostics =
        compile_and_get_diagnostics_with_merged_lib_contexts_and_shared_cache_and_options(
            source,
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        );
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 0,
        "Shared lib cache must not poison user-defined Component lookups during createElement inference. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_destructuring_from_this_in_constructor_reports_ts2715_per_property() {
    let source = r#"
abstract class C1 {
    abstract x: string;
    abstract y: string;

    constructor() {
        let { x, y: y1 } = this;
        ({ x, y: y1, "y": y1 } = this);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2715_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2715)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2715_messages.len(),
        5,
        "Expected one TS2715 per destructured abstract property occurrence. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2715_messages
            .iter()
            .filter(|message| message.contains("Abstract property 'x' in class 'C1'"))
            .count(),
        2,
        "Expected two TS2715 diagnostics for x destructuring. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2715_messages
            .iter()
            .filter(|message| message.contains("Abstract property 'y' in class 'C1'"))
            .count(),
        3,
        "Expected three TS2715 diagnostics for y destructuring. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_switch_default_narrows_typeof_domains() {
    let source = r#"
type Basic = number | boolean | string | symbol | object | Function | undefined;

function assertNever(x: never) { return x; }
function acceptRemainder(x: string | object | undefined) { return x; }

function exhaustive(x: Basic) {
    switch (typeof x) {
        case "number": return;
        case "boolean": return;
        case "function": return;
        case "symbol": return;
        case "object": return;
        case "string": return;
        case "undefined": return;
    }
    return assertNever(x);
}

function partial(x: Basic) {
    switch (typeof x) {
        case "number": return;
        case "boolean": return;
        case "function": return;
        case "symbol": return;
        default: return acceptRemainder(x);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected switch(typeof) defaults to narrow correctly. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_const_annotated_union_initializer_reduces_for_property_reads() {
    let source = r#"
type AOrArrA<T> = T | T[];
const arr: AOrArrA<{ x?: "ok" }> = [{ x: "ok" }];
const xs: { x?: "ok" }[] = arr;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected const annotated union initializer to reduce to the array member for downstream reads. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_switch_case_dispatch_excludes_prior_matching_cases() {
    let source = r#"
function assertNever(x: never) { return x; }

function f(x: string | number | boolean) {
    switch (typeof x) {
        case "string": return;
        case "number": return;
        case "boolean": return;
        case "number": return assertNever(x);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected duplicate switch case to see never after prior matching cases. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_typeof_switch_default_excludes_object_constrained_type_params() {
    let source = r#"
type L = (x: number) => string;
type R = { x: string, y: number };

function assertNever(x: never) { return x; }

function f<X extends L, Y extends R>(xy: X | Y) {
    switch (typeof xy) {
        case "function": return;
        case "object": return;
        default: return assertNever(xy);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected object-constrained type parameters to be excluded in switch(typeof) default. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_mixed_constructor_unions_still_report_ts2511() {
    let source = r#"
class ConcreteA {}
class ConcreteB {}
abstract class AbstractA {}
abstract class AbstractB {}

type Abstracts = typeof AbstractA | typeof AbstractB;
type Concretes = typeof ConcreteA | typeof ConcreteB;
type ConcretesOrAbstracts = Concretes | Abstracts;

declare const cls1: ConcretesOrAbstracts;
declare const cls2: Abstracts;
declare const cls3: typeof ConcreteA | typeof AbstractA | typeof AbstractB;

new cls1();
new cls2();
new cls3();
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2511_count = diagnostics.iter().filter(|(code, _)| *code == 2511).count();

    assert_eq!(
        ts2511_count, 3,
        "Expected TS2511 for mixed and all-abstract constructor unions. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_complicated_indexes_of_intersections_are_inferencable() {
    let source = r#"
interface FormikConfig<Values> {
    initialValues: Values;
    validate?: (props: Values) => void;
    validateOnChange?: boolean;
}

declare function Func<Values = object, ExtraProps = {}>(
    x: (string extends "validate" | "initialValues" | keyof ExtraProps
        ? Readonly<FormikConfig<Values> & ExtraProps>
        : Pick<Readonly<FormikConfig<Values> & ExtraProps>, "validate" | "initialValues" | Exclude<keyof ExtraProps, "validateOnChange">>
        & Partial<Pick<Readonly<FormikConfig<Values> & ExtraProps>, "validateOnChange" | Extract<keyof ExtraProps, "validateOnChange">>>)
): void;

Func({
    initialValues: {
        foo: ""
    },
    validate: props => {
        props.foo;
    }
});
"#;

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2339),
        "Expected no TS2339 for props.foo after inferring Values from initialValues. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_intersection_with_any_defaulted_alias_does_not_overconstrain_property() {
    let source = r#"
type ComputedGetter<T> = (oldValue?: T) => T;
type ComputedOptions = Record<string, ComputedGetter<any>>;
type ExtractComputedReturns<T extends any> = {
  [key in keyof T]: T[key] extends (...args: any[]) => infer TReturn ? TReturn : never;
};
interface ComponentOptionsBase<D, C extends ComputedOptions> {
  data?: D;
  computed?: C;
}
type ComponentPublicInstance<D = {}, C extends ComputedOptions = {}> = D & ExtractComputedReturns<C>;
type ComponentOptions<D = any, C extends ComputedOptions = any> =
  ComponentOptionsBase<D, C> & ThisType<ComponentPublicInstance<D, C>>;
interface App { mixin(mixin: ComponentOptions): this; }
interface InjectionKey<T> extends Symbol {}
interface Ref<T> { _v: T; }
declare function reactive<T extends object>(target: T): Ref<T>;
interface ThemeInstance { readonly name: Readonly<Ref<string>>; }
declare const ThemeSymbol: InjectionKey<ThemeInstance>;
declare function inject(this: ComponentPublicInstance, key: InjectionKey<any> | string): any;
declare const app: App;
app.mixin({
  computed: {
    $vuetify() {
      return reactive({
        theme: inject.call(this, ThemeSymbol),
      });
    },
  },
});
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "Expected no TS2322 from contextual defaulted any intersection. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_abstract_class_union_instantiation_shape_reports_all_ts2511s_with_libs() {
    let source = r#"
class ConcreteA {}
class ConcreteB {}
abstract class AbstractA { a: string; }
abstract class AbstractB { b: string; }

type Abstracts = typeof AbstractA | typeof AbstractB;
type Concretes = typeof ConcreteA | typeof ConcreteB;
type ConcretesOrAbstracts = Concretes | Abstracts;

declare const cls1: ConcretesOrAbstracts;
declare const cls2: Abstracts;
declare const cls3: Concretes;

new cls1();
new cls2();
new cls3();
"#;

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2511_count = diagnostics.iter().filter(|(code, _)| *code == 2511).count();

    assert_eq!(
        ts2511_count, 2,
        "Expected TS2511 for mixed and abstract declared constructor unions in the conformance shape. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_enum_union_display_collapses_members_to_enum_name() {
    let source = r#"
namespace X {
    export enum Foo {
        A, B
    }
}
namespace Z {
    export enum Foo {
        A = 1 << 1,
        B = 1 << 2,
    }
}
const e1: X.Foo | boolean = Z.Foo.A;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning computed enum member into X.Foo | boolean");

    assert!(
        message.contains("Type 'Foo.A' is not assignable to type 'boolean | Foo'."),
        "Expected enum union display to collapse to the enum name. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_modules_same_file_const_numeric_no_ts18056() {
    // tsc traces through same-file const variables: `const foo = 2` evaluates to
    // value=2, resolvedOtherFiles=false, so auto-increment works and TS18056 does
    // NOT fire. Our classify_symbol_backed_enum_initializer now correctly traces
    // same-file consts and classifies them as LiteralNumeric (not NonLiteralNumeric).
    let source = r#"
const foo = 2;
enum A {
    a = foo,
    b,
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            isolated_modules: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 18056),
        "Should NOT emit TS18056 for same-file const numeric — tsc traces through. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_modules_same_file_const_string_no_ts18055() {
    // tsc traces through same-file const variables: `const bar = "bar"` evaluates
    // to value="bar", isSyntacticallyString=true, so TS18055 does NOT fire.
    // Our classify_symbol_backed_enum_initializer now correctly traces same-file
    // consts and classifies them as LiteralString (not NonLiteralString).
    let source = r#"
const bar = "bar";
enum A {
    a = bar,
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            isolated_modules: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 18055),
        "Should NOT emit TS18055 for same-file const string — tsc traces through. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_override_tag_uses_jsdoc_diagnostic_family() {
    let source = r#"
class A {
    /**
     * @method
     * @param {string | number} a
     * @returns {boolean}
     */
    foo(a) {
        return typeof a === "string";
    }
    bar() {}
}

class B extends A {
    /**
     * @override
     * @method
     * @param {string | number} a
     * @returns {boolean}
     */
    foo(a) {
        return super.foo(a);
    }

    bar() {}

    /** @override */
    baz() {}
}

class C {
    /** @override */
    foo() {}
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 4119),
        "Expected TS4119 for missing JSDoc @override on overriding JS member. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 4121),
        "Expected TS4121 for JSDoc @override on class without extends. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 4122),
        "Expected TS4122 for JSDoc @override on missing base member. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 4112)
            && !has_error(&diagnostics, 4114)
            && !has_error(&diagnostics, 4123),
        "Did not expect TypeScript-keyword override diagnostics for JSDoc @override cases. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_override_tag_did_you_mean_emits_ts4123() {
    // When a JSDoc @override member has a close spelling match in the base class,
    // tsc emits TS4123 ("Did you mean 'X'?") instead of TS4122 (no suggestion).
    // This only fires for names longer than 3 characters.
    let source = r#"
class A {
    doSomething() {}
}

class B extends A {
    /** @override  */
    doSomethang() {}
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 4123),
        "Expected TS4123 for JSDoc @override typo with suggestion. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 4122),
        "Should emit TS4123 (with suggestion), not TS4122 (without). Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_template_brace_form_reports_ts1069_and_ts2304() {
    let source = r#"
/** @template {T} */
class Baz {
    m() {
        class Bar {
            static bar() { this.prototype.foo(); }
        }
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 1069),
        "Expected TS1069 for invalid JSDoc @template brace syntax. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2304),
        "Expected TS2304 for the unresolved JSDoc template name inside braces. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_malformed_jsdoc_satisfies_does_not_emit_duplicate_tag_error() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
/**
 * @typedef {Object} T1
 * @property {number} a
 */

/**
 * @satisfies T1
 */
const t1 = { a: 1 };
const t2 = /** @satisfies T1 */ ({ a: 1 });
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 1223),
        "Did not expect TS1223 for malformed @satisfies tags.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics.iter().filter(|d| d.0 == 1005).count(),
        4,
        "Expected only the four parse-shaped TS1005 diagnostics for malformed @satisfies tags.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_param_function_type_without_return_reports_ts7014() {
    let source = r#"
/** @param {function(...[*])} callback */
function g(callback) {
    callback([1], [2], [3]);
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7014),
        "Expected TS7014 for JSDoc function type without return annotation. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_type_function_constructor_does_not_report_ts7014() {
    // `function(new: object, string, number)` is a constructor type — the `new: object`
    // part implies the return type, so TS7014 must not fire.
    let source = r#"
/** @type {function(new: object, string, number)} */
const g = null;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7014),
        "TS7014 should NOT be emitted for constructor function types. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_type_function_no_implicit_any_guard() {
    // Without noImplicitAny, TS7014 must not be emitted even for function types
    // without a return annotation.
    let source = r#"
/** @type {function(string)} */
const f = null;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: false,
            no_implicit_any: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7014),
        "TS7014 should NOT be emitted without noImplicitAny. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_type_function_at_param_reports_ts7014_ts1110_ts2304() {
    let source = r#"
// @ts-check
/**
 * @type {function(@foo)}
 */
let x;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7014),
        "Expected TS7014 for malformed JSDoc function type. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 1110),
        "Expected TS1110 for malformed JSDoc function parameter type. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2304),
        "Expected TS2304 for malformed JSDoc function parameter name. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_function_object_type_does_not_suppress_implicit_any_parameter() {
    let source = r#"
// @ts-check
/** @type {Function} */
const x = (a) => a + 1;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 7006),
        "Expected TS7006 for broad JSDoc Function type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_unwrapped_multiline_typedef_reports_ts1110() {
    let source = r#"
/** 
   Multiline type expressions in comments without leading * are not supported.
   @typedef {{
     foo:
     *,
     bar:
     *
   }} Type7
 */
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "mod7.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts1110_count = diagnostics.iter().filter(|(code, _)| *code == 1110).count();
    assert_eq!(
        ts1110_count, 2,
        "Expected two TS1110 diagnostics for unsupported multiline typedef wrapping. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_unwrapped_multiline_typedef_does_not_leak_to_sibling_files() {
    // Regression: `check_jsdoc_unwrapped_multiline_typedefs` previously walked
    // every arena's comments while `error_at_position` attaches diagnostics
    // to the current file's name. With mod1.js as the entry file (well-formed
    // typedef) and mod7.js as a sibling (malformed wrapping), TS1110s for
    // mod7.js's comment offsets were re-anchored onto mod1.js. The check now
    // operates strictly on the current file's arena, so checking mod1.js as
    // entry must yield zero TS1110 diagnostics regardless of sibling content.
    let mod1 = r#"
/**
 * @typedef {function(string): boolean}
 * Type1
 */
function callIt(func, arg) {
  return func(arg);
}
"#;

    let mod7 = r#"
/**
   Multiline type expressions in comments without leading * are not supported.
   @typedef {{
     foo:
     *,
     bar:
     *
   }} Type7
 */
"#;

    let files: &[(&str, &str)] = &[("mod1.js", mod1), ("mod7.js", mod7)];

    let entry_diagnostics = compile_named_files_get_diagnostics_with_options(
        files,
        "mod1.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let entry_ts1110 = entry_diagnostics
        .iter()
        .filter(|(code, _)| *code == 1110)
        .count();
    assert_eq!(
        entry_ts1110, 0,
        "mod1.js (well-formed @typedef) should not receive TS1110 from sibling mod7.js. \
         Actual diagnostics: {entry_diagnostics:#?}"
    );

    // Conversely, mod7.js as the entry must still report exactly two TS1110s.
    let sibling_diagnostics = compile_named_files_get_diagnostics_with_options(
        files,
        "mod7.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let sibling_ts1110 = sibling_diagnostics
        .iter()
        .filter(|(code, _)| *code == 1110)
        .count();
    assert_eq!(
        sibling_ts1110, 2,
        "mod7.js (unwrapped multiline @typedef) should still emit two TS1110 diagnostics \
         when checked alongside mod1.js. Actual diagnostics: {sibling_diagnostics:#?}"
    );
}

#[test]
fn test_js_commonjs_deep_exports_assignment_reports_ts2339_against_current_module_surface() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        "exports.a.b.c = 0;",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339 = diagnostic_message(&relevant, 2339)
        .expect("expected TS2339 for deep assignment through unresolved exports member");

    assert_eq!(relevant.len(), 1, "unexpected diagnostics: {relevant:#?}");
    assert!(
        ts2339.contains("Property 'a' does not exist on type 'typeof import(\"a\")'."),
        "Expected TS2339 to target the current file CommonJS namespace surface. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_js_commonjs_deep_module_exports_assignment_reports_ts2339_against_current_module_surface() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        "module.exports.a.b.c = 0;",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339 = diagnostic_message(&relevant, 2339)
        .expect("expected TS2339 for deep assignment through unresolved module.exports member");

    assert_eq!(relevant.len(), 1, "unexpected diagnostics: {relevant:#?}");
    assert!(
        ts2339.contains("Property 'a' does not exist on type 'typeof import(\"a\")'."),
        "Expected TS2339 to target the current file CommonJS namespace surface. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_js_commonjs_direct_exports_members_remain_visible() {
    let source = r#"
exports.x = 0;
{
    exports.Cls = function() {
        this.x = 0;
    }
}

const instance = new exports.Cls();
exports.x;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Expected direct CommonJS export member writes to stay visible. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_js_late_bound_commonjs_exports_preserve_typeof_import_display_name() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "lateBoundAssignmentDeclarationSupport1.js",
                r#"
const _sym = Symbol();
const _str = "my-fake-sym";

exports[_sym] = "ok";
exports[_str] = "ok";
exports.S = _sym;
"#,
            ),
            (
                "usage.js",
                r#"
const x = require("./lateBoundAssignmentDeclarationSupport1.js");
const y = x["my-fake-sym"];
const z = x[x.S];
"#,
            ),
        ],
        "usage.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    let ts7053_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts7053_messages.len() >= 2,
        "Expected TS7053 for late-bound key reads from the required namespace. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7053_messages.iter().any(|message| {
            message.contains("typeof import(\"lateBoundAssignmentDeclarationSupport1\")")
        }),
        "Expected TS7053 to preserve `typeof import(...)` namespace display. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_constructor_void_zero_assignment_does_not_create_member() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
function C() {
    this.p = 1;
    this.q = void 0;
}
var c = new C();
c.p + c.q;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == 2339 || *code == 18048)
        .collect();
    assert_eq!(relevant.len(), 2, "unexpected diagnostics: {relevant:#?}");
    assert!(
        relevant
            .iter()
            .all(|(_, message)| message.contains("Property 'q' does not exist on type 'C'.")),
        "Expected TS2339 for missing constructor property. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 18048),
        "Did not expect TS18048 once the void-zero constructor property is skipped. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_js_void_zero_expando_reports_named_receiver_type() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
var o = {};
o.y = void 0;
o.y;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert_eq!(relevant.len(), 2, "unexpected diagnostics: {relevant:#?}");
    assert!(
        relevant
            .iter()
            .all(|(_, message)| message.contains("Property 'y' does not exist on type 'typeof o'.")),
        "Expected TS2339 to display typeof o for missing JS expando property. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_js_constructor_factory_call_does_not_keep_undefined_return() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
/** @param {number} x */
function A(x) {
    if (!(this instanceof A)) {
        return new A(x);
    }
    this.x = x;
}
var k = A(1);
var j = new A(2);
k.x === j.x;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_this: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 18048 && message.contains("'k' is possibly 'undefined'")
        }),
        "Expected JS constructor-style factory call to return the instance type. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_current_file_commonjs_exports_use_late_bound_assignment_types() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
exports.y = exports.x = void 0;
exports.x = 1;
exports.y = 2;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(relevant.len(), 2, "unexpected diagnostics: {relevant:#?}");
    assert!(
        relevant
            .iter()
            .any(|(_, message)| message.contains("Type 'undefined' is not assignable to type '2'.")),
        "Expected exports.y chained assignment to use the later inferred type. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        relevant
            .iter()
            .any(|(_, message)| message.contains("Type 'undefined' is not assignable to type '1'.")),
        "Expected exports.x chained assignment to use the later inferred type. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_type_query_qualified_name_reports_possibly_undefined_on_optional_midpoint() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface DeepOptional {
    a?: {
        b?: {
            c?: string;
        };
    };
}

function init2(foo: DeepOptional) {
    if (foo.a) {
        type C = typeof foo.a.b.c;

        for (const _ of [1]) {
            type NestedC = typeof foo.a.b.c;
        }
    }
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2532_count = diagnostics.iter().filter(|(code, _)| *code == 2532).count();
    assert_eq!(
        ts2532_count, 2,
        "Expected two TS2532 diagnostics for typeof-qualified access through optional b. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_constructor_instance_missing_property_does_not_use_variable_typeof_display() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
function C() {
    this.p = 1;
    this.q = void 0;
}
var c = new C();
c.p + c.q;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts2339 = diagnostic_message(&diagnostics, 2339)
        .expect("expected TS2339 for missing constructor property");

    assert!(
        ts2339.contains("Property 'q' does not exist on type 'C'."),
        "Expected constructor instance missing-property display to use C. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2339.contains("typeof c"),
        "Did not expect constructor instance missing-property display to use typeof c. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_merged_declarations_non_exported_namespace_members_stay_hidden() {
    let source = r#"
namespace M {
 export enum Color {
   Red, Green
 }
}
namespace M {
 export namespace Color {
   export var Blue = 4;
  }
}
var p = M.Color.Blue;

namespace M {
    export function foo() {
    }
}

namespace M {
    namespace foo {
        export var x = 1;
    }
}

namespace M {
    export namespace foo {
        export var y = 2
    }
}

namespace M {
    namespace foo {
        export var z = 1;
    }
}

M.foo()
M.foo.x
M.foo.y
M.foo.z
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "mergedDeclarations3.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339: Vec<&str> = relevant
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339.len(),
        2,
        "Expected exactly 2 TS2339 errors. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("Property 'x' does not exist on type")),
        "Expected TS2339 for M.foo.x. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|message| message.contains("Property 'z' does not exist on type")),
        "Expected TS2339 for M.foo.z. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        !ts2339
            .iter()
            .any(|message| message.contains("Property 'y'")),
        "Did not expect TS2339 for M.foo.y. Actual diagnostics: {relevant:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_contextually_types_closure_parameters() {
    let source = r#"
/** @callback Sid
 * @param {string} s
 * @returns {string}
 */
var x = 1;

/** @type {Sid} */
var sid = s => s + "!";
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for closure parameter contextually typed from JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_on_constructor_scope_suppresses_ts7006() {
    let source = r#"
export class Preferences {
  assignability = "no";
  /**
   * @callback ValueGetter_2
   * @param {string} name
   * @returns {boolean|number|string|undefined}
   */
  constructor() {}
}

/** @type {ValueGetter_2} */
var ooscope2 = s => s.length > 0;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for closure typed from constructor-scoped JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_callback_typedef_contextually_types_function_declaration_parameters() {
    let source = r#"
/**
 * @callback Cb
 * @param {unknown} x
 * @return {x is number}
 */

/** @type {Cb} */
function isNumber(x) { return typeof x === "number"; }

/** @param {unknown} x */
function g(x) {
    if (isNumber(x)) {
        x * 2;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for function declaration typed from JSDoc callback typedef. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_function_return_mismatch_reports_inner_body_error_only() {
    let source = r#"
// @ts-check
/** @type {function (number): string} */
const x = (a) => a + 1;
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // TODO: tsc emits an inner body TS2322 ("Type 'number' is not assignable to type 'string'")
    // for JSDoc function return mismatch. We currently emit the outer function-level TS2322.
    // Update once inner body return-type elaboration is implemented.
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for JSDoc function return mismatch. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_generic_typedef_type_tag_no_erasure_reports_ts2345() {
    let source = r#"
/**
 * @template T
 * @typedef {<T1 extends T>(data: T1) => T1} Test
 */

/** @type {Test<number>} */
const test = dibbity => dibbity

test(1) // ok, T=1
test('hi') // error, T=number
"#;

    let diagnostics = compile_and_get_diagnostics_named(
        "typeTagNoErasure.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            emit_declarations: true,
            strict: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for generic JSDoc typedef call. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 for generic JSDoc typedef call. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_enum_assignment_preserves_numeric_literal_source_display() {
    let source = r#"
enum E {
    A = 1,
    B = 2,
}
let x: E.A = 4;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message =
        diagnostic_message(&diagnostics, 2322).expect("expected TS2322 for assigning 4 to E.A");

    assert!(
        message.contains("Type '4' is not assignable to type 'E.A'."),
        "Expected numeric literal source display to be preserved. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_namespaced_enum_assignability_uses_qualified_names() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Abcd {
    export enum E {
        a, b, c, d,
    }
}
declare let abc: First.E;
declare let secondAbcd: Abcd.E;
abc = secondAbcd;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning Abcd.E to First.E");

    assert!(
        message.contains("Type 'Abcd.E' is not assignable to type 'First.E'."),
        "Expected namespaced enum assignability to keep qualified names. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_unambiguous_namespaced_enum_assignability_uses_simple_names() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Abc {
    export enum Nope {
        a, b, c,
    }
}
declare let abc: First.E;
declare let nope: Abc.Nope;
abc = nope;
nope = abc;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type 'Nope' is not assignable to type 'E'.")),
        "Expected unambiguous namespaced enum display to use simple names. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type 'E' is not assignable to type 'Nope'.")),
        "Expected unambiguous reverse enum display to use simple names. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_merged_enum_assignability_uses_all_merged_members() {
    let source = r#"
namespace First {
    export enum E {
        a, b, c,
    }
}
namespace Merged {
    export enum E {
        a, b,
    }
    export enum E {
        c = 3, d,
    }
}
declare let abc: First.E;
declare let merged: Merged.E;
abc = merged;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2322)
        .expect("expected TS2322 for assigning merged enum to First.E");

    assert!(
        message.contains("Type 'Merged.E' is not assignable to type 'First.E'."),
        "Expected merged enum assignability to consider all merged members. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_namespaced_enum_object_property_access_uses_typeof_enum_name() {
    let source = r#"
namespace second {
    export enum E {
        A = 2,
    }
}

const value = second.E.B;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let message = diagnostic_message(&diagnostics, 2339)
        .expect("expected TS2339 for missing enum object property");

    assert!(
        message.contains("Property 'B' does not exist on type 'typeof E'."),
        "Expected namespaced enum object property access to display 'typeof E'. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_stringified_noncanonical_numeric_enum_member_name_is_allowed() {
    let source = r#"
enum Nums {
    "13e-1",
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
        ),
        "Expected non-canonical numeric string enum member names to be allowed. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_negative_infinity_string_enum_member_name_is_allowed() {
    let source = r#"
enum Nums {
    "-Infinity",
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
        ),
        "Expected '-Infinity' string enum member names to be allowed. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_const_enum_string_named_members_are_accessible_by_element_access() {
    let source = r#"
const enum E {
    "hyphen-member" = 1,
    "123startsWithNumber" = 2,
    "has space" = 3,
}

const a = E["hyphen-member"];
const b = E["123startsWithNumber"];
const c = E["has space"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(
            &diagnostics,
            tsz_common::diagnostics::diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN,
        ),
        "Expected string-named const enum members to be accessible via element access. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_const_enum_initializers_allow_merged_and_qualified_element_access() {
    let source = r#"
const enum Enum1 {
    A0 = 100,
}

const enum Enum1 {
    W1 = A0,
    W2 = Enum1.A0,
    W3 = Enum1["A0"],
    W4 = Enum1[`W2`],
}

namespace A {
    export namespace B {
        export namespace C {
            export const enum E {
                V1 = 1,
                V2 = A.B.C.E.V1 | 100
            }
        }
    }
}

namespace A {
    export namespace B {
        export namespace C {
            export const enum E {
                V3 = A.B.C.E["V2"] & 200,
                V4 = A.B.C.E[`V1`] << 1,
            }
        }
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        !has_error(&diagnostics, 2474),
        "Expected merged and qualified const enum initializer references to remain constant expressions.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_literal_computed_name_from_enum_object_reports_ts2464() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export namespace Foo {
  export enum Enum {
    A = "a",
    B = "b",
  }
}

export type Type = { x?: { [Foo.Enum]: 0 } };
"#,
    );

    assert!(
        has_error(&diagnostics, 2464),
        "Expected TS2464 for a computed type-literal property named by an enum object.\nActual diagnostics: {diagnostics:#?}"
    );
}
