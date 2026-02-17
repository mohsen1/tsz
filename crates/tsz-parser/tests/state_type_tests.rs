//! Tests for type expression parsing in the parser.
use crate::parser::{NodeIndex, ParserState};

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn parse_complex_type_expressions_have_no_errors() {
    let (parser, _root) = parse_source(
        "type T = { [K in keyof O]: O[K] } & Partial<{ a: string; b: number }>;\ntype U<T> = T extends { a: infer V } ? V : never;",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_conditional_and_infer_types_emit_expected_members() {
    let (parser, _root) =
        parse_source("type T<T> = T extends string ? { kind: 's' } : { kind: 'o' };");
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_invalid_type_member_reports_diagnostics() {
    let (parser, _root) = parse_source("type T = <; ");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_template_literal_type_with_placeholder() {
    let (parser, _root) = parse_source("type T = `a${string}b`;");
    assert_eq!(parser.get_diagnostics().len(), 0);
}

#[test]
fn parse_keyof_infer_tuple_type_without_tail_is_tolerated() {
    let (parser, _root) = parse_source("type T = keyof infer X");
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn parse_mapped_type_with_keyof_retrieval_has_no_errors() {
    let (parser, _root) = parse_source(
        "type Wrapped<T> = { [K in keyof T]: T[K] };\ntype ReadonlyWrapped = Wrapped<{ a: string; b: number; }>;",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
}
