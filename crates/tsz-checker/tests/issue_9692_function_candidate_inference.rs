//! Regression tests for issue #9692.
//!
//! Multiple function-typed candidates for the same type parameter should not
//! be unioned in a way that lets later incompatible candidates pass unchecked.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_messages(source: &str) -> Vec<(u32, String)> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

fn assert_has_ts2345(messages: &[(u32, String)], expected_fragment: &str) {
    assert!(
        messages
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains(expected_fragment)),
        "expected TS2345 containing {expected_fragment:?}, got: {messages:#?}"
    );
}

fn assert_no_ts2345(messages: &[(u32, String)]) {
    assert!(
        messages.iter().all(|(code, _)| *code != 2345),
        "expected no TS2345, got: {messages:#?}"
    );
}

#[test]
fn naked_type_parameter_function_candidates_fix_from_first_function() {
    let messages = strict_messages(
        r#"
declare function f<T>(a: T, b: T): T;

const r = f((x: number) => 1, (y: string) => 2);
"#,
    );

    assert_has_ts2345(&messages, "(x: number) => number");
}

#[test]
fn callback_parameter_function_candidates_fix_from_first_function() {
    let messages = strict_messages(
        r#"
declare function f<T>(a: (x: T) => void, b: (x: T) => void): T;

const r = f((x: number) => {}, (x: string) => {});
"#,
    );

    assert_has_ts2345(&messages, "(x: number) => void");
}

#[test]
fn renamed_type_parameter_function_candidates_use_same_rule() {
    let messages = strict_messages(
        r#"
declare function combine<Value>(first: Value, second: Value): Value;

const r = combine((left: number) => 1, (right: string) => 2);
"#,
    );

    assert_has_ts2345(&messages, "(left: number) => number");
}

#[test]
fn compatible_function_candidates_still_pass() {
    let messages = strict_messages(
        r#"
declare function f<T>(a: T, b: T): T;

const r = f((x: number) => {}, (x: number) => {});
"#,
    );

    assert_no_ts2345(&messages);
}

#[test]
fn object_candidates_still_union_without_error() {
    let messages = strict_messages(
        r#"
declare function f<T>(a: T, b: T): T;

const r = f({ a: 1 }, { b: 2 });
const a = r.a;
const b = r.b;
"#,
    );

    assert_no_ts2345(&messages);
}

#[test]
fn literal_keyof_contra_candidates_still_intersect() {
    let messages = strict_messages(
        r#"
interface Shape {
    alpha: string;
    beta: string;
}

declare function withDefault<Value = Shape>(first: keyof Value, second: keyof Value): Value;
declare function withoutDefault<Entity>(first: keyof Entity, second: keyof Entity): Entity;

const a = withDefault<Shape>("alpha", "beta");
const b = withDefault("alpha", "beta");
const c = withoutDefault("alpha", "beta");
"#,
    );

    assert_no_ts2345(&messages);
}
