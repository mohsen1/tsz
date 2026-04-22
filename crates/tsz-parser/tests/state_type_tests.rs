//! Tests for type expression parsing in the parser.
use crate::parser::{NodeIndex, ParserState};

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn parse_source_named(file_name: &str, source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
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
fn parse_flow_style_type_parameter_bound_reports_comma_expected() {
    let source = "export default class B<T: BaseA> {}";
    let (parser, _root) = parse_source_named("test.js", source);
    let diagnostics = parser.get_diagnostics();
    let colon_pos = source.find(':').expect("expected colon") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|d| { d.code == 1005 && d.start == colon_pos && d.message == "',' expected." }),
        "Expected TS1005 comma diagnostic at Flow-style type parameter bound, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !(d.code == 1005 && d.start == colon_pos && d.message == "'>' expected.")),
        "Type parameter list recovery should not report a closing `>` at the same colon, got {diagnostics:?}"
    );
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

#[test]
fn parse_call_signature_with_arrow_reports_colon_expected_not_property_signature_expected() {
    let (parser, _root) = parse_source("type T = { (n: number) => string; };");
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.message == "':' expected."),
        "Expected TS1005 ':' expected for malformed call signature, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1131),
        "Malformed call signature should not fall back to TS1131, got {diagnostics:?}"
    );
}

#[test]
fn parse_construct_signature_with_arrow_reports_colon_expected_not_property_signature_expected() {
    let (parser, _root) = parse_source("type T = { new (n: number) => string; };");
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1005 && d.message == "':' expected."),
        "Expected TS1005 ':' expected for malformed construct signature, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 1131),
        "Malformed construct signature should not fall back to TS1131, got {diagnostics:?}"
    );
}
