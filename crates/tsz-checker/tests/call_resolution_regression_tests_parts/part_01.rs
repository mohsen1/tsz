#[test]
fn property_call_through_type_alias() {
    // Method call on an object typed through a type alias.
    let source = r#"
type Handler = { handle(input: string): boolean };
declare let h: Handler;
let ok: boolean = h.handle("test");
"#;
    assert!(
        no_errors(source),
        "Method call through type alias should resolve correctly"
    );
}

#[test]
fn generic_call_with_multiple_constraints() {
    // Generic function with multiple constrained type params.
    let source = r#"
declare function merge<A extends object, B extends object>(a: A, b: B): A & B;
let result = merge({ x: 1 }, { y: 2 });
let x: number = result.x;
let y: number = result.y;
"#;
    assert!(
        no_errors(source),
        "Generic call with multiple constraints should infer intersection type"
    );
}

#[test]
fn overload_with_never_param() {
    // Overload that has `never` in a param position should be skippable.
    let source = r#"
declare function test(x: never): never;
declare function test(x: string): number;
let r: number = test("ok");
"#;
    assert!(
        no_errors(source),
        "Should skip never-param overload and match string overload"
    );
}

#[test]
fn call_expression_on_conditional_type_result() {
    // Calling a value whose type comes from a conditional type.
    let source = r#"
type IsString<T> = T extends string ? (x: T) => void : never;
declare let fn1: IsString<string>;
fn1("hello");
"#;
    assert!(
        no_errors(source),
        "Calling a value typed by conditional type should work when condition is true"
    );
}

#[test]
fn property_call_on_generic_class_instance() {
    // Method call on an instance of a generic class.
    let source = r#"
declare class Container<T> {
    get(): T;
    set(value: T): void;
}
declare let c: Container<number>;
let n: number = c.get();
c.set(42);
"#;
    assert!(
        no_errors(source),
        "Method calls on generic class instance should resolve with concrete type args"
    );
}

#[test]
fn property_call_wrong_arg_type_on_generic_class() {
    let source = r#"
declare class Container<T> {
    set(value: T): void;
}
declare let c: Container<number>;
c.set("wrong");
"#;
    assert!(
        has_error(source, 2345),
        "Passing string to Container<number>.set should emit TS2345"
    );
}

// ============================================================================
// Architecture regression: query boundary coverage
// ============================================================================
// These tests ensure call.rs routes through boundary helpers rather than
// inspecting solver internals directly.

#[test]
fn generic_two_pass_inference_with_annotated_callback_param() {
    // Pre-inference from annotated callback params: when a callback is
    // context-sensitive (has unannotated params) AND has some annotated
    // params, those annotations should contribute to inference.
    let source = r#"
declare function test<T>(fn: (a: T, b: T) => void): T;
let result = test((a: number, b) => {});
let check: number = result;
"#;
    assert!(
        no_errors(source),
        "Annotated callback param should contribute to generic inference"
    );
}

#[test]
fn generic_callback_rest_annotation_does_not_emit_false_extra_ts2345() {
    let source = r#"
class C {
  test: string
}

class D extends C {
  test2: string
}

declare function test<T extends C>(a: (t: T, t1: T) => void): T
declare function testRest<T extends C>(a: (t: T, t1: T, ...ts: T[]) => void): T

test((...ts: D[]) => {})
testRest((t2, ...t3: D[]) => {})
"#;

    let diagnostics = get_diagnostics(source);
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected only the trailing rest mismatch to emit TS2345, got diagnostics={diagnostics:?}"
    );
    assert!(
        ts2345[0].1.contains("(t2: C, ...t3: D[]) => void"),
        "Expected the surviving TS2345 to be the target-rest mismatch, got {:?}",
        ts2345[0]
    );
    assert!(
        !ts2345[0].1.contains("(...ts: D[]) => void"),
        "Unexpected false-positive TS2345 for source rest callback: {:?}",
        ts2345[0]
    );
}

#[test]
fn generic_return_context_inference() {
    // Return-context substitution: when a generic call is in a contextual
    // position, the return type context should help infer type params.
    let source = r#"
declare function identity<T>(x: T): T;
let result: string = identity("hello");
"#;
    assert!(
        no_errors(source),
        "Return context should help infer T = string from contextual type"
    );
}

