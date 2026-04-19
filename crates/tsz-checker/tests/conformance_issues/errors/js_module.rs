use crate::core::*;

#[test]
fn test_ts2536_still_emitted_for_concrete_invalid_index() {
    let code = r#"
type Obj = { a: string; b: number; };
type Bad = Obj["c"];
"#;
    let diagnostics = compile_and_get_diagnostics(code);
    let has_2536 = diagnostics.iter().any(|(code, _)| *code == 2536);
    assert!(
        has_2536,
        "TS2536 should be emitted for concrete invalid index 'c'.\nActual: {diagnostics:?}"
    );
}

// =============================================================================
// Interface Merged Declaration Property-vs-Method TS2300
// =============================================================================

#[test]
fn test_ts2300_interface_property_vs_method_conflict() {
    // When merged interfaces have the same member name as both a property
    // and a method, tsc emits TS2300 "Duplicate identifier" on both.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface A {
    foo: () => string;
}
interface A {
    foo(): number;
}
",
    );
    let ts2300_count = diagnostics.iter().filter(|(c, _)| *c == 2300).count();
    assert!(
        ts2300_count >= 2,
        "Expected at least 2 TS2300 for property-vs-method conflict, got {ts2300_count}.\nDiagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2300_for_method_overloads_in_merged_interfaces() {
    // Method overloads across merged interfaces are valid and should NOT
    // produce TS2300. Multiple methods with the same name are allowed.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface B {
    bar(x: number): number;
}
interface B {
    bar(x: string): string;
}
",
    );
    let ts2300_count = diagnostics.iter().filter(|(c, _)| *c == 2300).count();
    assert!(
        ts2300_count == 0,
        "Method overloads should not produce TS2300, got {ts2300_count}.\nDiagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2304_for_method_type_params_in_merged_interface() {
    // Method signatures with their own type parameters should not cause
    // TS2304 "Cannot find name" during merged interface checking.
    let diagnostics = compile_and_get_diagnostics(
        r"
interface C<T> {
    foo(x: T): T;
}
interface C<T> {
    foo<W>(x: W, y: W): W;
}
",
    );
    let ts2304_count = diagnostics.iter().filter(|(c, _)| *c == 2304).count();
    assert!(
        ts2304_count == 0,
        "Method type params should not cause TS2304, got {ts2304_count}.\nDiagnostics: {diagnostics:?}"
    );
}

// ─── TS2427: Interface name cannot be predefined type ───

/// `interface void {}` should emit TS2427, not TS1005.
/// Previously the parser rejected `void` as a reserved word, preventing
/// the checker from emitting the correct TS2427 diagnostic.
#[test]
fn ts2427_interface_void_name() {
    let diagnostics = compile_and_get_diagnostics("interface void {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface void {{}}`: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 1005),
        "Should not emit TS1005 for `interface void {{}}`: {diagnostics:?}"
    );
}

/// `interface null {}` should emit TS2427.
#[test]
fn ts2427_interface_null_name() {
    let diagnostics = compile_and_get_diagnostics("interface null {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface null {{}}`: {diagnostics:?}"
    );
}

/// `interface string {}` should emit TS2427 for predefined type name.
#[test]
fn ts2427_interface_string_name() {
    let diagnostics = compile_and_get_diagnostics("interface string {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface string {{}}`: {diagnostics:?}"
    );
}

/// `interface undefined {}` should emit TS2427.
#[test]
fn ts2427_interface_undefined_name() {
    let diagnostics = compile_and_get_diagnostics("interface undefined {}");
    assert!(
        has_error(&diagnostics, 2427),
        "Expected TS2427 for `interface undefined {{}}`: {diagnostics:?}"
    );
}

/// Regular interface names should not emit TS2427.
#[test]
fn no_ts2427_for_regular_interface_name() {
    let diagnostics = compile_and_get_diagnostics("interface Foo {}");
    assert!(
        !has_error(&diagnostics, 2427),
        "Should not emit TS2427 for `interface Foo {{}}`: {diagnostics:?}"
    );
}

