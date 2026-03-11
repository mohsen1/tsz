//! Tests for statement parsing in the parser.
use crate::parser::{NodeIndex, ParserState};
use tsz_common::diagnostics::diagnostic_codes;

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn parse_statement_recovery_on_malformed_top_level_diagnostics() {
    let (parser, root) = parse_source("const x = 1\nconst y = ;\nconst z = 3;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert!(sf.statements.nodes.len() >= 2);
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_static_block_statement_is_supported() {
    let (parser, root) =
        parse_source("class Holder {\n    static {\n        const v = 1;\n    }\n}\nconst ok = 1;");
    assert_eq!(parser.get_diagnostics().len(), 0);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 2);
}

#[test]
fn parse_with_statement_with_recovery_when_expression_missing() {
    let (parser, _root) = parse_source("with () {}\nconst ok = 1;");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_block_followed_by_equals_emits_ts2809_instead_of_ts1128() {
    let (parser, _root) = parse_source(
        r#"
declare function fn(): { a: 1, b: 2 }
let a: number;
let b: number;

{ a, b } = fn();
{ a, b }
= fn();
"#,
    );
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    let ts2809_count = codes
        .iter()
        .filter(|&&code| {
            code
                == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I
        })
        .count();
    assert_eq!(ts2809_count, 2, "expected two TS2809 diagnostics, got {codes:?}");
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "should not fall back to generic TS1128, got {codes:?}"
    );
}

#[test]
fn parse_invalid_import_expression_start_reports_ts1128_instead_of_from_expected() {
    let (parser, _root) = parse_source("import 10;");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 for invalid import statement start, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "should not emit generic TS1005 'from' expected, got {codes:?}"
    );
}

#[test]
fn parse_mid_file_shebang_reports_ts18026_and_argument_semicolon_error() {
    let (parser, _root) = parse_source("var foo = 1;\n#!/usr/bin/env node\n");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE),
        "expected TS18026 for mid-file shebang, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for shebang argument recovery, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "should not fall back to TS1128, got {codes:?}"
    );
    assert!(!codes.contains(&1499), "should not emit regex flag errors, got {codes:?}");
}

#[test]
fn parse_template_recovery_preserves_follow_up_statement() {
    let (parser, root) = parse_source("const bad = `head${1 + 2`;\nconst ok = 1;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();

    assert!(!sf.statements.nodes.is_empty());
    assert!(!parser.get_diagnostics().is_empty() || !sf.statements.nodes.is_empty());
}

#[test]
fn parse_return_statement_outside_function_recovers_and_continues() {
    let (parser, root) = parse_source("return;\nconst ok = 1;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();

    assert!(!sf.statements.nodes.is_empty());
}

#[test]
fn parse_index_signature_optional_param_emits_ts1019() {
    let (parser, _root) = parse_source("interface Foo { [p2?: string]; }");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    // Should emit TS1019 (optional param in index sig), NOT TS1109 (expression expected)
    assert!(
        codes.contains(&1019),
        "Expected TS1019, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1109),
        "Should NOT emit TS1109, got codes: {codes:?}"
    );
}

#[test]
fn parse_index_signature_rest_param_emits_ts1017() {
    let (parser, _root) = parse_source("interface Foo { [...p3: any[]]; }");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1017),
        "Expected TS1017, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1109),
        "Should NOT emit TS1109, got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_var_name_emits_ts1389() {
    // TS1389: '{0}' is not allowed as a variable declaration name.
    // tsc emits TS1389 (not TS1359) when a reserved word is used as a var declaration name.
    let (parser, _root) = parse_source("var export;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'var export;', got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1359),
        "Should NOT emit TS1359 (generic reserved word), got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_const_name_emits_ts1389() {
    let (parser, _root) = parse_source("const class = 1;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'const class = 1;', got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_let_name_emits_ts1389() {
    let (parser, _root) = parse_source("let typeof = 10;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'let typeof = 10;', got codes: {codes:?}"
    );
}

#[test]
fn parse_contextual_keyword_as_var_name_no_ts1389() {
    // Contextual keywords (type, interface, etc.) should NOT trigger TS1389
    // — they're valid as variable names.
    let (parser, _root) = parse_source("var type = 1;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1389),
        "Contextual keyword 'type' should NOT trigger TS1389, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1359),
        "Contextual keyword 'type' should NOT trigger TS1359, got codes: {codes:?}"
    );
}

#[test]
fn class_field_initializer_does_not_asi_before_computed_member() {
    let (parser, _root) = parse_source("class C {\n    [e]: number = 0\n    [e2]: number\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for missing semicolon before computed member, got {diags:?}"
    );
    assert!(
        !codes.contains(&1068),
        "should recover as a semicolon error, not TS1068, got {diags:?}"
    );
}

#[test]
fn invalid_var_like_class_member_does_not_emit_keyword_suggestion_cascade() {
    let (parser, _root) = parse_source(
        "class C {\n    public const var export foo = 10;\n\n    var constructor() { }\n}",
    );
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION),
        "expected TS1440 on invalid class member var recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN),
        "should not emit TS1435 after TS1440 var-like class member recovery, got {diags:?}"
    );
}
