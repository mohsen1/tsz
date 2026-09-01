use super::*;

#[test]
fn fix_identity_call_mutual_recursion_does_not_hang() {
    // Regression: const_literal_identity_call_text used to create a fresh
    // RecursionGuard on each call, so a mutually-recursive pair like
    //   const a = id(b); const b = id(a);
    // would loop forever.  The guard is now threaded through, so the cycle is
    // detected and both consts fall back to their inferred types.
    let source = r#"
function id<T>(x: T): T { return x; }
export const a = id(b);
export const b = id(a);
"#;
    // Must complete without hanging; output type is not the goal of this test.
    let output = emit_dts(source);
    assert!(
        !output.is_empty(),
        "emitter should produce output: {output}"
    );
}

#[test]
fn fix_exported_generic_call_literals_preserve_inferred_literal_types() {
    let output = emit_dts(
        r#"
export function generic<T>(value: T) {
  return value;
}

export const viaGeneric = generic("ok" as const);

export const genericArrow = <T>(value: T) => value;
export const viaGenericArrow = genericArrow("ok" as const);

function localGeneric<T>(value: T) {
  return value;
}
export const viaLocalGeneric = localGeneric("ok" as const);

const localGenericArrow = <T>(value: T) => value;
export const viaLocalGenericArrow = localGenericArrow("ok" as const);

export const viaInlineArrow = (<T>(value: T) => value)("ok" as const);
"#,
    );

    for name in [
        "viaGeneric",
        "viaGenericArrow",
        "viaLocalGeneric",
        "viaLocalGenericArrow",
        "viaInlineArrow",
    ] {
        assert!(
            output.contains(&format!("export declare const {name}: \"ok\";")),
            "expected {name} to preserve the literal call result: {output}"
        );
    }
}

#[test]
fn fix_generic_call_infers_literal_from_option_property() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Kind = "one" | "two" | "three";
declare function getInterfaceFromString<T extends Kind>(options?: { type?: T } & { type?: Kind }): T;

const result = getInterfaceFromString({ type: "two" });
"#,
    );

    assert!(
        output.contains("declare const result: \"two\";"),
        "expected generic call result to preserve inferred option literal: {output}"
    );
}

#[test]
fn fix_generic_call_option_literal_does_not_ignore_other_inference_sites() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Kind = "one" | "two" | "three";
declare function getInterfaceFromString<T extends Kind>(options: { type?: T }, fallback: T): T;

const result = getInterfaceFromString({ type: "two" }, "three");
"#,
    );

    assert!(
        output.contains("declare const result: string;"),
        "expected generic call result to fall back instead of narrowing from one argument: {output}"
    );
    assert!(
        !output.contains(r#"declare const result: "two";"#),
        "generic call result must not ignore the fallback argument inference site: {output}"
    );
}

