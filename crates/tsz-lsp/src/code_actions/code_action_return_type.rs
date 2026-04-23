//! Add explicit return type annotation to a function.
//!
//! When cursor is on a function without an explicit return type, offer to add one
//! based on the inferred type. Since we operate at the AST level without full type
//! inference, we analyze the return statements to suggest a type.

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Add explicit return type annotation to a function.
    pub fn add_return_type(&self, _root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        let func_idx = self.find_func_without_return_type(start_offset)?;
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        // Already has a return type
        if func_data.type_annotation.is_some() {
            return None;
        }

        // Infer the return type from the body
        let return_type = if func_data.body.is_none() {
            return None;
        } else {
            let body_node = self.arena.get(func_data.body)?;
            if func_node.kind == syntax_kind_ext::ARROW_FUNCTION
                && body_node.kind != syntax_kind_ext::BLOCK
            {
                // Arrow with expression body — infer from expression
                self.infer_expression_type(func_data.body)
            } else {
                // Block body — collect return statements
                let return_types = self.collect_return_types(func_data.body);
                if return_types.is_empty() {
                    "void".to_string()
                } else if return_types.len() == 1 {
                    return_types.into_iter().next().unwrap()
                } else {
                    // Union of return types
                    let unique: Vec<String> = return_types
                        .into_iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    unique.join(" | ")
                }
            }
        };

        // Find position: after close paren of parameters
        let insert_offset = func_data.parameters.end;

        // Check for close paren after parameters
        let mut actual_insert = insert_offset;
        if let Some(rest) = self.source.get(insert_offset as usize..)
            && rest.starts_with(')')
        {
            actual_insert += 1;
        }

        let insert_pos = self.line_map.offset_to_position(actual_insert, self.source);
        let edit = TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text: format!(": {return_type}"),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: format!("Add return type '{return_type}'"),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    fn find_func_without_return_type(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION =>
                {
                    let func = self.arena.get_function(node)?;
                    if func.type_annotation.is_none() {
                        return Some(current);
                    }
                    return None;
                }
                _ => {}
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    /// Infer a simple type from an expression node.
    fn infer_expression_type(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return "unknown".to_string();
        };

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                "string".to_string()
            }
            k if k == SyntaxKind::NumericLiteral as u16 => "number".to_string(),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                "boolean".to_string()
            }
            k if k == SyntaxKind::NullKeyword as u16 => "null".to_string(),
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => "string".to_string(),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => "object".to_string(),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => "unknown[]".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Collect inferred return types from return statements in a block.
    fn collect_return_types(&self, block_idx: NodeIndex) -> Vec<String> {
        let mut types = Vec::new();
        self.visit_returns(block_idx, &mut types);
        types
    }

    fn visit_returns(&self, idx: NodeIndex, types: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            if let Some(return_data) = self.arena.get_return_statement(node) {
                if return_data.expression.is_some() {
                    types.push(self.infer_expression_type(return_data.expression));
                } else {
                    types.push("void".to_string());
                }
            }
            return;
        }

        // Don't descend into nested functions
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.is_function_expression_or_arrow()
        {
            return;
        }

        for child_idx in self.arena.get_children(idx) {
            self.visit_returns(child_idx, types);
        }
    }
}
