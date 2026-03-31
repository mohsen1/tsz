//! Tests for generic type parameter handling and TS2322 errors

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_generic_type_argument_satisfies_constraint() {
    let source = r#"
function identity<T extends number>(x: T): T {
    return x;
}

const result1 = identity(42); // OK - 42 is number
const result2 = identity("string"); // TS2322: "string" doesn't satisfy "extends number"
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2345 for string argument not assignable to number parameter.
    // tsc reports TS2345 ("Argument of type 'string' is not assignable to parameter
    // of type 'number'") because the constraint violation is at the argument level.
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error, got {ts2345_count}"
    );
}

#[test]
fn test_readonly_generic_parameter_no_ts2339() {
    let source = r"
interface Props {
    onFoo?(value: string): boolean;
}

type Readonly<T> = { readonly [K in keyof T]: T[K] };

function test<P extends Props>(props: Readonly<P>) {
    props.onFoo;
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 for Readonly<P> property access, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_generic_with_default_type_parameter() {
    let source = r#"
function foo<T = string>(x: T): T {
    return x;
}

const result1 = foo("hello"); // OK - T inferred as string
const result2 = foo(42); // OK - T inferred as number
const result3 = foo<number>(true); // TS2345: boolean not assignable to number
const result4 = foo<number>([]); // TS2345: never[] not assignable to number
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2345 for argument type mismatches against explicit type params
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 2,
        "Expected at least 2 TS2345 errors for explicit type arg mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_generic_class_type_parameter_constraint() {
    let source = r#"
class Container<T extends number> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}

const c1 = new Container(42); // OK
const c2 = new Container("hello"); // TS2345: string not assignable to number
const c3 = new Container<number>(true); // TS2345: boolean not assignable to number
const c4 = new Container<number>({}); // TS2345: {} not assignable to number
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2345 for all 3 bad arguments (c2, c3, c4)
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 3,
        "Expected at least 3 TS2345 errors for type mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_generic_contravariance() {
    let source = r"
interface Producer<T> {
    produce(): T;
}

interface Consumer<T> {
    consume(value: T): void;
}

// Covariance: Producer<Derived> is assignable to Producer<Base>
interface Base {}
interface Derived extends Base {}

function useProducer(producer: Producer<Base>): void {
    producer.produce();
}

const prodBase: Producer<Base> = { produce: () => new Base() };
const prodDerived: Producer<Derived> = { produce: () => new Derived() };

useProducer(prodBase);     // OK
useProducer(prodDerived); // OK - covariant

// Contravariance: Consumer<Derived> is assignable to Consumer<Base>
function useConsumer(consumer: Consumer<Base>): void {
    consumer.consume(new Base());
}

const consBase: Consumer<Base> = { consume: (value: Base) => {} };
const consDerived: Consumer<Derived> = { consume: (value: Derived) => {} };

useConsumer(consBase);     // OK
useConsumer(consDerived); // TS2322 if invariance (should error)
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);
    // This test checks variance, which may or may not error
}

#[test]
fn test_generic_function_type_inference() {
    let source = r#"
function pair<T, U>(first: T, second: U): [T, U] {
    return [first, second];
}

const result1 = pair(1, "hello"); // OK - T inferred as number, U as string
const result2 = pair<number, string>(1, "hello"); // OK - explicit type args
const result3 = pair<number, number>(1, "hello"); // TS2345: string not assignable to number
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2345 for argument type mismatch
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for type mismatch, got {ts2345_count}"
    );
}

#[test]
fn test_no_type_arguments_needed_for_inferred_generics() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const result1 = identity(42); // OK - T inferred as number
const result2 = identity("hello"); // OK - T inferred as string
const result3 = identity<number>(42); // OK - explicit number
const result4 = identity<string>(42); // TS2345: number not assignable to string
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2345 for argument type mismatch against explicit type param
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 1,
        "Expected at least 1 TS2345 error for type mismatch, got {ts2345_count}"
    );
}

#[test]
fn test_multiple_type_parameters_with_defaults() {
    let source = r#"
function foo<T = number, U = string>(x: T, y: U): [T, U] {
    return [x, y];
}

const r1 = foo(1, "hello"); // OK - T inferred number, U inferred string
const r2 = foo<string, boolean>(true, false); // TS2345: boolean not assignable to string
const r3 = foo<number, number>(1, 2); // OK
const r4 = foo<number, boolean>(1, "hello"); // TS2345: string not assignable to boolean
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // tsc emits TS2345 for argument type mismatches (r2 and r4)
    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert!(
        ts2345_count >= 2,
        "Expected at least 2 TS2345 errors for type mismatches, got {ts2345_count}"
    );
}

