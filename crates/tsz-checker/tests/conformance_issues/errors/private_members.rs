use crate::core::*;

#[test]
fn test_static_private_accessor_not_visible_on_derived_constructor_type() {
    let diagnostics = compile_and_get_diagnostics_named(
        "privateNameStaticAccessorssDerivedClasses.ts",
        r#"
class Base {
    static get #prop(): number { return 123; }
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
class Derived extends Base {
    static method(x: typeof Derived) {
        console.log(x.#prop);
    }
}
        "#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2339_count = relevant.iter().filter(|(code, _)| *code == 2339).count();

    assert_eq!(
        ts2339_count, 2,
        "Expected TS2339 at both static private accessor accesses through typeof Derived.\nActual diagnostics: {relevant:#?}"
    );
    assert!(
        !has_error(&relevant, 18013),
        "Should not emit TS18013 for static private accessor access through a derived constructor type.\nActual diagnostics: {relevant:#?}"
    );
}

/// TS2416 base type name should include type arguments from the extends clause,
/// not the generic parameter names. E.g., `Base<{ bar: string; }>` instead of `Base<T>`.
#[test]
fn test_ts2416_base_type_name_includes_type_arguments() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> { foo: T; }
class Derived2 extends Base<{ bar: string; }> {
    foo: { bar?: string; }
}
        "#,
    );

    let ts2416_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2416)
        .map(|(_, m)| m.as_str())
        .collect();

    assert!(
        !ts2416_messages.is_empty(),
        "Should emit TS2416 for incompatible property type.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts2416_messages[0].contains("Base<{ bar: string; }>"),
        "TS2416 should show instantiated base type 'Base<{{ bar: string; }}>', not 'Base<T>'.\n\
         Actual message: {}",
        ts2416_messages[0]
    );
}

#[test]
fn test_ts2416_uses_derived_constraint_not_shadowed_base_type_param() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Foo { foo: number = 1; }
class Base<T> { foo: T; }
class Derived<T extends Foo> extends Base<Foo> {
    [x: string]: Foo;
    foo: T;
}
        "#,
    );

    let ts2416_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2416)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2416_messages.is_empty(),
        "Derived constrained type parameter should remain in scope during override checks.\nActual TS2416 diagnostics: {ts2416_messages:?}"
    );
}

#[test]
fn test_ts2416_respects_transitive_class_type_parameter_constraints() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class C3<T> { foo: T; }
class D7<T extends U, U extends V, V> extends C3<V> {
    [x: string]: V;
    foo: T;
}
class D14<T extends U, U extends V, V extends Date> extends C3<Date> {
    [x: string]: Date;
    foo: T;
}
        "#,
    );

    let ts2411_or_ts2416: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2411 | 2416))
        .collect();

    assert!(
        ts2411_or_ts2416.is_empty(),
        "Transitive type-parameter constraints should satisfy inherited property and index-signature checks.\nActual diagnostics: {ts2411_or_ts2416:?}"
    );
}

/// Mutually recursive class hierarchy: X extends L<X>, where L<RT> extends T<RT[RT['a']]>.
/// The inherited property `a` from T<A> should be properly instantiated through the chain:
/// A → RT[RT['a']] → X[X['a']] = X['a' | 'b'] = ('a'|'b') | number.
/// Since X.a is 'a'|'b' which is assignable to 'a'|'b'|number, no TS2416 should be emitted.
#[test]
fn test_no_false_ts2416_for_mutually_recursive_class_hierarchy() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class T<A> {
    a: A;
    b: any
}
class L<RT extends { a: 'a' | 'b', b: any }> extends T<RT[RT['a']]> {
    m() { this.a }
}
class X extends L<X> {
    a: 'a' | 'b'
    b: number
    m2() {
        this.a
    }
}
        "#,
    );

    let ts2416: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2416)
        .collect();

    assert!(
        ts2416.is_empty(),
        "Should not emit false TS2416 for mutually recursive class hierarchy where inherited \
         property types are compatible after full substitution chain.\n\
         Actual TS2416 diagnostics: {ts2416:?}"
    );
}

