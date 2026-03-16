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
    if arena.get(node_idx).is_some() {
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

#[test]
fn test_find_node_at_offset_end_of_file() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, source.len() as u32);
    // At end of file, may or may not find a node
    let _ = node;
}

#[test]
fn test_find_node_at_offset_zero() {
    let source = "let x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 0);
    assert!(node.is_some(), "Should find node at offset 0");
}

#[test]
fn test_find_node_at_offset_in_string_literal() {
    let source = "const s = 'hello world';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 14);
    assert!(node.is_some(), "Should find node inside string literal");
}

#[test]
fn test_find_node_at_offset_in_number() {
    let source = "const n = 12345;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 12);
    assert!(node.is_some(), "Should find node in number literal");
}

#[test]
fn test_find_node_at_offset_multiline_return() {
    let source = "function foo() {\n  return 42;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // "return" keyword at offset 19
    let node = find_node_at_offset(arena, 19);
    assert!(node.is_some(), "Should find node on second line");
}

#[test]
fn test_find_node_at_offset_beyond_source() {
    let source = "x";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 100);
    // Beyond source should return None or last node
    let _ = node;
}

#[test]
fn test_is_require_identifier_non_require() {
    let source = "const notRequire = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let ident_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::Identifier as u16 {
            let text = source.get(node.pos as usize..node.end as usize)?;
            if text == "notRequire" {
                return Some(tsz_parser::NodeIndex(idx as u32));
            }
        }
        None
    });

    if let Some(node_idx) = ident_node {
        assert!(
            !is_require_identifier(arena, node_idx),
            "'notRequire' should not be detected as require"
        );
    }
}

#[test]
fn test_find_node_at_offset_in_template_literal() {
    let source = "const s = `hello ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 19);
    assert!(
        node.is_some(),
        "Should find node inside template expression"
    );
}

// =========================================================================
// Additional coverage tests
// =========================================================================

#[test]
fn test_find_node_at_offset_in_function_body() {
    let source = "function add(a: number, b: number) { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 44 should be inside 'a + b'
    let node = find_node_at_offset(arena, 44);
    assert!(node.is_some(), "Should find node in function body");
}

#[test]
fn test_find_node_at_offset_at_arrow_function() {
    let source = "const f = (x: number) => x * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let node = find_node_at_offset(arena, 25);
    assert!(node.is_some(), "Should find node in arrow function body");
}

#[test]
fn test_find_nodes_in_range_single_line() {
    let source = "const x = 1; const y = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();
    let nodes = find_nodes_in_range(arena, 0, 12);
    assert!(!nodes.is_empty(), "Should find nodes in first statement");
}

#[test]
fn test_find_nodes_in_range_middle_of_file() {
    let source = "const a = 1;\nconst b = 2;\nconst c = 3;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();
    // Range covering second line only
    let nodes = find_nodes_in_range(arena, 13, 25);
    assert!(!nodes.is_empty(), "Should find nodes in middle of file");
}

#[test]
fn test_identifier_text_for_function_name() {
    let source = "function myFunc() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 9);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("myFunc".to_string()));
}

#[test]
fn test_identifier_text_for_class_name() {
    let source = "class MyClass {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 6);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("MyClass".to_string()));
}

#[test]
fn test_is_comment_context_nested_block_comments() {
    let source = "code /* comment1 */ more /* comment2 */ end";
    assert!(!is_comment_context(source, 0), "Before first comment");
    assert!(is_comment_context(source, 8), "Inside first block comment");
    assert!(!is_comment_context(source, 20), "Between comments");
    assert!(
        is_comment_context(source, 30),
        "Inside second block comment"
    );
    assert!(!is_comment_context(source, 40), "After second comment");
}

#[test]
fn test_is_comment_context_line_comment_on_second_line() {
    let source = "const x = 1;\n// comment here";
    assert!(
        !is_comment_context(source, 6),
        "Not in comment on first line"
    );
    // Comment detection depends on implementation
    let _ = is_comment_context(source, 16);
}

