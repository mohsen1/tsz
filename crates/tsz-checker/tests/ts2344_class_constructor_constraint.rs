//! Tests for TS2344: class constructor types (typeof C) should not satisfy
//! call-signature constraints like `(...args: any) => any`.
//!
//! A class constructor type has construct signatures (new) but no call signatures.
//! `Parameters<T>` requires `T extends (...args: any) => any`, so `Parameters<typeof C>`
//! must emit TS2344 because `typeof C` is not callable (only constructable).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// `Parameters<typeof C>` must emit TS2344 because a class constructor
/// (typeof C) has construct signatures but no call signatures.
/// The constraint `T extends (...args: any) => any` requires call signatures.
#[test]
fn test_parameters_of_class_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;

class C {
    constructor(a: number, b: string) {}
}

type Cps = Parameters<typeof C>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        !ts2344_errors.is_empty(),
        "Should emit TS2344 for Parameters<typeof C> because typeof C only has construct signatures.\nAll diagnostics: {diagnostics:#?}"
    );
}

/// `Parameters<typeof f>` where f is a regular function should NOT emit TS2344.
#[test]
fn test_parameters_of_function_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;

function foo(a: number, b: string): boolean { return true; }

type Fps = Parameters<typeof foo>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for Parameters<typeof foo> because foo has call signatures.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `ConstructorParameters<typeof C>` should NOT emit TS2344 because the constraint
/// is `T extends abstract new (...args: any) => any`, which class constructors satisfy.
#[test]
fn test_constructor_parameters_of_class_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type ConstructorParameters<T extends abstract new (...args: any) => any> = T extends abstract new (...args: infer P) => any ? P : never;

class C {
    constructor(a: number, b: string) {}
}

type Ccps = ConstructorParameters<typeof C>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for ConstructorParameters<typeof C> because typeof C has construct signatures.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `InstanceType<typeof C>` must reject private constructors because the
/// constraint requires a public constructor signature.
#[test]
fn test_instance_type_of_private_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

class WithPrivateCtor {
    private constructor() {}
}

type Bad = InstanceType<typeof WithPrivateCtor>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344_errors.len(),
        1,
        "Should emit one TS2344 for InstanceType<typeof WithPrivateCtor> with a private constructor.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `InstanceType<typeof C>` must reject protected constructors for the same
/// public-constructor constraint.
#[test]
fn test_instance_type_of_protected_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

class WithProtectedCtor {
    protected constructor() {}
}

type Bad = InstanceType<typeof WithProtectedCtor>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344_errors.len(),
        1,
        "Should emit one TS2344 for InstanceType<typeof WithProtectedCtor> with a protected constructor.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `typeof` applied to a generic class expression with type arguments remains
/// value-space. It satisfies constructor constraints like `InstanceType`'s.
#[test]
fn test_instance_type_of_generic_class_expression_type_query_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}

type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

let Anon = class <out T> {
    foo(): InstanceType<(typeof Anon<T>)> {
        return this;
    }
}
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof Anon<T>> because typeof Anon<T> is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}
