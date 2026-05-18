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
        let target_display =
            self.checked_js_global_element_access_fallback_target_display(fallback_idx);

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
                if let Some(display) = target_display.as_deref()
                    && let Some(name) = display.strip_prefix("typeof ")
                {
                    let bare = format!("required in type '{name}'");
                    let qualified = format!("required in type '{display}'");
                    diag.message_text = diag.message_text.replace(&bare, &qualified);
                }
            }
        }
    }

    pub(crate) fn checked_js_global_element_access_fallback_target_display(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let target_name = self
            .checked_js_global_element_access_fallback_target_name(idx)
            .filter(|name| {
                name.chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            })?;
        Some(format!("typeof {target_name}"))
    }

    fn checked_js_global_element_access_fallback_target_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return None;
        }

        let (assignment_idx, binary_idx, fallback_idx) =
            self.global_element_access_fallback_assignment_parts(idx)?;
        let assignment_node = self.ctx.arena.get(assignment_idx)?;
        let assignment = self.ctx.arena.get_binary_expr(assignment_node)?;
        if assignment.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        if self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(assignment.right)
            != binary_idx
        {
            return None;
        }

        let binary_node = self.ctx.arena.get(binary_idx)?;
        let binary = self.ctx.arena.get_binary_expr(binary_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }
        if self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(binary.right)
            != fallback_idx
        {
            return None;
        }

        let left_key = self.literal_element_access_key(assignment.left)?;
        let rhs_key = self.literal_element_access_key(binary.left)?;
        if left_key != rhs_key {
            return None;
        }

        let left_access = self
            .ctx
            .arena
            .get(assignment.left)
            .and_then(|node| self.ctx.arena.get_access_expr(node))?;
        let rhs_access = self
            .ctx
            .arena
            .get(binary.left)
            .and_then(|node| self.ctx.arena.get_access_expr(node))?;
        if !self.is_global_this_like_expression(left_access.expression)
            || !self.is_global_this_like_expression(rhs_access.expression)
        {
            return None;
        }

        Some(left_key)
    }

    fn global_element_access_fallback_assignment_parts(
        &self,
        idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex, NodeIndex)> {
        let current = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(current)?;
        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let binary_idx = self.parent_skipping_expression_wrappers(current)?;
            let assignment_idx = self.parent_skipping_expression_wrappers(binary_idx)?;
            return Some((assignment_idx, binary_idx, current));
        }
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = self.ctx.arena.get_binary_expr(node)?;
            if binary.operator_token == SyntaxKind::BarBarToken as u16 {
                let fallback_idx = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(binary.right);
                if self.ctx.arena.get(fallback_idx).is_some_and(|fallback| {
                    fallback.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                }) {
                    let assignment_idx = self.parent_skipping_expression_wrappers(current)?;
                    return Some((assignment_idx, current, fallback_idx));
                }
            }
            if binary.operator_token == SyntaxKind::EqualsToken as u16 {
                let rhs_idx = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(binary.right);
                let rhs_node = self.ctx.arena.get(rhs_idx)?;
                let rhs_binary = self.ctx.arena.get_binary_expr(rhs_node)?;
                if rhs_binary.operator_token == SyntaxKind::BarBarToken as u16 {
                    let fallback_idx = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(rhs_binary.right);
                    if self.ctx.arena.get(fallback_idx).is_some_and(|fallback| {
                        fallback.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    }) {
                        return Some((current, rhs_idx, fallback_idx));
                    }
                }
            }
        }
        let fallback_idx = self.enclosing_object_literal(current)?;
        let binary_idx = self.parent_skipping_expression_wrappers(fallback_idx)?;
        let assignment_idx = self.parent_skipping_expression_wrappers(binary_idx)?;
        Some((assignment_idx, binary_idx, fallback_idx))
    }

    fn enclosing_object_literal(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        for _ in 0..32 {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(current);
            }
            let parent = self.ctx.arena.parent_of(current)?;
            if parent.is_none() {
                return None;
            }
            current = parent;
        }
        None
    }

    fn parent_skipping_expression_wrappers(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..8 {
            let parent = self.ctx.arena.parent_of(current)?;
            if parent.is_none() {
                return None;
            }
            let node = self.ctx.arena.get(parent)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION
                || node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            {
                current = parent;
                continue;
            }
            return Some(parent);
        }
        None
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
