//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — template recovery.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_ts1125_tagged_template_does_not_emit_errors() {
    // Tagged templates (ES2018+) allow invalid escape sequences per spec.
    // tsc does NOT emit TS1125 for tagged templates — only for untagged templates.
    let source =
        r#"const x = tag`\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // Tagged templates should NOT get TS1125 errors
    assert_eq!(
        ts1125_diagnostics.len(),
        0,
        "Expected 0 TS1125 errors for tagged template, got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}

#[test]
fn test_ts1125_untagged_template_emits_errors() {
    // Untagged templates with invalid escape sequences DO get TS1125.
    let source =
        r#"const y = `\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // We should get 3 TS1125 errors (for \u{hello}, \xtraordinary, \uworld)
    assert_eq!(
        ts1125_diagnostics.len(),
        3,
        "Expected 3 TS1125 errors (for \\u{{hello}}, \\xtraordinary, \\uworld), got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}
