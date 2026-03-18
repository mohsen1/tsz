//! Convert between template strings and string concatenation.
//!
//! Provides two refactoring code actions:
//! - "Convert to template string": converts string concatenation to a template literal
//! - "Convert to string concatenation": converts a template literal to string concatenation

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert a string concatenation expression to a template literal.
    ///
    /// `"hello " + name + "!"` → `` `hello ${name}!` ``
    pub fn convert_to_template_string(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let binary_idx = self.find_plus_concatenation_at_offset(start_offset)?;

        // Collect all parts of the concatenation chain
        let parts = self.collect_concat_parts(binary_idx);
        if parts.len() < 2 {
            return None;
        }

        // At least one part must be a string literal
        let has_string = parts
            .iter()
            .any(|p| matches!(p, ConcatPart::StringLiteral(_)));
        if !has_string {
            return None;
        }

        // Build the template literal
        let mut template = String::from('`');
        for part in &parts {
            match part {
                ConcatPart::StringLiteral(s) => {
                    // Escape backticks and dollar signs in the string content
                    let escaped = s
                        .replace('\\', "\\\\")
                        .replace('`', "\\`")
                        .replace("${", "\\${");
                    template.push_str(&escaped);
                }
                ConcatPart::Expression(expr_text) => {
                    template.push_str("${");
                    template.push_str(expr_text);
                    template.push('}');
                }
            }
        }
        template.push('`');

        // Find the outermost binary expression in the concatenation chain
        let outer_idx = self.find_outermost_plus_chain(binary_idx);
        let outer_node = self.arena.get(outer_idx)?;

        let replace_start = self
            .line_map
            .offset_to_position(outer_node.pos, self.source);
        let replace_end = self
            .line_map
            .offset_to_position(outer_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: template,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to template string".to_string(),
            kind: CodeActionKind::RefactorRewrite,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Convert a template literal to string concatenation.
    ///
    /// `` `hello ${name}!` `` → `"hello " + name + "!"`
    pub fn convert_to_string_concatenation(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let template_idx = self.find_template_at_offset(start_offset)?;
        let template_node = self.arena.get(template_idx)?;

        let result = if template_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
            // Simple template with no substitutions: `hello` → "hello"
            let text = self
                .source
                .get((template_node.pos + 1) as usize..(template_node.end - 1) as usize)?;
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        } else if template_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            let template_data = self.arena.get_template_expr(template_node)?;
            let mut parts: Vec<String> = Vec::new();

            // Process head
            let head_node = self.arena.get(template_data.head)?;
            let head_text = self
                .source
                .get((head_node.pos + 1) as usize..(head_node.end - 2) as usize)?;
            if !head_text.is_empty() {
                let escaped = head_text.replace('"', "\\\"");
                parts.push(format!("\"{escaped}\""));
            }

            // Process spans
            for &span_idx in &template_data.template_spans.nodes {
                let span_node = self.arena.get(span_idx)?;
                let span_data = self.arena.get_template_span(span_node)?;

                // Expression part
                let expr_node = self.arena.get(span_data.expression)?;
                let expr_text = self
                    .source
                    .get(expr_node.pos as usize..expr_node.end as usize)?;
                parts.push(expr_text.to_string());

                // Literal part (middle or tail)
                let lit_node = self.arena.get(span_data.literal)?;
                let lit_end_offset = if lit_node.kind == SyntaxKind::TemplateTail as u16 {
                    1 // backtick
                } else {
                    2 // ${
                };
                let lit_text = self
                    .source
                    .get((lit_node.pos + 1) as usize..(lit_node.end - lit_end_offset) as usize)?;
                if !lit_text.is_empty() {
                    let escaped = lit_text.replace('"', "\\\"");
                    parts.push(format!("\"{escaped}\""));
                }
            }

            if parts.is_empty() {
                "\"\"".to_string()
            } else {
                parts.join(" + ")
            }
        } else {
            return None;
        };

        let replace_start = self
            .line_map
            .offset_to_position(template_node.pos, self.source);
        let replace_end = self
            .line_map
            .offset_to_position(template_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: result,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to string concatenation".to_string(),
            kind: CodeActionKind::RefactorRewrite,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Find a binary expression with `+` operator at the given offset.
    fn find_plus_concatenation_at_offset(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.arena.get_binary_expr(node)?;
                if binary.operator_token == SyntaxKind::PlusToken as u16 {
                    return Some(current);
                }
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    /// Find the outermost `+` binary expression in a chain.
    fn find_outermost_plus_chain(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                return current;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return current;
            }
            let Some(parent_node) = self.arena.get(parent) else {
                return current;
            };
            if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return current;
            }
            let Some(binary) = self.arena.get_binary_expr(parent_node) else {
                return current;
            };
            if binary.operator_token != SyntaxKind::PlusToken as u16 {
                return current;
            }
            current = parent;
        }
    }

    /// Collect all parts of a `+` concatenation chain.
    fn collect_concat_parts(&self, idx: NodeIndex) -> Vec<ConcatPart> {
        let outer = self.find_outermost_plus_chain(idx);
        let mut parts = Vec::new();
        self.flatten_concat(outer, &mut parts);
        parts
    }

    fn flatten_concat(&self, idx: NodeIndex, parts: &mut Vec<ConcatPart>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(node) {
                if binary.operator_token == SyntaxKind::PlusToken as u16 {
                    self.flatten_concat(binary.left, parts);
                    self.flatten_concat(binary.right, parts);
                    return;
                }
            }
        }

        // Check if this is a string literal
        if node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(text) = self
                .source
                .get((node.pos + 1) as usize..(node.end - 1) as usize)
            {
                parts.push(ConcatPart::StringLiteral(text.to_string()));
                return;
            }
        }

        // Otherwise it's an expression
        if let Some(text) = self.source.get(node.pos as usize..node.end as usize) {
            parts.push(ConcatPart::Expression(text.to_string()));
        }
    }

    /// Find a template expression or no-substitution template literal at offset.
    fn find_template_at_offset(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
                || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }
}

enum ConcatPart {
    StringLiteral(String),
    Expression(String),
}