#[test]
fn union_callee_not_treated_as_overloads() {
    // Union callee types must NOT be treated as overloads.
    // Overload resolution succeeds if ANY signature matches, but union
    // call semantics require ALL members to accept the call.
    let source = r#"
type F1 = (a: string) => void;
type F2 = (a: string, b: number) => void;
declare let f: F1 | F2;
f("hello");
"#;
    // This should emit an error because F2 requires 2 args
    let codes = get_codes(source);
    // The error could be TS2554 (wrong arg count) or TS2349 (not callable)
    assert!(
        !codes.is_empty(),
        "Union callee should require all members to accept the call, got: {codes:?}",
    );
}

#[test]
fn overload_resolution_first_match_wins() {
    // Overload resolution should pick the first matching signature.
    let source = r#"
declare function f(x: string): string;
declare function f(x: number): number;
declare function f(x: string | number): string | number;
let r1: string = f("hello");
let r2: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick first matching signature"
    );
}

#[test]
fn generic_excess_property_skip_for_type_param() {
    // When a generic param is a type parameter (T), excess property
    // checking should be skipped because T captures the full object type.
    let source = r#"
interface Named { name: string; }
declare function parrot<T extends Named>(obj: T): T;
parrot({ name: "hello", extra: true });
"#;
    assert!(
        no_errors(source),
        "Excess property check should be skipped for type parameter params"
    );
}

#[test]
fn property_call_through_optional_chain_on_nullable() {
    // Optional chaining should strip nullish from the callee before
    // attempting call resolution.
    let source = r#"
declare let obj: { method(x: number): string } | null;
let r = obj?.method(42);
"#;
    assert!(
        no_errors(source),
        "Optional chain call on nullable should work"
    );
}

#[test]
fn overload_resolution_with_generic_and_nongeneric() {
    // Mixed generic/non-generic overloads should resolve correctly.
    let source = r#"
declare function wrap(x: string): string;
declare function wrap<T>(x: T): T[];
let s: string = wrap("hello");
"#;
    assert!(
        no_errors(source),
        "Non-generic overload should be preferred for string arg"
    );
}

#[test]
fn call_with_spread_from_array_to_rest_param() {
    // Spreading an array into a rest parameter should be valid.
    let source = r#"
declare function sum(...nums: number[]): number;
let arr: number[] = [1, 2, 3];
sum(...arr);
"#;
    assert!(
        no_errors(source),
        "Spreading array into rest param should be valid"
    );
}

#[test]
fn construct_call_prefers_generic_signature() {
    // For new expressions with overloaded constructors, prefer generic
    // signatures so type parameters can be inferred.
    let source = r#"
declare class Box<T> {
    constructor();
    constructor(value: T);
}
let b = new Box(42);
"#;
    assert!(
        no_errors(source),
        "Constructor overload resolution should work"
    );
}

#[test]
fn stable_call_recovery_return_type_on_mismatch() {
    // When type argument count is wrong, recovery should still produce
    // a usable return type for downstream checking.
    let source = r#"
declare function f<T>(x: T): T;
let r = f<string, number>("hello");
"#;
    // TS2558 for wrong type arg count, but should still recover return type
    assert!(
        has_error(source, 2558),
        "Wrong type arg count should emit TS2558"
    );
}

#[test]
fn overload_candidate_callback_body_errors_do_not_suppress_legitimate_errors() {
    // Regression test: overload resolution must not suppress legitimate
    // callback body errors like TS2454 (used before assigned) when
    // rejecting overload candidates due to type-relation errors.
    // The callback body error rejection only considers type-relation codes
    // (TS2322, TS2345, TS2339, TS2769), not TS2454.
    let source = r#"
declare function foo(func: (x: string, y: string) => any): boolean;
declare function foo(func: (x: string, y: number) => any): string;

var out = foo((x, y) => {
    var bar: { (a: typeof x): void; (b: typeof y): void; };
    return bar;
});
"#;
    let codes = get_codes(source);
    // TS2454 should still be emitted for the unassigned `bar` variable,
    // not suppressed by overload candidate rejection.
    assert!(
        codes.contains(&2454),
        "TS2454 for unassigned 'bar' should not be suppressed by overload resolution, got: {codes:?}"
    );
}

