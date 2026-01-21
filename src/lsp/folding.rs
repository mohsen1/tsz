//! LSP Folding Ranges implementation
//!
//! Provides folding range information for code blocks (functions, classes, etc.).

use crate::lsp::position::LineMap;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};

/// A folding range
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRange {
    /// The start line of the folding range (0-based)
    pub start_line: u32,
    /// The end line of the folding range (0-based)
    pub end_line: u32,
    /// The kind of folding range (region, comment, imports, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl FoldingRange {
    /// Create a new folding range.
    pub fn new(start_line: u32, end_line: u32) -> Self {
        Self {
            start_line,
            end_line,
            kind: None,
        }
    }

    /// Set the kind of folding range.
    pub fn with_kind(mut self, kind: &str) -> Self {
        self.kind = Some(kind.to_string());
        self
    }
}

/// Provider for folding ranges.
pub struct FoldingRangeProvider<'a> {
    arena: &'a NodeArena,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> FoldingRangeProvider<'a> {
    /// Create a new folding range provider.
    pub fn new(arena: &'a NodeArena, line_map: &'a LineMap, source_text: &'a str) -> Self {
        Self {
            arena,
            line_map,
            source_text,
        }
    }

    /// Get all folding ranges in the document.
    pub fn get_folding_ranges(&self, root: NodeIndex) -> Vec<FoldingRange> {
        let mut ranges = Vec::new();

        // Collect folding ranges from AST nodes
        self.collect_from_node(root, &mut ranges);

        // Collect multi-line comment folding ranges
        self.collect_comment_ranges(&mut ranges);

        // Remove duplicates and sort
        ranges.sort_by_key(|r| (r.start_line, r.end_line));
        ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

        ranges
    }

    /// Collect folding ranges from AST nodes.
    fn collect_from_node(&self, node_idx: NodeIndex, ranges: &mut Vec<FoldingRange>) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            // Source File: Recurse into statements
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        self.collect_from_node(stmt, ranges);
                    }
                }
            }

            // Function Declaration - fold the body
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let body_range = self.get_line_range(func.body);

                    // Only add if multi-line
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }

                    // Recurse into body
                    self.collect_from_node(func.body, ranges);
                }
            }

            // Class Declaration - fold the body
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    let class_range = self.get_line_range(node_idx);

                    // Only add if multi-line class
                    if class_range.0 < class_range.1 {
                        ranges.push(
                            FoldingRange::new(class_range.0, class_range.1).with_kind("region"),
                        );
                    }

                    // Recurse into class members
                    for &member in &class.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }

            // Block statement
            k if k == syntax_kind_ext::BLOCK => {
                let block_range = self.get_line_range(node_idx);

                // Only fold multi-line blocks
                if block_range.0 < block_range.1 {
                    ranges.push(FoldingRange::new(block_range.0, block_range.1));
                }

                // Recurse into block
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_from_node(stmt, ranges);
                    }
                }
            }

            // If statement - recurse into then/else blocks
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.collect_from_node(if_stmt.then_statement, ranges);
                    self.collect_from_node(if_stmt.else_statement, ranges);
                }
            }

            // Interface Declaration
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    let iface_range = self.get_line_range(node_idx);

                    if iface_range.0 < iface_range.1 {
                        ranges.push(
                            FoldingRange::new(iface_range.0, iface_range.1).with_kind("region"),
                        );
                    }

                    // Recurse into interface members
                    for &member in &iface.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }

            // Type Alias
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(_alias) = self.arena.get_type_alias(node) {
                    let alias_range = self.get_line_range(node_idx);

                    if alias_range.0 < alias_range.1 {
                        ranges.push(
                            FoldingRange::new(alias_range.0, alias_range.1).with_kind("region"),
                        );
                    }
                }
            }

            // Enum Declaration
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    let enum_range = self.get_line_range(node_idx);

                    if enum_range.0 < enum_range.1 {
                        ranges.push(
                            FoldingRange::new(enum_range.0, enum_range.1).with_kind("region"),
                        );
                    }

                    // Recurse into enum members
                    for &member in &enum_decl.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }

            // Module/namespace Declaration
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    let module_range = self.get_line_range(node_idx);

                    if module_range.0 < module_range.1 {
                        ranges.push(
                            FoldingRange::new(module_range.0, module_range.1).with_kind("region"),
                        );
                    }

                    // Recurse into module body
                    self.collect_from_node(module.body, ranges);
                }
            }

            // Method declaration
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let body_range = self.get_line_range(method.body);

                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }

                    self.collect_from_node(method.body, ranges);
                }
            }

            // Property declaration
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    // Recurse into initializer if present
                    self.collect_from_node(prop.initializer, ranges);
                }
            }

            // Constructor
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    let body_range = self.get_line_range(ctor.body);

                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }

                    self.collect_from_node(ctor.body, ranges);
                }
            }

            // Get accessor
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);

                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }

                    self.collect_from_node(accessor.body, ranges);
                }
            }

            // Set accessor
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);

                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }

                    self.collect_from_node(accessor.body, ranges);
                }
            }

            // Export declaration
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node) {
                    // Recurse into export clause
                    self.collect_from_node(export.export_clause, ranges);
                }
            }

            _ => {}
        }
    }

    /// Collect multi-line comment folding ranges.
    fn collect_comment_ranges(&self, ranges: &mut Vec<FoldingRange>) {
        let lines: Vec<&str> = self.source_text.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();

            // Check for multi-line comment start
            if line.starts_with("/*") && !line.contains("*/") {
                let start_line = i as u32;

                // Find the end of the comment
                let mut end_line = start_line;
                for j in (i + 1)..lines.len() {
                    if lines[j].contains("*/") {
                        end_line = j as u32;
                        break;
                    }
                }

                // Only add if multi-line
                if end_line > start_line {
                    ranges.push(FoldingRange::new(start_line, end_line).with_kind("comment"));
                }

                i = end_line as usize + 1;
                continue;
            }

            i += 1;
        }
    }

    /// Get the line range (start, end) for a node.
    fn get_line_range(&self, node_idx: NodeIndex) -> (u32, u32) {
        let Some(node) = self.arena.get(node_idx) else {
            return (0, 0);
        };

        let lo = node.pos;
        let hi = node.end;

        // Get line positions
        let start_pos = self
            .line_map
            .offset_to_position(lo, self.source_text);
        let end_pos = self
            .line_map
            .offset_to_position(hi.saturating_sub(1), self.source_text);

        (start_pos.line, end_pos.line)
    }
}

