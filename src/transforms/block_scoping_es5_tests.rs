use super::*;
use crate::parser::NodeIndex;
use crate::thin_parser::ThinParserState;

fn parse_first_loop(source: &str) -> (ThinParserState, NodeIndex, NodeIndex) {
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let root_node = parser.arena.get(root).expect("expected root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("expected source file");
    let stmt_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("expected loop statement");
    let stmt_node = parser.arena.get(stmt_idx).expect("expected loop node");
    let loop_data = parser
        .arena
        .get_loop(stmt_node)
        .expect("expected loop data")
        .clone();

    (parser, loop_data.initializer, loop_data.statement)
}

#[test]
fn test_collect_loop_vars_from_initializer() {
    let (parser, initializer_idx, _) = parse_first_loop("for (let i = 0, j = 1; i < 3; i++) { }");
    let vars = collect_loop_vars(&parser.arena, initializer_idx);

    assert_eq!(vars, vec!["i".to_string(), "j".to_string()]);
}

#[test]
fn test_collect_loop_vars_expression_initializer() {
    let (parser, initializer_idx, _) = parse_first_loop("for (i = 0; i < 3; i++) { }");
    let vars = collect_loop_vars(&parser.arena, initializer_idx);

    assert!(
        vars.is_empty(),
        "Expected no vars from expression initializer"
    );
}

#[test]
fn test_analyze_loop_capture_detects_capture() {
    let (parser, initializer_idx, body_idx) =
        parse_first_loop("for (let i = 0; i < 3; i++) { setTimeout(() => i, 0); }");
    let loop_vars = collect_loop_vars(&parser.arena, initializer_idx);
    let info = analyze_loop_capture(&parser.arena, body_idx, &loop_vars);

    assert!(info.needs_capture, "Expected loop capture to be required");
    assert_eq!(info.captured_vars, vec!["i".to_string()]);
}

#[test]
fn test_analyze_loop_capture_ignores_non_capture() {
    let (parser, initializer_idx, body_idx) = parse_first_loop(
        "for (let i = 0; i < 3; i++) { setTimeout(() => console.log(\"done\"), 0); }",
    );
    let loop_vars = collect_loop_vars(&parser.arena, initializer_idx);
    let info = analyze_loop_capture(&parser.arena, body_idx, &loop_vars);

    assert!(!info.needs_capture, "Expected no loop capture");
    assert!(info.captured_vars.is_empty());
}
