//! Tests for parser module.

use super::*;

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

// Legacy AST tests removed: the crate is Node-only.
