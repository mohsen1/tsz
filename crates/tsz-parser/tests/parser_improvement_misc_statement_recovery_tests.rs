//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — misc statement recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_await_using_array_target_assignment_recovers_with_semicolon_expected() {
    let source = r"
{
    await using [a] = null;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let equals_pos = source.find('=').expect("assignment token") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.message == "';' expected." && diag.start == equals_pos
        }),
        "Expected TS1005 ';' expected at '=' for await using recovery, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.message != "Expression expected."),
        "Should not emit TS1109 for this recovery shape: {diagnostics:?}"
    );
}

#[test]
fn test_in_expression_assignment_recovers_with_semicolon_expected() {
    let source = "'prop' in v = 10;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let equals_pos = source.find('=').expect("assignment token") as u32;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 1005 && diag.message == "';' expected." && diag.start == equals_pos
        }),
        "Expected TS1005 ';' expected at '=' for in-expression assignment recovery, got {diagnostics:?}"
    );
}

#[test]
fn test_property_access_missing_name_at_eof_reports_ts1003_after_dot() {
    let source = "var p2 = window. ";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let expected_start = source.find('.').expect("dot position") as u32 + 1;

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::IDENTIFIER_EXPECTED
                && diag.message == "Identifier expected."
                && diag.start == expected_start
        }),
        "Expected TS1003 immediately after '.' at EOF, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_strict_binder_parse_diagnostics_are_preserved() {
    let source = r#"
export default 12
const yield = 1
async function f() {
    const await = 2
}
class C {
    #constructor = 3
}
"#;
    let (parser, _root) = parse_source_named("plainJSBinderErrors.js", source);

    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    for code in [1359, 18012] {
        assert!(
            codes.contains(&code),
            "Expected parser diagnostic TS{code} in plain JS async/class strict contexts. Got: {codes:?}"
        );
    }
}

#[test]
fn test_top_level_modifier_recovery_keeps_try_block_error() {
    let source = "cla <ss {\n  _ static try\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    let ts1434: Vec<_> = diagnostics.iter().filter(|d| d.code == 1434).collect();
    let ts1128: Vec<_> = diagnostics.iter().filter(|d| d.code == 1128).collect();

    assert_eq!(
        ts1005.len(),
        2,
        "Expected both the leading ';' recovery and the trailing try-block '{{' recovery, got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == 8 && diag.message == "';' expected."),
        "Expected the leading ';' recovery at the stray '{{', got {diagnostics:?}"
    );
    assert!(
        ts1005
            .iter()
            .any(|diag| diag.start == 25 && diag.message == "'{' expected."),
        "Expected the downstream try-statement '{{' recovery at EOF, got {diagnostics:?}"
    );
    assert_eq!(
        ts1434.len(),
        1,
        "Expected the stray identifier recovery to remain, got {diagnostics:?}"
    );
    assert_eq!(
        ts1128.len(),
        1,
        "Expected the top-level modifier recovery to keep TS1128, got {diagnostics:?}"
    );
}

#[test]
fn test_repeated_top_level_close_parens_emit_separate_ts1128() {
    let source = "function foo() {\n}\n\nfunction foo() {\n}\n\n)\n)";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1128_count = diagnostics.iter().filter(|diag| diag.code == 1128).count();

    assert_eq!(
        ts1128_count, 2,
        "Expected one TS1128 per stray top-level close paren, got {diagnostics:?}"
    );
}

#[test]
fn test_empty_element_access_reports_after_open_bracket() {
    let source = r#"
class Z {
 public x = "";
}

var a6: Z[][] = new   Z     [      ]   [  ];
"#;
    let (parser, _root) = parse_source(source);

    let ts1011_starts: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1011)
        .map(|d| d.start)
        .collect();

    assert_eq!(
        ts1011_starts,
        vec![59, 70],
        "Empty element-access diagnostics should anchor immediately after '[' even with inner whitespace: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_export_type_alias_missing_type_at_eof_reports_after_equals() {
    let source = "import test from \"./test\";\nexport type test = \n";
    let (parser, _root) = parse_source_named("types2.ts", source);

    let ts1110_starts: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_EXPECTED)
        .map(|d| d.start)
        .collect();
    let expected_start = source.find("= \n").unwrap() as u32 + 1;

    assert_eq!(
        ts1110_starts,
        vec![expected_start],
        "Missing export type bodies at EOF should anchor TS1110 after '=': {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_decimal_numeric_literal_recovery_matches_conformance_shape() {
    let source = "1.toString();\nvar test2 = 2.toString();\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        ],
        "Trailing-decimal recovery should match the numeric literal conformance shape, got diagnostics: {diagnostics:?}"
    );

    let standalone_identifier_pos = source.find("toString").unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == diagnostic_codes::EXPECTED
                && diag.start as usize == standalone_identifier_pos)),
        "Standalone `1.toString()` should not emit a spurious missing-semicolon diagnostic: {diagnostics:?}"
    );

    let var_stmt_start = source.find("var test2 = 2.toString();").unwrap();
    let open_paren_pos = var_stmt_start + "var test2 = 2.toString".len();
    let close_paren_pos = open_paren_pos + 1;

    let ts1005 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the recovered call tail");
    assert_eq!(
        ts1005.start as usize, open_paren_pos,
        "TS1005 should anchor at the opening paren after the recovered identifier tail: {diagnostics:?}"
    );

    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for the empty recovered call expression");
    assert_eq!(
        ts1109.start as usize, close_paren_pos,
        "TS1109 should anchor at the closing paren after the recovered empty call: {diagnostics:?}"
    );
}