#[test]
fn union_multi_overload_incompatible_this_emits_ts2349() {
    // When a union has multiple members each with multiple overloads, and no
    // compatible pair of signatures exists across members, the union is not
    // callable (TS2349). This matches tsc's getUnionSignatures behavior.
    // Regression test for unionTypeCallSignatures6.ts line 39: x1.f3()
    let source = r#"
type A = { a: string };
type B = { b: number };
type C = { c: string };
type D = { d: number };

interface F3 {
    (this: A): void;
    (this: B): void;
}
interface F4 {
    (this: C): void;
    (this: D): void;
}

declare var x1: A & C & {
    f3: F3 | F4;
};
x1.f3();
    "#;
    assert!(
        has_error(source, 2349),
        "Union of multi-overload interfaces with no compatible this-pairs should emit TS2349 (not callable), matching tsc's getUnionSignatures"
    );
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2684),
        "TS2684 must not fire — tsc routes the no-compat case through TS2349 only. Got: {codes:?}"
    );
}

#[test]
fn union_multi_overload_incompatible_without_this_emits_ts2349() {
    let source = r#"
interface F1 {
    (x: string): void;
    (x: number): void;
}
interface F2 {
    (x: boolean): void;
    (x: undefined): void;
}
declare let f: F1 | F2;
f("hello");
"#;
    assert!(
        has_error(source, 2349),
        "Union of incompatible non-this multi-overload interfaces should remain not callable"
    );
}

#[test]
fn union_multi_overload_compatible_this_no_ts2349() {
    // When multi-overload union members DO have a compatible signature pair,
    // the union IS callable (no TS2349). The this-type is intersected.
    // Regression test for unionTypeCallSignatures6.ts line 40: x1.f4()
    let source = r#"
type A = { a: string };
type B = { b: number };
type C = { c: string };

interface F3 {
    (this: A): void;
    (this: B): void;
}
interface F5 {
    (this: C): void;
    (this: B): void;
}

declare var x2: A & B & {
    f4: F3 | F5;
};
x2.f4();
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2349),
        "Union of multi-overload interfaces with compatible this-pair (B) should NOT emit TS2349, got: {codes:?}"
    );
}

#[test]
fn union_single_and_multi_overload_matching_this_no_ts2349() {
    // Regression test for unionTypeCallSignatures6.ts line 37: `F1 | F3`
    // is callable because F3 has an overload with the same `this: A` as F1.
    let source = r#"
type A = { a: string };
type B = { b: number };
type C = { c: string };

type F1 = (this: A) => void;
interface F3 {
    (this: A): void;
    (this: B): void;
}

declare var x1: A & C & {
    f1: F1 | F3;
};
x1.f1();
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2349),
        "Union of single signature and overload set with matching `this` should \
         be callable, got: {codes:?}"
    );
}

#[test]
fn union_single_and_multi_overload_intersected_this_no_ts2349() {
    // Regression test for unionTypeCallSignatures6.ts line 38: `F1 | F4`
    // is callable because the receiver satisfies F1's `this: A` and F4's
    // selected overload `this: C`.
    let source = r#"
type A = { a: string };
type C = { c: string };
type D = { d: number };

type F1 = (this: A) => void;
interface F4 {
    (this: C): void;
    (this: D): void;
}

declare var x1: A & C & {
    f2: F1 | F4;
};
x1.f2();
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2349),
        "Union of single signature and overload set should intersect `this` \
         types rather than reporting not-callable, got: {codes:?}"
    );
}

#[test]
fn block_body_callback_emits_ts2345_not_ts2322_for_return_type_mismatch() {
    // When a block-bodied callback's return type doesn't match the expected
    // parameter type, tsc emits TS2345 at the argument level ("Argument of
    // type ... is not assignable to parameter of type ..."), not TS2322 at
    // the return statement. The elaboration path in handle_call_result must
    // skip callback return elaboration for block-bodied callbacks so the
    // outer TS2345 is emitted instead of an inner TS2322.
    // Use simple types that don't require built-in lib.d.ts
    let source = r#"
interface Target { tag: "target" }

declare function callWithCallback<T>(f: (x: number) => T): T;

// Block-bodied callback whose return type (string) doesn't match T=Target
var r1 = callWithCallback<Target>((x) => { return "hello" as string; });
"#;
    let diags = get_diagnostics(source);
    let ts2322_count = diags.iter().filter(|(code, _)| *code == 2322).count();
    let ts2345_count = diags.iter().filter(|(code, _)| *code == 2345).count();
    assert_eq!(
        ts2322_count, 0,
        "Block-bodied callback should NOT emit TS2322 for return type mismatch. Diagnostics: {diags:?}"
    );
    assert!(
        ts2345_count >= 1,
        "Block-bodied callback should emit TS2345 for argument type mismatch. Diagnostics: {diags:?}"
    );
}

