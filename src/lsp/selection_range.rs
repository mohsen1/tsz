//! LSP Selection Range implementation.
//!
//! Provides "Expand/Shrink Selection" functionality that allows users
//! to expand or shrink their selection based on semantic boundaries.
//!
//! For example, in `foo.bar().baz`:
//! - Cursor at `bar` → select `bar` → `bar()` → `foo.bar()` → `foo.bar().baz`

use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::node::NodeArena;
use crate::parser::{syntax_kind_ext, NodeIndex};

/// A selection range with a parent pointer.
///
/// Selection ranges form a linked list from innermost to outermost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionRange {
    /// The range of this selection.
    pub range: Range,
    /// The parent selection range (one level up).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Box<SelectionRange>>,
}

impl SelectionRange {
    /// Create a new selection range.
    pub fn new(range: Range) -> Self {
        Self {
            range,
            parent: None,
        }
    }

    /// Create a selection range with a parent.
    pub fn with_parent(range: Range, parent: SelectionRange) -> Self {
        Self {
            range,
            parent: Some(Box::new(parent)),
        }
    }
}

/// Provider for selection ranges.
pub struct SelectionRangeProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> SelectionRangeProvider<'a> {
    /// Create a new selection range provider.
    pub fn new(arena: &'a NodeArena, line_map: &'a LineMap, source_text: &'a str) -> Self {
        Self {
            arena,
            line_map,
            source_text,
        }
    }

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
    /// Returns a nested structure where each SelectionRange points to its parent,
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
    fn should_skip_node(&self, kind: u16) -> bool {
        use syntax_kind_ext::*;

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
mod selection_range_tests {
    use super::*;
    use crate::parser::ParserState;

    #[test]
    fn test_selection_range_simple_identifier() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position at 'x' (column 4)
        let pos = Position::new(0, 4);
        let result = provider.get_selection_range(pos);

        assert!(result.is_some(), "Should find selection range for identifier");
        let selection = result.unwrap();

        // Should have at least one parent (the identifier should expand to larger constructs)
        // The innermost range should cover 'x'
        assert!(selection.range.start.character <= 4);
        assert!(selection.range.end.character >= 5);
    }

    #[test]
    fn test_selection_range_nested_expression() {
        let source = "foo.bar().baz";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position at 'bar' (column 4)
        let pos = Position::new(0, 4);
        let result = provider.get_selection_range(pos);

        assert!(result.is_some(), "Should find selection range");

        // Count the depth of the selection chain
        let mut depth = 0;
        let mut current = result.as_ref();
        while let Some(sel) = current {
            depth += 1;
            current = sel.parent.as_deref();
        }

        // Should have multiple levels for nested member access
        assert!(depth >= 2, "Should have nested selection ranges, got {}", depth);
    }

    #[test]
    fn test_selection_range_function_body() {
        let source = "function foo() {\n  return 1;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position at 'return' (line 1, column 2)
        let pos = Position::new(1, 2);
        let result = provider.get_selection_range(pos);

        assert!(result.is_some(), "Should find selection range in function body");

        // Should eventually expand to include the whole function
        let mut current = result.as_ref();
        let mut found_function = false;
        while let Some(sel) = current {
            // Check if this range covers the whole function
            if sel.range.start.line == 0 && sel.range.end.line == 2 {
                found_function = true;
                break;
            }
            current = sel.parent.as_deref();
        }

        assert!(found_function, "Selection should expand to include function");
    }

    #[test]
    fn test_selection_range_multiple_positions() {
        let source = "let a = 1;\nlet b = 2;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        let positions = vec![Position::new(0, 4), Position::new(1, 4)];
        let results = provider.get_selection_ranges(&positions);

        assert_eq!(results.len(), 2);
        assert!(results[0].is_some(), "First position should have selection");
        assert!(results[1].is_some(), "Second position should have selection");
    }

    #[test]
    fn test_selection_range_no_node() {
        let source = "";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position in empty file
        let pos = Position::new(0, 0);
        let result = provider.get_selection_range(pos);

        // Should handle gracefully - may return None or a source file range
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_selection_range_block_statement() {
        let source = "if (x) {\n  y = 1;\n  z = 2;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position at 'y' inside the block
        let pos = Position::new(1, 2);
        let result = provider.get_selection_range(pos);

        assert!(result.is_some(), "Should find selection in block");

        // Verify we can find a range that covers the block
        let mut current = result.as_ref();
        let mut found_block = false;
        while let Some(sel) = current {
            if sel.range.start.line == 0 && sel.range.end.line == 3 {
                found_block = true;
                break;
            }
            current = sel.parent.as_deref();
        }

        assert!(found_block, "Should expand to if statement block");
    }

    #[test]
    fn test_selection_range_class_member() {
        let source = "class Foo {\n  bar() {\n    return 1;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);

        let provider = SelectionRangeProvider::new(arena, &line_map, source);

        // Position at 'return' inside method
        let pos = Position::new(2, 4);
        let result = provider.get_selection_range(pos);

        assert!(result.is_some(), "Should find selection in class method");

        // Count depth
        let mut depth = 0;
        let mut current = result.as_ref();
        while let Some(sel) = current {
            depth += 1;
            current = sel.parent.as_deref();
        }

        // Should have several levels: return -> statement -> block -> method -> class -> file
        assert!(depth >= 4, "Should have deep nesting in class, got {}", depth);
    }
}
