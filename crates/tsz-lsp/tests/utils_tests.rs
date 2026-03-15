use super::*;
use std::path::Path;
use tsz_common::position::LineMap;
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

// ---- find_node_at_or_before_offset tests ----

#[test]
fn test_find_node_at_or_before_offset_exact_hit() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 6 is inside 'x', should return the same as find_node_at_offset
    let node = find_node_at_or_before_offset(arena, 6, source);
    assert!(node.is_some(), "Should find node at exact position");
}

#[test]
fn test_find_node_at_or_before_offset_after_whitespace() {
    // After the semicolon there's nothing, but just past the identifier with trailing space
    let source = "const x = 1;  ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 14 is in trailing whitespace, should backtrack to find the last node
    let node = find_node_at_or_before_offset(arena, 14, source);
    assert!(
        node.is_some(),
        "Should backtrack through whitespace to find a node"
    );
}

#[test]
fn test_find_node_at_or_before_offset_after_dot() {
    let source = "foo.";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 4 is right after the dot, should backtrack past the dot
    let node = find_node_at_or_before_offset(arena, 4, source);
    assert!(node.is_some(), "Should backtrack past dot to find 'foo'");
}

#[test]
fn test_find_node_at_or_before_offset_optional_chaining() {
    let source = "foo?.";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 5 is right after '?.', should backtrack past '?.'
    let node = find_node_at_or_before_offset(arena, 5, source);
    assert!(
        node.is_some(),
        "Should backtrack past optional chaining to find 'foo'"
    );
}

#[test]
fn test_find_node_at_or_before_offset_at_zero() {
    let source = "  ";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 0 in an empty/whitespace-only file should not panic
    let node = find_node_at_or_before_offset(arena, 0, source);
    // The function should handle this gracefully (returning NONE or a source file node)
    let _ = node;
}

// ---- node_range tests ----

#[test]
fn test_node_range_valid_node() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let node_idx = find_node_at_offset(arena, 6); // 'x'
    assert!(node_idx.is_some());
    let range = node_range(arena, &line_map, source, node_idx);
    assert_eq!(range.start.line, 0);
    assert_eq!(range.end.line, 0);
    // The range should be non-empty
    assert!(
        range.end.character > range.start.character,
        "Range should be non-empty for a valid identifier"
    );
}

#[test]
fn test_node_range_invalid_node() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let range = node_range(arena, &line_map, source, tsz_parser::NodeIndex::NONE);
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 0);
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 0);
}

// ---- identifier_text tests ----

#[test]
fn test_identifier_text_for_identifier_node() {
    let source = "const myVar = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the identifier node for 'myVar' at offset 6
    let node_idx = find_node_at_offset(arena, 6);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("myVar".to_string()));
}

#[test]
fn test_identifier_text_for_none_node() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let text = identifier_text(arena, tsz_parser::NodeIndex::NONE);
    assert_eq!(text, None);
}

#[test]
fn test_identifier_text_for_non_identifier_node() {
    let source = "const x = 123;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the numeric literal node at offset 10 ('123')
    let node_idx = find_node_at_offset(arena, 10);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, None, "Non-identifier nodes should return None");
}

// ---- is_symbol_query_node tests (keyword, identifier, template) ----

#[test]
fn test_is_symbol_query_node_for_identifier() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the identifier 'x' at offset 6
    let node_idx = find_node_at_offset(arena, 6);
    assert!(node_idx.is_some());
    assert!(
        is_symbol_query_node(arena, node_idx),
        "Identifiers should be symbol query nodes"
    );
}

#[test]
fn test_is_symbol_query_node_for_keyword() {
    let source = "break;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the 'break' keyword node
    let break_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::BreakKeyword as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(node_idx) = break_node {
        assert!(
            is_symbol_query_node(arena, node_idx),
            "Keyword nodes should be symbol query nodes"
        );
    }
}

#[test]
fn test_is_symbol_query_node_for_none() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    assert!(
        !is_symbol_query_node(arena, tsz_parser::NodeIndex::NONE),
        "NONE node should not be a symbol query node"
    );
}

#[test]
fn test_is_symbol_query_node_for_template_literal() {
    let source = "const x = `hello`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let template_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(node_idx) = template_node {
        assert!(
            is_symbol_query_node(arena, node_idx),
            "Template literal nodes should be symbol query nodes"
        );
    }
}

// ---- is_comment_context tests ----