/// Generic class constructor type must not be decomposed into a plain Function
/// during `instantiate_generic_function_argument_against_target`. When a generic
/// class with a constructor and static methods is passed to a `typeof Class`
/// parameter in generic overloaded resolution, the Callable type (construct
/// signatures + property members) must be preserved. Without this fix, the
/// Callable is decomposed into a Function (just the construct signature),
/// losing static members and causing false TS2769/TS2345 errors.
///
/// Regression: bluebirdStaticThis.ts conformance test
#[test]
fn generic_class_typeof_arg_in_generic_overload_no_false_ts2769() {
    let source = r#"
        interface Thing<R> {
            value: R;
        }

        declare class Prom<T> {
            constructor(x: T);
            static foo<R>(dit: typeof Prom, fn: () => Thing<R>): Prom<R>;
            static foo<R>(dit: typeof Prom, fn: () => R): Prom<R>;
        }

        interface Bar { a: number; }
        declare var bar: Bar;

        Prom.foo(Prom, () => bar);
    "#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769) && !codes.contains(&2345),
        "Should NOT emit TS2769 or TS2345 for passing generic class to typeof param in overloaded call.\n\
         Got: {codes:?}"
    );
}

/// Same as above but with different type parameter names on class vs overloads.
#[test]
fn generic_class_typeof_arg_different_type_param_names() {
    let source = r#"
        interface Thing<X> {
            value: X;
        }

        declare class MyClass<T> {
            constructor(x: T);
            static make<R>(ctor: typeof MyClass, fn: () => Thing<R>): MyClass<R>;
            static make<R>(ctor: typeof MyClass, fn: () => R): MyClass<R>;
        }

        interface Foo { a: number; }
        declare var foo: Foo;

        MyClass.make(MyClass, () => foo);
    "#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769) && !codes.contains(&2345),
        "Different type param names should NOT cause false TS2769/TS2345.\n\
         Got: {codes:?}"
    );
}

/// Non-generic class with constructor should still work with typeof parameter
/// in generic overloads (pre-existing behavior, sanity check).
#[test]
fn non_generic_class_typeof_arg_in_generic_overload() {
    let source = r#"
        declare class Prom {
            constructor(x: number);
            static foo<R>(dit: typeof Prom, fn: () => R): void;
            static foo(dit: typeof Prom, fn: () => string): void;
        }

        Prom.foo(Prom, () => 42);
    "#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769) && !codes.contains(&2345),
        "Non-generic class should NOT produce false errors for typeof arg.\n\
         Got: {codes:?}"
    );
}

/// Regression test for TypeScript/tests/cases/compiler/arrayFrom.ts:
///
/// When inferring a generic type parameter `T` from a call argument whose type
/// is an instantiation of an interface that *inherits* from the parameter's
/// interface (e.g., source `MyChildIter<A>` extends `MyMidIter<A,...>` extends
/// `MyBaseIter<A,...>`, target `MyBaseIter<T>`), the constraint walker must
/// still pair signatures even when no source overload is *strictly assignable*
/// to the target's erased form.
///
/// Before the fix, `select_signature_for_target` returned `None` whenever the
/// pre-check `is_assignable_to(source_fn, target_erased)` failed for every
/// source overload — which is the typical situation for inheritance-merged
/// callable overloads (e.g. `[Symbol.iterator]` returning the more derived
/// interface). This silently dropped inference, so the placeholder `T` only
/// received its contextual return-type seed (e.g. `B` from `let r: B[] =`),
/// and subsequent argument assignability check rejected the call as
/// TS2769 "No overload matches this call". tsc instead pairs the LAST source
/// overload with the target overload (matching `inferFromSignaturesOfType`).
///
/// With the fix, `constrain_matching_signatures` falls back to the last
/// non-generic source signature (gated by an arity-compatibility check),
/// allowing inference to populate `T` from the argument's element type. The
/// assignment then surfaces a TS2322 "A[] is not assignable to B[]" — matching
/// tsc's behaviour exactly.
#[test]
fn inheritance_merged_overload_pairs_last_source_sig_for_inference() {
    let source = r#"
        interface MyBaseIter<T, R = any, N = any> {
            [Symbol.iterator](): MyBaseIter<T, R, N>;
        }
        interface MyMidIter<T, R = unknown, N = unknown> extends MyBaseIter<T, R, N> {
            [Symbol.iterator](): MyMidIter<T, R, N>;
        }
        interface MyChildIter<T> extends MyMidIter<T, any, unknown> {
            [Symbol.iterator](): MyChildIter<T>;
        }

        function from<T>(x: MyBaseIter<T>): T[];
        function from(x: any): any { return null as any; }

        interface A { a: string; }
        interface B { b: string; }
        declare const aIter: MyChildIter<A>;

        const result: B[] = from(aIter);
    "#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2322),
        "Cross-interface inference should produce TS2322 (A[] not assignable to B[]),\n\
         not TS2769. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2769),
        "Cross-interface inference must NOT emit TS2769 (no overload matches).\n\
         Got: {codes:?}"
    );
}