/// TS2416 for interface method with type parameters instantiated from the interface level.
/// After `IFoo<number>`, the method `foo(x: T): T` becomes `foo(x: number): number`.
/// The class method `foo(x: string): string` is incompatible.
///
/// Uses `get_type_of_interface_member_simple` to build proper function types
/// for interface methods in the implements checker (rather than just the return type).
#[test]
fn test_ts2416_implements_interface_method_type_mismatch() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface IFoo<T> {
    foo(x: T): T;
}
class Bad implements IFoo<number> {
    foo(x: string): string { return "a"; }
}
class Good implements IFoo<number> {
    foo(x: number): number { return 1; }
}
        "#,
    );

    // Bad: foo(x: string): string vs IFoo<number>.foo(x: number): number - should be TS2416
    assert!(
        diagnostics
            .iter()
            .any(|(code, msg)| *code == 2416 && msg.contains("Bad")),
        "Expected TS2416 for Bad.\nActual: {diagnostics:#?}"
    );
    // Good should NOT get TS2416
    assert!(
        !diagnostics
            .iter()
            .any(|(code, msg)| *code == 2416 && msg.contains("Good")),
        "Good should NOT get TS2416.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_interface_class_merge_preserves_override_and_assignment_failures() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
// @target: es2015
interface Foo {
    method(a: number): string;
    optionalMethod?(a: number): string;
    property: string;
    optionalProperty?: string;
}

class Foo {
    additionalProperty: string;

    additionalMethod(a: number): string {
        return this.method(0);
    }
}

class Bar extends Foo {
    method(a: number) {
        return this.optionalProperty;
    }
}

var bar = new Bar();
bar.method(0);
bar.optionalMethod(1);
bar.property;
bar.optionalProperty;
bar.additionalProperty;
bar.additionalMethod(2);

var direct: {
    method(a: number): string;
    property: string;
    additionalProperty: string;
    additionalMethod(a: number): string;
} = new Bar();
var obj: {
    method(a: number): string;
    property: string;
    additionalProperty: string;
    additionalMethod(a: number): string;
};

bar = obj;
obj = bar;
        "#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2416 && message.contains("Property 'method' in type 'Bar'")
        }),
        "Expected TS2416 for the merged interface/class override.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 when assigning Bar to the object type.\nActual: {diagnostics:#?}"
    );
}

/// TS2344 false positive: indexed access type constraints must be evaluated
/// before checking constraint satisfaction. `WeakKeyTypes[keyof WeakKeyTypes]`
/// should evaluate to `object | symbol`, and `K extends object` satisfies that.
#[test]
fn test_ts2344_indexed_access_constraint_is_evaluated() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface MyWeakKeyTypes {
    object: object;
    symbol: symbol;
}
type MyWeakKey = MyWeakKeyTypes[keyof MyWeakKeyTypes];

declare class MyWeakMap<K extends MyWeakKey, V> { }

class Foo<K extends object> {
    m = new MyWeakMap<K, string>();
}
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2344),
        "Should NOT get TS2344: object extends object | symbol.\nActual: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2345_function_argument_display_widens_unannotated_literal_return() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function foo3(cb: (x: number) => number): typeof cb;
var r5 = foo3((x: number) => '');
        "#,
    );

    // In call argument contexts, we report TS2345 on the outer argument.
    // TODO: tsc elaborates to TS2322 on the callback body for generic calls
    // but not for non-generic calls like this one. When generic call detection
    // is available, update to match tsc's per-context behavior.
    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 on the outer argument.\nActual diagnostics: {diagnostics:?}"
    );
}

/// Verify that private name access works correctly for instance members accessed
/// via parameters typed as the same class (e.g., `a.#x` where `a: A` inside class A).
///
/// Previously, `resolve_lazy_class_to_constructor` was incorrectly converting the
/// parameter type to a constructor type (typeof A), causing TS2339 false positives.
#[test]
fn test_private_name_instance_access_via_parameter() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class A {
    #x = 1;
    test(a: A) {
        a.#x;
    }
}
class B {
    #y() { return 1; };
    test(b: B) {
        b.#y;
    }
}
        "#,
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Should NOT emit TS2339 for private member access within the declaring class.\n\
         Private fields/methods accessed via a parameter of the same class type should be valid.\n\
         Got: {ts2339:?}"
    );
}