#[test]
fn test_should_backtrack_between_two_identifiers_with_space() {
    let source = "foo bar";
    // Offset 4 is at 'b' in 'bar', prev is space
    assert!(
        !should_backtrack_to_previous_symbol(source, 4),
        "Should not backtrack when at start of new identifier"
    );
}

#[test]
fn test_should_backtrack_after_close_paren() {
    let source = "foo()";
    // Offset 5 is at EOF after ')'
    // Backtrack behavior at close paren is implementation-defined
    let _ = should_backtrack_to_previous_symbol(source, 5);
}

#[test]
fn test_node_range_for_multiline_function() {
    let source = "function foo(\n  a: number,\n  b: string\n) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    // Find 'a' parameter on second line
    let node_idx = find_node_at_offset(arena, 16);
    if node_idx.is_some() {
        let range = node_range(arena, &line_map, source, node_idx);
        assert_eq!(range.start.line, 1, "Parameter should be on line 1");
    }
}

#[test]
fn test_calculate_new_relative_path_to_same_file() {
    let result = calculate_new_relative_path(
        Path::new("/project/main.ts"),
        Path::new("/project/utils.ts"),
        Path::new("/project/utils.ts"),
        "./utils",
    );
    assert_eq!(result, Some("./utils.ts".to_string()));
}

#[test]
fn test_calculate_new_relative_path_across_directories() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/app/main.ts"),
        Path::new("/project/src/lib/utils.ts"),
        Path::new("/project/dist/lib/utils.ts"),
        "../lib/utils",
    );
    assert!(result.is_some(), "Should handle cross-directory moves");
}

// =========================================================================
// Batch 3: additional coverage tests
// =========================================================================

#[test]
fn test_find_node_at_offset_in_class_method() {
    let source = "class Foo { bar() { return 42; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 12 should be at 'bar'
    let node = find_node_at_offset(arena, 12);
    assert!(node.is_some(), "Should find node in class method name");
}

#[test]
fn test_find_node_at_offset_in_type_annotation() {
    let source = "const x: number = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 9 should be at 'number'
    let node = find_node_at_offset(arena, 9);
    assert!(node.is_some(), "Should find node in type annotation");
}

#[test]
fn test_find_node_at_offset_in_interface() {
    let source = "interface Foo { bar: string; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 10 should be near 'Foo'
    let node = find_node_at_offset(arena, 10);
    assert!(node.is_some(), "Should find node in interface");
}

#[test]
fn test_find_node_at_offset_in_enum() {
    let source = "enum Color { Red, Green, Blue }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 13 should be at 'Red'
    let node = find_node_at_offset(arena, 13);
    assert!(node.is_some(), "Should find node in enum member");
}

#[test]
fn test_find_nodes_in_range_multiline() {
    let source = "const a = 1;\nconst b = 2;\nconst c = 3;\nconst d = 4;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();
    // Range covering lines 2-3
    let nodes = find_nodes_in_range(arena, 13, 38);
    assert!(!nodes.is_empty(), "Should find nodes across multiple lines");
}

#[test]
fn test_find_nodes_in_range_zero_width() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();
    // Zero-width range at offset 6
    let nodes = find_nodes_in_range(arena, 6, 6);
    // May or may not find nodes depending on implementation
    let _ = nodes;
}

#[test]
fn test_node_range_for_string_literal() {
    let source = "const s = 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let node_idx = find_node_at_offset(arena, 11);
    if node_idx.is_some() {
        let range = node_range(arena, &line_map, source, node_idx);
        assert_eq!(range.start.line, 0);
        assert!(range.end.character > range.start.character);
    }
}

#[test]
fn test_identifier_text_for_interface_name() {
    let source = "interface MyInterface {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 10);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("MyInterface".to_string()));
}

#[test]
fn test_identifier_text_for_enum_name() {
    let source = "enum Direction { Up, Down }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 5);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("Direction".to_string()));
}