#[test]
fn union_receiver_inherited_promise_this_return_preserves_class_identity() {
    let source = r#"
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}
interface Promise<T> extends PromiseLike<T> {}

declare class Foo {
    doThing(): Promise<this>;
}
declare class Bar extends Foo {
    bar: number;
}
declare class Baz extends Foo {
    baz: number;
}

declare const a: Bar | Baz;
a.doThing().then((result: Bar | Baz) => {});
"#;

    assert!(
        no_errors_with_options(
            source,
            &CheckerOptions {
                strict: true,
                ..CheckerOptions::default()
            },
        ),
        "Inherited Promise<this> calls on union receivers should keep each \
         substituted class instance assignable to the matching nominal class"
    );
}

// ============================================================================
// Overload resolution composes argument-inferred and return-context-inferred
// type parameters when picking contextual types for callback arguments.
// ============================================================================

#[test]
fn overload_with_outer_contextual_type_preserves_arg_inferred_type_params() {
    // Mirrors the failure surface from `Array.from<T,U>(arrayLike: ArrayLike<T>,
    // mapfn: (v: T, k: number) => U): U[]` when the call site has both an outer
    // contextual return type (binding `U` via the return type) AND non-callback
    // arguments (binding `T` via the iterable argument). Both inferences must
    // contribute to the contextual type used when checking the callback body —
    // otherwise the callback parameter gets the unresolved type parameter,
    // producing TS2339 / TS2769 instead of compiling cleanly.
    let source = r#"
interface I {
    call<T>(xs: T[]): T[];
    call<T, U>(xs: T[], f: (v: T, k: number) => U, thisArg?: any): U[];
}
declare const i: I;
interface A { a: string; }
interface B { b: string; }
declare const inputB: B[];
const r: A[] = i.call(inputB, ({ b }): A => ({ a: b }));
"#;
    assert!(
        no_errors(source),
        "two-arg overload `call<T,U>(T[], (v:T,k:number)=>U):U[]` with outer \
         contextual `A[]` must compose round-1 `T=B` (from `inputB:B[]`) with \
         return-context `U=A` (from `A[]`) when forming the callback's \
         contextual type; got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn overload_with_outer_contextual_type_no_spurious_ts2769() {
    // Variant of the previous test using a `(v: T) => U` callback without
    // destructuring or a contextually-annotated return type. The inner type
    // parameter `T` must still be inferred from the iterable argument so the
    // `x.b` member access in the callback body resolves; otherwise tsz emits
    // a spurious TS2339 ("Property 'b' does not exist on type 'T'") and
    // bubbles it up to a TS2769 on the call site.
    let source = r#"
interface I {
    call<T>(xs: { readonly length: number; readonly [n: number]: T }): T[];
    call<T, U>(xs: { readonly length: number; readonly [n: number]: T },
                f: (v: T, k: number) => U, thisArg?: any): U[];
}
declare const i: I;
interface B { b: string; }
declare const inputB: { readonly length: number; readonly [n: number]: B };
const r: string[] = i.call(inputB, (x) => x.b);
const r2: number[] = i.call(inputB, (x) => x.b);
"#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 for `string[]` not assignable to `number[]`; got: {:?}",
        get_diagnostics(source)
    );
    assert!(
        !codes.contains(&2769),
        "must not emit TS2769 — the overload resolves; the only error is the \
         outer-contextual return-type mismatch (TS2322); got: {:?}",
        get_diagnostics(source)
    );
    assert!(
        !codes.contains(&2339),
        "must not emit TS2339 — `x` must be inferred as `B` (not unresolved \
         type parameter) when the callback body is checked; got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn contextual_overload_callback_list_infers_generic_result() {
    let source = r#"
interface Callback<T> {
    (error: null, result: T): unknown;
    (error: Error, result: null): unknown;
}

interface Error {
    message: string;
}

interface Task<T> {
    (callback: Callback<T>): unknown;
}

declare function setTimeout(handler: () => void, timeout: number): void;

declare function series<T>(tasks: Task<T>[], callback: Callback<T[]>): void;

series([
    cb => setTimeout(() => cb(null, 1), 300),
    cb => setTimeout(() => cb(null, 2), 200),
    cb => setTimeout(() => cb(null, 3), 100),
], (error, results) => {});
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769),
        "contextual callback overloads should infer T from successful callback \
         result calls; got diagnostics: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn overloaded_callback_unknown_accepts_success_branch() {
    let source = r#"
interface Error {
    message: string;
}
interface Callback<T> {
    (error: null, result: T): unknown;
    (error: Error, result: null): unknown;
}
declare const cb: Callback<unknown>;
cb(null, 1);
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769),
        "Callback<unknown> should accept the null/result overload; got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn contextual_task_unknown_accepts_success_branch() {
    let source = r#"
