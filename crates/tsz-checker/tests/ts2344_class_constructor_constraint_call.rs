//! Tests for TS2344 on class constructor types and call-signature constraints.

#[path = "ts2344_class_constructor_constraint_common.rs"]
mod common;

use common::{compile_and_get_diagnostics, diagnostics_for_code};

/// `Parameters<typeof C>` must emit TS2344 because a class constructor
/// has construct signatures but no call signatures.
#[test]
fn parameters_of_class_constructor_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;

class C {
    constructor(a: number, b: string) {}
}

type Cps = Parameters<typeof C>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        !ts2344_errors.is_empty(),
        "Should emit TS2344 for Parameters<typeof C> because typeof C only has construct signatures.\nAll diagnostics: {diagnostics:#?}"
    );
}

/// `Parameters<typeof f>` where f is a regular function should NOT emit TS2344.
#[test]
fn parameters_of_function_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Parameters<T extends (...args: any) => any> = T extends (...args: infer P) => any ? P : never;

function foo(a: number, b: string): boolean { return true; }

type Fps = Parameters<typeof foo>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for Parameters<typeof foo> because foo has call signatures.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}

/// `ConstructorParameters<typeof C>` should NOT emit TS2344 because the
/// abstract-constructor constraint accepts class constructors.
#[test]
fn constructor_parameters_of_class_no_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type ConstructorParameters<T extends abstract new (...args: any) => any> = T extends abstract new (...args: infer P) => any ? P : never;

class C {
    constructor(a: number, b: string) {}
}

type Ccps = ConstructorParameters<typeof C>;
"#,
    );
    let ts2344_errors = diagnostics_for_code(&diagnostics, 2344);
    assert!(
        ts2344_errors.is_empty(),
        "Should NOT emit TS2344 for ConstructorParameters<typeof C> because typeof C has construct signatures.\nGot: {ts2344_errors:#?}\nAll: {diagnostics:#?}"
    );
}
