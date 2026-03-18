//! Convert to optional chaining.
//!
//! Provides refactoring to convert:
//! - `a && a.b && a.b.c` → `a?.b?.c`
//! - `a != null ? a.b : undefined` → `a?.b`

use crate::rename::{TextEdit, WorkspaceEdit};
use crate::utils::find_node_at_offset;
use rustc_hash::FxHashMap;
use tsz_parser::NodeIndex;
use tsz_parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::code_action_provider::{CodeAction, CodeActionKind, CodeActionProvider};
use tsz_common::position::Range;

impl<'a> CodeActionProvider<'a> {
    /// Convert `a && a.b && a.b.c` to `a?.b?.c`
    /// Convert `a != null ? a.b : undefined` to `a?.b`
    pub fn convert_to_optional_chaining(
        &self,
        _root: NodeIndex,
        range: Range,
    ) -> Option<CodeAction> {
        let start_offset = self.line_map.position_to_offset(range.start, self.source)?;
        let mut current = find_node_at_offset(self.arena, start_offset);

        // Walk up to find a suitable expression
        while current.is_some() {
            let node = self.arena.get(current)?;

            // Try && chain pattern: a && a.b && a.b.c
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                if let Some(result) = self.try_convert_and_chain_to_optional(current) {
                    return Some(result);
                }
            }

            // Try ternary pattern: a != null ? a.b : undefined
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
                if let Some(result) = self.try_convert_ternary_to_optional(current) {
                    return Some(result);
                }
            }

            current = self.arena.get_extended(current)?.parent;
        }

        None
    }

    /// Try to convert `a && a.b && a.b.c` → `a?.b?.c`
    fn try_convert_and_chain_to_optional(&self, idx: NodeIndex) -> Option<CodeAction> {
        // Collect all && operands
        let parts = self.collect_and_chain_parts(idx);
        if parts.len() < 2 {
            return None;
        }

        // Verify the chain pattern: each part must be a prefix of the next
        let part_texts: Vec<&str> = parts
            .iter()
            .filter_map(|&idx| {
                let node = self.arena.get(idx)?;
                self.source.get(node.pos as usize..node.end as usize)
            })
            .collect();

        if part_texts.len() < 2 {
            return None;
        }

        // Check that each part is a property access extending the previous
        for i in 1..part_texts.len() {
            let prev = part_texts[i - 1];
            if !part_texts[i].starts_with(prev) {
                return None;
            }
        }

        // Build optional chaining expression from the last (most complete) access
        let last_text = part_texts.last()?;
        let first_text = part_texts[0];

        // Replace each `.` after a chain segment with `?.`
        let mut result = first_text.to_string();
        for i in 1..part_texts.len() {
            let suffix = part_texts[i].strip_prefix(part_texts[i - 1])?;
            let suffix = suffix.strip_prefix('.')?;
            result.push_str("?.");
            result.push_str(suffix);
        }

        // If there's more after the last chain part in the original expression
        if let Some(remainder) = last_text.strip_prefix(part_texts[part_texts.len() - 1]) {
            result.push_str(remainder);
        }

        let outer_node = self.arena.get(idx)?;
        let replace_start = self
            .line_map
            .offset_to_position(outer_node.pos, self.source);
        let replace_end = self
            .line_map
            .offset_to_position(outer_node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: result,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to optional chaining".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Try to convert `a != null ? a.b : undefined` → `a?.b`
    fn try_convert_ternary_to_optional(&self, idx: NodeIndex) -> Option<CodeAction> {
        let node = self.arena.get(idx)?;
        let cond_data = self.arena.get_conditional_expr(node)?;

        // Check condition is `a != null` or `a !== null` or `a !== undefined` or `a != undefined`
        let cond_node = self.arena.get(cond_data.condition)?;
        if cond_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(cond_node)?;

        let is_not_equal = binary.operator_token == SyntaxKind::ExclamationEqualsToken as u16
            || binary.operator_token == SyntaxKind::ExclamationEqualsEqualsToken as u16;
        if !is_not_equal {
            return None;
        }

        // Check right side is null or undefined
        let right_node = self.arena.get(binary.right)?;
        let right_text = self
            .source
            .get(right_node.pos as usize..right_node.end as usize)?;
        if right_text != "null" && right_text != "undefined" {
            return None;
        }

        // Get the checked variable text
        let left_node = self.arena.get(binary.left)?;
        let checked_text = self
            .source
            .get(left_node.pos as usize..left_node.end as usize)?;

        // Check when_true is `a.something`
        let when_true_node = self.arena.get(cond_data.when_true)?;
        let when_true_text = self
            .source
            .get(when_true_node.pos as usize..when_true_node.end as usize)?;

        if !when_true_text.starts_with(checked_text) {
            return None;
        }

        // Check when_false is undefined
        let when_false_node = self.arena.get(cond_data.when_false)?;
        let when_false_text = self
            .source
            .get(when_false_node.pos as usize..when_false_node.end as usize)?;
        if when_false_text != "undefined" {
            return None;
        }

        // Build optional chaining
        let suffix = &when_true_text[checked_text.len()..];
        let suffix = suffix.strip_prefix('.')?;
        let result = format!("{checked_text}?.{suffix}");

        let replace_start = self.line_map.offset_to_position(node.pos, self.source);
        let replace_end = self.line_map.offset_to_position(node.end, self.source);

        let edit = TextEdit {
            range: Range::new(replace_start, replace_end),
            new_text: result,
        };

        let mut changes = FxHashMap::default();
        changes.insert(self.file_name.clone(), vec![edit]);

        Some(CodeAction {
            title: "Convert to optional chaining".to_string(),
            kind: CodeActionKind::Refactor,
            edit: Some(WorkspaceEdit { changes }),
            is_preferred: false,
            data: None,
        })
    }

    /// Collect all operands from an `&&` chain.
    fn collect_and_chain_parts(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        let mut parts = Vec::new();
        self.flatten_and_chain(idx, &mut parts);
        parts
    }

    fn flatten_and_chain(&self, idx: NodeIndex, parts: &mut Vec<NodeIndex>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = self.arena.get_binary_expr(node) {
                if binary.operator_token == SyntaxKind::AmpersandAmpersandToken as u16 {
                    self.flatten_and_chain(binary.left, parts);
                    self.flatten_and_chain(binary.right, parts);
                    return;
                }
            }
        }

        parts.push(idx);
    }
}