#[test]
fn test_is_symbol_query_node_for_string_literal() {
    let source = "const x = 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let str_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::StringLiteral as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(node_idx) = str_node {
        // String literals may or may not be symbol query nodes
        let _ = is_symbol_query_node(arena, node_idx);
    }
}

#[test]
fn test_is_comment_context_string_not_comment() {
    let source = "const s = '// not a comment';";
    // Offset 14 is inside the string literal, not a real comment
    assert!(
        !is_comment_context(source, 14),
        "String content should not be detected as comment context"
    );
}

#[test]
fn test_is_comment_context_at_comment_start() {
    let source = "/* start */";
    assert!(
        is_comment_context(source, 0),
        "Should detect at start of block comment"
    );
}

#[test]
fn test_should_backtrack_after_number() {
    let source = "42 ";
    // Offset 2 is at the space after '42'
    // The prev char is a digit, which is alphanumeric
    assert!(
        should_backtrack_to_previous_symbol(source, 2),
        "Should backtrack after numeric literal"
    );
}

#[test]
fn test_should_backtrack_at_end_of_keyword() {
    let source = "return ";
    assert!(
        should_backtrack_to_previous_symbol(source, 6),
        "Should backtrack after keyword 'return'"
    );
}

#[test]
fn test_find_symbol_query_node_at_or_before_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let result = find_symbol_query_node_at_or_before(arena, source, 0);
    // Should not panic on empty file
    let _ = result;
}

#[test]
fn test_find_symbol_query_node_at_or_before_in_function_call() {
    let source = "console.log('hello');";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // After 'console' at offset 7 (at the dot)
    let result = find_symbol_query_node_at_or_before(arena, source, 7);
    if let Some(node_idx) = result {
        assert!(is_symbol_query_node(arena, node_idx));
    }
}

#[test]
fn test_calculate_new_relative_path_sibling_directory() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/main.ts"),
        Path::new("/project/src/utils/helper.ts"),
        Path::new("/project/src/lib/helper.ts"),
        "./utils/helper",
    );
    assert!(result.is_some(), "Should handle sibling directory move");
    let path = result.unwrap();
    assert!(
        path.contains("lib"),
        "Should reference new directory, got: {path}"
    );
}

#[test]
fn test_calculate_new_relative_path_up_two_levels() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/a/b/c/main.ts"),
        Path::new("/project/src/utils.ts"),
        Path::new("/project/lib/utils.ts"),
        "../../../utils",
    );
    assert!(result.is_some(), "Should handle going up multiple levels");
}

#[test]
fn test_is_import_keyword_for_non_import() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Find the 'const' keyword
    let const_node = arena.nodes.iter().enumerate().find_map(|(idx, node)| {
        if node.kind == SyntaxKind::ConstKeyword as u16 {
            Some(tsz_parser::NodeIndex(idx as u32))
        } else {
            None
        }
    });

    if let Some(node_idx) = const_node {
        assert!(
            !is_import_keyword(arena, node_idx),
            "'const' should not be detected as import keyword"
        );
    }
}

// =========================================================================
// Batch 4: additional edge-case and coverage tests
// =========================================================================

#[test]
fn test_find_node_at_offset_in_generic_type() {
    let source = "const x: Array<number> = [];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 9 should be at 'Array'
    let node = find_node_at_offset(arena, 9);
    assert!(node.is_some(), "Should find node in generic type");
}

#[test]
fn test_find_node_at_offset_in_async_function() {
    let source = "async function fetchData() { return await fetch('/api'); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 15 should be at 'fetchData'
    let node = find_node_at_offset(arena, 15);
    assert!(node.is_some(), "Should find node in async function name");
}

#[test]
fn test_find_node_at_offset_in_decorator() {
    let source = "@Component\nclass MyComponent {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 1 should be at 'Component' (after @)
    let node = find_node_at_offset(arena, 1);
    assert!(node.is_some(), "Should find node in decorator");
}

