#[test]
fn test_window_alias_unknown_property_reports_ts2339() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface ConsoleLike {
    log(...args: any[]): void;
}

interface Window {
    console: ConsoleLike;
}

declare var globalThis: {};
declare var window: Window & typeof globalThis;
declare var self: Window & typeof globalThis;

window.z = 3;
self.console;
"#,
    );

    let ts2339_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected exactly one TS2339 for the missing window property alias, got: {diagnostics:?}"
    );
    assert!(
        ts2339_messages[0].contains("Property 'z' does not exist on type"),
        "Expected TS2339 to point at the missing window property, got: {diagnostics:?}"
    );
}

#[test]
fn test_array_is_array_false_branch_keeps_original_union_surface() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
var maybeArray: number | number[];

if (Array.isArray(maybeArray)) {
    maybeArray.length;
} else {
    maybeArray.toFixed();
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    let ts2339_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected exactly one TS2339 on the false branch of Array.isArray, got: {diagnostics:?}"
    );
    assert!(
        ts2339_messages[0].contains("toFixed") && ts2339_messages[0].contains("number | number[]"),
        "Expected TS2339 to preserve the original union surface, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("length")),
        "Did not expect the true branch to lose Array.isArray narrowing, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_mismatch_reports_ts2345() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo6<T>(cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}

declare var b: { new <T>(x: T, y: T): string };
var r10 = foo6(b);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for the incompatible generic constructor callback, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "Expected the single-signature generic call to stay TS2345-only, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_valid_cases_stay_clean() {
    // foo5<T>(cb) has a single argument, so the deferral logic doesn't apply.
    // These cases should remain clean.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo5<T>(cb: { new(x: T): string; new(x: number): T }) {
    return cb;
}

declare var a: { new <T>(x: T): T };
var r6 = foo5(a);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Did not expect TS2345 for valid generic constructor callback cases, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "Did not expect TS2769 for valid generic constructor callback cases, got: {diagnostics:?}"
    );
}

#[test]
fn test_overloaded_constructor_callback_infers_pairwise_construct_signatures() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo5<T>(cb: { new(x: T): string; new(x: number): T }) {
    return cb;
}

declare var a: { new (x: boolean): string; new (x: number): boolean; }
var r5 = foo5(a);

function foo6<T>(cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}

var r8 = foo6(a);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Expected overloaded constructor callbacks to infer from bottom-up signature pairs, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "Did not expect TS2769 for valid overloaded constructor callbacks, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_with_leading_arg() {
    // foo7<T>(x:T, cb) has two arguments. With the deferral fix (non-context-sensitive
    // args are no longer deferred), T is correctly inferred from arg 0. The constructor
    // suppression narrowing ensures we no longer emit a false positive TS2345 when the
    // argument is a constructor-like type application; tsc accepts both calls here.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo7<T>(x:T, cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}

declare var a: { new <T>(x: T): T };
var r13 = foo7(1, a);
declare var c: { new<T>(x: T): number; new<T>(x: number): T; }
var r14 = foo7(1, c);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // Matches tsc: both invocations type-check without TS2345.
    assert!(
        !has_error(&diagnostics, 2345),
        "Expected no TS2345 (constructor callback inference should match tsc)"
    );
}

#[test]
fn test_generic_construct_signature_arg_survives_concrete_target() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo<T>(x: new(a: T) => T) {
    return new x(null);
}

interface I {
    new <T>(x: T): T;
}
interface I2<T> {
    new (x: T): T;
}
declare var i: I;
declare var i2: I2<string>;
declare var a: {
    new <T>(x: T): T;
}

var r = foo(i);
var r2 = foo<string>(i);
var r3 = foo(i2);
var r3b = foo(a);

function foo2<T, U>(x: T, cb: new(a: T) => U) {
    return new cb(x);
}

var r4 = foo2(1, i2);
var r4b = foo2(1, a);
var r5 = foo2(1, i);
var r6 = foo2<string, string>('', i2);

function foo3<T, U>(x: T, cb: new(a: T) => U, y: U) {
    return new cb(x);
}

var r7 = foo3(null, i, '');
var r7b = foo3(null, a, '');
var r8 = foo3(1, i2, 1);
var r9 = foo3<string, string>('', i2, '');
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        3,
        "Expected only the three tsc TS2345s from genericCallWithFunctionTypedArguments2, got: {diagnostics:?}"
    );
    assert!(
        !ts2345.iter().any(|(_, message)| message.contains(
            "Argument of type 'new <T>(x: T) => T' is not assignable to parameter of type 'new (a: null) => string'"
        )),
        "Did not expect TS2345 for foo3(null, generic constructor, ''), got: {diagnostics:?}"
    );
}

#[test]
fn test_object_literal_generic_construct_signature_argument_survives_concrete_return_context() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo3<T, U>(x: T, cb: new(a: T) => U, y: U) {
    return new cb(x);
}

