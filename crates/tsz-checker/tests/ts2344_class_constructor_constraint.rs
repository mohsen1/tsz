//! Tests for TS2344: class constructor types (typeof C) should not satisfy
//! call-signature constraints like `(...args: any) => any`.
//!
//! A class constructor type has construct signatures (new) but no call signatures.
//! `Parameters<T>` requires `T extends (...args: any) => any`, so `Parameters<typeof C>`
//! must emit TS2344 because `typeof C` is not callable (only constructable).

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
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

/// `InstanceType<typeof GenericExpr>` where `GenericExpr` is a bare generic
/// class expression variable must NOT emit TS2344. The class expression
/// produces a constructor type with generic construct signatures, which
/// satisfies `abstract new (...args: any) => any`.
#[test]
fn test_instance_type_of_bare_generic_class_expr_var_no_ts2344() {
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

const GenericExpr = class<T> {
  constructor(public value: T) {}
};

type GenericExprType = InstanceType<typeof GenericExpr>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof GenericExpr> because typeof GenericExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `ConstructorParameters<typeof GenericExpr>` should also work: bare generic
/// class expression variable's typeof satisfies abstract constructor constraints.
#[test]
fn test_constructor_parameters_of_bare_generic_class_expr_var_no_ts2344() {
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

type ConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;

const GenericExpr = class<T> {
  constructor(public value: T) {}
};

type GenericExprParams = ConstructorParameters<typeof GenericExpr>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for ConstructorParameters<typeof GenericExpr> because typeof GenericExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// Non-generic class expressions also work: typeof satisfies constructor constraints.
#[test]
fn test_instance_type_of_non_generic_class_expr_var_no_ts2344() {
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

const SimpleExpr = class {
  constructor(public value: string) {}
};

type SimpleExprType = InstanceType<typeof SimpleExpr>;
        "#,
    );
    let ts2344_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof SimpleExpr> because typeof SimpleExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `InstanceType<typeof GenericExpr>` must resolve to a concrete type (no free
/// type parameters). When the construct signature's type params are not erased,
/// R retains them and the alias cannot be used without explicit type arguments.
#[test]
fn test_instance_type_of_generic_class_expr_assignable_no_ts2322() {
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

const GenericExpr = class<T> {
    constructor(public value: T) {}
};

type GenericExprType = InstanceType<typeof GenericExpr>;

declare const inst: InstanceType<typeof GenericExpr>;
declare const inst2: GenericExprType;
        "#,
    );
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 — InstanceType<typeof GenericExpr> must resolve without free type parameters.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// Type-parameter erasure is structural: renaming the parameter must not change behavior.
#[test]
fn test_instance_type_of_generic_class_expr_renamed_tparam_no_ts2322() {
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

const Box = class<K extends object> {
    constructor(public contents: K) {}
};

type BoxInstance = InstanceType<typeof Box>;

declare const b: BoxInstance;
        "#,
    );
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 — erasure must be structural, not name-keyed.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
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
