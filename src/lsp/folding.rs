//! LSP Folding Ranges implementation
//!
//! Provides folding range information for code blocks (functions, classes, etc.),
//! #region/#endregion markers, consecutive single-line comments, and import groups.

use crate::lsp::position::LineMap;
use crate::parser::node::{NodeAccess, NodeArena};
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

/// Result of parsing a region delimiter comment.
#[derive(Debug)]
struct RegionDelimiter {
    is_start: bool,
    label: String,
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
        self.collect_from_node(root, &mut ranges);
        self.collect_comment_ranges(&mut ranges);
        self.collect_region_ranges(&mut ranges);
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
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    self.collect_import_groups(&sf.statements.nodes, ranges);
                    for &stmt in &sf.statements.nodes {
                        self.collect_from_node(stmt, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    let body_range = self.get_line_range(func.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    let class_range = self.get_line_range(node_idx);
                    if class_range.0 < class_range.1 {
                        ranges.push(
                            FoldingRange::new(class_range.0, class_range.1).with_kind("region"),
                        );
                    }
                    for &member in &class.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                let block_range = self.get_line_range(node_idx);
                if block_range.0 < block_range.1 {
                    ranges.push(FoldingRange::new(block_range.0, block_range.1));
                }
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_from_node(stmt, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.collect_from_node(if_stmt.then_statement, ranges);
                    self.collect_from_node(if_stmt.else_statement, ranges);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    let iface_range = self.get_line_range(node_idx);
                    if iface_range.0 < iface_range.1 {
                        ranges.push(
                            FoldingRange::new(iface_range.0, iface_range.1).with_kind("region"),
                        );
                    }
                    for &member in &iface.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }
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
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    let enum_range = self.get_line_range(node_idx);
                    if enum_range.0 < enum_range.1 {
                        ranges.push(
                            FoldingRange::new(enum_range.0, enum_range.1).with_kind("region"),
                        );
                    }
                    for &member in &enum_decl.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    let module_range = self.get_line_range(node_idx);
                    if module_range.0 < module_range.1 {
                        ranges.push(
                            FoldingRange::new(module_range.0, module_range.1).with_kind("region"),
                        );
                    }
                    self.collect_from_node(module.body, ranges);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let body_range = self.get_line_range(method.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(method.body, ranges);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    self.collect_from_node(prop.initializer, ranges);
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    let body_range = self.get_line_range(ctor.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(ctor.body, ranges);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(accessor.body, ranges);
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(accessor.body, ranges);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node) {
                    self.collect_from_node(export.export_clause, ranges);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if let Some(func) = self.arena.get_function(node) {
                    let body_range = self.get_line_range(func.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    let body_range = self.get_line_range(func.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(body_range.0, body_range.1));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    // Fold the case block
                    let case_block_range = self.get_line_range(switch.case_block);
                    if case_block_range.0 < case_block_range.1 {
                        ranges.push(FoldingRange::new(case_block_range.0, case_block_range.1));
                    }
                    self.collect_from_node(switch.case_block, ranges);
                }
            }
            k if k == syntax_kind_ext::CASE_BLOCK => {
                // CaseBlock uses the same block data structure
                if let Some(block) = self.arena.get_block(node) {
                    for &clause in &block.statements.nodes {
                        self.collect_from_node(clause, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    // Fold the clause body
                    let clause_range = self.get_line_range(node_idx);
                    if clause_range.0 < clause_range.1 {
                        ranges.push(FoldingRange::new(clause_range.0, clause_range.1));
                    }
                    // Walk children for nested blocks
                    for &stmt in &clause.statements.nodes {
                        self.collect_from_node(stmt, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren_range = self.get_line_range(node_idx);
                if paren_range.0 < paren_range.1 {
                    ranges.push(FoldingRange::new(paren_range.0, paren_range.1));
                }
                for child in self.arena.get_children(node_idx) {
                    self.collect_from_node(child, ranges);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    // Fold multi-line argument lists
                    if let Some(ref args) = call.arguments {
                        if !args.nodes.is_empty() {
                            if let (Some(first), Some(last)) = (
                                args.nodes.first().and_then(|&n| self.arena.get(n)),
                                args.nodes.last().and_then(|&n| self.arena.get(n)),
                            ) {
                                let first_line = self
                                    .line_map
                                    .offset_to_position(first.pos, self.source_text)
                                    .line;
                                let last_line = self
                                    .line_map
                                    .offset_to_position(last.end, self.source_text)
                                    .line;
                                if first_line < last_line {
                                    ranges.push(FoldingRange::new(first_line, last_line));
                                }
                            }
                        }
                        for &arg in &args.nodes {
                            self.collect_from_node(arg, ranges);
                        }
                    }
                    self.collect_from_node(call.expression, ranges);
                }
            }
            k if k == syntax_kind_ext::NAMED_IMPORTS || k == syntax_kind_ext::NAMED_EXPORTS => {
                let range = self.get_line_range(node_idx);
                if range.0 < range.1 {
                    ranges.push(FoldingRange::new(range.0, range.1));
                }
            }
            _ => {
                // Generic tree walking for any unhandled node types
                for child in self.arena.get_children(node_idx) {
                    self.collect_from_node(child, ranges);
                }
            }
        }
    }

    /// Collect import group folding ranges.
    fn collect_import_groups(&self, statements: &[NodeIndex], ranges: &mut Vec<FoldingRange>) {
        let mut i = 0;
        while i < statements.len() {
            if !self.is_import_node(statements[i]) {
                i += 1;
                continue;
            }
            let first_import = i;
            while i < statements.len() && self.is_import_node(statements[i]) {
                i += 1;
            }
            let last_import = i - 1;
            if last_import > first_import {
                let start_range = self.get_line_range(statements[first_import]);
                let end_range = self.get_line_range(statements[last_import]);
                if end_range.1 > start_range.0 {
                    ranges.push(FoldingRange::new(start_range.0, end_range.1).with_kind("imports"));
                }
            }
        }
    }

    /// Check if a node is an import statement.
    fn is_import_node(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::IMPORT_DECLARATION
            || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
    }

    /// Collect comment-based folding ranges.
    fn collect_comment_ranges(&self, ranges: &mut Vec<FoldingRange>) {
        let lines: Vec<&str> = self.source_text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();

            // Multi-line block comment
            if trimmed.starts_with("/*") && !trimmed.contains("*/") {
                let start_line = i as u32;
                let mut end_line = start_line;
                for j in (i + 1)..lines.len() {
                    if lines[j].contains("*/") {
                        end_line = j as u32;
                        break;
                    }
                }
                if end_line > start_line {
                    ranges.push(FoldingRange::new(start_line, end_line).with_kind("comment"));
                }
                i = end_line as usize + 1;
                continue;
            }

            // Single-line block comment on one line
            if trimmed.starts_with("/*") && trimmed.ends_with("*/") {
                i += 1;
                continue;
            }

            // Consecutive single-line comments (skip region markers)
            if is_single_line_comment(trimmed) && !is_region_comment(trimmed) {
                let start_line = i as u32;
                let mut end_line = start_line;
                let mut j = i + 1;
                while j < lines.len() {
                    let next_trimmed = lines[j].trim();
                    if is_single_line_comment(next_trimmed) && !is_region_comment(next_trimmed) {
                        end_line = j as u32;
                        j += 1;
                    } else {
                        break;
                    }
                }
                if end_line > start_line {
                    ranges.push(FoldingRange::new(start_line, end_line).with_kind("comment"));
                }
                i = j;
                continue;
            }

            i += 1;
        }
    }

    /// Collect region folding ranges from #region/#endregion markers.
    fn collect_region_ranges(&self, ranges: &mut Vec<FoldingRange>) {
        let lines: Vec<&str> = self.source_text.lines().collect();
        let mut region_stack: Vec<(u32, String)> = Vec::new();
        let mut in_block_comment = false;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if !in_block_comment {
                if trimmed.starts_with("/*") && !trimmed.contains("*/") {
                    in_block_comment = true;
                    continue;
                }
            } else {
                if trimmed.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }

            if let Some(delimiter) = parse_region_delimiter(trimmed) {
                if delimiter.is_start {
                    region_stack.push((i as u32, delimiter.label));
                } else if let Some((start_line, _label)) = region_stack.pop() {
                    let end_line = i as u32;
                    if end_line > start_line {
                        ranges.push(FoldingRange::new(start_line, end_line).with_kind("region"));
                    }
                }
            }
        }
    }

    /// Get the line range (start, end) for a node.
    fn get_line_range(&self, node_idx: NodeIndex) -> (u32, u32) {
        let Some(node) = self.arena.get(node_idx) else {
            return (0, 0);
        };
        let lo = node.pos;
        let hi = node.end;
        let start_pos = self.line_map.offset_to_position(lo, self.source_text);
        let end_pos = self
            .line_map
            .offset_to_position(hi.saturating_sub(1), self.source_text);
        (start_pos.line, end_pos.line)
    }
}

fn is_single_line_comment(trimmed: &str) -> bool {
    trimmed.starts_with("//")
}

fn is_region_comment(trimmed: &str) -> bool {
    parse_region_delimiter(trimmed).is_some()
}

fn parse_region_delimiter(trimmed: &str) -> Option<RegionDelimiter> {
    if !trimmed.starts_with("//") {
        return None;
    }
    let after_slashes = trimmed[2..].trim_start();
    if let Some(rest) = after_slashes.strip_prefix("#endregion") {
        let _ = rest;
        Some(RegionDelimiter {
            is_start: false,
            label: String::new(),
        })
    } else if let Some(rest) = after_slashes.strip_prefix("#region") {
        let label = rest.trim().to_string();
        Some(RegionDelimiter {
            is_start: true,
            label: if label.is_empty() {
                "#region".to_string()
            } else {
                label
            },
        })
    } else {
        None
    }
}

#[cfg(test)]
mod folding_tests {
    use super::*;
    use crate::parser::ParserState;

    fn get_ranges(source: &str) -> Vec<FoldingRange> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let line_map = LineMap::build(source);
        let provider = FoldingRangeProvider::new(arena, &line_map, source);
        provider.get_folding_ranges(root)
    }

    #[test]
    fn test_folding_ranges_simple_function() {
        let source = "\nfunction foo() {\n    return 1;\n}\n";
        let ranges = get_ranges(source);
        assert!(!ranges.is_empty(), "Should find at least one folding range");
        let function_range = ranges.iter().find(|r| r.start_line == 1 && r.end_line == 3);
        assert!(
            function_range.is_some(),
            "Should find function body folding range"
        );
    }

    #[test]
    fn test_folding_ranges_nested_functions() {
        let source = "\nfunction outer() {\n    function inner() {\n        return 1;\n    }\n    return inner();\n}\n";
        let ranges = get_ranges(source);
        assert!(ranges.len() >= 2, "Should find at least 2 folding ranges");
    }

    #[test]
    fn test_folding_ranges_class() {
        let source = "\nclass MyClass {\n    method1() {\n        return 1;\n    }\n\n    method2() {\n        return 2;\n    }\n}\n";
        let ranges = get_ranges(source);
        assert!(!ranges.is_empty(), "Should find folding ranges for class");
        let class_range = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(
            class_range.is_some(),
            "Should find class body folding range"
        );
    }

    #[test]
    fn test_folding_ranges_block_statement() {
        let source = "\nif (true) {\n    console.log(\"yes\");\n}\n";
        let ranges = get_ranges(source);
        assert!(
            !ranges.is_empty(),
            "Should find block statement folding range"
        );
    }

    #[test]
    fn test_folding_ranges_interface() {
        let source = "\ninterface Point {\n    x: number;\n    y: number;\n}\n";
        let ranges = get_ranges(source);
        assert!(!ranges.is_empty(), "Should find interface folding range");
    }

    #[test]
    fn test_folding_ranges_enum() {
        let source = "\nenum Color {\n    Red,\n    Green,\n    Blue\n}\n";
        let ranges = get_ranges(source);
        assert!(!ranges.is_empty(), "Should find enum folding range");
    }

    #[test]
    fn test_folding_ranges_namespace() {
        let source = "\nnamespace MyNamespace {\n    function foo() {}\n    const bar = 1;\n}\n";
        let ranges = get_ranges(source);
        assert!(!ranges.is_empty(), "Should find namespace folding range");
    }

    #[test]
    fn test_folding_ranges_no_single_line() {
        let source = "function foo() { return 1; }";
        let ranges = get_ranges(source);
        assert!(
            ranges.is_empty(),
            "Should not find folding ranges for single-line code"
        );
    }

    #[test]
    fn test_folding_ranges_empty_source() {
        let source = "";
        let ranges = get_ranges(source);
        assert!(
            ranges.is_empty(),
            "Should not find folding ranges in empty source"
        );
    }

    // --- #region/#endregion tests ---

    #[test]
    fn test_region_basic() {
        let source = "// #region My Region\nconst a = 1;\nconst b = 2;\n// #endregion\n";
        let ranges = get_ranges(source);
        let region = ranges
            .iter()
            .find(|r| r.kind.as_deref() == Some("region") && r.start_line == 0);
        assert!(region.is_some(), "Should find #region folding range");
        let region = region.unwrap();
        assert_eq!(region.start_line, 0);
        assert_eq!(region.end_line, 3);
    }

    #[test]
    fn test_region_no_space_before_hash() {
        let source = "//#region NoSpace\nconst x = 1;\n//#endregion\n";
        let ranges = get_ranges(source);
        let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(
            region.is_some(),
            "Should find #region without space after //"
        );
        let region = region.unwrap();
        assert_eq!(region.start_line, 0);
        assert_eq!(region.end_line, 2);
    }

    #[test]
    fn test_region_nested() {
        let source = "// #region Outer\nconst a = 1;\n// #region Inner\nconst b = 2;\n// #endregion\nconst c = 3;\n// #endregion\n";
        let ranges = get_ranges(source);
        let regions: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("region"))
            .collect();
        assert!(
            regions.len() >= 2,
            "Should find at least 2 nested regions, found {}",
            regions.len()
        );
        let inner = regions
            .iter()
            .find(|r| r.start_line == 2 && r.end_line == 4);
        assert!(inner.is_some(), "Should find inner region (lines 2-4)");
        let outer = regions
            .iter()
            .find(|r| r.start_line == 0 && r.end_line == 6);
        assert!(outer.is_some(), "Should find outer region (lines 0-6)");
    }

    #[test]
    fn test_region_inside_block_comment_ignored() {
        let source = "/*\n// #region Should Be Ignored\nsome comment text\n// #endregion\n*/\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let regions: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("region"))
            .collect();
        assert!(
            regions.is_empty(),
            "Should NOT find region markers inside block comments"
        );
    }

    #[test]
    fn test_region_unclosed_is_ignored() {
        let source = "// #region Unclosed\nconst a = 1;\nconst b = 2;\n";
        let ranges = get_ranges(source);
        let regions: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("region"))
            .collect();
        assert!(
            regions.is_empty(),
            "Unclosed #region should not produce a folding range"
        );
    }

    #[test]
    fn test_region_with_label() {
        let source = "// #region Database Setup\nconst db = connect();\n// #endregion\n";
        let ranges = get_ranges(source);
        let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(region.is_some(), "Should find region with label");
    }

    #[test]
    fn test_region_without_label() {
        let source = "// #region\nconst db = connect();\n// #endregion\n";
        let ranges = get_ranges(source);
        let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(region.is_some(), "Should find region without label");
    }

    // --- Consecutive single-line comment tests ---

    #[test]
    fn test_consecutive_single_line_comments() {
        let source = "// This is a\n// multi-line description\n// using single-line comments\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let comment = ranges
            .iter()
            .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
        assert!(
            comment.is_some(),
            "Should find consecutive single-line comment fold"
        );
        let comment = comment.unwrap();
        assert_eq!(comment.start_line, 0);
        assert_eq!(comment.end_line, 2);
    }

    #[test]
    fn test_single_comment_no_fold() {
        let source = "// Just one comment\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let comment = ranges
            .iter()
            .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
        assert!(
            comment.is_none(),
            "A single // comment should not produce a fold"
        );
    }

    #[test]
    fn test_two_single_line_comments_fold() {
        let source = "// Comment line 1\n// Comment line 2\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let comment = ranges
            .iter()
            .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 0);
        assert!(comment.is_some(), "Two consecutive // comments should fold");
        let comment = comment.unwrap();
        assert_eq!(comment.start_line, 0);
        assert_eq!(comment.end_line, 1);
    }

    #[test]
    fn test_region_comments_excluded_from_single_line_group() {
        let source = "// #region Test\n// normal comment 1\n// normal comment 2\n// #endregion\n";
        let ranges = get_ranges(source);
        let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(region.is_some(), "Should find region fold");
        let comment = ranges
            .iter()
            .find(|r| r.kind.as_deref() == Some("comment") && r.start_line == 1);
        assert!(
            comment.is_some(),
            "Should find comment fold for normal comments inside region"
        );
        if let Some(c) = comment {
            assert_eq!(c.end_line, 2);
        }
    }

    // --- Block comment tests ---

    #[test]
    fn test_multiline_block_comment() {
        let source =
            "/*\n * This is a block comment\n * spanning multiple lines\n */\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let comment = ranges.iter().find(|r| r.kind.as_deref() == Some("comment"));
        assert!(
            comment.is_some(),
            "Should find multi-line block comment fold"
        );
        let comment = comment.unwrap();
        assert_eq!(comment.start_line, 0);
        assert_eq!(comment.end_line, 3);
    }

    #[test]
    fn test_jsdoc_comment() {
        let source =
            "/**\n * JSDoc comment\n * @param x - the value\n */\nfunction foo(x: number) {}\n";
        let ranges = get_ranges(source);
        let comment = ranges.iter().find(|r| r.kind.as_deref() == Some("comment"));
        assert!(comment.is_some(), "Should find JSDoc comment fold");
        let comment = comment.unwrap();
        assert_eq!(comment.start_line, 0);
        assert_eq!(comment.end_line, 3);
    }

    #[test]
    fn test_adjacent_block_comments_separate() {
        let source = "/*\n * First block\n */\n/*\n * Second block\n */\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let comments: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("comment"))
            .collect();
        assert_eq!(
            comments.len(),
            2,
            "Two adjacent block comments should produce two separate folds, got {}",
            comments.len()
        );
    }

    // --- Import group tests ---

    #[test]
    fn test_import_group_fold() {
        let source = "import { a } from \"a\";\nimport { b } from \"b\";\nimport { c } from \"c\";\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
        assert!(imports.is_some(), "Should find import group fold");
        let imports = imports.unwrap();
        assert_eq!(imports.start_line, 0, "Import group should start at line 0");
        // The end line depends on parser node boundaries; just verify it covers multiple lines
        assert!(
            imports.end_line >= 2,
            "Import group should span at least 3 lines"
        );
    }

    #[test]
    fn test_single_import_no_fold() {
        let source = "import { a } from \"a\";\n\nconst x = 1;\n";
        let ranges = get_ranges(source);
        let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
        assert!(
            imports.is_none(),
            "Single import should not produce imports fold"
        );
    }

    #[test]
    fn test_two_import_groups_separated() {
        let source = "import { a } from \"a\";\nimport { b } from \"b\";\n\nconst mid = 1;\n\nimport { c } from \"c\";\nimport { d } from \"d\";\n";
        let ranges = get_ranges(source);
        let imports: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("imports"))
            .collect();
        assert_eq!(
            imports.len(),
            2,
            "Should find two import groups, found {}",
            imports.len()
        );
    }

    // --- parse_region_delimiter unit tests ---

    #[test]
    fn test_parse_region_delimiter_basic() {
        let result = parse_region_delimiter("// #region My Region");
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(d.is_start);
        assert_eq!(d.label, "My Region");
    }

    #[test]
    fn test_parse_region_delimiter_no_space() {
        let result = parse_region_delimiter("//#region NoSpace");
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(d.is_start);
        assert_eq!(d.label, "NoSpace");
    }

    #[test]
    fn test_parse_region_delimiter_endregion() {
        let result = parse_region_delimiter("// #endregion");
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(!d.is_start);
    }

    #[test]
    fn test_parse_region_delimiter_no_label() {
        let result = parse_region_delimiter("// #region");
        assert!(result.is_some());
        let d = result.unwrap();
        assert!(d.is_start);
        assert_eq!(d.label, "#region");
    }

    #[test]
    fn test_parse_region_delimiter_not_a_region() {
        assert!(parse_region_delimiter("// just a comment").is_none());
        assert!(parse_region_delimiter("const x = 1;").is_none());
        assert!(parse_region_delimiter("/* #region */").is_none());
    }

    #[test]
    fn test_parse_region_delimiter_with_preceding_text() {
        assert!(parse_region_delimiter("test // #region").is_none());
    }

    // --- Combined test ---

    #[test]
    fn test_combined_imports_comments_regions() {
        let source = "// #region Imports\nimport { a } from \"a\";\nimport { b } from \"b\";\n// #endregion\n\n// Header comment\n// describing the module\nconst x = 1;\n\n/*\n * Block comment\n */\nfunction foo() {\n    return x;\n}\n";
        let ranges = get_ranges(source);
        let region = ranges.iter().find(|r| r.kind.as_deref() == Some("region"));
        assert!(region.is_some(), "Should find region fold");
        let imports = ranges.iter().find(|r| r.kind.as_deref() == Some("imports"));
        assert!(imports.is_some(), "Should find imports fold");
        let comments: Vec<&FoldingRange> = ranges
            .iter()
            .filter(|r| r.kind.as_deref() == Some("comment"))
            .collect();
        assert!(
            comments.len() >= 2,
            "Should find at least 2 comment folds, found {}",
            comments.len()
        );
    }
}
