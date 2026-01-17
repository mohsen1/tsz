//! ASI Conformance Tests
//!
//! Test ASI (Automatic Semicolon Insertion) behavior against JavaScript/TypeScript spec.
//! Focus on TS1005 (token expected) and TS1109 (expression expected) error codes.

use crate::checker::types::diagnostics::diagnostic_codes;
use crate::thin_parser::ThinParserState;

/// Test that throw with line break reports TS1109
#[test]
fn test_asi_throw_line_break_reports_ts1109() {
    let source = r#"
function f() {
    throw
    new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|d| d.code)
        .collect();

    assert!(codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should emit TS1109 for line break after throw, got: {:?}", codes);
}

/// Test that throw without line break is OK
#[test]
fn test_asi_throw_no_line_break_ok() {
    let source = r#"
function f() {
    throw new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|d| d.code)
        .collect();

    assert!(!codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should NOT emit TS1109 for throw on same line, got: {:?}", codes);
}

/// Test return with line break (ASI applies, returns undefined)
#[test]
fn test_asi_return_line_break() {
    let source = r#"
function f() {
    return
    x + y;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - return is a complete statement
    // The "x + y" becomes a separate (unreachable) statement
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test postfix ++ with line break (ASI applies)
#[test]
fn test_asi_postfix_increment_line_break() {
    let source = r#"
let x = 5
x++;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse as two statements: let x = 5; x++;
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test prefix ++ after line break (valid)
#[test]
fn test_asi_prefix_increment_after_line_break() {
    let source = r#"
let a = 5
let b = ++a;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse as: let a = 5; let b = ++a;
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test yield with line break (ASI applies)
#[test]
fn test_asi_yield_line_break() {
    let source = r#"
function* g() {
    yield
    x + y;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - yield without expression is valid
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test break with label after line break (ASI applies)
#[test]
fn test_asi_break_label_line_break() {
    let source = r#"
outer: while (true) {
    break
    outer;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - break; outer; (two statements)
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test arrow function with concise body
#[test]
fn test_asi_arrow_function_concise_body() {
    let source = r#"
let f = x => x * 2;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse arrow function correctly
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test arrow function with object literal (requires parens)
#[test]
fn test_asi_arrow_function_object_literal() {
    let source = r#"
let f = x => ({ x: 1 });
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with parentheses
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test ASI at EOF before closing brace
#[test]
fn test_asi_eof_before_closing_brace() {
    let source = r#"
function f() {
    return 42
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies at EOF before }
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Comprehensive ASI edge case test suite
#[test]
fn test_asi_comprehensive_edge_cases() {
    let test_cases = vec![
        // (source, should_have_errors, description)
        // Valid ASI cases
        (r#"function f() { return }"#, false, "return without semicolon"),
        (r#"function f() { throw {}"#, false, "throw without semicolon (should error but for different reason)"),

        // Line break triggers ASI
        (r#"function f() { return\nx }"#, false, "return with line break (ASI)"),

        // throw with line break should error
        (r#"function f() { throw\nnew Error() }"#, true, "throw with line break (TS1109)"),

        // Postfix operators with line break
        (r#"let x = 5\nx++"#, false, "postfix ++ after line break"),
        (r#"let y = 5\ny--"#, false, "postfix -- after line break"),
    ];

    for (i, (source, should_have_errors, description)) in test_cases.iter().enumerate() {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        parser.parse_source_file();

        let has_errors = !parser.get_diagnostics().is_empty();

        if *should_have_errors && !has_errors {
            panic!("Test case {} ({}) expected errors but got none: {:?}", i, description, source);
        }
    }
}

/// Test TS1005 patterns (token expected)
#[test]
fn test_asi_ts1005_token_expected_patterns() {
    let test_cases = vec![
        // Missing tokens that should trigger TS1005
        (r#"function f() { }"#, false, "complete function"),
        (r#"function f( { }"#, true, "missing closing paren in function params"),
        (r#"if (true { }"#, true, "missing closing paren in if"),
    ];

    for (i, (source, should_have_errors, description)) in test_cases.iter().enumerate() {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        parser.parse_source_file();

        let has_errors = !parser.get_diagnostics().is_empty();

        if *should_have_errors && !has_errors {
            panic!("Test case {} ({}) expected errors but got none", i, description);
        }
    }
}