#[test]
fn test_is_comment_context_line_comment() {
    let source = "// hello";
    assert!(
        is_comment_context(source, 0),
        "Should detect at start of //"
    );
    assert!(is_comment_context(source, 1), "Should detect at second /");
}

#[test]
fn test_is_comment_context_block_comment_interior() {
    let source = "/* comment */";
    // Offset 5 is inside the block comment, between /* and */
    assert!(
        is_comment_context(source, 5),
        "Should detect inside block comment"
    );
}

#[test]
fn test_is_comment_context_not_in_comment() {
    let source = "const x = 1;";
    assert!(
        !is_comment_context(source, 6),
        "Should not detect comment context in normal code"
    );
}

#[test]
fn test_is_comment_context_empty_source() {
    assert!(
        !is_comment_context("", 0),
        "Should return false for empty source"
    );
}

#[test]
fn test_is_comment_context_unclosed_block_comment() {
    let source = "/* unclosed comment";
    assert!(
        is_comment_context(source, 10),
        "Should detect inside unclosed block comment"
    );
}

// ---- should_backtrack_to_previous_symbol tests ----

#[test]
fn test_should_backtrack_at_end_of_identifier() {
    let source = "foo ";
    // Offset 3 is right after 'foo' (at the space)
    assert!(
        should_backtrack_to_previous_symbol(source, 3),
        "Should backtrack when cursor is at end of identifier followed by space"
    );
}

#[test]
fn test_should_backtrack_inside_identifier() {
    let source = "foobar";
    // Offset 3 is inside 'foobar' (between 'foo' and 'bar')
    assert!(
        !should_backtrack_to_previous_symbol(source, 3),
        "Should NOT backtrack when cursor is inside an identifier"
    );
}

#[test]
fn test_should_backtrack_after_punctuation() {
    let source = "foo(";
    // Offset 3 is at '(' - prev char is 'o' (alphanumeric), current is '('
    assert!(
        should_backtrack_to_previous_symbol(source, 3),
        "Should backtrack when cursor is after identifier and before punctuation"
    );
}

#[test]
fn test_should_backtrack_at_zero() {
    let source = "foo";
    assert!(
        !should_backtrack_to_previous_symbol(source, 0),
        "Should not backtrack at offset 0"
    );
}

#[test]
fn test_should_backtrack_empty_source() {
    assert!(
        !should_backtrack_to_previous_symbol("", 0),
        "Should not backtrack in empty source"
    );
}

#[test]
fn test_should_backtrack_at_end_of_file() {
    let source = "foo";
    // Offset 3 is at EOF, prev char is 'o'
    assert!(
        should_backtrack_to_previous_symbol(source, 3),
        "Should backtrack when cursor is at end of file after identifier"
    );
}

// ---- is_import_keyword / is_require_identifier tests ----

#[test]
fn test_is_import_keyword_true() {
    let source = "import('foo');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let import_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::ImportKeyword as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(node_idx) = import_node {
        assert!(is_import_keyword(arena, node_idx));
    }
}

#[test]
fn test_is_import_keyword_false_for_none() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    assert!(!is_import_keyword(arena, tsz_parser::NodeIndex::NONE));
}

#[test]
fn test_is_require_identifier_false_for_none() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    assert!(!is_require_identifier(arena, tsz_parser::NodeIndex::NONE));
}

#[test]
fn test_is_require_identifier_false_for_other_identifier() {
    let source = "const foo = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // 'foo' is an identifier but not 'require'
    let node_idx = find_node_at_offset(arena, 6);
    assert!(node_idx.is_some());
    assert!(
        !is_require_identifier(arena, node_idx),
        "'foo' should not be identified as require"
    );
}

// ---- calculate_new_relative_path tests ----

#[test]
fn test_calculate_new_relative_path_same_dir_with_dot_slash() {
    let result = calculate_new_relative_path(
        Path::new("/project/main.ts"),
        Path::new("/project/utils.ts"),
        Path::new("/project/helpers.ts"),
        "./utils",
    );
    assert_eq!(result, Some("./helpers.ts".to_string()));
}

#[test]
fn test_calculate_new_relative_path_moved_to_subdir() {
    let result = calculate_new_relative_path(
        Path::new("/project/main.ts"),
        Path::new("/project/utils.ts"),
        Path::new("/project/src/utils.ts"),
        "./utils",
    );
    assert_eq!(result, Some("./src/utils.ts".to_string()));
}

#[test]
fn test_calculate_new_relative_path_parent_reference() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/main.ts"),
        Path::new("/project/utils.ts"),
        Path::new("/project/lib/utils.ts"),
        "../utils",
    );
    assert_eq!(result, Some("../lib/utils.ts".to_string()));
}