#[test]
fn test_ts2313_direct_circular_constraint() {
    let source = r"
class C<T extends T> { }
function f<T extends T>() { }
interface I<T extends T> { }
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2313_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2313)
        .count();
    assert_eq!(
        ts2313_count,
        3,
        "Expected 3 TS2313 errors for direct circular constraints, got {ts2313_count}. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_indirect_circular_constraint() {
    let source = r"
class C<U extends T, T extends U> { }
class C2<T extends U, U extends V, V extends T> { }
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2313_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2313)
        .count();
    assert_eq!(
        ts2313_count,
        5,
        "Expected 5 TS2313 errors for indirect circular constraints (2 for C, 3 for C2), got {ts2313_count}. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_no_false_positive_for_non_circular() {
    // S extends Foo<S> is NOT circular - the constraint wraps S in Foo<>
    let source = r"
type Foo<T> = [T] extends [number] ? {} : {};
function foo<S extends Foo<S>>() {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2313_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2313)
        .count();
    assert_eq!(
        ts2313_count,
        0,
        "Expected 0 TS2313 errors for non-circular constraint Foo<S>, got {ts2313_count}. Diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2313_non_cyclic_chain_not_flagged() {
    // U extends T, T extends V, V extends T: U is NOT part of the cycle
    let source = r"
class D<U extends T, T extends V, V extends T> { }
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2313_diags: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2313)
        .collect();

    // T and V form a cycle, but U just points to the cycle - should not be flagged
    assert_eq!(
        ts2313_diags.len(),
        2,
        "Expected 2 TS2313 errors (T and V), not U. Got: {:?}",
        ts2313_diags
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// Test that array literal elements preserve literal types during generic call inference.
///
/// When `['foo', 'bar']` is passed to a function like `f<K extends string>(list: K[]): K`,
/// K should be inferred as `"foo" | "bar"` (not `string`). This requires the array literal
/// to preserve literal element types instead of widening them via BCT.
///
/// Repro from TypeScript's isomorphicMappedTypeInference test (#29765).
#[test]
fn test_generic_call_preserves_array_literal_types() {
    let source = r#"
function getProps<T, K extends keyof T>(obj: T, list: K[]): Pick<T, K> {
    return {} as any;
}
const myAny: any = {};
const o2: { foo: any; bar: any } = getProps(myAny, ['foo', 'bar']);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should have NO errors — Pick<any, "foo" | "bar"> = { foo: any; bar: any }
    let errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322 errors for Pick<any, 'foo' | 'bar'> assignment, got {}: {:?}",
        errors.len(),
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn test_promise_all_spread_any_does_not_produce_false_ts1360() {
    let source = r#"
declare function getT<T>(): T;

Promise.all([getT<string>(), ...getT<any>()]).then((result) => {
  const tail = result.slice(1);
  tail satisfies string[];
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts1360_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1360)
        .collect();
    assert!(
        ts1360_errors.is_empty(),
        "Expected no TS1360 for Promise.all spread-any tail satisfies check, got: {:?}",
        ts1360_errors
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

/// When a type argument has no properties in common with an all-optional
/// constraint (a "weak type"), tsc emits TS2559 instead of TS2344.
/// Regression test for incorrectNumberOfTypeArgumentsDuringErrorReporting.ts.
#[test]
fn test_weak_type_constraint_emits_ts2559() {
    let source = r#"
interface ObjA {
  y?: string;
}

interface ObjB { [key: string]: any }

interface Opts<A, B> { a: A; b: B }

const fn2 = <
  A extends ObjA,
  B extends ObjB = ObjB
>(opts: Opts<A, B>): string => 'Z';

interface MyObjA {
  x: string;
}

fn2<MyObjA>({
  a: { x: 'X', y: 'Y' },
  b: {},
});
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2559_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2559)
        .count();
    assert!(
        ts2559_count >= 1,
        "Expected TS2559 (weak type: no common properties) for MyObjA vs ObjA constraint, got {} TS2559 errors. All errors: {:?}",
        ts2559_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Should NOT emit TS2344 (constraint not satisfied) — TS2559 is more specific
    let ts2344_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2344)
        .count();
    assert_eq!(
        ts2344_count, 0,
        "Expected no TS2344 when TS2559 (weak type) applies, got {ts2344_count}"
    );
}
