//! Tests for parser module.

use super::*;
use crate::scanner::SyntaxKind;

#[test]
fn test_node_flags() {
    assert_eq!(node_flags::NONE, 0);
    assert_eq!(node_flags::LET, 1);
    assert_eq!(node_flags::CONST, 2);
    assert_eq!(node_flags::AWAIT_USING, 6); // Const | Using
}

#[test]
fn test_modifier_flags() {
    assert_eq!(modifier_flags::NONE, 0);
    assert_eq!(modifier_flags::PUBLIC, 1);
    assert_eq!(modifier_flags::EXPORT, 32);
    assert_eq!(modifier_flags::ASYNC, 1024);
}

#[test]
fn test_node_index() {
    let index = NodeIndex(0);
    assert!(index.is_some());
    assert!(!index.is_none());

    let none = NodeIndex::NONE;
    assert!(none.is_none());
    assert!(!none.is_some());
}

#[test]
fn test_node_arena() {
    let mut arena = NodeArena::new();

    let id = Identifier::new("test".to_string(), 0, 4);
    let idx = arena.add(Node::Identifier(id));

    assert_eq!(idx.0, 0);
    assert_eq!(arena.len(), 1);

    let node = arena.get(idx).unwrap();
    assert_eq!(node.kind(), SyntaxKind::Identifier as u16);
    assert_eq!(node.pos(), 0);
    assert_eq!(node.end(), 4);
}

#[test]
fn test_identifier() {
    let id = Identifier::new("myVar".to_string(), 10, 15);
    assert_eq!(id.escaped_text, "myVar");
    assert_eq!(id.base.kind, SyntaxKind::Identifier as u16);
    assert_eq!(id.base.pos, 10);
    assert_eq!(id.base.end, 15);
}

#[test]
fn test_source_file() {
    let sf = SourceFile::new("test.ts".to_string(), "const x = 1;".to_string());
    // SourceFile kind is 308 in TypeScript, stored directly in kind field
    assert_eq!(sf.base.kind as u16, syntax_kind_ext::SOURCE_FILE);
    assert_eq!(sf.file_name, "test.ts");
    assert_eq!(sf.text.len(), 12);
}
