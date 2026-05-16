//! Checked-JS global element-access fallback assignment diagnostics.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn is_checked_js_global_element_access_fallback_assignment(
        &self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> bool {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }

        let Some(left_key) = self.literal_element_access_key(left_idx) else {
            return false;
        };
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return false;
        };
        let Some(left_access) = self.ctx.arena.get_access_expr(left_node) else {
            return false;
        };
        if !self.is_global_this_like_expression(left_access.expression) {
            return false;
        }

        let right_idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        let Some(right_node) = self.ctx.arena.get(right_idx) else {
            return false;
        };
        if right_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(right_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return false;
        }

        let fallback_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(binary.right);
        if self
            .ctx
            .arena
            .get(fallback_idx)
            .is_none_or(|node| node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            return false;
        }

        let Some(rhs_key) = self.literal_element_access_key(binary.left) else {
            return false;
        };
        if rhs_key != left_key {
            return false;
        }
        let Some(rhs_left_node) = self.ctx.arena.get(binary.left) else {
            return false;
        };
        let Some(rhs_left_access) = self.ctx.arena.get_access_expr(rhs_left_node) else {
            return false;
        };
        self.is_global_this_like_expression(rhs_left_access.expression)
    }

    pub(super) fn relocate_js_global_element_access_fallback_diagnostics(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        diag_count_before: usize,
    ) {
        let right_idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        let Some(right_node) = self.ctx.arena.get(right_idx) else {
            return;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(right_node) else {
            return;
        };
        let fallback_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(binary.right);
        let Some(fallback_node) = self.ctx.arena.get(fallback_idx) else {
            return;
        };

        let anchor_idx = self.resolve_diagnostic_anchor_node(
            left_idx,
            crate::error_reporter::DiagnosticAnchorKind::RewriteAssignment,
        );
        let Some(anchor) = self.resolve_diagnostic_anchor(
            anchor_idx,
            crate::error_reporter::DiagnosticAnchorKind::Exact,
        ) else {
            return;
        };
        let target_name = self.literal_element_access_key(left_idx).filter(|name| {
            name.chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        });

        for diag in &mut self.ctx.diagnostics[diag_count_before..] {
            if matches!(
                diag.code,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            ) && diag.start >= fallback_node.pos
                && diag.start < fallback_node.end
            {
                diag.start = anchor.start;
                diag.length = anchor.length;
                if let Some(name) = target_name.as_deref() {
                    let bare = format!("required in type '{name}'");
                    let qualified = format!("required in type 'typeof {name}'");
                    diag.message_text = diag.message_text.replace(&bare, &qualified);
                }
            }
        }
    }

    fn literal_element_access_key(&self, idx: NodeIndex) -> Option<String> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let key_node = self.ctx.arena.get(access.name_or_argument)?;
        if key_node.kind == SyntaxKind::StringLiteral as u16
            || key_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self
                .ctx
                .arena
                .get_literal(key_node)
                .map(|lit| lit.text.clone());
        }
        None
    }
}
