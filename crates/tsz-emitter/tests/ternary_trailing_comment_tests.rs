//! Tests for ternary (conditional) expression trailing-comment handling in
//! concise arrow function bodies.
//!
//! Structural rule: when the then-branch of a ternary inside a concise arrow
//! body is a call expression and a trailing line comment follows, tsz must not
//! write the source text between the call's closing `)` and the comment as
//! "leading comment trivia" — that text (` : <else> `) was already emitted by
//! `emit_conditional` and must not appear twice in the output.
//!
//! Root cause: `find_token_end_before_trivia` previously preferred
//! `last_token_end` (set by the `)` of the call expression) over
//! `last_non_trivia_at_depth0` (set by the last character of the else branch).
//! `emit_comments_in_range` then wrote the source slice between those two
//! positions — the else branch — as comment leading trivia, duplicating it.

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_print;

// ── Positive cases: no duplication ───────────────────────────────────────────

/// Reported repro: call expression in then-branch with trailing line comment.
#[test]
fn ternary_call_then_trailing_comment_no_else_duplication() {
    let source =
        "declare function foo(): string;\nconst f = (x: boolean) => x ? foo() : '-' // c\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("'-' : '-'"),
        "else branch '-' must not appear twice\nOutput:\n{output}"
    );
    assert!(
        output.contains("x ? foo() : '-'"),
        "full ternary must be present\nOutput:\n{output}"
    );
    assert!(
        output.contains("// c"),
        "trailing comment must be preserved\nOutput:\n{output}"
    );
}

/// Method call in then-branch (different call spelling).
#[test]
fn ternary_method_call_then_trailing_comment_no_else_duplication() {
    let source = "const g = (x: string) => x ? x.toUpperCase() : '-' // transform\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("'-' : '-'"),
        "else branch '-' must not appear twice\nOutput:\n{output}"
    );
    assert!(
        output.contains("x ? x.toUpperCase() : '-'"),
        "full ternary must be present\nOutput:\n{output}"
    );
    assert!(
        output.contains("// transform"),
        "trailing comment must be preserved\nOutput:\n{output}"
    );
}

/// Renamed parameter — structural rule is not name-dependent.
#[test]
fn ternary_call_then_trailing_comment_renamed_param_no_duplication() {
    let source = "declare function bar(): number;\nconst h = (flag: boolean) => flag ? bar() : 0 // result\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("0 : 0"),
        "else branch 0 must not appear twice\nOutput:\n{output}"
    );
    assert!(
        output.contains("flag ? bar() : 0"),
        "full ternary must be present\nOutput:\n{output}"
    );
}

/// String else-branch (not just `-`).
#[test]
fn ternary_call_then_string_else_trailing_comment_no_duplication() {
    let source = "declare function foo(): string;\nconst k = (x: boolean) => x ? foo() : \"default\" // note\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("\"default\" : \"default\""),
        "else branch 'default' must not appear twice\nOutput:\n{output}"
    );
    assert!(
        output.contains("// note"),
        "trailing comment must be preserved\nOutput:\n{output}"
    );
}

// ── Negative cases: these must keep working ──────────────────────────────────

/// Identifier in then-branch: must continue to work correctly.
#[test]
fn ternary_identifier_then_trailing_comment_correct() {
    let source = "const j = (x: string) => x ? x : '-' // id\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("'-' : '-'"),
        "identifier then-branch must not cause duplication\nOutput:\n{output}"
    );
    assert!(
        output.contains("x ? x : '-'"),
        "full ternary must be present\nOutput:\n{output}"
    );
    assert!(
        output.contains("// id"),
        "trailing comment must be preserved\nOutput:\n{output}"
    );
}

/// No trailing comment: call expression in then-branch must emit correctly.
#[test]
fn ternary_call_then_no_comment_correct() {
    let source = "declare function foo(): string;\nconst m = (x: boolean) => x ? foo() : '-'\n";
    let output = parse_and_print(source);
    assert!(
        !output.contains("'-' : '-'"),
        "no-comment case must not duplicate else branch\nOutput:\n{output}"
    );
    assert!(
        output.contains("x ? foo() : '-'"),
        "full ternary must be present\nOutput:\n{output}"
    );
}
