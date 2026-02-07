//! Utility functions for LSP operations.
//!
//! Provides efficient node lookup using the flat NodeArena structure.

use std::path::{Path, PathBuf};

use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

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
        if prev == b'?' && bytes.get(idx as usize) == Some(&b'.') {
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

/// Calculate the new relative path for an import statement after a file rename.
///
/// # Arguments
/// * `importer_path` - Path of the file containing the import statement
/// * `_old_target_path` - Original path of the imported file (before rename) - unused for now
/// * `new_target_path` - New path of the imported file (after rename)
/// * `current_specifier` - The current import specifier (e.g., "./utils" or "../types")
///
/// # Returns
/// * `Some(String)` - The new import specifier in the same style as current_specifier
/// * `None` - If the calculation fails
///
/// # Examples
/// ```ignore
/// // When utils.ts moves to src/utils.ts
/// calculate_new_relative_path(
///     Path::new("/project/main.ts"),
///     Path::new("/project/utils.ts"),
///     Path::new("/project/src/utils.ts"),
///     "./utils"
/// ) // Returns: Some("./src/utils")
/// ```
pub fn calculate_new_relative_path(
    importer_path: &Path,
    _old_target_path: &Path,
    new_target_path: &Path,
    current_specifier: &str,
) -> Option<String> {
    // Parse the current specifier to understand the user's style
    let has_dot_slash_prefix = current_specifier.starts_with("./");
    let has_parent_reference = current_specifier.starts_with("../");

    // Get the directory containing the importer
    let importer_dir = importer_path.parent()?;

    // Calculate relative path from importer_dir to new_target
    // Use Path::strip_prefix to find common ancestor
    let new_relative = relative_path(importer_dir, new_target_path)?;

    // Convert to string and apply the same style as current_specifier
    let mut result = new_relative.to_string_lossy().to_string();

    // Apply user's prefix style
    if has_dot_slash_prefix && !result.starts_with("./") && !result.starts_with("../") {
        result = format!("./{}", result);
    } else if !has_dot_slash_prefix && !has_parent_reference && result.starts_with("./") {
        // Remove ./ if user didn't use it originally
        result = result[2..].to_string();
    }

    Some(result)
}

/// Calculate a relative path from `from` to `to`.
///
/// This is a simplified version of pathdiff that uses std::path only.
fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    use std::path::PathBuf;

    // Try to find a common ancestor
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();

    // Remove filename from 'from' (it's a directory path)
    if !from_components.is_empty() {
        // from_components is already a directory (from parent()), so keep as is
    }

    // Find common prefix
    let mut common = 0;
    while common < from_components.len().min(to_components.len())
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    // Build "../" for each remaining component in 'from'
    let mut result = PathBuf::new();
    for _ in 0..(from_components.len() - common) {
        result.push("..");
    }

    // Add remaining components from 'to'
    for &component in &to_components[common..] {
        result.push(component);
    }

    // Special case: same directory
    if result.as_os_str().is_empty() {
        result.push(".");
    }

    Some(result)
}

#[cfg(test)]
mod utils_tests {
    use super::*;
    use tsz_parser::ParserState;

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
