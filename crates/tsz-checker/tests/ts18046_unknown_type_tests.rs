//! Tests for TS18046: "'x' is of type 'unknown'."
//!
//! TS18046 is emitted when an expression of type `unknown` is used in a position
//! that requires a more specific type: property access, function calls, constructors,
//! element access, binary/unary operators, etc. Falls back to TS2571 when the
//! expression name is unavailable.

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_source(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = crate::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_property_access_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x.foo;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 1,
        "Expected TS18046 for property access on unknown, got diagnostics: {diags:?}"
    );
    // Verify no TS2339 is emitted (should be TS18046 instead)
    let ts2339_count = diags.iter().filter(|(code, _)| *code == 2339).count();
    assert_eq!(
        ts2339_count, 0,
        "Should emit TS18046 instead of TS2339 for property access on unknown"
    );
}

#[test]
fn test_function_call_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x();
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 1,
        "Expected TS18046 for call on unknown, got diagnostics: {diags:?}"
    );
    // Verify no TS2349 ("not callable") is emitted
    let ts2349_count = diags.iter().filter(|(code, _)| *code == 2349).count();
    assert_eq!(
        ts2349_count, 0,
        "Should emit TS18046 instead of TS2349 for call on unknown"
    );
}

#[test]
fn test_new_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    new x();
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 1,
        "Expected TS18046 for new on unknown, got diagnostics: {diags:?}"
    );
    // Verify no TS2351 ("not constructable") is emitted
    let ts2351_count = diags.iter().filter(|(code, _)| *code == 2351).count();
    assert_eq!(
        ts2351_count, 0,
        "Should emit TS18046 instead of TS2351 for new on unknown"
    );
}

#[test]
fn test_element_access_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x[10];
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 1,
        "Expected TS18046 for element access on unknown, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_binary_arithmetic_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x + 1;
    x * 2;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 2,
        "Expected TS18046 for arithmetic ops on unknown, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_binary_relational_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x >= 0;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 1,
        "Expected TS18046 for relational op on unknown, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_equality_on_unknown_allowed() {
    // Equality operators (==, !=, ===, !==) are allowed on unknown
    let diags = check_source(
        r"
function f(x: unknown) {
    x == 5;
    x !== 10;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert_eq!(
        ts18046_count, 0,
        "Equality operators should not emit TS18046, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_unary_on_unknown_emits_ts18046() {
    let diags = check_source(
        r"
function f(x: unknown) {
    -x;
    +x;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert!(
        ts18046_count >= 2,
        "Expected TS18046 for unary +/- on unknown, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_ts18046_message_includes_variable_name() {
    let diags = check_source(
        r"
function f(x: unknown) {
    x.foo;
}
",
    );
    let ts18046_messages: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 18046)
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert!(
        ts18046_messages
            .iter()
            .any(|msg| msg.contains("'x' is of type 'unknown'.")),
        "TS18046 message should include variable name 'x', got: {ts18046_messages:?}"
    );
}

#[test]
fn test_narrowed_unknown_no_ts18046() {
    // After typeof narrowing, unknown should be narrowed and no TS18046
    let diags = check_source(
        r"
function f(x: unknown) {
    if (typeof x === 'string') {
        x.toUpperCase();
    }
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert_eq!(
        ts18046_count, 0,
        "After typeof narrowing, should not emit TS18046, got diagnostics: {diags:?}"
    );
}

#[test]
fn test_any_type_no_ts18046() {
    // `any` should NOT emit TS18046 — it's fully permissive
    let diags = check_source(
        r"
function f(x: any) {
    x.foo;
    x();
    new x();
    x[10];
    x + 1;
    -x;
}
",
    );
    let ts18046_count = diags.iter().filter(|(code, _)| *code == 18046).count();
    assert_eq!(
        ts18046_count, 0,
        "Should not emit TS18046 for `any` type, got diagnostics: {diags:?}"
    );
}
