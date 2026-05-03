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
fn test_jsx_unclosed_tag_suppressed_after_conflict_marker() {
    // `<div>` followed by a Git merge conflict marker — tsc emits TS1185 for
    // the marker but does NOT emit TS17008 for the unclosed `<div>`. tsz
    // matches.
    let source = "const x = <div>\n<<<<<<< HEAD\n";
    let errors = get_parser_errors(source, "test.tsx");
    let ts17008: Vec<_> = errors.iter().filter(|(c, _)| *c == 17008).collect();
    assert!(
        ts17008.is_empty(),
        "Conflict marker should suppress TS17008, got: {errors:?}"
    );
    let ts1185: Vec<_> = errors.iter().filter(|(c, _)| *c == 1185).collect();
    assert!(
        !ts1185.is_empty(),
        "Expected TS1185 for conflict marker, got: {errors:?}"
    );
}

#[test]
fn test_jsx_missing_closing_tag_anchor_after_conflict_marker_uses_opening_end() {
    // After a Git merge conflict marker terminates JSX child scanning, tsc
    // anchors the missing `</` error at the END of the opening element, not
    // at the EOF position. Match that anchor so the diagnostic fingerprint
    // lines up with tsc.
    let source = "const x = <div>\n<<<<<<< HEAD";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let ts1005: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1005 && d.message == "'</' expected.")
        .collect();
    assert_eq!(
        ts1005.len(),
        1,
        "Expected exactly one TS1005 `</` diagnostic, got: {ts1005:?}",
    );
    // Anchor must be at end of `<div>`. The opening tag occupies bytes 10..15
    // (`<div>`); its end is byte 15.
    let opening_end = source.find("<div>").unwrap() as u32 + "<div>".len() as u32;
    let actual_start = ts1005[0].start;
    assert_eq!(
        actual_start, opening_end,
        "TS1005 must anchor at the end of `<div>` (offset {opening_end}), got offset {actual_start}",
    );
}

#[test]
fn test_jsx_unterminated_attribute_string_suppresses_missing_closing_tag() {
    let errors = get_parser_errors("let x = <div attr=\"unterminated", "test.tsx");
    assert!(
        errors.iter().any(|(code, _)| *code == 1002),
        "Expected TS1002 for unterminated string literal, got: {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|(code, message)| *code == 1005 && message == "'</' expected."),
        "Unterminated string literal should suppress cascading TS1005 `</`, got: {errors:?}"
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

#[test]
fn test_no_ts1382_in_unclosed_recovery_patterns() {
    // These patterns appear in jsxUnclosedParserRecovery.ts and must NOT emit TS1382.
    // TS1382 = "Unexpected token. Did you mean `{'>'}` or `&gt;`?" (bare > in JSX text)
    // That error is only valid for a literal `>` appearing inside JSX text content.
    let patterns = vec![
        ("noClose", "var d = <div>\n    <diddy\n</div>;"),
        (
            "noCloseTypeArg",
            "var d = <div>\n    <diddy<boolean>\n</div>;",
        ),
        (
            "noCloseTypeArgAttrs",
            "var d = <div>\n    <diddy<boolean> bananas=\"please\"\n</div>;",
        ),
        ("noCloseBracket", "var d = <div>\n    <diddy/\n</div>;"),
        (
            "noCloseBracketTypeArgAttrs",
            "var d = <div>\n    <diddy<boolean> bananas=\"please\"/\n</div>;",
        ),
        (
            "noSelfcloseTypeArgAttrs",
            "var d = <div>\n    <diddy<boolean> bananas=\"please\">\n</div>;",
        ),
        (
            "noCloseTypeArgTrailingTag",
            "var d = <div>\n    <diddy<boolean>\n    <diddy/>\n</div>;",
        ),
        (
            "noCloseBracketTypeArgAttrsTrailingTag",
            "var d = <div>\n    <diddy<boolean> bananas=\"please\"/\n    <diddy/>\n</div>;",
        ),
        (
            "noCloseTypeArgTrailingText",
            "var d = <div>\n    <diddy<boolean>\n    Cranky Wrinkly Funky\n</div>;",
        ),
        (
            "noCloseBracketTypeArgAttrsTrailingText",
            "var d = <div>\n    <diddy<boolean> bananas=\"please\"/\n    Cranky Wrinkly Funky\n</div>;",
        ),
    ];

    let mut found = vec![];
    for (name, src) in &patterns {
        let codes = get_parser_error_codes(src, "test.tsx");
        if codes.contains(&1382) {
            found.push(format!("{name}: {codes:?}"));
        }
    }
    assert!(
        found.is_empty(),
        "Unexpected TS1382 in recovery patterns:\n{}",
        found.join("\n")
    );
}

#[test]
fn test_jsx_unclosed_at_eof_emits_ts1005_not_ts17002() {
    // tsc behavior: when an unclosed JSX element reaches EOF, `parseExpected`
    // for `</` always emits TS1005 `'</' expected.` at the EOF position,
    // deduped only by exact same start. Without the EOF force-emit, tsz's
    // distance-based suppression hides the TS1005 (because TS17008 was just
    // emitted within 3 tokens), and the downstream tag-mismatch path then
    // wrongly emits TS17002 instead. Verify TS1005 fires and TS17002 does
    // not, mirroring tsc.
    //
    // Try multiple shapes — a bare `<a>;`, a nested `<a><a />;`, and an
    // attribute-bearing `<a b={}>;` — to ensure the fix is structural and
    // not tied to a particular identifier name or attribute shape.
    for source in ["<a>;", "<a><a />;", "<a b={}>;", "<x>;", "<x><x />;"] {
        let errors = get_parser_errors(source, "test.tsx");
        let ts1005_close: Vec<_> = errors
            .iter()
            .filter(|(c, m)| *c == 1005 && m == "'</' expected.")
            .collect();
        assert!(
            !ts1005_close.is_empty(),
            "Expected TS1005 `'</' expected.` for {source:?}, got: {errors:?}",
        );
        let ts17002: Vec<_> = errors.iter().filter(|(c, _)| *c == 17002).collect();
        assert!(
            ts17002.is_empty(),
            "Expected no TS17002 (tag mismatch should dedup against TS1005 at EOF) for {source:?}, got: {errors:?}",
        );
    }
}

#[test]
fn test_jsx_unclosed_at_eof_position_matches_tsc_no_trailing_newline() {
    // tsc emits TS1005 at the EOF position (= end of last content) when the
    // file has no trailing newline. Without the EOF force-emit, tsz's
    // suppression-distance path skips TS1005 and falls back to TS17002 at a
    // different position. Lock the position at the byte after `;` for `<a>;`.
    let source = "<a>;";
    let errors = get_parser_errors(source, "test.tsx");
    let ts1005: Vec<_> = errors
        .iter()
        .filter(|(c, m)| *c == 1005 && m == "'</' expected.")
        .collect();
    assert_eq!(
        ts1005.len(),
        1,
        "Expected exactly one TS1005 `'</' expected.`, got: {errors:?}",
    );
}