interface Error {
    message: string;
}
interface Callback<T> {
    (error: null, result: T): unknown;
    (error: Error, result: null): unknown;
}
interface Task<T> {
    (callback: Callback<T>): unknown;
}
declare const task: Task<unknown>;
const value: Task<unknown> = cb => task(() => cb(null, 1));
"#;
    let codes = get_codes(source);
    assert!(
        !codes.contains(&2769),
        "Task<unknown> contextual callback should accept the null/result overload; got: {:?}",
        get_diagnostics(source)
    );
}

// ============================================================================
// Non-generic overload deferred for generic overload with return-context (issue #6498)
// When a non-generic overload matches but returns an any-tainted type, a later
// generic overload should be preferred when it can bind its type param to the
// contextual return type (e.g., reduce<U> binding U = Output).
// ============================================================================

/// Pipeline builder pattern: Array<(a: any) => any>.reduce with contextual Output.
/// The non-generic reduce overload returns `(a: any) => any`, but a later generic
/// overload can return U = Output via return-context substitution.
#[test]
fn reduce_any_array_with_contextual_output_no_ts2322() {
    let source = r#"
type Pipe<A, B> = (a: A) => B;

class PipelineBuilder<Input, Output> {
    constructor(private fns: Array<Pipe<any, any>> = []) {}

    pipe<NextOutput>(fn: Pipe<Output, NextOutput>): PipelineBuilder<Input, NextOutput> {
        return new PipelineBuilder([...this.fns, fn]);
    }

    build(): Pipe<Input, Output> {
        return (input: Input): Output => {
            return this.fns.reduce((acc, fn) => fn(acc), input as any);
        };
    }
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "Pipeline builder reduce should not emit TS2322; got: {diags:#?}"
    );
}

/// Variant with renamed type params (K/V instead of Input/Output) to ensure
/// the fix is not hardcoded to specific names.
#[test]
fn reduce_any_array_with_contextual_output_renamed_type_params_no_ts2322() {
    let source = r#"
type Transform<A, B> = (a: A) => B;

class Chain<K, V> {
    constructor(private steps: Array<Transform<any, any>> = []) {}

    then<W>(step: Transform<V, W>): Chain<K, W> {
        return new Chain([...this.steps, step]);
    }

    run(): Transform<K, V> {
        return (k: K): V => {
            return this.steps.reduce((acc, step) => step(acc), k as any);
        };
    }
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "Chain.run reduce should not emit TS2322 regardless of type param names; got: {diags:#?}"
    );
}

/// When there is NO contextual return type, the overload should not emit TS2322.
/// (TS7006 for unannotated callback params is expected with noImplicitAny.)
#[test]
fn reduce_any_array_no_contextual_type_no_ts2322() {
    let source = r#"
declare const fns: Array<(a: any) => any>;
const result = fns.reduce((acc: any, fn: (a: any) => any) => fn(acc), "start" as any);
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "reduce without contextual return type should not emit TS2322; got: {diags:#?}"
    );
}

/// When the array element type does NOT contain any, the non-generic overload
/// should be selected normally (the fix must not over-defer).
#[test]
fn reduce_concrete_array_element_type_uses_nongeneric_overload() {
    // Use declare to avoid needing lib.es5.d.ts in the test env.
    let source = r#"
declare const nums: number[];
const sum: number = nums.reduce((acc: number, n: number) => acc + n, 0);
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "reduce on concrete array with no any should not emit TS2322; got: {diags:#?}"
    );
}