/// After `f ??= (a => a)`, f should be narrowed to exclude undefined.
/// The ??= creates a two-branch flow (short-circuit when non-nullish vs assignment),
/// and on the assignment branch the variable holds exactly the RHS value.
/// Regression test for false-positive TS2722.
#[test]
fn logical_nullish_assignment_narrows_out_undefined() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo(f?: (a: number) => void) {
    f ??= (a => a);
    f(42);
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 after f ??= ...: {diagnostics:?}"
    );
}

/// `if (x &&= y)` should narrow both x and y to truthy in the then-branch.
/// For &&=, the result is y when x was truthy, so if the if-condition is truthy
/// then y must be truthy.
#[test]
fn logical_and_assignment_condition_narrows_truthy() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface T { name: string; original?: T }
declare const v: number;
function test(thing: T | undefined, def: T | undefined) {
    if (thing &&= def) {
        thing.name;
        def.name;
    }
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 18048),
        "Should not emit TS18048 inside if(thing &&= def) truthy branch: {diagnostics:?}"
    );
}

/// Test: IIFE callee gets contextual return type wrapping.
/// When a function expression is immediately invoked and the call expression
/// has a contextual type (from a variable annotation), the function expression
/// should infer its return type from the contextual type, enabling contextual
/// typing of callback parameters in the return value.
/// Without wrapping the contextual type into a callable `() => T`, the
/// function type resolver cannot extract the return type.
#[test]
fn test_iife_contextual_return_type_for_callback() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // The IIFE `(() => n => n + 1)()` has contextual type `(n: number) => number`.
    // The inner arrow `n => n + 1` needs `n` contextually typed as `number`.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const result: (n: number) => number = (() => n => n + 1)();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "IIFE should contextually type callback return value params. Got: {relevant:#?}"
    );
}

/// Test: Parenthesized IIFE callee also gets contextual return type.
/// Same as above but with `(function(){})()` syntax (parens around callee).
#[test]
fn test_iife_parenthesized_contextual_return_type() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const result: (n: number) => number = (function() { return function(n) { return n + 1; }; })();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Parenthesized IIFE should contextually type return value params. Got: {relevant:#?}"
    );
}

#[test]
fn test_async_iife_block_body_preserves_contextual_tuple_return() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const test1: Promise<[one: number, two: string]> = (async () => {
    return [1, 'two'];
})();
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Async IIFE block body should preserve contextual tuple return typing. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_augmented_error_constructor_subtypes_remain_assignable() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface ErrorConstructor {
  captureStackTrace(targetObject: Object, constructorOpt?: Function): void;
}

declare var x: ErrorConstructor;
x = Error;
x = RangeError;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Augmented ErrorConstructor subtypes should stay assignable. Got: {diagnostics:#?}"
    );
}

/// Test: IIFE with object return type provides contextual typing for nested callbacks.
#[test]
fn test_iife_contextual_return_type_object_with_callback() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Handler = { handle: (x: string) => number };
const h: Handler = (() => ({ handle: x => x.length }))();
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "IIFE returning object with callback should contextually type callback params. Got: {relevant:#?}"
    );
}