#[cfg(test)]
mod folding_tests {
    use super::*;
    use crate::parser::ParserState;

    #[test]
    fn test_folding_ranges_simple_function() {
        let source = r#"
function foo() {
    return 1;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(!ranges.is_empty(), "Should find at least one folding range");

        // Should find a folding range for the function body (lines 1-3)
        let function_range = ranges.iter().find(|r| r.start_line == 1 && r.end_line == 3);
        assert!(
            function_range.is_some(),
            "Should find function body folding range"
        );
    }

    #[test]
    fn test_folding_ranges_nested_functions() {
        let source = r#"
function outer() {
    function inner() {
        return 1;
    }
    return inner();
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        // Should find at least 2 folding ranges (outer and inner function bodies)
        assert!(ranges.len() >= 2, "Should find at least 2 folding ranges");
    }

    #[test]
    fn test_folding_ranges_class() {
        let source = r#"
class MyClass {
    method1() {
        return 1;
    }

    method2() {
        return 2;
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(!ranges.is_empty(), "Should find folding ranges for class");

        // Should find the class body as a region
        let class_range = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(
            class_range.is_some(),
            "Should find class body folding range"
        );
    }

    #[test]
    fn test_folding_ranges_block_statement() {
        let source = r#"
if (true) {
    console.log("yes");
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        // Should find a folding range for the if block
        assert!(
            !ranges.is_empty(),
            "Should find block statement folding range"
        );
    }

    #[test]
    fn test_folding_ranges_interface() {
        let source = r#"
interface Point {
    x: number;
    y: number;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(!ranges.is_empty(), "Should find interface folding range");
    }

    #[test]
    fn test_folding_ranges_enum() {
        let source = r#"
enum Color {
    Red,
    Green,
    Blue
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(!ranges.is_empty(), "Should find enum folding range");
    }

    #[test]
    fn test_folding_ranges_namespace() {
        let source = r#"
namespace MyNamespace {
    function foo() {}
    const bar = 1;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(!ranges.is_empty(), "Should find namespace folding range");
    }

    #[test]
    fn test_folding_ranges_no_single_line() {
        let source = "function foo() { return 1; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        // Single-line constructs should not be foldable
        assert!(
            ranges.is_empty(),
            "Should not find folding ranges for single-line code"
        );
    }

    #[test]
    fn test_folding_ranges_empty_source() {
        let source = "";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        let ranges = provider.get_folding_ranges(root);

        assert!(
            ranges.is_empty(),
            "Should not find folding ranges in empty source"
        );
    }
}