declare var ctor: { new <T>(x: T): T };
var ok = foo3(null, ctor, '');

declare var nongeneric: { new (x: string): string };
var err = foo3(null, nongeneric, '');
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected only the non-generic constructor argument to remain TS2345, got: {diagnostics:?}"
    );
}

/// Generic constructor calls should widen scalar literal argument types
/// (e.g., `true` → `boolean`) for TS2345 error messages, matching tsc.
/// Regression test for exportAssignmentConstrainedGenericType conformance.
#[test]
fn test_generic_constructor_widens_boolean_literal_for_error_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo<T extends {a: string; b: number}> {
    test: T;
    constructor(x: T) {}
}
var x = new Foo(true);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for boolean arg to generic constructor, got: {diagnostics:?}"
    );
    // Verify the error message uses the widened type 'boolean', not literal 'true'
    let ts2345_msg = diagnostics
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, msg)| msg.as_str())
        .unwrap_or("");
    assert!(
        ts2345_msg.contains("boolean"),
        "Expected widened 'boolean' in error message (not literal 'true'), got: {ts2345_msg}"
    );
}

#[test]
fn test_unresolved_computed_class_method_contributes_indexed_callable_type() {
    let source = r#"
declare var something: string;
export const dataSomething = `data-${something}` as const;

class WithData {
    [dataSomething]?() {
        return "something";
    }
}

const s: string = (new WithData())["ahahahaahah"]!();
const n: number = (new WithData())["ahahahaahah"]!();
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert_eq!(
        ts2322_count, 1,
        "Expected only the number assignment to fail after unresolved computed method indexing is typed, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'string' is not assignable to type 'number'")),
        "Expected the remaining failure to be the string-to-number assignment, got: {diagnostics:#?}"
    );
}

#[test]
fn test_unresolved_computed_instance_methods_produce_union_lookup_types() {
    let source = r#"
export const fieldName = Math.random() > 0.5 ? "f1" : "f2";

class Holder {
    [fieldName]() {
        return "value";
    }
    [fieldName === "f1" ? "f2" : "f1"]() {
        return 42;
    }
    static [fieldName]() {
        return { static: true };
    }
    static [fieldName]() {
        return { static: "sometimes" };
    }
}

const instanceOk: (() => string) | (() => number) = (new Holder())["x"];
const instanceBad: number = (new Holder())["x"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert_eq!(
        ts2322_count, 1,
        "Expected only the instance number assignment to fail once computed method lookups form unions, got: {diagnostics:#?}"
    );
    // Computed method types may resolve to `() => any` or a union of callable
    // types depending on the constructor type caching order. Either is acceptable
    // as long as exactly one TS2322 is emitted for the bad assignment.
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2322 && message.contains("number")),
        "Expected instance lookup assignment error to mention 'number', got: {diagnostics:#?}"
    );
}

#[test]
fn test_recursive_type_parameter_constraint_missing_args_reports_generic_name_with_params() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface A<T extends A> {}
"#,
    );

    let message = diagnostic_message(&diagnostics, 2314)
        .expect("Expected TS2314 for recursive type parameter constraint");
    assert!(
        message.contains("Generic type 'A<T>' requires 1 type argument(s)."),
        "Expected TS2314 message to include generic parameter list, got: {diagnostics:?}"
    );
}

#[test]
fn test_unresolved_computed_static_methods_produce_union_lookup_types() {
    let source = r#"
declare const f1: string;
declare const f2: string;

class Holder {
    static [f1]() {
        return { static: true };
    }
    static [f2]() {
        return { static: "sometimes" };
    }
}

const ok:
    | Holder
    | (() => { static: boolean })
    | (() => { static: string }) = Holder["x"];
const bad: number = Holder["x"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected only the bad static lookup assignment to fail once late-bound static methods are typed, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0].contains("Type 'Holder' is not assignable to type 'number'"),
        "Expected static late-bound lookup to stay non-any and still include the prototype branch in diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn test_constructor_implementation_with_more_required_params_reports_ts2394() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Customers {
    constructor(name: string);
    constructor(name: string, age: number) {}
}
"#,
    );

    assert!(
        has_error(&diagnostics, 2394),
        "Expected TS2394 for constructor overload/implementation arity mismatch, got: {diagnostics:?}"
    );
}

#[test]
fn test_repeated_generic_call_does_not_reuse_prior_inferred_literal_object() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Named { name: string }
interface Aged { age: number }

function greet<T extends Named & Aged>(person: T): string {
  return person.name;
}

greet({ name: "Alice", age: 30 });
greet({ name: "Bob" });

export {};
"#,
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for the second call missing age, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Bob") && message.contains("Alice")
        }),
        "A later generic call must not compare against a previous call's inferred literal object, got: {diagnostics:?}"
    );
}