#[test]
fn test_iife_optional_parameters_preserve_undefined_in_body() {
    let options = CheckerOptions {
        no_implicit_any: true,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
((j?) => j + 1)(12);
((k?) => k + 1)();
((l, o?) => l + o)(12);
"#,
        options,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts18048_count = relevant.iter().filter(|(code, _)| *code == 18048).count();
    assert!(
        ts18048_count >= 3,
        "Expected TS18048 for optional IIFE params used in arithmetic. Got: {relevant:#?}"
    );
}

// =========================================================================
// Array spread into variadic tuple rest params — no false TS2556
// =========================================================================

#[test]
fn test_array_spread_into_variadic_tuple_rest_no_ts2556() {
    // Spreading an array into a function with variadic tuple rest parameter
    // (e.g., ...args: [...T, number]) should NOT emit TS2556.
    // The variadic_tuple_element_type function must correctly handle the
    // rest parameter probe at large indices.
    let source = r#"
declare function foo<T extends unknown[]>(x: number, ...args: [...T, number]): T;
function bar<U extends unknown[]>(u: U) {
    foo(1, ...u, 2);
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for array spread to variadic tuple rest param. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_into_variadic_tuple_curry_pattern_no_ts2556() {
    // The curry pattern: spreading generic array params into a function call
    // within the body. This was a false TS2556 because the rest parameter
    // probe returned None for variadic tuple parameters.
    let source = r#"
function curry<T extends unknown[], U extends unknown[], R>(
    f: (...args: [...T, ...U]) => R, ...a: T
) {
    return (...b: U) => f(...a, ...b);
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for spread of generic arrays into variadic tuple. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_spread_into_generic_variadic_round2_no_ts2556() {
    // Generic function with context-sensitive callback arg — tests the
    // Round 2 closure correctly falls back to ctx_helper for rest param
    // probes at large indices.
    let source = r#"
declare function call<T extends unknown[], R>(
    ...args: [...T, (...args: T) => R]
): [T, R];
declare const sa: string[];
call(...sa, (...x) => 42);
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2556),
        "Should not emit TS2556 for spread+callback in generic variadic. Got: {diagnostics:?}"
    );
}

#[test]
fn test_zero_param_callback_partial_return_participates_in_round1_inference() {
    let source = r#"
interface Foo<A> {
    a: A;
    b: (x: A) => void;
}

declare function canYouInferThis<A>(fn: () => Foo<A>): A;

const result = canYouInferThis(() => ({
    a: { BLAH: 33 },
    b: x => { }
}));

result.BLAH;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2345),
        "Round 1 should infer from the non-sensitive callback return member and avoid TS2345. Got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Round 2 should contextualize the callback parameter after inference. Got: {diagnostics:?}"
    );
}

/// Return type inference should use narrowed types from type guard predicates.
/// When `isFunction(item)` narrows `item` to `Extract<T, Function>` inside an
/// if-block, the inferred return type should reflect the narrowed type, not the
/// declared parameter type `T`. Without evaluating the if-condition during
/// return type collection, flow narrowing can't find the type predicate.
#[test]
fn return_type_inference_uses_type_guard_narrowing() {
    let source = r#"
declare function isFunction<T>(value: T): value is Extract<T, Function>;

function getFunction<T>(item: T) {
    if (isFunction(item)) {
        return item;
    }
    throw new Error();
}

function f12(x: string | (() => string) | undefined) {
    const f = getFunction(x);
    f();
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 for calling result of type-guard-narrowed return. Got: {diagnostics:?}"
    );
}

/// Non-generic type guard predicates should also work in return type inference.
/// User-defined type guards with non-generic predicate types should also
/// produce correct narrowing during return type inference.
#[test]
fn return_type_inference_uses_non_generic_type_guard() {
    let source = r#"
interface Callable { (): string; }
declare function isCallable(value: unknown): value is Callable;

function getCallable(item: string | Callable | undefined) {
    if (isCallable(item)) {
        return item;
    }
    throw "not callable";
}

declare const x: string | Callable | undefined;
const f = getCallable(x);
const result: string = f();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2722),
        "Should not emit TS2722 for non-generic type guard return inference. Got: {diagnostics:?}"
    );
}

/// Switch clause narrowing must use the narrowed type from preceding control flow.
/// When `if (c !== undefined)` narrows a union, the switch default should see the
/// narrowed type (without undefined), not the original declared type.
#[test]
fn test_switch_clause_uses_narrowed_type_from_preceding_if() {
    let source = r#"
interface A { kind: 'A'; }
interface B { kind: 'B'; }
type C = A | B | undefined;
declare var c: C;
if (c !== undefined) {
    switch (c.kind) {
        case 'A': break;
        case 'B': break;
        default: let x: never = c;
    }
}
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        !has_error(&diagnostics, 2322),
        "Switch default should narrow to `never` after exhaustive cases when preceded by undefined-excluding guard. Got: {diagnostics:?}"
    );
}

