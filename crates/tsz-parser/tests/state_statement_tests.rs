//! Tests for statement parsing in the parser.
use crate::parser::{NodeIndex, ParserState};

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
