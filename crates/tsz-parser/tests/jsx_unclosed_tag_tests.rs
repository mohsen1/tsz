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

// TS1382: bare `>` in JSX text
#[test]
fn test_jsx_bare_greater_than_emits_ts1382() {
    let codes = get_parser_error_codes("let x = <div>></div>;", "test.tsx");
    assert!(
        codes.contains(&1382),
        "Expected TS1382 for bare '>' in JSX text, got codes: {codes:?}"
    );
}

#[test]
fn test_jsx_bare_greater_than_after_expression_emits_ts1382() {
    let codes = get_parser_error_codes("let x = <div>{\"foo\"}></div>;", "test.tsx");
    assert!(
        codes.contains(&1382),
        "Expected TS1382 for bare '>' after expression, got codes: {codes:?}"
    );
}

// TS1381: bare `}` in JSX text
#[test]
fn test_jsx_bare_close_brace_emits_ts1381() {
    let codes = get_parser_error_codes("let x = <div>}</div>;", "test.tsx");
    assert!(
        codes.contains(&1381),
        "Expected TS1381 for bare '}}' in JSX text, got codes: {codes:?}"
    );
}

#[test]
fn test_jsx_no_ts1382_without_bare_greater_than() {
    // Normal JSX text without bare > should not emit TS1382
    let codes = get_parser_error_codes("let x = <div>hello</div>;", "test.tsx");
    assert!(
        !codes.contains(&1382),
        "Should not emit TS1382 for normal JSX text, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1381),
        "Should not emit TS1381 for normal JSX text, got codes: {codes:?}"
    );
}

#[test]
fn test_jsx_child_comma_expression_emits_ts18007() {
    let codes = get_parser_error_codes("let x = <div>{foo, bar}</div>;", "test.tsx");
    let ts18007_count = codes.iter().filter(|&&code| code == 18007).count();
    assert_eq!(
        ts18007_count, 1,
        "Expected one TS18007 for JSX child comma expression, got codes: {codes:?}"
    );
}

#[test]
fn test_jsx_attribute_comma_expression_emits_ts18007() {
    let codes = get_parser_error_codes("let x = <div className={foo, bar} />;", "test.tsx");
    let ts18007_count = codes.iter().filter(|&&code| code == 18007).count();
    assert_eq!(
        ts18007_count, 1,
        "Expected one TS18007 for JSX attribute comma expression, got codes: {codes:?}"
    );
}
