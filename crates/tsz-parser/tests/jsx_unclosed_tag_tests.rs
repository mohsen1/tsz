//! Tests for JSX unclosed tag detection (TS17008) and mismatched closing tag (TS17002)

use crate::parser::state::ParserState;

fn get_parser_error_codes(source: &str, filename: &str) -> Vec<u32> {
    let mut parser = ParserState::new(filename.to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser.parse_diagnostics.iter().map(|d| d.code).collect()
}

fn get_parser_errors(source: &str, filename: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(filename.to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .parse_diagnostics
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

#[test]
fn test_jsx_child_steals_parent_closer() {
    // <div><span></div> → TS17008 on 'span' (span is unclosed)
    let errors = get_parser_errors("let x = <div><span></div>;", "test.tsx");
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert_eq!(ts17008.len(), 1, "Expected 1 TS17008, got: {errors:?}");
    assert!(
        ts17008[0].1.contains("'span'"),
        "TS17008 should mention 'span', got: {}",
        ts17008[0].1
    );
    // Should NOT emit TS17002
    assert!(
        !errors.iter().any(|(c, _)| *c == 17002),
        "Should not emit TS17002 when child steals parent closer, got: {errors:?}"
    );
}

#[test]
fn test_jsx_wrong_closing_tag() {
    // <div></span> → TS17002 on closing tag
    let errors = get_parser_errors("let x = <div></span>;", "test.tsx");
    let ts17002: Vec<_> = errors.iter().filter(|(c, _)| *c == 17002).collect();
    assert_eq!(ts17002.len(), 1, "Expected 1 TS17002, got: {errors:?}");
    assert!(
        ts17002[0].1.contains("'div'"),
        "TS17002 should mention 'div', got: {}",
        ts17002[0].1
    );
}

#[test]
fn test_jsx_eof_unclosed() {
    // <div> at EOF → TS17008 on 'div'
    let errors = get_parser_errors("let x = <div>", "test.tsx");
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert!(
        !ts17008.is_empty(),
        "Expected at least 1 TS17008, got: {errors:?}"
    );
    assert!(
        ts17008[0].1.contains("'div'"),
        "TS17008 should mention 'div', got: {}",
        ts17008[0].1
    );
}

#[test]
fn test_jsx_nested_eof_unclosed() {
    // <div><span> at EOF → TS17008 on both 'div' and 'span'
    let errors = get_parser_errors("let x = <div><span>", "test.tsx");
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert!(
        ts17008.len() >= 2,
        "Expected at least 2 TS17008, got: {errors:?}"
    );
}

#[test]
fn test_jsx_dotted_tag_unclosed() {
    // <Foo.Bar> at EOF → TS17008 on 'Foo.Bar'
    let errors = get_parser_errors("let x = <Foo.Bar>", "test.tsx");
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert!(
        !ts17008.is_empty(),
        "Expected at least 1 TS17008, got: {errors:?}"
    );
    assert!(
        ts17008[0].1.contains("'Foo.Bar'"),
        "TS17008 should mention 'Foo.Bar', got: {}",
        ts17008[0].1
    );
}

#[test]
fn test_jsx_no_error_on_matching_tags() {
    // <div></div> → no TS17008 or TS17002
    let codes = get_parser_error_codes("let x = <div></div>;", "test.tsx");
    assert!(
        !codes.contains(&17008),
        "Should not emit TS17008 for matching tags, got: {codes:?}"
    );
    assert!(
        !codes.contains(&17002),
        "Should not emit TS17002 for matching tags, got: {codes:?}"
    );
}

#[test]
fn test_jsx_self_closing_no_error() {
    // <div /> → no TS17008 or TS17002
    let codes = get_parser_error_codes("let x = <div />;", "test.tsx");
    assert!(
        !codes.contains(&17008),
        "Should not emit TS17008 for self-closing, got: {codes:?}"
    );
}

#[test]
fn test_jsx_nested_wrong_closer_no_parent_match() {
    // <div><div></span> → TS17002 on span (no parent match), TS17008 on outer div (EOF)
    let errors = get_parser_errors("let x = <div><div></span>;", "test.tsx");
    let ts17002: Vec<_> = errors.iter().filter(|(c, _)| *c == 17002).collect();
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert!(
        !ts17002.is_empty(),
        "Expected TS17002 for wrong closer, got: {errors:?}"
    );
    assert!(
        !ts17008.is_empty(),
        "Expected TS17008 for unclosed outer div, got: {errors:?}"
    );
}
