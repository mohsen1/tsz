//! Utility functions for LSP operations.
//!
//! Provides efficient node lookup using the flat `NodeArena` structure.

use std::path::{Path, PathBuf};

use tsz_common::position::{LineMap, Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

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

/// Convert a node to an LSP Range using the line map.
///
/// Returns a zero-width range at (0,0) if the node index is invalid.
pub fn node_range(
    arena: &NodeArena,
    line_map: &LineMap,
    source_text: &str,
    node_idx: NodeIndex,
) -> Range {
    if let Some(node) = arena.get(node_idx) {
        let start = line_map.offset_to_position(node.pos, source_text);
        let end = line_map.offset_to_position(node.end, source_text);
        Range::new(start, end)
    } else {
        Range::new(Position::new(0, 0), Position::new(0, 0))
    }
}

/// Get the text of an identifier node, or `None` if the node is not an identifier.
pub fn identifier_text(arena: &NodeArena, node_idx: NodeIndex) -> Option<String> {
    arena.identifier_text_owned(node_idx)
}

/// Check whether a node is a valid symbol-query target for LSP symbol-resolution flows.
/// This includes identifiers and keyword tokens (used for declaration keyword fallbacks).
pub fn is_symbol_query_node(arena: &NodeArena, node: NodeIndex) -> bool {
    let Some(node_data) = arena.get(node) else {
        return false;
    };

    if node_data.kind == SyntaxKind::Identifier as u16
        || node_data.kind == SyntaxKind::PrivateIdentifier as u16
    {
        return true;
    }

    if node_data.kind == SyntaxKind::StringLiteral as u16
        && let Some(ext) = arena.get_extended(node)
        && ext.parent.is_some()
        && let Some(parent_node) = arena.get(ext.parent)
        && (parent_node.kind == tsz_parser::syntax_kind_ext::IMPORT_SPECIFIER
            || parent_node.kind == tsz_parser::syntax_kind_ext::EXPORT_SPECIFIER)
    {
        return true;
    }

    // Include tagged template span nodes so references can fall back to the tag symbol.
    if node_data.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        || node_data.kind == SyntaxKind::TemplateHead as u16
        || node_data.kind == SyntaxKind::TemplateMiddle as u16
        || node_data.kind == SyntaxKind::TemplateTail as u16
    {
        return true;
    }

    node_data.kind >= SyntaxKind::BreakKeyword as u16
        && node_data.kind <= SyntaxKind::DeferKeyword as u16
}

/// Search backward from `offset` (up to 256 chars or newline) for the nearest
/// symbol-query node.  Returns `None` if no identifier/keyword is found.
pub fn find_symbol_query_node_at_or_before(
    arena: &NodeArena,
    source_text: &str,
    offset: u32,
) -> Option<NodeIndex> {
    let mut probe = offset.min(source_text.len() as u32);
    let bytes = source_text.as_bytes();
    let mut remaining = 256u32;

    while probe > 0 && remaining > 0 {
        probe -= 1;
        remaining -= 1;

        let candidate = find_node_at_or_before_offset(arena, probe, source_text);
        if candidate.is_some() && is_symbol_query_node(arena, candidate) {
            return Some(candidate);
        }

        let ch = bytes[probe as usize];
        if ch == b'\n' || ch == b'\r' {
            break;
        }
    }

    None
}

/// Heuristic: is the cursor sitting inside (or immediately adjacent to) a comment?
pub fn is_comment_context(source_text: &str, offset: u32) -> bool {
    let bytes = source_text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let idx = (offset as usize).min(bytes.len());

    if idx > 0 {
        let prev = bytes[idx - 1];
        if prev == b'/' || prev == b'*' {
            return true;
        }
    }
    if idx < bytes.len() {
        let current = bytes[idx];
        if current == b'/' || current == b'*' {
            return true;
        }
    }

    let prefix = &source_text[..idx];
    if let Some(start) = prefix.rfind("/*")
        && prefix[start + 2..].rfind("*/").is_none()
    {
        return true;
    }

    false
}

/// Heuristic: the cursor is at the end of an identifier token (i.e. previous
/// char is word-like, current char is not), so the user likely wants the
/// symbol immediately before the cursor.
pub fn should_backtrack_to_previous_symbol(source_text: &str, offset: u32) -> bool {
    let bytes = source_text.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let idx = (offset as usize).min(bytes.len());
    if idx == 0 {
        return false;
    }

    let prev = bytes[idx - 1];
    if !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$') {
        return false;
    }

    if idx >= bytes.len() {
        return true;
    }

    let current = bytes[idx];
    !(current.is_ascii_alphanumeric() || current == b'_' || current == b'$')
}

/// Check if a node is the `import` keyword (for dynamic import expressions).
pub fn is_import_keyword(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    if node_idx.is_none() {
        return false;
    }
    let Some(node) = arena.get(node_idx) else {
        return false;
    };
    node.kind == SyntaxKind::ImportKeyword as u16
}

/// Check if a node is a `require` identifier.
pub fn is_require_identifier(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    if node_idx.is_none() {
        return false;
    }
    let Some(node) = arena.get(node_idx) else {
        return false;
    };
    if node.kind != SyntaxKind::Identifier as u16 {
        return false;
    }
    let Some(ident_data) = arena.get_identifier(node) else {
        return false;
    };
    ident_data.escaped_text == "require"
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
/// * `Some(String)` - The new import specifier in the same style as `current_specifier`
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
        result = format!("./{result}");
    } else if !has_dot_slash_prefix && !has_parent_reference && result.starts_with("./") {
        // Remove ./ if user didn't use it originally
        result = result[2..].to_string();
    }

    Some(result)
}

/// Calculate a relative path from `from` to `to`.
///
/// This is a simplified version of pathdiff that uses `std::path` only.
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
#[path = "../../tests/utils_tests.rs"]
mod utils_tests;
