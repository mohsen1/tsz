//! Linked Editing implementation for LSP.
//!
//! Provides linked editing ranges for JSX/TSX files.
//! When editing an opening JSX tag (e.g., `<div>`), the closing tag (`</div>`)
//! automatically syncs.

use crate::utils::find_node_at_or_before_offset;
use tsz_common::position::{Position, Range};
use tsz_parser::{NodeIndex, syntax_kind_ext};

const JSX_TAG_WORD_PATTERN: &str = "[a-zA-Z0-9:\\-\\._$]*";

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
    /// * `root` - The root node of the AST (typically `SourceFile`)
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
        let node_idx = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        // Check if we're on a JSX tag name (opening or closing)
        let (element_idx, _is_opening) = self.find_jsx_tag_context(node_idx, offset)?;

        // Get the parent of the opening/closing element (this must be JSX_ELEMENT).
        // Some JSX property-access tag names can lose parent links, so fall back
        // to the JSX element payload when walking from the tag name.
        let mut jsx_element_idx = self.arena.get_extended(element_idx)?.parent;
        if jsx_element_idx.is_none()
            && let Some(found) = self.find_parent_jsx_element(element_idx)
        {
            jsx_element_idx = found;
        }
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
        let open_tag = self.arena.get(open_tag_name)?;
        let close_tag = self.arena.get(close_tag_name)?;
        let open_tag_end = self.jsx_tag_name_tight_end(open_tag_name);
        let close_tag_end = self.jsx_tag_name_tight_end(close_tag_name);

        // Only return linked editing when the cursor is within one of the tag names.
        let inside_open = open_tag.pos <= offset && offset <= open_tag_end;
        let inside_close = close_tag.pos <= offset && offset <= close_tag_end;
        if !inside_open && !inside_close {
            return None;
        }

        // Do not link malformed tags with missing names or mismatched texts.
        if open_tag.pos <= opening_node.pos
            || close_tag.pos <= closing_node.pos
            || open_tag_end >= opening_node.end
            || close_tag_end >= closing_node.end
        {
            return None;
        }

        let open_text = self
            .source_text
            .get(open_tag.pos as usize..open_tag_end as usize)?;
        let close_text = self
            .source_text
            .get(close_tag.pos as usize..close_tag_end as usize)?;
        if open_text != close_text {
            return None;
        }

        Some(LinkedEditingRanges {
            ranges: vec![
                self.jsx_tag_name_range(open_tag_name),
                self.jsx_tag_name_range(close_tag_name),
            ],
            word_pattern: Some(JSX_TAG_WORD_PATTERN.to_string()),
        })
    }

    /// Find if the given node (or its parents) is a JSX tag name.
    ///
    /// Walks up the parent chain to check if we're inside a `tag_name`
    /// of a JSX opening or closing element.
    ///
    /// # Returns
    /// * `Some((opening_element_idx, true))` if we're on an opening tag name
    /// * `Some((closing_element_idx, false))` if we're on a closing tag name
    /// * `None` if we're not on a JSX tag name
    fn find_jsx_tag_context(
        &self,
        start_node: NodeIndex,
        offset: u32,
    ) -> Option<(NodeIndex, bool)> {
        let mut current = start_node;

        while current.is_some() {
            let current_node = self.arena.get(current)?;

            if current_node.kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
                let data = self.arena.get_jsx_opening(current_node)?;
                if self.is_offset_inside_node(data.tag_name, offset) {
                    return Some((current, true));
                }
            } else if current_node.kind == syntax_kind_ext::JSX_CLOSING_ELEMENT {
                let data = self.arena.get_jsx_closing(current_node)?;
                if self.is_offset_inside_node(data.tag_name, offset) {
                    return Some((current, false));
                }
            }

            let parent_idx = self.arena.get_extended(current)?.parent;

            if parent_idx.is_none() {
                break;
            }

            let parent = self.arena.get(parent_idx)?;

            // Check if parent is an opening element and we're the tag_name
            if parent.kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
                let data = self.arena.get_jsx_opening(parent)?;
                if self.is_within_jsx_tag_name(data.tag_name, current, offset) {
                    return Some((parent_idx, true));
                }
            }
            // Check if parent is a closing element and we're the tag_name
            else if parent.kind == syntax_kind_ext::JSX_CLOSING_ELEMENT {
                let data = self.arena.get_jsx_closing(parent)?;
                if self.is_within_jsx_tag_name(data.tag_name, current, offset) {
                    return Some((parent_idx, false));
                }
            }

            current = parent_idx;
        }

        self.find_jsx_tag_context_by_span(offset)
    }

    fn is_offset_inside_node(&self, node_idx: NodeIndex, offset: u32) -> bool {
        self.arena.get(node_idx).is_some_and(|node| {
            let end = self.jsx_tag_name_tight_end(node_idx);
            node.pos <= offset && offset <= end
        })
    }

    fn is_within_jsx_tag_name(&self, tag_name: NodeIndex, current: NodeIndex, offset: u32) -> bool {
        if tag_name == current {
            return true;
        }

        self.arena.get(tag_name).is_some_and(|tag| {
            let end = self.jsx_tag_name_tight_end(tag_name);
            tag.pos <= offset
                && offset <= end
                && self
                    .arena
                    .get(current)
                    .is_some_and(|node| tag.pos <= node.pos && node.end <= end)
        })
    }

    fn find_jsx_tag_context_by_span(&self, offset: u32) -> Option<(NodeIndex, bool)> {
        for (index, node) in self.arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(index as u32);
            if node.kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
                let data = self.arena.get_jsx_opening(node)?;
                if self.is_offset_inside_node(data.tag_name, offset) {
                    return Some((node_idx, true));
                }
            } else if node.kind == syntax_kind_ext::JSX_CLOSING_ELEMENT {
                let data = self.arena.get_jsx_closing(node)?;
                if self.is_offset_inside_node(data.tag_name, offset) {
                    return Some((node_idx, false));
                }
            }
        }
        None
    }

    fn find_parent_jsx_element(&self, element_idx: NodeIndex) -> Option<NodeIndex> {
        for (index, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != syntax_kind_ext::JSX_ELEMENT {
                continue;
            }

            let data = self.arena.get_jsx_element(node)?;
            if data.opening_element == element_idx || data.closing_element == element_idx {
                return Some(NodeIndex(index as u32));
            }
        }
        None
    }

    fn jsx_tag_name_tight_end(&self, tag_name: NodeIndex) -> u32 {
        let Some(node) = self.arena.get(tag_name) else {
            return 0;
        };

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.arena.get_access_expr(node)
        {
            return self.jsx_tag_name_tight_end(access.name_or_argument);
        }

        node.end
    }

    fn jsx_tag_name_range(&self, tag_name: NodeIndex) -> Range {
        let Some(node) = self.arena.get(tag_name) else {
            return Range::new(Position::new(0, 0), Position::new(0, 0));
        };

        Range::new(
            self.line_map.offset_to_position(node.pos, self.source_text),
            self.line_map
                .offset_to_position(self.jsx_tag_name_tight_end(tag_name), self.source_text),
        )
    }
}
