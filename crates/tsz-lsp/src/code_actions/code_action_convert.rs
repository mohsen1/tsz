//! Convert between arrow functions and named functions.
//!
//! Provides two refactoring code actions:
//! - "Convert to arrow function": converts a function expression/declaration to an arrow function
//! - "Convert to named function": converts an arrow function to a named function declaration

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert a function expression or function declaration at the cursor to an arrow function.
    ///
    /// For function expressions assigned to a variable:
    ///   `const name = function(params): RetType { body }` -> `const name = (params): RetType => { body }`
    ///
    /// For function declarations:
    ///   `function name(params): RetType { body }` -> `const name = (params): RetType => { body }`
    pub fn convert_to_arrow_function(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        // Find the function node at cursor
        let func_idx = self.find_function_at_offset(root, start_offset)?;
        let func_node = self.arena.get(func_idx)?;

        // Must be a function expression or function declaration
        if func_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && func_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
        {
            return None;
        }

        let func_data = self.arena.get_function(func_node)?;

        // Don't convert generator functions — arrows can't be generators
        if func_data.asterisk_token {
            return None;
        }

        let is_declaration = func_node.kind == syntax_kind_ext::FUNCTION_DECLARATION;

        // Get the function name (if any)
        let func_name = if func_data.name.is_some() {
            self.arena.get_identifier_text(func_data.name)
        } else {
            None
        };

        // Build async prefix
        let async_prefix = if func_data.is_async { "async " } else { "" };

        // Get type parameters text
        let type_params_text = func_data
            .type_parameters
            .as_ref()
            .and_then(|tp| self.source.get(tp.pos as usize..tp.end as usize));

        // Get parameters text (the content between parentheses)
        let params = &func_data.parameters;
        let params_text = self.source.get(params.pos as usize..params.end as usize)?;

        // Get return type annotation text
        let return_type_text = if func_data.type_annotation.is_some() {
            let type_node = self.arena.get(func_data.type_annotation)?;
            self.source
                .get(type_node.pos as usize..type_node.end as usize)
        } else {
            None
        };

        // Get body text
        if func_data.body.is_none() {
            return None;
        }
        let body_node = self.arena.get(func_data.body)?;
        let body_text = self
            .source
            .get(body_node.pos as usize..body_node.end as usize)?;

        // Build the arrow function text
        let mut arrow_text = String::new();

        if is_declaration {
            // function name(params): RetType { body } -> const name = (params): RetType => { body }
            let name = func_name?; // declarations must have a name
            arrow_text.push_str("const ");
            arrow_text.push_str(name);
            arrow_text.push_str(" = ");
            arrow_text.push_str(async_prefix);
            if let Some(tp) = type_params_text {
                arrow_text.push_str(tp);
            }
            arrow_text.push('(');
            arrow_text.push_str(params_text);
            arrow_text.push(')');
            if let Some(rt) = return_type_text {
                arrow_text.push_str(": ");
                arrow_text.push_str(rt);
            }
            arrow_text.push_str(" => ");
            arrow_text.push_str(body_text);
        } else {
            // function expression: function(params) { body } -> (params) => { body }
            arrow_text.push_str(async_prefix);
            if let Some(tp) = type_params_text {
                arrow_text.push_str(tp);
            }
            arrow_text.push('(');
            arrow_text.push_str(params_text);
            arrow_text.push(')');
            if let Some(rt) = return_type_text {
                arrow_text.push_str(": ");
                arrow_text.push_str(rt);
            }
            arrow_text.push_str(" => ");
            arrow_text.push_str(body_text);
        }

        // Determine the replacement range
        let replace_start = self.line_map.offset_to_position(func_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(func_node.end, self.source);
        let replacement_range = Range::new(replace_start, replace_end);

        let edit = TextEdit {
            range: replacement_range,
            new_text: arrow_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to arrow function".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Convert an arrow function at the cursor to a named function declaration.
    ///
    /// `const name = (params): RetType => expr` -> `function name(params): RetType { return expr; }`
    /// `const name = (params): RetType => { body }` -> `function name(params): RetType { body }`
    pub fn convert_to_named_function(&self, root: NodeIndex, range: Range) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        // Find the arrow function node at cursor
        let arrow_idx = self.find_arrow_at_offset(root, start_offset)?;
        let arrow_node = self.arena.get(arrow_idx)?;

        if arrow_node.kind != syntax_kind_ext::ARROW_FUNCTION {
            return None;
        }

        let func_data = self.arena.get_function(arrow_node)?;

        // We need a parent variable declaration to get the name
        let ext = self.arena.get_extended(arrow_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;

        // The parent should be a variable declaration
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }

        let var_decl = self.arena.get_variable_declaration(parent_node)?;
        let func_name = self.arena.get_identifier_text(var_decl.name)?;

        // Build async prefix
        let async_prefix = if func_data.is_async { "async " } else { "" };

        // Get type parameters text
        let type_params_text = func_data
            .type_parameters
            .as_ref()
            .and_then(|tp| self.source.get(tp.pos as usize..tp.end as usize));

        // Get parameters text
        let params = &func_data.parameters;
        let params_text = self.source.get(params.pos as usize..params.end as usize)?;

        // Get return type annotation text
        let return_type_text = if func_data.type_annotation.is_some() {
            let type_node = self.arena.get(func_data.type_annotation)?;
            self.source
                .get(type_node.pos as usize..type_node.end as usize)
        } else {
            None
        };

        // Get the body
        if func_data.body.is_none() {
            return None;
        }
        let body_node = self.arena.get(func_data.body)?;
        let body_text = self
            .source
            .get(body_node.pos as usize..body_node.end as usize)?;

        // Determine body: if the body is a block, use it directly; otherwise wrap with return
        let is_block = body_node.kind == syntax_kind_ext::BLOCK;
        let body_output = if is_block {
            body_text.to_string()
        } else {
            format!("{{ return {body_text}; }}")
        };

        // Build the function declaration
        let mut func_text = String::new();
        func_text.push_str(async_prefix);
        func_text.push_str("function ");
        func_text.push_str(func_name);
        if let Some(tp) = type_params_text {
            func_text.push_str(tp);
        }
        func_text.push('(');
        func_text.push_str(params_text);
        func_text.push(')');
        if let Some(rt) = return_type_text {
            func_text.push_str(": ");
            func_text.push_str(rt);
        }
        func_text.push(' ');
        func_text.push_str(&body_output);

        // We need to replace the entire variable statement if possible,
        // otherwise just replace the variable declaration.
        // Walk up from variable declaration to find variable statement.
        let var_decl_ext = self.arena.get_extended(parent_idx)?;
        let grandparent_idx = var_decl_ext.parent;

        // Check if grandparent is a variable declaration list, and its parent is a variable statement
        let (replace_start_offset, replace_end_offset) = if grandparent_idx.is_some() {
            let gp_node = self.arena.get(grandparent_idx)?;
            if gp_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                // Check if there's only one declaration in the list
                let gp_ext = self.arena.get_extended(grandparent_idx)?;
                let ggp_idx = gp_ext.parent;
                if ggp_idx.is_some() {
                    let ggp_node = self.arena.get(ggp_idx)?;
                    if ggp_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        (ggp_node.pos, ggp_node.end)
                    } else {
                        (parent_node.pos, parent_node.end)
                    }
                } else {
                    (parent_node.pos, parent_node.end)
                }
            } else {
                (parent_node.pos, parent_node.end)
            }
        } else {
            (parent_node.pos, parent_node.end)
        };

        let replace_start = self
            .line_map
            .offset_to_position(replace_start_offset, self.source);
        let replace_end = self
            .line_map
            .offset_to_position(replace_end_offset, self.source);
        let replacement_range = Range::new(replace_start, replace_end);

        let edit = TextEdit {
            range: replacement_range,
            new_text: func_text,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to named function".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Find a function expression or function declaration at the given offset.
    fn find_function_at_offset(&self, _root: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Find an arrow function at the given offset.
    fn find_arrow_at_offset(&self, _root: NodeIndex, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::ARROW_FUNCTION {
                return Some(current);
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
        }
        None
    }

    /// Add braces to an arrow function with an expression body.
    ///
    /// `(x) => x * 2` → `(x) => { return x * 2; }`
    pub fn add_braces_to_arrow_function(
        &self,
        root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let arrow_idx = self.find_arrow_at_offset(root, start_offset)?;
        let arrow_node = self.arena.get(arrow_idx)?;

        if arrow_node.kind != syntax_kind_ext::ARROW_FUNCTION {
            return None;
        }

        let func_data = self.arena.get_function(arrow_node)?;
        if func_data.body.is_none() {
            return None;
        }

        let body_node = self.arena.get(func_data.body)?;

        // Only apply when body is NOT a block (i.e., expression body)
        if body_node.kind == syntax_kind_ext::BLOCK {
            return None;
        }

        let body_text = self
            .source
            .get(body_node.pos as usize..body_node.end as usize)?;

        let new_body = format!("{{ return {body_text}; }}");

        let replace_start = self.line_map.offset_to_position(body_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(body_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: new_body,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Add braces to arrow function".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Remove braces from an arrow function with a single return statement.
    ///
    /// `(x) => { return x * 2; }` → `(x) => x * 2`
    pub fn remove_braces_from_arrow_function(
        &self,
        root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let arrow_idx = self.find_arrow_at_offset(root, start_offset)?;
        let arrow_node = self.arena.get(arrow_idx)?;

        if arrow_node.kind != syntax_kind_ext::ARROW_FUNCTION {
            return None;
        }

        let func_data = self.arena.get_function(arrow_node)?;
        if func_data.body.is_none() {
            return None;
        }

        let body_node = self.arena.get(func_data.body)?;

        // Only apply when body IS a block
        if body_node.kind != syntax_kind_ext::BLOCK {
            return None;
        }

        let block_data = self.arena.get_block(body_node)?;

        // Must have exactly one statement, and it must be a return statement
        if block_data.statements.nodes.len() != 1 {
            return None;
        }

        let stmt_idx = block_data.statements.nodes[0];
        let stmt_node = self.arena.get(stmt_idx)?;

        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return None;
        }

        let return_data = self.arena.get_return_statement(stmt_node)?;
        if return_data.expression.is_none() {
            return None;
        }

        let expr_node = self.arena.get(return_data.expression)?;
        let expr_text = self
            .source
            .get(expr_node.pos as usize..expr_node.end as usize)?;

        let replace_start = self.line_map.offset_to_position(body_node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(body_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: expr_text.to_string(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Remove braces from arrow function".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }
}
