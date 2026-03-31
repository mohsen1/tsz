//! Tests for TS2344: type argument constraint validation in accessor (getter/setter)
//! declarations within interfaces and type literals.
//!
//! When a type reference like `Fail<string>` appears as a setter parameter type
//! inside an interface or type literal, the checker must validate that the type
//! argument satisfies the constraint. Previously, `check_type_member_for_missing_names`
//! did not handle GET_ACCESSOR/SET_ACCESSOR members, so constraint violations in
//! accessor parameter types were silently ignored.

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

/// Interface setter parameter with unsatisfied type constraint must emit TS2344.
#[test]
fn test_interface_setter_constraint_violation_emits_ts2344() {
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

type Fail<T extends never> = T;
interface I1 {
    get x(): number;
    set x(value: Fail<string>);
}
"#,
    );

    let ts2344_count = diagnostics.iter().filter(|(code, _)| *code == 2344).count();
    assert!(
        ts2344_count >= 1,
        "Expected at least one TS2344 for Fail<string> in interface setter, got {ts2344_count}. Diagnostics: {diagnostics:?}"
    );

    let has_constraint_msg = diagnostics.iter().any(|(code, msg)| {
        *code == 2344 && msg.contains("does not satisfy the constraint")
    });
    assert!(
        has_constraint_msg,
        "Expected TS2344 message about constraint violation. Diagnostics: {diagnostics:?}"
    );
}

/// Type literal setter parameter with unsatisfied type constraint must emit TS2344.
#[test]
fn test_type_literal_setter_constraint_violation_emits_ts2344() {
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

type Fail<T extends never> = T;
type T1 = {
    get x(): number;
    set x(value: Fail<string>);
};
"#,
    );

    let ts2344_count = diagnostics.iter().filter(|(code, _)| *code == 2344).count();
    assert!(
        ts2344_count >= 1,
        "Expected at least one TS2344 for Fail<string> in type literal setter, got {ts2344_count}. Diagnostics: {diagnostics:?}"
    );
}

/// Getter return type with unsatisfied type constraint must also emit TS2344.
#[test]
fn test_interface_getter_constraint_violation_emits_ts2344() {
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

type Fail<T extends never> = T;
interface I1 {
    get x(): Fail<string>;
}
"#,
    );

    let ts2344_count = diagnostics.iter().filter(|(code, _)| *code == 2344).count();
    assert!(
        ts2344_count >= 1,
        "Expected at least one TS2344 for Fail<string> in interface getter return type, got {ts2344_count}. Diagnostics: {diagnostics:?}"
    );
}
