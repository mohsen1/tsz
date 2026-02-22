use super::*;
use tsz_parser::ParserState;
use tsz_scanner::SyntaxKind;

#[test]
fn test_find_node_at_offset_simple() {
    // const x = 1;
    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 6 should be at 'x'
    let node = find_node_at_offset(arena, 6);
    assert!(node.is_some(), "Should find a node at offset 6");

    // Check that we got the identifier, not a larger container
    if let Some(n) = arena.get(node) {
        assert!(
            n.end - n.pos < 10,
            "Should find a small node (identifier), not the whole statement"
        );
    }
}

#[test]
fn test_find_node_at_offset_none() {
    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset beyond the file
    let node = find_node_at_offset(arena, 1000);
    assert!(node.is_none(), "Should return NONE for offset beyond file");
}

#[test]
fn test_find_nodes_in_range() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const x = 1;\nlet y = 2;".to_string(),
    );
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find nodes in the first line
    let nodes = find_nodes_in_range(arena, 0, 12);
    assert!(!nodes.is_empty(), "Should find nodes in first line");
}

#[test]
fn test_is_symbol_query_node_for_module_namespace_string_literal() {
    let source = "const foo = \"foo\";\nexport { foo as \"__alias\" };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let target = arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != SyntaxKind::StringLiteral as u16 {
                return None;
            }
            let text = source.get(node.pos as usize..node.end as usize)?;
            (text == "\"__alias\"").then_some(tsz_parser::NodeIndex(idx as u32))
        })
        .expect("expected string-literal export alias node");

    assert!(
        is_symbol_query_node(arena, target),
        "module namespace string literal alias should be a symbol-query node"
    );
}