/// Verify that shadowed private names in nested classes produce TS18014 without
/// spurious TS2339 for valid access on the inner class.
#[test]
fn test_private_name_nested_class_shadowing() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
    #x() { };
    constructor() {
        class Derived {
            #x() { };
            testBase(x: Base) {
                console.log(x.#x);
            }
            testDerived(x: Derived) {
                console.log(x.#x);
            }
        }
    }
}
        "#,
    );

    let ts18014: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 18014).collect();
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();

    assert!(
        !ts18014.is_empty(),
        "Should emit TS18014 for shadowed private name access (x.#x where x: Base).\n\
         Actual errors: {diagnostics:?}"
    );
    assert!(
        ts2339.is_empty(),
        "Should NOT emit TS2339 alongside TS18014 for shadowed private names.\n\
         Derived.testDerived accessing x.#x (x: Derived) should be valid.\n\
         Got: {ts2339:?}"
    );
}

// =============================================================================
// Closure narrowing for destructured parameter bindings
// =============================================================================

#[test]
fn test_destructured_parameter_preserves_narrowing_in_closure() {
    // Destructured parameter bindings (like `a` from `{ a, b }`) are const-like
    // because they cannot be reassigned. Narrowing should persist in closures.
    let source = r#"
function ff({ a, b }: { a: string | undefined, b: () => void }) {
  if (a !== undefined) {
    b = () => {
      const x: string = a;
    }
  }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Destructured parameter binding 'a' should preserve narrowing in closure.\n\
         Expected 0 TS2322 errors, got {}: {ts2322:?}",
        ts2322.len()
    );
}

#[test]
fn test_type_query_in_type_literal_signature_parameter_uses_declared_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(a: number | string) {
  if (typeof a === "number") {
    const fn: { (arg: typeof a): boolean; } = () => true;
    fn("");
  }
}
"#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    // tsc narrows `typeof a` in type positions inside control flow blocks.
    // Inside `if (typeof a === "number")`, `typeof a` resolves to `number`,
    // so `fn("")` should error because `string` is not assignable to `number`.
    assert!(
        !ts2345.is_empty(),
        "Type-literal call signature parameters should resolve `typeof` from the narrowed branch type.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_type_query_in_type_alias_index_signature_stays_flow_sensitive() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(a: number | string) {
  if (typeof a === "number") {
    type I = { [key: string]: typeof a };
    const i: I = { x: "" };
  }
}
"#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "Index-signature value types should still see flow-sensitive `typeof` inside narrowed branches.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_returned_arrow_type_query_preserves_branch_narrowing() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(a: number | string) {
  if (typeof a === "number") {
    return (arg: typeof a) => {};
  }
  throw 0;
}

