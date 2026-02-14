//! LSP Folding Ranges implementation
//!
//! Provides folding range information for code blocks (functions, classes, etc.),
//! #region/#endregion markers, consecutive single-line comments, and import groups.

use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeIndex, syntax_kind_ext};

/// A folding range
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRange {
    /// The start line of the folding range (0-based)
    pub start_line: u32,
    /// The end line of the folding range (0-based)
    pub end_line: u32,
    /// Byte offset of the span start (for precise textSpan in getOutliningSpans)
    pub start_offset: u32,
    /// Byte offset of the span end (for precise textSpan in getOutliningSpans)
    pub end_offset: u32,
    /// The kind of folding range (region, comment, imports, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl FoldingRange {
    /// Create a new folding range.
    pub fn new(start_line: u32, end_line: u32, start_offset: u32, end_offset: u32) -> Self {
        Self {
            start_line,
            end_line,
            start_offset,
            end_offset,
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
}

define_lsp_provider!(minimal FoldingRangeProvider, "Provider for folding ranges.");

impl<'a> FoldingRangeProvider<'a> {
    /// Get all folding ranges in the document.
    pub fn get_folding_ranges(&self, root: NodeIndex) -> Vec<FoldingRange> {
        let mut ranges = Vec::new();
        self.collect_from_node(root, &mut ranges);
        self.collect_comment_ranges(&mut ranges);
        self.collect_region_ranges(&mut ranges);
        ranges.sort_by_key(|r| (r.start_offset, r.end_offset));
        ranges.dedup_by(|a, b| a.start_offset == b.start_offset && a.end_offset == b.end_offset);
        ranges
    }

    /// Collect folding ranges from AST nodes.
    fn collect_from_node(&self, node_idx: NodeIndex, ranges: &mut Vec<FoldingRange>) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SOURCE_FILE => {
                // TypeScript starts walk from sourceFile's children at full depth
                // (sourceFile itself isn't "walked")
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
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                if let Some(class) = self.arena.get_class(node) {
                    // Find the opening brace in the source text for the body span
                    if let Some(body_range) = self.find_brace_range(node) {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    for &member in &class.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                let block_range = self.get_line_range(node_idx);
                if block_range.0 < block_range.1 {
                    ranges.push(FoldingRange::new(
                        block_range.0,
                        block_range.1,
                        block_range.2,
                        block_range.3,
                    ));
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
                    if let Some(body_range) = self.find_brace_range(node) {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
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
                        ranges.push(FoldingRange::new(
                            alias_range.0,
                            alias_range.1,
                            alias_range.2,
                            alias_range.3,
                        ));
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    if let Some(body_range) = self.find_brace_range(node) {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    for &member in &enum_decl.members.nodes {
                        self.collect_from_node(member, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    if let Some(body_range) = self.find_brace_range(node) {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(module.body, ranges);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    let body_range = self.get_line_range(method.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
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
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(ctor.body, ranges);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(accessor.body, ranges);
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let body_range = self.get_line_range(accessor.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
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
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if let Some(func) = self.arena.get_function(node) {
                    let body_range = self.get_line_range(func.body);
                    if body_range.0 < body_range.1 {
                        ranges.push(FoldingRange::new(
                            body_range.0,
                            body_range.1,
                            body_range.2,
                            body_range.3,
                        ));
                    }
                    self.collect_from_node(func.body, ranges);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    // Fold the case block
                    let case_block_range = self.get_line_range(switch.case_block);
                    if case_block_range.0 < case_block_range.1 {
                        ranges.push(FoldingRange::new(
                            case_block_range.0,
                            case_block_range.1,
                            case_block_range.2,
                            case_block_range.3,
                        ));
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
                        ranges.push(FoldingRange::new(
                            clause_range.0,
                            clause_range.1,
                            clause_range.2,
                            clause_range.3,
                        ));
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
                    ranges.push(FoldingRange::new(
                        paren_range.0,
                        paren_range.1,
                        paren_range.2,
                        paren_range.3,
                    ));
                }
                for child in self.arena.get_children(node_idx) {
                    self.collect_from_node(child, ranges);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    // Fold multi-line argument lists (span from open to close paren)
                    if let Some(ref args) = call.arguments {
                        if !args.nodes.is_empty() {
                            // Use the node's own span which includes ( ... )
                            let call_range = self.get_line_range(node_idx);
                            if call_range.0 < call_range.1 {
                                ranges.push(FoldingRange::new(
                                    call_range.0,
                                    call_range.1,
                                    call_range.2,
                                    call_range.3,
                                ));
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
                    ranges.push(FoldingRange::new(range.0, range.1, range.2, range.3));
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                // Array literals: always report span (TypeScript reports even single-line ones)
                if let Some(data) = self.arena.get_literal_expr(node) {
                    let range = self.get_line_range(node_idx);
                    // Only skip truly empty ranges
                    if range.3 > range.2 {
                        ranges.push(FoldingRange::new(range.0, range.1, range.2, range.3));
                    }
                    for &elem in &data.elements.nodes {
                        self.collect_from_node(elem, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                // Object literals: always report span
                if let Some(data) = self.arena.get_literal_expr(node) {
                    let range = self.get_line_range(node_idx);
                    if range.3 > range.2 {
                        ranges.push(FoldingRange::new(range.0, range.1, range.2, range.3));
                    }
                    for &elem in &data.elements.nodes {
                        self.collect_from_node(elem, ranges);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                // Template expressions with substitutions (e.g. `hello ${name}`)
                let range = self.get_line_range(node_idx);
                if range.3 > range.2 {
                    ranges.push(FoldingRange::new(range.0, range.1, range.2, range.3));
                }
                // Don't recurse into template spans
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                // Tagged template expressions: span covers the template part
                if let Some(data) = self.arena.get_tagged_template(node) {
                    // The template child is the actual template literal
                    let template_range = self.get_line_range(data.template);
                    if template_range.3 > template_range.2 {
                        ranges.push(FoldingRange::new(
                            template_range.0,
                            template_range.1,
                            template_range.2,
                            template_range.3,
                        ));
                    }
                }
            }
            // NoSubstitutionTemplateLiteral (SyntaxKind = 15, token node)
            15 => {
                // Simple template literals without substitutions (e.g. `hello`)
                // Skip empty templates (just ``)
                if node.end.saturating_sub(node.pos) > 2 {
                    ranges.push(FoldingRange::new(
                        self.line_map
                            .offset_to_position(node.pos, self.source_text)
                            .line,
                        self.line_map
                            .offset_to_position(node.end.saturating_sub(1), self.source_text)
                            .line,
                        node.pos,
                        node.end,
                    ));
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
                    ranges.push(
                        FoldingRange::new(start_range.0, end_range.1, start_range.2, end_range.3)
                            .with_kind("imports"),
                    );
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
                    let so = self.line_start_offset(start_line);
                    let eo = self.line_end_offset(end_line);
                    ranges
                        .push(FoldingRange::new(start_line, end_line, so, eo).with_kind("comment"));
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
                    let so = self.line_start_offset(start_line);
                    let eo = self.line_end_offset(end_line);
                    ranges
                        .push(FoldingRange::new(start_line, end_line, so, eo).with_kind("comment"));
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
        let mut region_stack: Vec<(u32, u32)> = Vec::new(); // (line, byte offset of //)
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
                let line_start = self.line_start_offset(i as u32);
                // Find the position of "//" within the line
                let comment_offset = line.find("//").unwrap_or(0) as u32;
                let abs_offset = line_start + comment_offset;

                if delimiter.is_start {
                    region_stack.push((i as u32, abs_offset));
                } else if let Some((start_line, start_off)) = region_stack.pop() {
                    let end_line = i as u32;
                    let end_off = self.line_end_offset(end_line);
                    if end_line > start_line {
                        ranges.push(
                            FoldingRange::new(start_line, end_line, start_off, end_off)
                                .with_kind("region"),
                        );
                    }
                }
            }
        }
    }

    /// Get the line range and byte offsets for a node.
    /// Returns (start_line, end_line, start_offset, end_offset).
    fn get_line_range(&self, node_idx: NodeIndex) -> (u32, u32, u32, u32) {
        let Some(node) = self.arena.get(node_idx) else {
            return (0, 0, 0, 0);
        };
        let lo = node.pos;
        let hi = node.end;
        let start_pos = self.line_map.offset_to_position(lo, self.source_text);
        let end_pos = self
            .line_map
            .offset_to_position(hi.saturating_sub(1), self.source_text);
        (start_pos.line, end_pos.line, lo, hi)
    }

    /// Find the brace range for a declaration node (class, interface, enum, module).
    /// Scans the source text within the node's range to find `{` and uses that position
    /// (including leading whitespace, matching TypeScript's trivia handling) as the start.
    /// Returns (start_line, end_line, start_offset, end_offset).
    fn find_brace_range(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<(u32, u32, u32, u32)> {
        let bytes = self.source_text.as_bytes();
        let start = node.pos as usize;
        let end = node.end as usize;
        if end > bytes.len() {
            return None;
        }

        // Find the first '{' in the node's text
        let brace_pos = bytes[start..end].iter().position(|&b| b == b'{')?;
        let brace_offset = (start + brace_pos) as u32;

        // Include leading whitespace before '{' (matching TypeScript's pos with trivia)
        let mut span_start = brace_offset;
        if span_start > 0 && bytes[span_start as usize - 1] == b' ' {
            span_start -= 1;
        }

        let span_end = node.end;
        let start_pos = self
            .line_map
            .offset_to_position(span_start, self.source_text);
        let end_pos = self
            .line_map
            .offset_to_position(span_end.saturating_sub(1), self.source_text);
        Some((start_pos.line, end_pos.line, span_start, span_end))
    }

    /// Get a byte offset corresponding to the start of a given line.
    fn line_start_offset(&self, line: u32) -> u32 {
        self.line_map
            .position_to_offset(
                tsz_common::position::Position::new(line, 0),
                self.source_text,
            )
            .unwrap_or(0) as u32
    }

    /// Get a byte offset corresponding to the end of a given line (after newline).
    fn line_end_offset(&self, line: u32) -> u32 {
        let lines: Vec<&str> = self.source_text.lines().collect();
        let line_text = lines.get(line as usize).unwrap_or(&"");
        let start = self.line_start_offset(line);
        start + line_text.len() as u32
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
        Some(RegionDelimiter { is_start: false })
    } else if let Some(_rest) = after_slashes.strip_prefix("#region") {
        Some(RegionDelimiter { is_start: true })
    } else {
        None
    }
}

#[cfg(test)]
#[path = "../tests/folding_tests.rs"]
mod folding_tests;
