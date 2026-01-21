//! Utility functions for LSP operations.
//!
//! Provides efficient node lookup using the flat NodeArena structure.

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;

/// Find the most specific node containing the given byte offset.
///
/// This performs a linear scan over the arena, which is extremely cache-efficient
/// because Nodes are 16 bytes each and stored contiguously. This is much faster
/// than pointer-chasing a traditional tree structure.
///
/// Returns the smallest (most specific) node that contains the offset.
/// For example, if offset points to an identifier inside a call expression,
/// this returns the identifier node, not the call expression.
///
/// Returns `NodeIndex::NONE` if no node contains the offset.
pub fn find_node_at_offset(arena: &NodeArena, offset: u32) -> NodeIndex {
    let mut best_match = NodeIndex::NONE;
    let mut min_len = u32::MAX;

    // Iterate all nodes to find the tightest fit
    for (i, node) in arena.nodes.iter().enumerate() {
        if node.pos <= offset && node.end > offset {
            let len = node.end - node.pos;
            // We want the smallest node that contains the offset
            if len < min_len {
                min_len = len;
                best_match = NodeIndex(i as u32);
            }
        }
    }

    best_match
}

/// Find the nearest node at or before an offset, skipping whitespace and
/// optional chaining/member access punctuation when no node is found.
pub fn find_node_at_or_before_offset(arena: &NodeArena, offset: u32, source: &str) -> NodeIndex {
    let node = find_node_at_offset(arena, offset);
    if node.is_some() {
        return node;
    }

    let max_len = source.len() as u32;
    let mut idx = offset.min(max_len);
    if idx == 0 {
        return NodeIndex::NONE;
    }

    let bytes = source.as_bytes();
    while idx > 0 {
        let prev = bytes[(idx - 1) as usize];
        if prev.is_ascii_whitespace() {
            idx -= 1;
            continue;
        }
        if prev == b'.' {
            idx -= 1;
            continue;
        }
        if prev == b'?'
            && bytes.get(idx as usize) == Some(&b'.') {
                idx -= 1;
                continue;
            }
        break;
    }

    if idx == 0 {
        return NodeIndex::NONE;
    }

    find_node_at_offset(arena, idx - 1)
}

/// Find all nodes that overlap with a given range.
///
/// Returns nodes where [node.pos, node.end) overlaps with [start, end).
pub fn find_nodes_in_range(arena: &NodeArena, start: u32, end: u32) -> Vec<NodeIndex> {
    let mut result = Vec::new();

    for (i, node) in arena.nodes.iter().enumerate() {
        // Check if ranges overlap
        if node.pos < end && node.end > start {
            result.push(NodeIndex(i as u32));
        }
    }

    result
}

#[cfg(test)]
mod utils_tests {
    use super::*;
    use crate::parser::ParserState;

    #[test]
    fn test_find_node_at_offset_simple() {
        // const x = 1;
        let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();

        // Offset 6 should be at 'x'
        let node = find_node_at_offset(arena, 6);
        assert!(!node.is_none(), "Should find a node at offset 6");

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
}
