//! Source-level coverage for method-target inference bivariance.
//!
//! TypeScript walks method and constructor parameters in the usual
//! contravariant direction, but candidates reached below that target signature
//! are ordinary inference candidates. Function-valued properties retain strict
//! contravariant candidate collection. These tests exercise the full
//! parser/binder/lowering pipeline so the `is_method` distinction cannot be
//! synthesized only in solver tests.

use tsz_checker::test_utils::{check_source_strict, diagnostic_codes};

#[test]
fn kysely_style_method_contextually_infers_generic_new_return() {
    let source = r#"
interface ExpressionBuilder<DB, TB extends keyof DB> {
  readonly database: DB;
  readonly table: TB;
}

interface QueryBuilder<DB, TB extends keyof DB, O> {
  limit(
    value: number | ((eb: ExpressionBuilder<DB, TB>) => number),
  ): QueryBuilder<DB, TB, O>;
}

class Impl<DB, TB extends keyof DB, O> implements QueryBuilder<DB, TB, O> {
  constructor(_props: {}) {}

  limit(
    value: number | ((eb: ExpressionBuilder<DB, TB>) => number),
  ): QueryBuilder<DB, TB, O> {
    return new Impl({});
  }
}
"#;

    let diagnostics = check_source_strict(source);
    assert!(
        diagnostics.is_empty(),
        "method-only type parameters should contextually infer for generic `new`: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn renamed_kysely_style_method_contextually_infers_generic_new_return() {
    let source = r#"
interface Expr<Schema, Scope extends keyof Schema> {
  readonly schema: Schema;
  readonly scope: Scope;
}

interface Builder<Schema, Scope extends keyof Schema, Result> {
  take(
    value: number | ((ctx: Expr<Schema, Scope>) => number),
  ): Builder<Schema, Scope, Result>;
}

class Renamed<Schema, Scope extends keyof Schema, Result>
  implements Builder<Schema, Scope, Result> {
  constructor(_state: {}) {}

  take(
    value: number | ((ctx: Expr<Schema, Scope>) => number),
  ): Builder<Schema, Scope, Result> {
    return new Renamed({});
  }
}
"#;

    let diagnostics = check_source_strict(source);
    assert!(
        diagnostics.is_empty(),
        "renamed method-only binders should contextually infer: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn function_valued_property_inference_remains_contravariant() {
    let source = r#"
interface PropertyBuilder<Value> {
  readonly output: Value;
  whereRef: (reference: Value) => PropertyBuilder<Value>;
}

type InferredValue<Source> =
  Source extends PropertyBuilder<infer Value> ? Value : never;

type ConcreteBuilder = {
  readonly output: number;
  whereRef: (reference: string) => PropertyBuilder<string>;
};

declare const candidate: string | number;
const rejected: InferredValue<ConcreteBuilder> = candidate;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(
        diagnostic_codes(&diagnostics),
        vec![2322],
        "function-valued properties must keep contravariant inference"
    );
}

#[test]
fn generic_call_inference_distinguishes_method_and_property_targets() {
    let source = r#"
declare function inferMethod<T>(
  argument: { value: T; use(entry: T): void },
): T;
declare const methodInput: {
  value: string;
  use(entry: number): void;
};
const methodResult: string = inferMethod(methodInput);

declare function inferProperty<T>(
  argument: { value: T; use: (entry: T) => void },
): T;
declare const propertyInput: {
  value: string;
  use: (entry: number) => void;
};
const propertyResult: number = inferProperty(propertyInput);
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(
        diagnostic_codes(&diagnostics),
        vec![2345, 2345],
        "generic calls infer from target declaration kind"
    );
}

#[test]
fn relation_variance_uses_target_declaration_kind_with_explicit_this() {
    let source = r#"
class Animal { animal = true; }
class Dog extends Animal { dog = true; }

interface MethodSource {
  use(this: unknown, entry: Dog): void;
}
interface PropertyTarget {
  use: (this: unknown, entry: Animal) => void;
}
declare const methodSource: MethodSource;
const rejected: PropertyTarget = methodSource;

interface PropertySource {
  use: (this: unknown, entry: Animal) => void;
}
interface MethodTarget {
  use(this: unknown, entry: Dog): void;
}
declare const propertySource: PropertySource;
const accepted: MethodTarget = propertySource;

interface ThisMethodSource {
  use(this: Dog): void;
}
interface ThisPropertyTarget {
  use: (this: Animal) => void;
}
declare const thisMethodSource: ThisMethodSource;
const rejectedThis: ThisPropertyTarget = thisMethodSource;

interface ThisPropertySource {
  use: (this: Animal) => void;
}
interface ThisMethodTarget {
  use(this: Dog): void;
}
declare const thisPropertySource: ThisPropertySource;
const acceptedThis: ThisMethodTarget = thisPropertySource;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(
        diagnostic_codes(&diagnostics),
        vec![2322, 2322],
        "only the target declaration grants method bivariance"
    );
}

#[test]
fn method_source_does_not_loosen_index_signature_target() {
    let source = r#"
class Animal { animal = true; }
class Dog extends Animal { dog = true; }
const methods = { use(entry: Dog): void {} };
const rejected: { [key: string]: (entry: Animal) => void } = methods;
declare const symbolKey: unique symbol;
const symbolMethods = { [symbolKey](entry: Dog): void {} };
const rejectedSymbol: { [key: symbol]: (entry: Animal) => void } = symbolMethods;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(diagnostic_codes(&diagnostics), vec![2322, 2322]);
}

#[test]
fn overloaded_method_source_does_not_loosen_property_target() {
    let source = r#"
class Animal { animal = true; }
class Dog extends Animal { dog = true; }
interface OverloadedSource {
  use(entry: Dog): void;
  use(entry: Dog, count?: number): void;
}
interface PropertyTarget {
  use: (entry: Animal) => void;
}
declare const methods: OverloadedSource;
const rejected: PropertyTarget = methods;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(diagnostic_codes(&diagnostics), vec![2322]);
}

#[test]
fn overload_tuple_rest_shortcut_uses_target_declaration_kind() {
    let source = r#"
interface PropertyOverloads {
  set: {
    (value: { [key: string]: unknown }): void;
    (name: string, value: unknown): void;
  };
}
interface MethodTarget {
  set(...args:
    | [{ [key: string]: unknown }]
    | [string, unknown]
    | [string]
  ): void;
}
interface PropertyTarget {
  set: (...args:
    | [{ [key: string]: unknown }]
    | [string, unknown]
    | [string]
  ) => void;
}
declare const source: PropertyOverloads;
const accepted: MethodTarget = source;
const rejected: PropertyTarget = source;

class ThisA { a = true; }
class ThisB { b = true; }
class ThisOverloads {
  set(this: ThisA, value: { [key: string]: unknown }): void;
  set(this: ThisA, name: string, value: unknown): void;
  set(this: ThisA, ..._args: any[]): void {}
}
interface ThisTarget {
  set(this: ThisB, ...args:
    | [{ [key: string]: unknown }]
    | [string, unknown]
    | [string]
  ): void;
}
declare const thisSource: ThisOverloads;
const rejectedThis: ThisTarget = thisSource;

class PredicateA { a = true; }
class PredicateB { b = true; }
interface PredicateOverloads {
  set: {
    (value: PredicateA | PredicateB): value is PredicateA;
    (value: PredicateA | PredicateB, name: string): value is PredicateA;
  };
}
interface BooleanOverloads {
  set: {
    (value: PredicateA | PredicateB): boolean;
    (value: PredicateA | PredicateB, name: string): boolean;
  };
}
interface MatchingPredicateOverloads {
  set: {
    (value: PredicateA | PredicateB): value is PredicateB;
    (value: PredicateA | PredicateB, name: string): value is PredicateB;
  };
}
interface PredicateTarget {
  set(
    value: PredicateA | PredicateB,
    ...args: [] | [string]
  ): value is PredicateB;
}
declare const predicateSource: PredicateOverloads;
declare const booleanSource: BooleanOverloads;
declare const matchingPredicateSource: MatchingPredicateOverloads;
const rejectedPredicate: PredicateTarget = predicateSource;
const rejectedBoolean: PredicateTarget = booleanSource;
const acceptedPredicate: PredicateTarget = matchingPredicateSource;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(diagnostic_codes(&diagnostics), vec![2322, 2322, 2322, 2322]);
}

#[test]
fn callback_mode_only_relaxes_explicit_this_parameter() {
    let source = r#"
class Animal { animal = true; }
class Dog extends Animal { dog = true; }
type MethodThisDog = { use(this: Dog): void }["use"];
type PropertyThisAnimal = (this: Animal) => void;
declare const thisForward: (callback: PropertyThisAnimal) => void;
const acceptedThisForward: (callback: MethodThisDog) => void = thisForward;
declare const thisReverse: (callback: MethodThisDog) => void;
const acceptedThisReverse: (callback: PropertyThisAnimal) => void = thisReverse;

type MethodParamDog = { use(entry: Dog): void }["use"];
type PropertyParamAnimal = (entry: Animal) => void;
declare const paramForward: (callback: PropertyParamAnimal) => void;
const rejectedParam: (callback: MethodParamDog) => void = paramForward;
declare const paramReverse: (callback: MethodParamDog) => void;
const acceptedParam: (callback: PropertyParamAnimal) => void = paramReverse;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(diagnostic_codes(&diagnostics), vec![2322]);
}

#[test]
fn contextual_generic_retry_preserves_callback_variance_mode() {
    let source = r#"
class Animal { animal = true; }
class Dog extends Animal { dog = true; }
type Mapped<Result extends string, Fallback = any> =
  Result extends "ok" ? number : Fallback;
interface Box<Value> { data?: Value; }

type MethodThis = {
  use<Value = any, Result extends string = "ok">(this: Dog): Box<Value>;
}["use"];
type PropertyThis =
  <Value = any, Result extends string = "ok">(this: Animal) =>
    Box<Mapped<Result, Value>>;
declare const thisSource: (callback: PropertyThis) => void;
const acceptedThis: (callback: MethodThis) => void = thisSource;

type MethodParam = {
  use<Value = any, Result extends string = "ok">(entry: Dog): Box<Value>;
}["use"];
type PropertyParam =
  <Value = any, Result extends string = "ok">(entry: Animal) =>
    Box<Mapped<Result, Value>>;
declare const paramSource: (callback: PropertyParam) => void;
const rejectedParam: (callback: MethodParam) => void = paramSource;
"#;

    let diagnostics = check_source_strict(source);
    assert_eq!(diagnostic_codes(&diagnostics), vec![2322]);
}