/// Array<any> with a concrete init argument and contextual return type: the
/// generic overload should bind U to the contextual type.
#[test]
fn reduce_fully_any_array_with_contextual_string_return_no_ts2322() {
    let source = r#"
declare const arr: any[];
function process(): string {
    return arr.reduce((acc, x) => acc, "init");
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "Array<any>.reduce with contextual string return should not emit TS2322; got: {diags:#?}"
    );
}

// ============================================================================
// Overloaded callee: callback parameters contextually typed from the resolved
// signature (issue #9663). Selecting an overload via a discriminating argument
// must contextually type the callback parameter from that signature, so body
// errors (TS2339 / TS2322) surface — matching tsc — instead of being hidden by
// the lossy union contextual type used during overload selection.
// ============================================================================

#[test]
fn overloaded_callback_first_overload_bad_property_ts2339() {
    let source = r#"
declare function on(event: "click", cb: (e: { x: number }) => void): void;
declare function on(event: "key", cb: (e: { code: string }) => void): void;
on("click", e => e.code);
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|(code, msg)| *code == 2339
            && msg.contains("code")
            && msg.contains("{ x: number; }")),
        "Selecting the click overload should type `e` as {{ x: number }} and emit TS2339 on `e.code`; got: {diags:#?}"
    );
}

#[test]
fn overloaded_callback_second_overload_bad_property_ts2339() {
    // Not order-specific: selecting the second overload must also contextually
    // type the callback from that signature.
    let source = r#"
declare function on(event: "click", cb: (e: { x: number }) => void): void;
declare function on(event: "key", cb: (e: { code: string }) => void): void;
on("key", e => e.x);
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|(code, msg)| *code == 2339
            && msg.contains('x')
            && msg.contains("{ code: string; }")),
        "Selecting the key overload should type `e` as {{ code: string }} and emit TS2339 on `e.x`; got: {diags:#?}"
    );
}

#[test]
fn overloaded_callback_body_assignment_mismatch_ts2322() {
    let source = r#"
declare function on(event: "click", cb: (e: { x: number }) => void): void;
declare function on(event: "key", cb: (e: { code: string }) => void): void;
on("click", (e) => { const z: string = e.x; });
"#;
    assert!(
        has_error(source, 2322),
        "Assignment inside an overloaded callback body must be checked against the resolved signature (TS2322)"
    );
}

#[test]
fn overloaded_callback_rule_is_structural_not_name_bound() {
    // Different event literals and property names prove the fix is structural.
    let source = r#"
declare function reg(kind: "a", cb: (p: { alpha: number }) => void): void;
declare function reg(kind: "b", cb: (p: { beta: string }) => void): void;
reg("a", p => p.beta);
reg("b", p => p.alpha);
"#;
    let diags = get_diagnostics(source);
    let alpha_target = diags.iter().any(|(code, msg)| {
        *code == 2339 && msg.contains("beta") && msg.contains("{ alpha: number; }")
    });
    let beta_target = diags.iter().any(|(code, msg)| {
        *code == 2339 && msg.contains("alpha") && msg.contains("{ beta: string; }")
    });
    assert!(
        alpha_target && beta_target,
        "Both overloaded callbacks should report property errors against their own resolved parameter type; got: {diags:#?}"
    );
}

#[test]
fn overloaded_callback_valid_property_no_error() {
    // Negative case: accessing the property that exists on the resolved
    // signature's parameter must not error.
    let source = r#"
declare function on(event: "click", cb: (e: { x: number }) => void): void;
declare function on(event: "key", cb: (e: { code: string }) => void): void;
on("click", e => e.x);
on("key", e => e.code);
"#;
    assert!(
        no_errors(source),
        "Accessing the property valid for the selected overload must not error"
    );
}

#[test]
fn overloaded_callback_continuation_access_no_false_positive() {
    // A deeper-but-valid continuation access through the resolved parameter
    // type must remain error-free.
    let source = r#"
declare function on(event: "click", cb: (e: { x: { v(): number } }) => void): void;
declare function on(event: "key", cb: (e: { code: string }) => void): void;
on("click", e => e.x.v());
"#;
    assert!(
        no_errors(source),
        "Valid continuation access on the resolved parameter type must not error"
    );
}

#[test]
fn non_callback_overload_selection_still_works() {
    // Guard: overload selection without callbacks is unaffected.
    let source = r#"
declare function f(x: string): string;
declare function f(x: number): number;
const a: string = f("hi");
const b: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Plain overload selection must remain unaffected by the callback fix"
    );
}

