//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — incremental parse.

use crate::parser::ParserState;

#[test]
fn test_incremental_parse_from_middle_of_file() {
    // Test parsing from an offset in the middle of a source file
    let source = r"const a = 1;
const b = 2;
function foo() {
    return a + b;
}
const c = 3;";

    // Parse from the start of "function foo()"
    let offset = u32::try_from(
        source
            .find("function")
            .expect("pattern should exist in source"),
    )
    .expect("function offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should have parsed the remaining statements (function and const c)
    let statement_count = result.statements.len();
    assert!(
        statement_count >= 2,
        "Expected at least 2 statements from offset, got {statement_count}",
    );

    // Should not produce errors for valid code
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for incremental parse, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_start() {
    // Test incremental parsing from offset 0 (should be equivalent to full parse)
    let source = r#"const x = 42;
let y = "hello";"#;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        0,
    );

    // Should have parsed both statements
    let statement_count = result.statements.len();
    assert_eq!(
        statement_count, 2,
        "Expected 2 statements, got {statement_count}",
    );

    // reparse_start should be 0
    assert_eq!(result.reparse_start, 0);

    // Should not produce errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_end() {
    // Test incremental parsing from beyond the end of file
    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        1000, // Beyond EOF
    );

    // Should handle gracefully - clamped to source length
    assert!(
        result.statements.is_empty(),
        "Expected no statements when starting at EOF"
    );
}

#[test]
fn test_incremental_parse_records_reparse_start() {
    // Test that reparse_start is recorded correctly
    let source = "const a = 1;\nconst b = 2;";
    let offset = 13u32; // Start of "const b"

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // reparse_start should match the offset we provided
    let reparse_start = result.reparse_start;
    assert_eq!(
        reparse_start, offset,
        "Expected reparse_start to be {offset}, got {reparse_start}",
    );
}

#[test]
fn test_incremental_parse_with_syntax_error() {
    // Test incremental parsing recovers from syntax errors
    let source = r"const a = 1;
const b = ;
const c = 3;";

    // Parse from start of "const b = ;" (syntax error)
    let offset = u32::try_from(
        source
            .find("const b")
            .expect("pattern should exist in source"),
    )
    .expect("const b offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should still parse statements (with recovery)
    let statement_count = result.statements.len();
    assert!(
        !result.statements.is_empty(),
        "Expected at least 1 statement after recovery, got {statement_count}",
    );

    // Should produce an error for the syntax issue
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected at least one diagnostic for syntax error"
    );
}
