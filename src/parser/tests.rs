```rust
// src/parser/tests.rs

//! Integration tests for the parser.

use crate::parser::flags::{NodeKind, NodeRef, ThinNode};

// Mock Arena structure for testing context
struct ParseArena {
    storage: Vec<String>,
}

impl ParseArena {
    fn new() -> Self {
        Self { storage: Vec::new() }
    }

    // Helper to simulate allocating a node and returning a ThinNode
    fn allocate_node(&mut self, data: &str, kind: NodeKind) -> ThinNode {
        let index = self.storage.len() as u32;
        self.storage.push(data.to_string());
        
        ThinNode::new(kind, NodeRef::new(index, 0))
    }
}

#[test]
fn test_parser_basic_structure() {
    let mut arena = ParseArena::new();
    
    // Simulate parsing a simple text node
    let text_node = arena.allocate_node("Hello World", NodeKind::Text);

    // Assert on the internal structure (offset) rather than just the value
    assert_eq!(text_node.kind, NodeKind::Text);
    assert_eq!(text_node.get_arena_offset(), 0);
    
    // Verify generation logic
    assert_eq!(text_node.generation(), 0);
}

#[test]
fn test_parser_multiple_nodes() {
    let mut arena = ParseArena::new();

    let root = arena.allocate_node("root", NodeKind::Root);
    let child1 = arena.allocate_node("child1", NodeKind::Element);
    let child2 = arena.allocate_node("child2", NodeKind::Element);

    // Verify offsets are sequential and correct based on Arena insertion order
    assert_eq!(root.get_arena_offset(), 0);
    assert_eq!(child1.get_arena_offset(), 1);
    assert_eq!(child2.get_arena_offset(), 2);
}

#[test]
fn test_parser_node_flags() {
    let node_ref = NodeRef::new(5, 2);
    let flagged_node = ThinNode::new(NodeKind::NodeFlag, node_ref);

    assert_eq!(flagged_node.kind, NodeKind::NodeFlag);
    // Ensure the offset is correctly wrapped in the ThinNode
    assert_eq!(flagged_node.get_arena_offset(), 5);
    assert_eq!(flagged_node.generation(), 2);
}
```
