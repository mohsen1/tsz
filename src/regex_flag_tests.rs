//! Unit tests for regex flag error detection
//!
//! These tests verify that the scanner correctly detects and reports
//! regex flag errors (invalid flags, duplicate flags, incompatible u/v flags).

use crate::parser::state::ParserState;

#[test]
fn test_invalid_flag_x() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/x;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        1,
        "Expected 1 error for invalid flag 'x'"
    );
    assert_eq!(diagnostics[0].code, 1499, "Expected TS1499 (Unknown flag)");
    assert!(
        diagnostics[0].message.contains("Unknown"),
        "Error message should mention 'Unknown'"
    );
}

#[test]
fn test_duplicate_flag_gg() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/gg;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        1,
        "Expected 1 error for duplicate flag 'g'"
    );
    assert_eq!(
        diagnostics[0].code, 1500,
        "Expected TS1500 (Duplicate flag)"
    );
    assert!(
        diagnostics[0].message.contains("Duplicate"),
        "Error message should mention 'Duplicate'"
    );
}

#[test]
fn test_incompatible_flags_uv() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/uv;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        1,
        "Expected 1 error for incompatible u/v flags"
    );
    assert_eq!(
        diagnostics[0].code, 1502,
        "Expected TS1502 (Incompatible flags)"
    );
    assert!(
        diagnostics[0].message.contains("Unicode")
            || diagnostics[0].message.contains("u")
            || diagnostics[0].message.contains("v"),
        "Error message should mention Unicode, u, or v flags"
    );
}

#[test]
fn test_multiple_invalid_flags() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/xx;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        2,
        "Expected 2 errors for two invalid 'x' flags"
    );
    assert_eq!(diagnostics[0].code, 1499, "First error should be TS1499");
    assert_eq!(diagnostics[1].code, 1499, "Second error should be TS1499");
}

#[test]
fn test_mixed_errors() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/ggxx;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    // ggxx should have: 1x duplicate (second g) + 2x invalid (x,x) = 3 errors
    // (first g is valid, second g is duplicate, both x's are invalid)
    assert_eq!(diagnostics.len(), 3, "Expected 3 errors for 'ggxx'");

    // First should be duplicate flag error (second g)
    assert_eq!(
        diagnostics[0].code, 1500,
        "First error should be TS1500 (duplicate g)"
    );

    // Last two should be invalid flag errors (x's)
    assert_eq!(
        diagnostics[1].code, 1499,
        "Second error should be TS1499 (invalid x)"
    );
    assert_eq!(
        diagnostics[2].code, 1499,
        "Third error should be TS1499 (invalid x)"
    );
}

#[test]
fn test_valid_regex_no_errors() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/gim;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no errors for valid regex with flags 'gim'"
    );
}

#[test]
fn test_valid_regex_no_flags() {
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no errors for valid regex with no flags"
    );
}

#[test]
fn test_all_valid_flags() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const r = /test/gimsuyd;".to_string(),
    );
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    assert_eq!(
        diagnostics.len(),
        0,
        "Expected no errors for all valid flags"
    );
}

#[test]
fn test_complex_incompatible_flags() {
    // /test/guvx has: g (valid), u (valid), v (incompatible with u), x (invalid)
    let mut parser = ParserState::new("test.ts".to_string(), "const r = /test/guvx;".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    // Should have 1 incompatible error + 1 invalid error = 2 total
    assert!(
        diagnostics.len() >= 2,
        "Expected at least 2 errors (incompatible + invalid)"
    );

    // Check that we have the incompatible flags error
    let has_incompatible = diagnostics.iter().any(|d| d.code == 1502);
    assert!(
        has_incompatible,
        "Should have TS1502 (incompatible u/v flags)"
    );

    // Check that we have the invalid flag error
    let has_invalid = diagnostics.iter().any(|d| d.code == 1499);
    assert!(has_invalid, "Should have TS1499 (invalid flag x)");
}
