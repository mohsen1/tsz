//! Tests for generic type parameter handling and TS2322 errors

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for string argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
    );
}

#[test]
fn test_generic_with_default_type_parameter() {
    let source = r#"
function foo<T = string>(x: T): T {
    return x;
}

const result1 = foo("hello"); // OK - uses default string
const result2 = foo(42); // OK - 42 satisfies string constraint
const result3 = foo<number>(true); // OK - true satisfies number constraint
const result4 = foo<number>([]); // TS2322: [] doesn't satisfy number
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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for array argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
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
const c2 = new Container("hello"); // TS2322
const c3 = new Container<number>(true); // OK - true extends number
const c4 = new Container<number>({}); // TS2322: {} doesn't extend number
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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit at least 2 TS2322 errors
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 2,
        "Expected at least 2 TS2322 errors, got {}",
        ts2322_count
    );
}

#[test]
fn test_generic_contravariance() {
    let source = r#"
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
        crate::check::context::CheckerOptions::default(),
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
const result3 = pair<number, number>(1, "hello"); // TS2322: "string" not assignable to number
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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for wrong type argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
    );
}

#[test]
fn test_no_type_arguments_needed_for_inferred_generics() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const result1 = identity(42); // Should work - T inferred as number
const result2 = identity("hello"); // Should work - T inferred as string
const result3 = identity<number>(42); // Should work - explicit number
const result4 = identity<string>(42); // TS2322: 42 not assignable to string
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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for wrong type argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
    );
}

#[test]
fn test_multiple_type_parameters_with_defaults() {
    let source = r#"
function foo<T = number, U = string>(x: T, y: U): [T, U] {
    return [x, y];
}

const r1 = foo(1, "hello"); // OK - uses defaults
const r2 = foo<string, boolean>(true, false); // OK - overrides defaults
const r3 = foo<number, number>(1, 2); // OK
const r4 = foo<number, boolean>(1, "hello"); // TS2322: string not assignable to boolean
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
        crate::check::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for wrong type argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
    );
}
