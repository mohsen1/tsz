//! Tests for TS2344: class constructor types (typeof C) should not satisfy
//! call-signature constraints like `(...args: any) => any`.
//!
//! A class constructor type has construct signatures (new) but no call signatures.
//! `Parameters<T>` requires `T extends (...args: any) => any`, so `Parameters<typeof C>`
//! must emit TS2344 because `typeof C` is not callable (only constructable).

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
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
interface Function {}
interface IArguments {}
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
interface Function {}
interface IArguments {}
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
interface Function {}
interface IArguments {}
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