f(1)("");
"#,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        !ts2345.is_empty(),
        "Returned arrow parameter `typeof` queries should inherit the narrowed return-site flow.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_computed_binding_element_literal_key_does_not_require_index_signature() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
{
    interface Window {
        window: Window;
    }

    let foo: string | undefined;
    let window = {} as Window;
    window.window = window;

    const { [(() => {  return 'window' as const })()]:
        { [(() => { foo = ""; return 'window' as const })()]: bar } } = window;

    foo;
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2537: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2537)
        .collect();
    assert!(
        ts2537.is_empty(),
        "Computed binding-element keys that resolve to a literal property name should not require an index signature.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_computed_binding_element_assignment_key_uses_exact_tuple_index() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
{
    let a: 0 | 1 = 0;
    const [{ [(a = 1)]: b } = [9, a] as const] = [];
    const bb: 0 = b;
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Computed assignment keys in binding patterns should use the exact tuple index without leaking sibling elements or undefined.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_computed_binding_element_identifier_key_unions_pre_and_default_assignment_values() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
{
    let a: 0 | 1 | 2 = 1;
    const [{ [a]: b } = [9, a = 0, 5] as const] = [];
    const bb: 0 | 9 = b;
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Bare identifier computed keys should keep the old-or-assigned key union from enclosing binding defaults, without widening to unrelated tuple elements.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_computed_assignment_pattern_order_uses_exact_rhs_tuple_access() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
{
    let a: 0 | 1 = 0;
    let b: 0 | 1 | 9;
    [{ [(a = 1)]: b } = [9, a] as const] = [];
    const bb: 0 = b;
}
{
    let a: 0 | 1 = 1;
    let b: 0 | 1 | 9;
    [{ [a]: b } = [9, a = 0] as const] = [];
    const bb: 9 = b;
}
{
    let a: 0 | 1 = 0;
    let b: 0 | 1 | 8 | 9;
    [{ [(a = 1)]: b } = [9, a] as const] = [[9, 8] as const];
    const bb: 0 | 8 = b;
}
{
    let a: 0 | 1 = 1;
    let b: 0 | 1 | 8 | 9;
    [{ [a]: b } = [a = 0, 9] as const] = [[8, 9] as const];
    const bb: 0 | 8 = b;
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Computed keys in destructuring assignment patterns should read exact tuple elements from the fully evaluated RHS.\nGot: {diagnostics:?}"
    );
}

#[test]
#[ignore = "regression: dispatch refactor"]
fn test_loop_assignment_uses_call_return_type_during_fixed_point() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
let cond: boolean;

function len(s: string) {
    return s.length;
}

function f() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x = len(x);
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        !ts2345.is_empty(),
        "Loop fixed-point should synthesize the call return type and report the recursive call-site error.\nGot: {diagnostics:?}"
    );
}

#[test]
#[ignore = "regression: dispatch refactor"]
fn test_loop_assignment_await_uses_awaited_call_return_type_during_fixed_point() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
let cond: boolean;

async function len(s: string) {
    return s.length;
}

