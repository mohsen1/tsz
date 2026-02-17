//! Tests for expression parsing in the parser.
use crate::parser::{NodeIndex, ParserState};

fn parse_diagnostics(source: &str) -> usize {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser.get_diagnostics().len()
}

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn expression_parsing_handles_shift_and_greater_token_ambiguity() {
    let diag_count = parse_diagnostics("const shifted = 1 >> 2 >>> 3; let rhs = x >= 1;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_handles_regex_division_boundary_after_tokens() {
    let diag_count =
        parse_diagnostics("const n = 10 / 2; const re = /foo/g; const tail = (a / b) / c;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_reports_template_recovery_for_unterminated_tail() {
    let diag_count = parse_diagnostics("const t = `a${1 + 2`; const ok = 1;");
    assert!(
        diag_count > 0,
        "expected diagnostics for unterminated template tail"
    );
}

#[test]
fn expression_parsing_rejects_incomplete_shift_rhs() {
    let diag_count = parse_diagnostics("const x = 1 >> ;");
    assert!(
        diag_count > 0,
        "expected diagnostics for incomplete shift expression"
    );
}

#[test]
fn expression_parsing_generic_arrow_after_shift_restores_state() {
    let diag_count = parse_diagnostics("const f = <T>(value: T) => value >> 0;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_regex_literals_and_division_paths() {
    let diag_count =
        parse_diagnostics("const re = /foo/g;\nconst n = 10 / 2;\nlet x = 1;\nx /= 2;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_tagged_and_plain_templates() {
    let diag_count =
        parse_diagnostics("const tag = String.raw`head${1 + 2}tail`;\nconst plain = `x${1 + 2}y`;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");

    let (parser, root) = parse_source("const bad = `head${1 + 2`;\nconst ok = 1;");
    assert!(!parser.get_diagnostics().is_empty());
    let sf = parser
        .get_arena()
        .get_source_file_at(root)
        .unwrap_or_else(|| panic!("missing source file node"));
    assert!(!sf.statements.nodes.is_empty());
}

#[test]
fn expression_parsing_handles_regex_and_division_tokens() {
    let diag_count =
        parse_diagnostics("const re = /foo/g;\nconst value = 10 / 2;\nconst bad = a / 0;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_compound_shift_assignment() {
    let diag_count = parse_diagnostics("let n = 8;\nn >>>= 2;\nn = n >> 1;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}
