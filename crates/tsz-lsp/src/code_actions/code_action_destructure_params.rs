//! Convert parameters to destructured object.
//!
//! `function foo(a: string, b: number)` → `function foo({ a, b }: { a: string; b: number })`

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::syntax_kind_ext;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert function parameters to a destructured object parameter.
    pub fn convert_params_to_destructured(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;

        // Find function at cursor
        let func_idx = self.find_any_function_at_offset(start_offset)?;
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        let params = &func_data.parameters;
        if params.nodes.len() < 2 {
            return None; // Need at least 2 params to make destructuring worthwhile
        }

        // Collect parameter info
        let mut param_names = Vec::new();
        let mut type_members = Vec::new();

        for &param_idx in &params.nodes {
            let param_node = self.arena.get(param_idx)?;
            let param_data = self.arena.get_parameter(param_node)?;

            // Skip rest parameters
            if param_data.dot_dot_dot_token {
                return None;
            }

            let name = self.arena.get_identifier_text(param_data.name)?;
            let optional = if param_data.question_token { "?" } else { "" };

            let type_text = if param_data.type_annotation.is_some() {
                let type_node = self.arena.get(param_data.type_annotation)?;
                self.source
                    .get(type_node.pos as usize..type_node.end as usize)?
                    .to_string()
            } else {
                "any".to_string()
            };

            param_names.push(name.to_string());
            type_members.push(format!("{name}{optional}: {type_text}"));
        }

        // Build the destructured parameter
        let names_text = param_names.join(", ");
        let type_text = type_members.join("; ");
        let new_params = format!("{{ {names_text} }}: {{ {type_text} }}");

        // Replace the parameters
        let params_start = self.line_map.offset_to_position(params.pos, self.source);
        let params_end = self.line_map.offset_to_position(params.end, self.source);

        let edit = TextEdit {
            range: Range::new(params_start, params_end),
            new_text: new_params,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert parameters to destructured object".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Find any function (declaration, expression, arrow, method) at offset.
    fn find_any_function_at_offset(&self, offset: u32) -> Option<NodeIndex> {
        let mut current = find_node_at_offset(self.arena, offset);
        while current.is_some() {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.is_function_expression_or_arrow()
                || node.kind == syntax_kind_ext::METHOD_DECLARATION
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }
}
