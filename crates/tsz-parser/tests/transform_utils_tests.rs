//! Tests for transform utility helpers (`contains_this_reference`, `contains_arguments_reference`).
use crate::parser::{NodeIndex, ParserState};
use crate::syntax::transform_utils::contains_arguments_reference;
use crate::syntax::transform_utils::contains_this_reference;

fn parse_arena(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn class_member_initializer(source: &str, member_index: usize) -> (ParserState, NodeIndex) {
    let (parser, root) = parse_arena(source);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = sf.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    let member_idx = class_data.members.nodes[member_index];
    let member_node = parser.get_arena().get(member_idx).unwrap();
    let initializer = parser
        .get_arena()
        .get_property_decl(member_node)
        .unwrap()
        .initializer;
    (parser, initializer)
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

#[test]
fn contains_arguments_reference_ignores_object_literal_property_names() {
    let (parser, root) = parse_arena("function f() { foo({ x, arguments: [] }); return 0; }");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    let statement = sf.statements.nodes[0];
    let statement_node = parser.get_arena().get(statement).unwrap();
    let func = parser.get_arena().get_function(statement_node).unwrap();
    let body = func.body;

    assert!(!contains_arguments_reference(parser.get_arena(), body));
}

#[test]
fn contains_this_reference_ignores_nested_class_instance_scope() {
    let (parser, initializer) = class_member_initializer(
        "class C { static bar = class Inner { value = this; method() { return this; } } }",
        0,
    );

    assert!(!contains_this_reference(parser.get_arena(), initializer));
}

#[test]
fn contains_this_reference_detects_nested_class_computed_property_names() {
    let (parser, initializer) = class_member_initializer(
        "class C { static c = 'foo'; static bar = class Inner { static [this.c] = 123; [this.c] = 123; } }",
        1,
    );

    assert!(contains_this_reference(parser.get_arena(), initializer));
}

#[test]
fn contains_this_reference_detects_nested_class_heritage_clauses() {
    let (parser, initializer) = class_member_initializer(
        "class C { static Base = class {}; static bar = class Inner extends this.Base {} }",
        1,
    );

    assert!(contains_this_reference(parser.get_arena(), initializer));
}
