//! Convert to nullish coalescing operator.
//!
//! Provides refactoring to convert:
//! - `a !== null && a !== undefined ? a : defaultVal` → `a ?? defaultVal`
//! - `a || defaultVal` → `a ?? defaultVal`

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert `a !== null && a !== undefined ? a : defaultVal` → `a ?? defaultVal`
    /// Convert `a || defaultVal` → `a ?? defaultVal`
    pub fn convert_to_nullish_coalescing(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let mut current = find_node_at_offset(self.arena, start_offset);

        while current.is_some() {
            let node = self.arena.get(current)?;

            // Try ternary pattern: a !== null && a !== undefined ? a : defaultVal
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
                if let Some(result) = self.try_convert_ternary_to_nullish(current) {
                    return Some(result);
                }
            }

            // Try || pattern: a || defaultVal
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                if let Some(result) = self.try_convert_or_to_nullish(current) {
                    return Some(result);
                }
            }

            current = self.arena.get_extended(current)?.parent;
        }

        None
    }

    /// Convert `a !== null && a !== undefined ? a : default` → `a ?? default`
    fn try_convert_ternary_to_nullish(&self, idx: NodeIndex) -> Option<CodeAction> {
        let node = self.arena.get(idx)?;
        let cond_data = self.arena.get_conditional_expr(node)?;

        let cond_node = self.arena.get(cond_data.condition)?;

        // Simple case: `a != null ? a : default` (loose equality covers both null and undefined)
        if cond_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = self.arena.get_binary_expr(cond_node)?;

            let is_not_equal = binary.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                || binary.operator_token == SyntaxKind::ExclamationEqualsEqualsToken as u16;

            if is_not_equal {
                let right_node = self.arena.get(binary.right)?;
                let right_text = self
                    .source
                    .get(right_node.pos as usize..right_node.end as usize)?;
                if right_text == "null" || right_text == "undefined" {
                    let left_node = self.arena.get(binary.left)?;
                    let checked = self
                        .source
                        .get(left_node.pos as usize..left_node.end as usize)?;

                    let when_true_node = self.arena.get(cond_data.when_true)?;
                    let when_true = self
                        .source
                        .get(when_true_node.pos as usize..when_true_node.end as usize)?;

                    // when_true should be the same as the checked expression
                    if checked == when_true {
                        let when_false_node = self.arena.get(cond_data.when_false)?;
                        let default_val = self
                            .source
                            .get(when_false_node.pos as usize..when_false_node.end as usize)?;

                        let result = format!("{checked} ?? {default_val}");
                        return self.build_nullish_action(idx, &result);
                    }
                }
            }
        }

        None
    }

    /// Convert `a || defaultVal` → `a ?? defaultVal`
    fn try_convert_or_to_nullish(&self, idx: NodeIndex) -> Option<CodeAction> {
        let node = self.arena.get(idx)?;
        let binary = self.arena.get_binary_expr(node)?;

        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }

        let left_node = self.arena.get(binary.left)?;
        let left_text = self
            .source
            .get(left_node.pos as usize..left_node.end as usize)?;

        let right_node = self.arena.get(binary.right)?;
        let right_text = self
            .source
            .get(right_node.pos as usize..right_node.end as usize)?;

        let result = format!("{left_text} ?? {right_text}");
        self.build_nullish_action(idx, &result)
    }

    fn build_nullish_action(&self, idx: NodeIndex, new_text: &str) -> Option<CodeAction> {
        let node = self.arena.get(idx)?;
        let replace_start = self.line_map.offset_to_position(node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: new_text.to_string(),
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to nullish coalescing".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }
}