/// Switch clause narrowing must propagate truthiness narrowing.
/// After `if (c)` (truthy check), switch cases should see the non-falsy type.
#[test]
fn test_switch_clause_uses_truthiness_narrowing() {
    let source = r#"
interface A { kind: 'A'; }
interface B { kind: 'B'; }
type C = A | B | null | undefined;
declare var c: C;
if (c) {
    switch (c.kind) {
        case 'A': break;
        case 'B': break;
        default: let x: never = c;
    }
}
"#;
    let options = CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(source, options);
    assert!(
        !has_error(&diagnostics, 2322),
        "Switch default should narrow to `never` after exhaustive cases when preceded by truthiness guard. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_from_contextual_destructuring_does_not_emit_ts2339() {
    let source = r#"
interface A { a: string; }
interface B { b: string; }
declare function from<T, U>(items: Iterable<T> | ArrayLike<T>, mapfn: (value: T) => U): U[];
const inputB: B[] = [];
const result: A[] = from(inputB, ({ b }): A => ({ a: b }));
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Contextual destructuring in Array.from callback should not emit TS2339. Got: {diagnostics:?}"
    );
}

#[test]
fn test_array_from_iterable_uses_lib_default_type_arguments_without_ts2314() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        &[(
            "/test.ts",
            r#"
interface A { a: string; }
const inputA: A[] = [];

function getEither<T>(in1: Iterable<T>, in2: ArrayLike<T>) {
    return Math.random() > 0.5 ? in1 : in2;
}

const inputARand = getEither(inputA, { length: 0 } as ArrayLike<A>);
const result: A[] = Array.from(inputARand);
"#,
        )],
        "/test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "Expected Array.from Iterable<T> inputs to use lib default type arguments without TS2314. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_type_alias_inside_instance_method_does_not_emit_ts2526() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class MyClass {
    t: number;

    fn() {
        type ContainingThis = this;
        let value: ContainingThis = this;
        return value.t;
    }
}
"#,
        CheckerOptions {
            no_implicit_any: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2526),
        "Expected `type T = this` inside an instance method to be valid. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_destructuring_union_with_undefined_reports_ts2339() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
const fInferred = ({ a = 0 } = {}) => a;
const fAnnotated: typeof fInferred = ({ a = 0 } = {}) => a;

declare var t: { s: string } | undefined;
const { s } = t;
function fst({ s } = t) { }
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    let ts2339_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        2,
        "Expected TS2339 on both destructuring sites from contextualTypeForInitalizedVariablesFiltersUndefined.ts. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339_messages.iter().all(|message| message
            .contains("Property 's' does not exist on type '{ s: string; } | undefined'.")),
        "Expected TS2339 to preserve the union-with-undefined message for both destructuring sites. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_binding_default_initializer_does_not_suppress_missing_property_ts2339() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
declare const source: {};
const { x = 1 } = source;
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    let ts2339_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected TS2339 even when the binding element has a default initializer. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339_messages[0].contains("Property 'x' does not exist on type '{}'."),
        "Expected TS2339 to report the missing property on '{{}}'. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_empty_object_literal_missing_property_formats_as_empty_object() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A { a: string; }
const value: A = {};
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    let message = diagnostic_message(&diagnostics, 2741)
        .expect("expected TS2741 for assignment from empty object literal");
    assert!(
        message.contains("type '{}'"),
        "Expected TS2741 to format the empty object literal as '{{}}'. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !message.contains("{ ; }"),
        "Did not expect the legacy '{{ ; }}' empty-object formatting. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_like_length_only_assignment_does_not_emit_ts2322() {
    let source = r#"
interface A { a: string; }
const inputALike: ArrayLike<A> = { length: 0 };
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        !has_error(&diagnostics, 2322),
        "ArrayLike<T> assignment from a length-only object should be accepted. Got: {diagnostics:?}"
    );
}
