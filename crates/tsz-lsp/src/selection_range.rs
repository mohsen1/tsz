//! LSP Selection Range implementation.
//!
//! Provides "Expand/Shrink Selection" functionality that allows users
//! to expand or shrink their selection based on semantic boundaries.
//!
//! For example, in `foo.bar().baz`:
//! - Cursor at `bar` → select `bar` → `bar()` → `foo.bar()` → `foo.bar().baz`

use crate::utils::find_node_at_offset;
use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};

/// A selection range with a parent pointer.
///
/// Selection ranges form a linked list from innermost to outermost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionRange {
    /// The range of this selection.
    pub range: Range,
    /// The parent selection range (one level up).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Box<Self>>,
}

impl SelectionRange {
    /// Create a new selection range.
    pub const fn new(range: Range) -> Self {
        Self {
            range,
            parent: None,
        }
    }

    /// Create a selection range with a parent.
    pub fn with_parent(range: Range, parent: Self) -> Self {
        Self {
            range,
            parent: Some(Box::new(parent)),
        }
    }
}

define_lsp_provider!(minimal SelectionRangeProvider, "Provider for selection ranges.");

impl<'a> SelectionRangeProvider<'a> {
    /// Get selection ranges for a list of positions.
    ///
    /// Returns one selection range per position, each with nested parents
    /// representing increasingly larger semantic selections.
    pub fn get_selection_ranges(&self, positions: &[Position]) -> Vec<Option<SelectionRange>> {
        positions
            .iter()
            .map(|pos| self.get_selection_range(*pos))
            .collect()
    }

    /// Get the selection range at a specific position.
    ///
    /// Returns a nested structure where each `SelectionRange` points to its parent,
    /// representing successively larger semantic regions.
    pub fn get_selection_range(&self, position: Position) -> Option<SelectionRange> {
        // Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // Find the most specific node at this offset
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Build the parent chain from innermost to outermost
        self.build_selection_chain(node_idx)
    }

    /// Build a chain of selection ranges from innermost to outermost.
    fn build_selection_chain(&self, start_node: NodeIndex) -> Option<SelectionRange> {
        let mut current = start_node;
        let mut ranges: Vec<Range> = Vec::new();

        // Collect all ranges from innermost to outermost
        while !current.is_none() {
            let node = self.arena.get(current)?;
            let range = self.node_to_range(current)?;

            // Only add this range if it's different from the last one
            // (some nodes may have identical spans)
            if ranges.last() != Some(&range) {
                // Skip certain node types that don't represent meaningful selections
                if !self.should_skip_node(node.kind) {
                    ranges.push(range);
                }
            }

            // Move to parent
            current = self
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        if ranges.is_empty() {
            return None;
        }

        // Build the linked list in reverse (innermost first)
        let mut result: Option<SelectionRange> = None;
        for range in ranges.into_iter().rev() {
            result = Some(match result {
                None => SelectionRange::new(range),
                Some(parent) => SelectionRange::with_parent(range, parent),
            });
        }

        result
    }

    /// Convert a node to a Range.
    fn node_to_range(&self, node_idx: NodeIndex) -> Option<Range> {
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);
        Some(Range::new(start, end))
    }

    /// Determine if a node kind should be skipped in the selection chain.
    ///
    /// Some internal nodes don't represent meaningful selection boundaries.
    const fn should_skip_node(&self, kind: u16) -> bool {
        use syntax_kind_ext::{EMPTY_STATEMENT, OMITTED_EXPRESSION, SEMICOLON_CLASS_ELEMENT};

        matches!(
            kind,
            // Skip list/container nodes that just wrap other nodes
            OMITTED_EXPRESSION |
            SEMICOLON_CLASS_ELEMENT |
            // Skip some internal structural nodes
            EMPTY_STATEMENT
        )
    }
}

#[cfg(test)]
#[path = "../tests/selection_range_tests.rs"]
mod selection_range_tests;
