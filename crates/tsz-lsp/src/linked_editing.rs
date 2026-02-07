//! Linked Editing implementation for LSP.
//!
//! Provides linked editing ranges for JSX/TSX files.
//! When editing an opening JSX tag (e.g., `<div>`), the closing tag (`</div>`)
//! automatically syncs.

use crate::utils::find_node_at_offset;
use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};

/// Result of a linked editing request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkedEditingRanges {
    /// The ranges that should be edited together.
    pub ranges: Vec<Range>,
    /// Optional pattern for valid identifiers (e.g., for word selection).
    pub word_pattern: Option<String>,
}

define_lsp_provider!(minimal LinkedEditingProvider, "Provider for linked editing ranges.");

impl<'a> LinkedEditingProvider<'a> {
    /// Provide linked editing ranges for the given position.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST (typically SourceFile)
    /// * `position` - The LSP position (line, column) where the cursor is
    ///
    /// # Returns
    /// * `Some(LinkedEditingRanges)` if the cursor is on a JSX tag name
    /// * `None` otherwise
    pub fn provide_linked_editing_ranges(
        &self,
        _root: NodeIndex,
        position: Position,
    ) -> Option<LinkedEditingRanges> {
        // Convert LSP position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // Find the node at the cursor
        let node_idx = find_node_at_offset(self.arena, offset);

        // Check if we're on a JSX tag name (opening or closing)
        let (tag_name_idx, _is_opening) = self.find_jsx_tag_context(node_idx)?;

        // Get the parent of the tag name (this is the opening/closing element)
        let element_idx = self.arena.get_extended(tag_name_idx)?.parent;
        if element_idx.is_none() {
            return None;
        }

        // Get the parent of the opening/closing element (this must be JSX_ELEMENT)
        let jsx_element_idx = self.arena.get_extended(element_idx)?.parent;
        if jsx_element_idx.is_none() {
            return None;
        }

        // Verify it's actually a JSX_ELEMENT
        let jsx_element = self.arena.get(jsx_element_idx)?;
        if jsx_element.kind != syntax_kind_ext::JSX_ELEMENT {
            return None;
        }

        // Get the JSX element data
        let data = self.arena.get_jsx_element(jsx_element)?;

        // Get both opening and closing elements
        let opening_node = self.arena.get(data.opening_element)?;
        let closing_node = self.arena.get(data.closing_element)?;

        // Extract the tag names from both elements
        let open_tag_name = self.arena.get_jsx_opening(opening_node)?.tag_name;
        let close_tag_name = self.arena.get_jsx_closing(closing_node)?.tag_name;

        Some(LinkedEditingRanges {
            ranges: vec![
                self.get_range(open_tag_name),
                self.get_range(close_tag_name),
            ],
            word_pattern: None,
        })
    }

    /// Find if the given node (or its parents) is a JSX tag name.
    ///
    /// Walks up the parent chain to check if we're inside a tag_name
    /// of a JSX opening or closing element.
    ///
    /// # Returns
    /// * `Some((tag_name_idx, true))` if we're on an opening tag name
    /// * `Some((tag_name_idx, false))` if we're on a closing tag name
    /// * `None` if we're not on a JSX tag name
    fn find_jsx_tag_context(&self, start_node: NodeIndex) -> Option<(NodeIndex, bool)> {
        let mut current = start_node;

        while !current.is_none() {
            let _node = self.arena.get(current)?;
            let parent_idx = self.arena.get_extended(current)?.parent;

            if parent_idx.is_none() {
                break;
            }

            let parent = self.arena.get(parent_idx)?;

            // Check if parent is an opening element and we're the tag_name
            if parent.kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
                let data = self.arena.get_jsx_opening(parent)?;
                if data.tag_name == current {
                    return Some((current, true));
                }
            }
            // Check if parent is a closing element and we're the tag_name
            else if parent.kind == syntax_kind_ext::JSX_CLOSING_ELEMENT {
                let data = self.arena.get_jsx_closing(parent)?;
                if data.tag_name == current {
                    return Some((current, false));
                }
            }

            current = parent_idx;
        }

        None
    }

    /// Convert a node's position to an LSP Range.
    fn get_range(&self, idx: NodeIndex) -> Range {
        let node = self.arena.get(idx).unwrap();
        Range::new(
            self.line_map.offset_to_position(node.pos, self.source_text),
            self.line_map.offset_to_position(node.end, self.source_text),
        )
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests once we have test infrastructure setup
    // Test cases to cover:
    // 1. Simple element: <div></div>
    // 2. Nested elements: <div><span></span></div>
    // 3. Self-closing: <div /> (should return None)
    // 4. Fragments: <></> (should return None)
    // 5. Component with dots: <My.Component></My.Component>
}
