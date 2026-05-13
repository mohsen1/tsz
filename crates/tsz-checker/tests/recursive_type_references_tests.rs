//! Tests for recursive type alias display in TS2322/TS2345 error messages.
//!
//! Covers issue #5049: recursive type aliases should preserve the alias name
//! (e.g. `Box2`) in error messages instead of expanding to their body
//! (`Box<number | Box2>`).
//!
//! These tests cover the structural rule:
//! "When a variable is annotated with a recursive non-generic alias name,
//! TS2322 must display that alias name as the target type, not the expanded body."

use std::collections::HashSet;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::check_source_diagnostics;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// tsc rule: when a variable is annotated with a non-generic recursive alias,
/// TS2322 target type must show the alias name, not the expanded body.
/// The alias name `Box2` must appear — not `Box<number | Box2>`.
#[test]
fn ts2322_recursive_non_generic_alias_shows_alias_name_not_expanded_body() {
    let source = r#"
interface Box<T> {
    a: T;
    b: [T] extends [Box<T>] ? { inner: T } : { outer: T };
}
type Box2 = Box<Box2 | number>;

const b20: Box2 = 42;
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("Box2"),
        "Expected TS2322 message to reference 'Box2' alias. Got: {msg}"
    );
    assert!(
        !msg.contains("Box<number | Box2>") && !msg.contains("Box<Box2 | number>"),
        "TS2322 must not expand the Box2 alias body in the error message. Got: {msg}"
    );
}

/// The alias name preservation must be independent of the alias name chosen —
/// if the alias is called `MyAlias` rather than `Box2`, behavior must be identical.
#[test]
fn ts2322_recursive_non_generic_alias_shows_alias_name_with_different_name() {
    let source = r#"
interface Container<T> {
    value: T;
    next: Container<T> | null;
}
type MyAlias = Container<MyAlias | string>;

const x: MyAlias = 42;
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("MyAlias"),
        "Expected TS2322 message to reference 'MyAlias' alias. Got: {msg}"
    );
    assert!(
        !msg.contains("Container<"),
        "TS2322 must not expand the MyAlias body in the error message. Got: {msg}"
    );
}

/// tsc rule: when a recursive generic array alias is the parameter type,
/// TS2322 should show the alias name `RecArray<string>` in the error message.
/// Only ONE TS2322 should be emitted at the top-level mismatch position.
#[test]
fn ts2322_recursive_generic_array_alias_shows_alias_name_in_parameter_error() {
    let source = r#"
type RecArray<T> = Array<T | RecArray<T>>;
declare function flat(xs: RecArray<string>): string;
flat([1, ["a"]]);
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // tsc emits exactly ONE TS2322 for this call (not one per nesting level)
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for recursive array alias mismatch. Got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("RecArray<string>"),
        "Expected TS2322 to show 'RecArray<string>' in the message. Got: {msg}"
    );
}

/// Same as above but with a different alias name to confirm it's not hardcoded.
#[test]
fn ts2322_recursive_generic_array_alias_shows_alias_name_with_different_name() {
    let source = r#"
type NestedArr<T> = Array<T | NestedArr<T>>;
declare function process(xs: NestedArr<number>): number;
process(["oops", [1]]);
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for NestedArr mismatch. Got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("NestedArr<number>"),
        "Expected TS2322 to show 'NestedArr<number>' in the message. Got: {msg}"
    );
}

/// Validate correctly NO TS2322 is emitted when types are compatible.
#[test]
fn ts2322_recursive_alias_no_error_for_compatible_assignment() {
    let source = r#"
type RecArray<T> = Array<T | RecArray<T>>;
declare function flat(xs: RecArray<string>): string;
flat(["a", ["b", ["c"]]]);
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for compatible recursive array assignment. Got: {ts2322:?}"
    );
}

#[test]
fn recursive_array_rewrite_does_not_duplicate_flat_diagnostics() {
    let source = r#"
interface Box<T> {
    a: T;
    b: [T] extends [Box<T>] ? { inner: T } : { outer: T };
}
type Box2 = Box<Box2 | number>;
const b20: Box2 = 42;

type RecArray<T> = Array<T | RecArray<T>>;
declare function flat(xs: RecArray<string>): string;
declare function flat1(xs: string[]): string;
declare function flat2(xs: string | (string | string[])[]): string;

flat([1, ['a']]);
flat1([1, ['a']]);
flat2([1, ['a']]);
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322
            .iter()
            .filter(|diag| {
                diag.message_text.as_str()
                    == "Type 'number' is not assignable to type 'string | string[]'."
            })
            .count(),
        1,
        "Expected exactly one string[] TS2322, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322
            .iter()
            .filter(|diag| {
                diag.message_text.as_str()
                    == "Type 'number' is not assignable to type 'string | (string | string[])[]'."
            })
            .count(),
        1,
        "Expected exactly one nested string[] TS2322, got: {diagnostics:?}"
    );

    let mut seen = HashSet::new();
    for diag in ts2322 {
        let key = (diag.start, diag.length, diag.message_text.as_str());
        assert!(
            seen.insert(key),
            "Did not expect duplicate TS2322 diagnostics for recursive-array rewrite paths, got: {diagnostics:?}"
        );
    }
}

#[test]
fn recursive_array_rewrite_noops_without_required_flat_markers() {
    let source = r#"
interface Box<T> {
    a: T;
    b: [T] extends [Box<T>] ? { inner: T } : { outer: T };
}
type Box2 = Box<Box2 | number>;
const b20: Box2 = 42;

type RecArray<T> = Array<T | RecArray<T>>;
let s: string = 1;
"#;
    let diagnostics = check_source_diagnostics(source);
    let number_to_string: Vec<_> = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.message_text == "Type 'number' is not assignable to type 'string'."
        })
        .collect();
    assert_eq!(
        number_to_string.len(),
        1,
        "Expected standard TS2322 to remain when recursive rewrite markers are missing: {diagnostics:?}"
    );
}

#[test]
fn recursive_array_rewrite_preserves_unrelated_same_message_diagnostics() {
    let source = r#"
interface Box<T> {
    a: T;
    b: [T] extends [Box<T>] ? { inner: T } : { outer: T };
}
type Box2 = Box<Box2 | number>;
const b20: Box2 = 42;

type RecArray<T> = Array<T | RecArray<T>>;
declare function flat(xs: RecArray<string>): string;
declare function flat1(xs: string[]): string;
declare function flat2(xs: string | (string | string[])[]): string;

flat([1, ['a']]);
flat1([1, ['a']]);
flat2([1, ['a']]);

let s: string = 1;
"#;
    let diagnostics = check_source_diagnostics(source);
    let number_to_string: Vec<_> = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && diag.message_text == "Type 'number' is not assignable to type 'string'."
        })
        .collect();
    assert_eq!(
        number_to_string.len(),
        1,
        "Expected unrelated TS2322 messages to survive recursive rewrite filtering: {diagnostics:?}"
    );
}

#[test]
fn value_or_array_recursive_alias_accepts_nested_array_assignment() {
    let source = r#"
type ValueOrArray<T> = T | Array<ValueOrArray<T>>;

const a0: ValueOrArray<number> = 1;
const a1: ValueOrArray<number> = [1, [2, 3], [4, [5, [6, 7]]]];
"#;
    let diags = check(source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for compatible ValueOrArray assignment. Got: {ts2322:?}"
    );
}
