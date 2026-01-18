```rust
// src/parser/thin_node_tests.rs

//! Unit tests for ThinNode internals and offset management.

use crate::parser::flags::{NodeKind, NodeRef, ThinNode};

#[test]
fn test_thin_node_creation() {
    let kind = NodeKind::Text;
    let node_ref = NodeRef::new(10, 1);
    let thin_node = ThinNode::new(kind, node_ref);

    assert_eq!(thin_node.kind, NodeKind::Text);
    // Verify internal offset rather than just equality
    assert_eq!(thin_node.get_arena_offset(), 10);
    assert_eq!(thin_node.generation(), 1);
}

#[test]
fn test_thin_node_offset_calculation() {
    // Test that offset is correctly extracted from u32 index
    let offsets = vec![0u32, 1, 100, 9999];
    
    for offset in offsets {
        let node_ref = NodeRef::new(offset, 0);
        let thin_node = ThinNode::new(NodeKind::Element, node_ref);
        assert_eq!(thin_node.get_arena_offset(), offset as usize);
    }
}

#[test]
fn test_thin_node_copy_behavior() {
    // ThinNode is Copy, ensure offsets remain consistent after copy
    let original = ThinNode::new(NodeKind::Comment, NodeRef::new(42, 5));
    let copied = original;

    // Asserting on the internals to ensure the copy didn't corrupt data
    assert_eq!(copied.get_arena_offset(), 42);
    assert_eq!(original.get_arena_offset(), 42); // Original should also be valid
    assert_eq!(copied.generation(), 5);
}
```
