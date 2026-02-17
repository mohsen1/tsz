//! Tests for transform utility helpers (`contains_this_reference`, `contains_arguments_reference`).
use crate::parser::{NodeIndex, ParserState};
use crate::syntax::transform_utils::contains_arguments_reference;
use crate::syntax::transform_utils::contains_this_reference;

fn parse_arena(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn contains_this_reference_detects_this_in_function_body() {
    let (parser, root) = parse_arena("function f() { return this; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(contains_this_reference(parser.get_arena(), body));
}

#[test]
fn contains_this_reference_ignores_literal_tree() {
    let (parser, root) = parse_arena("function noThis() { return 42; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_this_reference(parser.get_arena(), body));
}

#[test]
fn contains_arguments_reference_detects_arguments_in_function_body() {
    let (parser, root) = parse_arena("function f() { return arguments; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(contains_arguments_reference(parser.get_arena(), body));
}

#[test]
fn contains_arguments_reference_ignores_missing_reference() {
    let (parser, root) = parse_arena("function noArgs() { return 42; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_arguments_reference(parser.get_arena(), body));
}