#[test]
fn test_find_nodes_in_range_empty() {
    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();

    // Range that doesn't overlap with any nodes (way past end)
    let nodes = find_nodes_in_range(arena, 5000, 6000);
    assert!(
        nodes.is_empty(),
        "Should find no nodes in out-of-range region"
    );
}

#[test]
fn test_find_node_at_offset_multiline() {
    let source = "const a = 1;\nconst b = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 19 should be at 'b' on the second line
    let node = find_node_at_offset(arena, 19);
    assert!(
        node.is_some(),
        "Should find a node at offset 19 (second line)"
    );
}

#[test]
fn test_find_node_at_offset_at_zero() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 0 should find the first node (likely SourceFile or const keyword)
    let node = find_node_at_offset(arena, 0);
    assert!(node.is_some(), "Should find a node at offset 0");
}

#[test]
fn test_find_nodes_in_range_full_file() {
    let source = "let x = 1;\nlet y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();

    // Range covering the entire file
    let nodes = find_nodes_in_range(arena, 0, source.len() as u32);
    assert!(
        nodes.len() >= 2,
        "Should find multiple nodes covering entire file, got {}",
        nodes.len()
    );
}

#[test]
fn test_node_range_multiline() {
    let source = "const x = 1;\nconst y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    // Find 'y' on the second line (offset ~19)
    let node_idx = find_node_at_offset(arena, 19);
    if let Some(_) = arena.get(node_idx) {
        let range = node_range(arena, &line_map, source, node_idx);
        assert_eq!(range.start.line, 1, "Second line node should be on line 1");
    }
}

#[test]
fn test_is_comment_context_after_block_comment() {
    let source = "/* comment */ code";
    // Offset 14 is at 'c' in 'code', outside the block comment
    assert!(
        !is_comment_context(source, 14),
        "Should not detect comment context after closed block comment"
    );
}

#[test]
fn test_is_comment_context_jsdoc() {
    let source = "/** jsdoc comment */";
    assert!(
        is_comment_context(source, 5),
        "Should detect inside JSDoc block comment"
    );
}

#[test]
fn test_should_backtrack_after_dollar_sign() {
    let source = "foo$ ";
    // Offset 4 is at the space after 'foo$'
    assert!(
        should_backtrack_to_previous_symbol(source, 4),
        "Should backtrack after $ identifier"
    );
}

#[test]
fn test_should_backtrack_after_underscore() {
    let source = "_private ";
    // Offset 8 is at the space after '_private'
    assert!(
        should_backtrack_to_previous_symbol(source, 8),
        "Should backtrack after underscore identifier"
    );
}

#[test]
fn test_calculate_new_relative_path_deep_nesting() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/a/b/main.ts"),
        Path::new("/project/src/utils.ts"),
        Path::new("/project/lib/helpers/utils.ts"),
        "../../utils",
    );
    assert!(
        result.is_some(),
        "Should calculate path for deeply nested file"
    );
    let path = result.unwrap();
    assert!(
        path.contains("lib") && path.contains("utils"),
        "Path should reference the new location, got '{path}'"
    );
}

#[test]
fn test_calculate_new_relative_path_no_extension_style() {
    // When the specifier has no extension, result should still include the extension
    let result = calculate_new_relative_path(
        Path::new("/project/main.ts"),
        Path::new("/project/old.ts"),
        Path::new("/project/sub/new.ts"),
        "./old",
    );
    assert!(
        result.is_some(),
        "Should produce a result for no-extension specifier"
    );
}

#[test]
fn test_find_symbol_query_node_at_or_before_in_code() {
    let source = "const myVar = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Position after 'myVar' (offset 11 = after 'myVar ')
    let result = find_symbol_query_node_at_or_before(arena, source, 11);
    if let Some(node_idx) = result {
        assert!(
            is_symbol_query_node(arena, node_idx),
            "Found node should be a symbol query node"
        );
    }
}

#[test]
fn test_is_require_identifier_for_require() {
    let source = "const x = require('foo');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the 'require' identifier
    let require_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::Identifier as u16 {
            let text = source.get(node.pos as usize..node.end as usize)?;
            if text == "require" {
                return Some(tsz_parser::NodeIndex(idx as u32));
            }
        }
        None
    });

    if let Some(node_idx) = require_node {
        assert!(
            is_require_identifier(arena, node_idx),
            "'require' identifier should be detected as require"
        );
    }
}
