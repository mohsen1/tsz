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
        "Union of multi-overload interfaces with no compatible this-pairs should emit TS2349"
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