async function f() {
    let x: string | number | boolean;
    x = "";
    while (cond) {
        x = await len(x);
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() == 1,
        "Awaited loop assignments should report exactly one recursive call-site error.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2345[0].1.contains("string | number") && !ts2345[0].1.contains("boolean"),
        "Awaited loop assignments should narrow the recursive call-site to string | number, not leak boolean back in.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_awaited_thenable_alias_reports_ts2589_and_ts7010() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Awaited<T> =
    T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
        F extends ((value: infer V, ...args: infer _) => any) ?
            Awaited<V> :
            never :
    T;

interface BadPromise { then(cb: (value: BadPromise) => void): void; }
type T16 = Awaited<BadPromise>;

interface BadPromise1 { then(cb: (value: BadPromise2) => void): void; }
interface BadPromise2 { then(cb: (value: BadPromise1) => void): void; }
type T17 = Awaited<BadPromise1>;

type T18 = Awaited<{ then(cb: (value: number, other: { }) => void)}>;
"#,
        CheckerOptions {
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    let ts2589_count = diagnostics.iter().filter(|(code, _)| *code == 2589).count();
    let ts7010_count = diagnostics.iter().filter(|(code, _)| *code == 7010).count();

    assert_eq!(
        ts2589_count, 2,
        "Expected TS2589 for both recursive Awaited thenables. Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts7010_count, 1,
        "Expected a single TS7010 for the malformed then signature inside Awaited. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_relational_operator_diagnostic_widens_literal_operand_types() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f() {
    let x: string | number = "";
    while (x > 1) {
        x = 1;
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2365: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2365)
        .collect();
    assert!(
        ts2365.len() == 1,
        "Expected exactly one relational operator diagnostic.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2365[0].1.contains("'string | number' and 'number'")
            && !ts2365[0].1.contains("'string | number' and '1'"),
        "Relational operator diagnostics should widen literal operands to their primitive types.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2344_for_explicit_array_subtype_type_arguments() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface CoolArray<E> extends Array<E> {
    hello: number;
}

declare function foo<T extends any[]>(cb: (...args: T) => void): void;
foo<CoolArray<any>>(function (...args: CoolArray<any>) {});

function bar<T extends any[]>(...args: T): T {
    return args;
}

bar<CoolArray<number>>(10, 20);
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344.is_empty(),
        "Explicit array-subtype type arguments should not fail `T extends any[]` with TS2344.\nGot: {diagnostics:?}"
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        !ts2345.is_empty(),
        "The explicit `bar<CoolArray<number>>(10, 20)` call should still fail on the argument shape, just not with TS2344.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_constraint_with_indexed_access_reports_nested_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type ReturnType<T extends (...args: any) => any> =
    T extends (...args: any) => infer R ? R : any;

type DataFetchFns = {
    Boat: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        description: (id: string) => string;
        displacement: (id: string) => number;
        name: (id: string) => string;
    };
    Plane: {
        requiresLicense: (id: string) => boolean;
        maxGroundSpeed: (id: string) => number;
        maxTakeoffWeight: (id: string) => number;
        maxCruisingAltitude: (id: string) => number;
        name: (id: string) => string;
    }
};

export type TypeGeneric2<T extends keyof DataFetchFns, F extends keyof DataFetchFns[T]> =
    ReturnType<DataFetchFns[T][T]>;
export type TypeGeneric3<T extends keyof DataFetchFns, F extends keyof DataFetchFns[T]> =
    ReturnType<DataFetchFns[F][F]>;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2536: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2536)
        .collect();
    assert!(
        ts2536.len() == 3,
        "Expected the indexed-access checker to report all nested TS2536 diagnostics.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2536.iter().any(|(_, message)| message
            .contains("Type 'T' cannot be used to index type 'DataFetchFns[T]'")),
        "Missing TS2536 for `DataFetchFns[T][T]`.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2536
            .iter()
            .any(|(_, message)| message
                .contains("Type 'F' cannot be used to index type 'DataFetchFns'")),
        "Missing TS2536 for the inner `DataFetchFns[F]` access.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2536.iter().any(|(_, message)| message
            .contains("Type 'F' cannot be used to index type 'DataFetchFns[F]'")),
        "Missing TS2536 for the outer `DataFetchFns[F][F]` access.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_js_strict_false_suppresses_file_level_strict_mode_bind_errors() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
// @strict: false
// @allowJs: true
// @checkJs: true
// @target: es6
"use strict";
var a = {
    a: "hello",
    a: 10,
};
var let = 10;
delete a;
with (a) {}
var x = 009;
"#,
        CheckerOptions::default(),
    );

    for code in [1100, 1101, 1102, 1117, 1212, 1213, 1214, 2410, 2703] {
        assert!(
            !has_error(&diagnostics, code),
            "Did not expect TS{code} under `@strict: false` JS binding checks.\nGot: {diagnostics:?}"
        );
    }
}

#[test]
fn test_js_always_strict_override_restores_strict_mode_bind_errors() {
    let diagnostics = compile_and_get_diagnostics_named(
        "a.js",
        r#"
// @strict: false
// @alwaysStrict: true
// @allowJs: true
// @checkJs: true
var arguments = 1;
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 1100),
        "Expected explicit `@alwaysStrict: true` to restore JS strict-mode binding diagnostics.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_js_identifier_default_parameter_preserves_jsdoc_initializer_type() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "a.js",
        r#"
/** @type {number | undefined} */
var n;
function f(b = n) {
    b = 1;
    b = undefined;
    b = "error";
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: false,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected JS identifier default parameter to preserve the JSDoc initializer type and reject string assignment.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 7006 && message.contains("Parameter 'b' implicitly has an 'any' type.")
        }),
        "Did not expect the JS identifier default parameter to fall back to implicit any.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_js_declare_property_suppresses_downstream_semantic_checks() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        r#"
class Foo {
    constructor() {
        this.prop = {};
    }

    declare prop: string;

    method() {
        this.prop.foo;
    }
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    for code in [8009, 8010] {
        assert!(
            has_error(&diagnostics, code),
            "Expected TS{code} for declare property syntax in JS.\nGot: {diagnostics:#?}"
        );
    }
    for code in [2322, 2339] {
        assert!(
            !has_error(&diagnostics, code),
            "Did not expect downstream semantic TS{code} for declare property syntax in JS.\nGot: {diagnostics:#?}"
        );
    }
}
