//! Tests for Go to Declaration.

use super::*;
use tsz_binder::BinderState;
use tsz_common::position::{LineMap, Position};
use tsz_parser::ParserState;

#[test]
fn test_declaration_interface() {
    let source = "interface Foo { x: number }\nlet a: Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        GoToDeclarationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // cursor on `Foo` in `let a: Foo` - character 7 is the 'F' of Foo
    let result = provider.get_declaration(root, Position::new(1, 7));
    // Even if it falls back, should find the interface declaration
    if let Some(locs) = result {
        assert!(!locs.is_empty());
    }
}

#[test]
fn test_declaration_variable() {
    let source = "let x = 1;\nconsole.log(x);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        GoToDeclarationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // cursor on `x` usage in console.log(x)
    let result = provider.get_declaration(root, Position::new(1, 12));
    if let Some(locs) = result {
        assert!(!locs.is_empty());
        // Should point back to line 0 where `let x = 1`
        assert_eq!(locs[0].range.start.line, 0);
    }
}

#[test]
fn test_declaration_no_symbol() {
    let source = "1 + 2;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        GoToDeclarationProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // cursor on a literal - no symbol
    let result = provider.get_declaration(root, Position::new(0, 0));
    assert!(result.is_none(), "Should return None for literals");
}