// ---------------------------------------------------------------------------
// No-overload-match recovery result type (issue #9669).
//
// Structural rule: when a call matches no overload signature, the call
// expression's result type is the intersection of every candidate signature's
// return type (tsc's `createUnionOfSignaturesForOverloadFailure`). Disjoint
// primitive returns (`string` & `number`) collapse to `never`, which is
// assignable to any annotation and so suppresses spurious downstream cascades
// (the call already reported TS2769); structurally compatible object returns
// merge (`{ a }` & `{ b }`) so member access still resolves; uniform returns
// survive intact. Only TS2769 should fire for a hard no-match — no cascade.
// ---------------------------------------------------------------------------

/// Reported repro: failed overloaded call assigned to an incompatible type
/// must emit only TS2769, not a spurious TS2322 cascade.
#[test]
fn no_overload_match_assignment_no_cascade_ts2322() {
    let source = r#"
declare function f(x: number): string;
declare function f(x: string): number;
const c: string = f(true);
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        !codes.contains(&2322),
        "no spurious TS2322 cascade on failed overload assignment; got {codes:?}"
    );
}

/// Same rule via a function-type intersection (different surface, same
/// overload mechanism).
#[test]
fn no_overload_match_intersection_callable_no_cascade_ts2322() {
    let source = r#"
type F = ((x: number) => string) & ((x: string) => number);
declare const f: F;
const c: string = f(true);
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        !codes.contains(&2322),
        "no spurious TS2322 on failed intersection-callable assignment; got {codes:?}"
    );
}

/// Failed overload result used as an argument: no TS2345 cascade either.
#[test]
fn no_overload_match_argument_position_no_cascade_ts2345() {
    let source = r#"
declare function f(x: number): string;
declare function f(x: string): number;
declare function sink(x: 99): void;
sink(f(true));
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        !codes.contains(&2345),
        "no spurious TS2345 cascade on failed overload used as argument; got {codes:?}"
    );
}

/// Renamed function/parameters — the rule is structural, not keyed on spelling.
#[test]
fn no_overload_match_renamed_no_cascade_ts2322() {
    let source = r#"
declare function ZZ(qqq: number): string;
declare function ZZ(qqq: string): number;
const result: string = ZZ(true);
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        !codes.contains(&2322),
        "renamed overload should behave identically; got {codes:?}"
    );
}

/// Compatible object returns merge into `{ a } & { b }`: both members exist on
/// the recovery type, so member access must NOT report TS2339.
#[test]
fn no_overload_match_object_returns_merge_members_resolve() {
    let source = r#"
declare function f(x: number): { a: number };
declare function f(x: string): { b: string };
f(true).a;
f(true).b;
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        !codes.contains(&2339),
        "members of intersected object returns must resolve; got {codes:?}"
    );
}

/// A property that is absent from the intersection still reports TS2339 — the
/// recovery type is not blanket-`any`/error that silences everything.
#[test]
fn no_overload_match_object_returns_missing_member_reports_ts2339() {
    let source = r#"
declare function f(x: number): { a: number };
declare function f(x: string): { b: string };
f(true).nope;
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        codes.contains(&2339),
        "absent property on intersected returns must still report TS2339; got {codes:?}"
    );
}

/// Uniform candidate returns survive: `{ a: number }` & `{ a: number }`
/// dedups to `{ a: number }`, so assigning to an incompatible type still
/// reports TS2322 (matching tsc — the suppression is intersection-driven, not
/// a blanket no-match suppression).
#[test]
fn no_overload_match_uniform_returns_preserve_shape_and_cascade() {
    let source = r#"
declare function f(x: number): { a: number };
declare function f(x: string): { a: number };
const c: string = f(true);
"#;
    let codes = get_codes(source);
    assert!(codes.contains(&2769), "expected TS2769; got {codes:?}");
    assert!(
        codes.contains(&2322),
        "uniform object recovery type must still cascade TS2322; got {codes:?}"
    );
}

/// Negative control: a SUCCESSFUL overloaded call assigned to an incompatible
/// type must still emit TS2322 — the suppression is only for hard no-matches.
#[test]
fn successful_overload_call_still_cascades_ts2322() {
    let source = r#"
declare function g(x: number): string;
const c: number = g(1);
"#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2322),
        "successful overload assigned to incompatible type must still emit TS2322; got {codes:?}"
    );
    assert!(
        !codes.contains(&2769),
        "successful call must not report TS2769; got {codes:?}"
    );
}
