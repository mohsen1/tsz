//! Tests for TS2344 behavior on class expression constructor types.

#[path = "ts2344_class_constructor_constraint_common.rs"]
mod common;

use common::{compile_and_get_diagnostics, diagnostics_for_code};

/// `InstanceType<typeof GenericExpr>` where `GenericExpr` is a bare generic
/// class expression variable must NOT emit TS2344.
#[test]
fn instance_type_of_bare_generic_class_expr_var_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

const GenericExpr = class<T> {
  constructor(public value: T) {}
};

type GenericExprType = InstanceType<typeof GenericExpr>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof GenericExpr> because typeof GenericExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `ConstructorParameters<typeof GenericExpr>` should also work: bare generic
/// class expression variable's typeof satisfies abstract constructor constraints.
#[test]
fn constructor_parameters_of_bare_generic_class_expr_var_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type ConstructorParameters<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: infer P) => any ? P : never;

const GenericExpr = class<T> {
  constructor(public value: T) {}
};

type GenericExprParams = ConstructorParameters<typeof GenericExpr>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for ConstructorParameters<typeof GenericExpr> because typeof GenericExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// Non-generic class expressions also work: typeof satisfies constructor constraints.
#[test]
fn instance_type_of_non_generic_class_expr_var_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

const SimpleExpr = class {
  constructor(public value: string) {}
};

type SimpleExprType = InstanceType<typeof SimpleExpr>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof SimpleExpr> because typeof SimpleExpr is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `InstanceType<typeof GenericExpr>` must resolve to a concrete type.
#[test]
fn instance_type_of_generic_class_expr_assignable_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
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
    let ts2322_errors = diagnostics_for_code(&diagnostics, 2322);
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 - InstanceType<typeof GenericExpr> must resolve without free type parameters.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// Type-parameter erasure is structural: renaming the parameter must not change behavior.
#[test]
fn instance_type_of_generic_class_expr_renamed_tparam_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

const Box = class<K extends object> {
    constructor(public contents: K) {}
};

type BoxInstance = InstanceType<typeof Box>;

declare const b: BoxInstance;
"#,
    );
    let ts2322_errors = diagnostics_for_code(&diagnostics, 2322);
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 - erasure must be structural, not name-keyed.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `typeof` applied to a generic class expression with type arguments remains
/// value-space and satisfies constructor constraints like `InstanceType`'s.
#[test]
fn instance_type_of_generic_class_expression_type_query_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

let Anon = class <out T> {
    foo(): InstanceType<(typeof Anon<T>)> {
        return this;
    }
}
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for InstanceType<typeof Anon<T>> because typeof Anon<T> is constructable.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `return this` in a method whose return type is `InstanceType<(typeof Anon<T>)>`
/// must NOT emit TS2322. The constructor type must be preserved when evaluating
/// `Application(TypeQuery(ClassSym), [T])` so that `InstanceType<...>` correctly
/// reduces to the instance type rather than wrapping it again.
#[test]
fn instance_type_of_generic_class_expr_type_query_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

let Anon = class <out T> {
    foo(): InstanceType<(typeof Anon<T>)> {
        return this;
    }
}
"#,
    );
    let ts2322_errors = diagnostics_for_code(&diagnostics, 2322);
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 for return this in InstanceType<typeof Anon<T>> context.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// Same shape with a renamed type parameter proves the fix is structural, not name-keyed.
#[test]
fn instance_type_of_generic_class_expr_type_query_renamed_param_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type InstanceType<T extends abstract new (...args: any) => any> =
    T extends abstract new (...args: any) => infer R ? R : any;

let Container = class <out Value> {
    wrap(): InstanceType<(typeof Container<Value>)> {
        return this;
    }
}
"#,
    );
    let ts2322_errors = diagnostics_for_code(&diagnostics, 2322);
    assert!(
        ts2322_errors.is_empty(),
        "Should NOT emit TS2322 regardless of type parameter name choice.\nGot: {ts2322_errors:#?}\nAll: {diagnostics:#?}"
    );
}