#[test]
fn fix_generic_call_callback_alias_does_not_trigger_conflicting_site() {
    // `Callback<T>` is a TypeReference alias — its type annotation text does not
    // contain `=>`, but it is NOT a direct object-property inference site either.
    // The structural walk must recognise this and preserve the literal from `a`.
    let output = emit_dts_with_usage_analysis(
        r#"
type Callback<T> = (x: T) => void;
declare function f<T extends string>(a: T, b: Callback<T>): T;

const result = f("hello", (_x) => {});
"#,
    );

    assert!(
        output.contains(r#"declare const result = "hello";"#),
        "expected callback-alias parameter not to trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_method_signature_does_not_trigger_conflicting_site() {
    // `{ cb(x: T): void }` is a MethodSignature member — indirect inference, not a
    // direct object-property site.  The structural walk must skip it and preserve
    // the literal from `a`.
    let output = emit_dts_with_usage_analysis(
        r#"
declare function f<T extends string>(a: T, b: { cb(x: T): void }): T;

const result = f("hello", { cb(_x: string) {} });
"#,
    );

    assert!(
        output.contains(r#"declare const result = "hello";"#),
        "expected method-signature callback not to trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_object_property_guard_requires_concrete_argument() {
    // The conflicting-site guard must be call-site-aware: an optional
    // `options?: { type?: T }` parameter that is omitted, receives `{}`,
    // or receives `undefined` supplies no concrete object-property inference
    // for T.  The direct literal from `a: T` should be preserved in all three.
    let output = emit_dts_with_usage_analysis(
        r#"
declare function f<T extends string>(a: T, options?: { type?: T }): T;

const onlyA    = f("hello");
const emptyObj = f("hello", {});
const undef    = f("hello", undefined);
"#,
    );

    assert!(
        output.contains(r#"declare const onlyA = "hello";"#),
        "omitted optional parameter must not trigger conflicting-site guard: {output}"
    );
    assert!(
        output.contains(r#"declare const emptyObj = "hello";"#),
        "empty-object argument must not trigger conflicting-site guard: {output}"
    );
    assert!(
        output.contains(r#"declare const undef = "hello";"#),
        "undefined argument must not trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_object_property_guard_same_literal_no_conflict() {
    // When both `a: T` and `options.type` contribute the SAME literal, tsc has
    // no disagreement between inference sites and keeps the literal.
    let output = emit_dts_with_usage_analysis(
        r#"
declare function f<T extends string>(a: T, options?: { type?: T }): T;

const same = f("hello", { type: "hello" });
"#,
    );

    assert!(
        output.contains(r#"declare const same = "hello";"#),
        "same-literal object-property value must not trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_object_property_guard_non_matching_property_no_conflict() {
    // `{ other: "world" }` has no property named `type`, so it supplies no
    // inference for T through the `options: { type?: T }` path — not a conflict.
    let output = emit_dts_with_usage_analysis(
        r#"
declare function f<T extends string>(a: T, options?: { type?: T }): T;

const unrelated = f("hello", { other: "world" });
"#,
    );

    assert!(
        output.contains(r#"declare const unrelated = "hello";"#),
        "object with non-matching property must not trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_quoted_key_conflict_widens_to_constraint() {
    // A quoted property key `{ "type"?: T }` is the same property as `type?: T`
    // for inference purposes.  Conflicting literals across the two sites must
    // still widen to the constraint.
    let output = emit_dts_with_usage_analysis(
        r#"
type Kind = "one" | "two" | "three";
declare function f<T extends Kind>(options: { "type"?: T }, fallback: T): T;

const result = f({ "type": "two" }, "three");
"#,
    );

    assert!(
        output.contains("declare const result: string;"),
        "quoted-key annotation with conflicting literals must widen to constraint: {output}"
    );
    assert!(
        !output.contains(r#"declare const result: "two";"#)
            && !output.contains(r#"declare const result: "three";"#),
        "quoted-key conflict must not narrow to either literal: {output}"
    );
}

#[test]
fn fix_generic_call_quoted_key_same_literal_no_conflict() {
    // Same literal on both the direct `a: T` site and the quoted-key property
    // site — no conflict, literal is preserved.
    let output = emit_dts_with_usage_analysis(
        r#"
declare function f<T extends string>(a: T, options?: { "type"?: T }): T;

const same = f("hello", { "type": "hello" });
"#,
    );

    assert!(
        output.contains(r#"declare const same = "hello";"#),
        "same-literal quoted-key property must not trigger conflicting-site guard: {output}"
    );
}

#[test]
fn fix_generic_call_non_literal_property_value_widens_to_constraint() {
    // `{ type: two }` carries an identifier value — the emitter cannot resolve
    // it to a primitive literal.  tsc widens to the constraint rather than
    // committing to either inference site.
    let output = emit_dts_with_usage_analysis(
        r#"
const two = "two";
declare function f<T extends string>(options: { type?: T }, fallback: T): T;

const result = f({ type: two }, "three");
"#,
    );

    assert!(
        output.contains("declare const result: string;"),
        "non-literal property value (identifier) must trigger conservative conflict: {output}"
    );
    assert!(
        !output.contains(r#"declare const result: "three";"#),
        "must not narrow to the fallback literal when property value is opaque: {output}"
    );
}

#[test]
fn fix_generic_call_as_const_object_arg_widens_to_constraint() {
    // `options` is an identifier bound to an `as const` object — it is not an
    // inline object literal, so its properties are opaque to the emitter.
    // tsc widens to the constraint when the two sites cannot be compared.
    let output = emit_dts_with_usage_analysis(
        r#"
const options = { type: "two" } as const;
declare function f<T extends string>(options: { type?: T }, fallback: T): T;

const result = f(options, "three");
"#,
    );

    assert!(
        output.contains("declare const result: string;"),
        "as-const identifier argument must trigger conservative conflict: {output}"
    );
    assert!(
        !output.contains(r#"declare const result: "three";"#),
        "must not narrow to the fallback literal when options arg is an opaque reference: {output}"
    );
}

#[test]
fn fix_generic_call_identity_callback_uses_type_parameter_constraint() {
    let output = emit_dts_with_usage_analysis(
        r#"
function foo<T extends "foo">(f: (x: T) => T) {
    return f;
}

function bar<T extends "foo" | "bar">(f: (x: T) => T) {
    return f;
}

let f = foo(x => x);
let fResult = f("foo");

let g = foo((x => x));
let gResult = g("foo");

let h = bar(x => x);
let hResult = h("foo");
hResult = h("bar");
"#,
    );

    for expected in [
        r#"declare let f: (x: "foo") => "foo";"#,
        r#"declare let fResult: "foo";"#,
        r#"declare let g: (x: "foo") => "foo";"#,
        r#"declare let gResult: "foo";"#,
        r#"declare let h: (x: "foo" | "bar") => "foo" | "bar";"#,
        r#"declare let hResult: "bar" | "foo";"#,
    ] {
        assert!(
            output.contains(expected),
            "expected constrained identity callback inference to emit `{expected}`: {output}"
        );
    }
}

#[test]
fn fix_generic_call_identity_callback_skips_explicit_recursive_type_arguments() {
    let output = emit_dts(
        r#"
export type Key<U> = keyof U;
export type Value<K extends Key<U>, U> = U[K];
export const updateIfChanged = <T>(t: T) => {
    const reduce = <U>(u: U, update: (u: U) => T) => {
        const set = (newU: U) => Object.is(u, newU) ? t : update(newU);
        return Object.assign(
            <K extends Key<U>>(key: K) =>
                reduce<Value<K, U>>(u[key as keyof U] as Value<K, U>, (v: Value<K, U>) => {
                    return update(Object.assign(Array.isArray(u) ? [] : {}, u, { [key]: v }));
                }),
            { map: (updater: (u: U) => U) => set(updater(u)), set });
    };
    return reduce<T>(t, (t: T) => t);
};
"#,
    );

    assert!(
        output.contains("export declare const updateIfChanged"),
        "recursive explicit generic calls should not crash declaration emit: {output}"
    );
}

#[test]
fn fix_correlated_alias_call_expands_renamed_discriminant_surface() {
    let output = emit_dts_with_usage_analysis(
        r#"
interface Registry {
    alpha: AlphaEvent;
    beta: BetaEvent;
}
interface AlphaEvent {
    alpha: true;
}
interface BetaEvent {
    beta: true;
}
type Entry<Key extends keyof Registry> = { [Choice in Key]: {
    readonly kind: Choice;
    readonly enabled?: boolean;
    readonly handler: (payload: Registry[Choice]) => void;
}}[Key];

function makeEntry<Key extends keyof Registry>({ kind, enabled = true, handler }: Entry<Key>): Entry<Key> {
    return { kind, enabled, handler };
}

const alphaEntry = makeEntry({
    kind: "alpha",
    handler: payload => {
        payload.alpha;
    },
});
"#,
    );

    for expected in [
        r#"declare const alphaEntry: {"#,
        r#"    readonly kind: "alpha";"#,
        r#"    readonly enabled?: boolean;"#,
        r#"    readonly handler: (payload: AlphaEvent) => void;"#,
    ] {
        assert!(
            output.contains(expected),
            "expected correlated alias call surface to include `{expected}`: {output}"
        );
    }
    assert!(
        !output.contains(r#"declare const alphaEntry: Entry<"alpha">;"#),
        "correlated alias call should expand the concrete surface instead of re-emitting the alias application: {output}"
    );
}

#[test]
fn fix_local_overloaded_call_uses_matching_literal_signature_return() {
    let output = emit_dts_with_usage_analysis(
        r#"
interface Base {
    x: string;
    y: number;
}
interface HelloOrWorld extends Base {
    p1: boolean;
}
interface JustHello extends Base {
    p2: boolean;
}
interface JustWorld extends Base {
    p3: boolean;
}

let hello: "hello";
let world: "world";
let helloOrWorld: "hello" | "world";

function f(p: "hello"): JustHello;
function f(p: "hello" | "world"): HelloOrWorld;
function f(p: "world"): JustWorld;
function f(p: string): Base;
function f(...args: any[]): any {
    return undefined;
}

let fResult1 = f(hello);
let fResult2 = f(world);
let fResult3 = f(helloOrWorld);

function g(p: string): Base;
function g(p: "hello"): JustHello;
function g(p: "hello" | "world"): HelloOrWorld;
function g(p: "world"): JustWorld;
function g(...args: any[]): any {
    return undefined;
}

let gResult1 = g(hello);
let gResult2 = g(world);
let gResult3 = g(helloOrWorld);
"#,
    );

    for expected in [
        "declare let fResult1: JustHello;",
        "declare let fResult2: JustWorld;",
        "declare let fResult3: HelloOrWorld;",
        "declare let gResult1: JustHello;",
        "declare let gResult2: JustWorld;",
        "declare let gResult3: Base;",
    ] {
        assert!(
            output.contains(expected),
            "expected overload call return `{expected}`: {output}"
        );
    }
}