#[test]
fn test_find_node_at_offset_in_conditional_type() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 5 should be at 'IsString'
    let node = find_node_at_offset(arena, 5);
    assert!(node.is_some(), "Should find node in conditional type");
}

#[test]
fn test_find_node_at_offset_in_mapped_type() {
    let source = "type Readonly<T> = { readonly [K in keyof T]: T[K] };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 5 should be at 'Readonly'
    let node = find_node_at_offset(arena, 5);
    assert!(node.is_some(), "Should find node in mapped type");
}

#[test]
fn test_find_node_at_offset_in_rest_params() {
    let source = "function sum(...args: number[]) { return args.reduce((a, b) => a + b); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 17 should be near 'args'
    let node = find_node_at_offset(arena, 17);
    assert!(node.is_some(), "Should find node in rest params");
}

#[test]
fn test_find_node_at_offset_in_optional_param() {
    let source = "function greet(name?: string) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    // Offset 15 should be at 'name'
    let node = find_node_at_offset(arena, 15);
    assert!(node.is_some(), "Should find node in optional parameter");
}

#[test]
fn test_identifier_text_for_type_alias_name() {
    let source = "type UserID = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 5);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("UserID".to_string()));
}

#[test]
fn test_identifier_text_for_arrow_function_param() {
    let source = "const f = (value: number) => value * 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    let node_idx = find_node_at_offset(arena, 11);
    assert!(node_idx.is_some());
    let text = identifier_text(arena, node_idx);
    assert_eq!(text, Some("value".to_string()));
}

#[test]
fn test_is_comment_context_double_slash_in_string() {
    let source = "const url = 'http://example.com';";
    // The simple comment detector may or may not handle string interiors
    // Just verify it doesn't panic
    let _ = is_comment_context(source, 19);
}

#[test]
fn test_is_comment_context_slash_star_in_string() {
    let source = "const s = '/* not a comment */';";
    // The simple comment detector may or may not handle string interiors
    // Just verify it doesn't panic
    let _ = is_comment_context(source, 14);
}

#[test]
fn test_should_backtrack_after_closing_bracket() {
    let source = "arr[0] ";
    // Offset 6 is at the space after ']'
    let _ = should_backtrack_to_previous_symbol(source, 6);
}

#[test]
fn test_should_backtrack_after_closing_brace() {
    let source = "const obj = {} ";
    // Offset 14 is at the space after '}'
    let _ = should_backtrack_to_previous_symbol(source, 14);
}

#[test]
fn test_find_nodes_in_range_in_class_body() {
    let source = "class Foo {\n  x = 1;\n  y = 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _ = parser.parse_source_file();
    let arena = parser.get_arena();
    // Range covering the class body
    let nodes = find_nodes_in_range(arena, 12, 29);
    assert!(!nodes.is_empty(), "Should find nodes in class body");
}

#[test]
fn test_node_range_for_multiline_class() {
    let source = "class MyClass {\n  method() {\n    return 1;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    // Find 'method' on line 1
    let node_idx = find_node_at_offset(arena, 18);
    if node_idx.is_some() {
        let range = node_range(arena, &line_map, source, node_idx);
        assert_eq!(range.start.line, 1, "method should be on line 1");
    }
}

#[test]
fn test_calculate_new_relative_path_index_file() {
    let result = calculate_new_relative_path(
        Path::new("/project/src/main.ts"),
        Path::new("/project/src/utils/index.ts"),
        Path::new("/project/src/helpers/index.ts"),
        "./utils/index",
    );
    assert!(result.is_some(), "Should handle index.ts file moves");
    let path = result.unwrap();
    assert!(
        path.contains("helpers"),
        "Should reference new directory, got: {path}"
    );
}

#[test]
fn test_find_node_at_or_before_offset_in_multiline() {
    let source = "const a = 1;\n\nconst b = 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Offset 13 is the blank line between statements
    let node = find_node_at_or_before_offset(arena, 13, source);
    // Should find something (backtrack to first statement)
    let _ = node;
}
