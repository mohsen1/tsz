//! Tests for `parse_test_option_bool` stability fixes

use crate::CheckerState;

#[test]
fn test_parse_test_option_bool_comma_separated() {
    let text = r"
        // @strict: true, false
        // @noimplicitany: false, true
    ";

    // Should handle comma-separated values correctly
    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true), "Should parse 'true' from 'true, false'");

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(
        result,
        Some(false),
        "Should parse 'false' from 'false, true'"
    );
}

#[test]
fn test_parse_test_option_bool_with_semicolon() {
    let text = r"
        // @strict: true;
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true), "Should parse 'true' from 'true;'");
}

#[test]
fn test_parse_test_option_bool_ignores_block_comment_pragmas() {
    // The TypeScript harness only recognizes `// @key: value` lines;
    // `/* @key: value */` is an ordinary comment, never a directive.
    let text = r"
        /* @strict: true; */
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, None, "Block-comment pragmas are not directives");
}

#[test]
fn test_parse_test_option_bool_requires_exact_key() {
    // `@strict` must not match inside `@strictNullChecks` (and vice
    // versa): substring matching previously let a `@strictNullChecks`
    // directive flip the unrelated `strict` option.
    let text = r"
        // @strictNullChecks: false
    ";

    assert_eq!(CheckerState::parse_test_option_bool(text, "@strict"), None);
    assert_eq!(
        CheckerState::parse_test_option_bool(text, "@strictnullchecks"),
        Some(false)
    );
}

#[test]
fn test_parse_test_option_bool_with_comma_and_space() {
    let text = r"
        // @noimplicitany: true , false
    ";

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(
        result,
        Some(true),
        "Should parse 'true' from 'true , false'"
    );
}

#[test]
fn test_parse_test_option_bool_simple() {
    let text = r"
        // @strict: true
        // @noimplicitany: false
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true));

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(result, Some(false));
}

#[test]
fn test_parse_test_option_bool_not_found() {
    let text = r"
        // @strict: true
    ";

    let result = CheckerState::parse_test_option_bool(text, "@noimplicitany");
    assert_eq!(result, None, "Should return None when key not found");
}

#[test]
fn test_parse_test_option_bool_invalid_value() {
    let text = r"
        // @strict: maybe
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, None, "Should return None for invalid boolean value");
}

#[test]
fn test_parse_test_option_bool_empty_value() {
    let text = r"
        // @strict:
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, None, "Should return None for empty value");
}

#[test]
fn test_parse_test_option_bool_trailing_comma() {
    let text = r"
        // @strict: true,
    ";

    let result = CheckerState::parse_test_option_bool(text, "@strict");
    assert_eq!(result, Some(true), "Should parse 'true' from 'true,'");
}
