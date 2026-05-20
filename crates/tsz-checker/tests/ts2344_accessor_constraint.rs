//! Tests for TS2344: type argument constraint validation in accessor (getter/setter)
//! declarations within interfaces and type literals.
//!
//! When a type reference like `Fail<string>` appears as a setter parameter type
//! inside an interface or type literal, the checker must validate that the type
//! argument satisfies the constraint. Previously, `check_type_member_for_missing_names`
//! did not handle `GET_ACCESSOR/SET_ACCESSOR` members, so constraint violations in
//! accessor parameter types were silently ignored.

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
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

    let has_constraint_msg = diagnostics
        .iter()
        .any(|(code, msg)| *code == 2344 && msg.contains("does not satisfy the constraint"));
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
