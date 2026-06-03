use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    fn jsx_tag_initializer(&self, tag_name_idx: NodeIndex) -> Option<NodeIndex> {
        let sym_id = self.resolve_identifier_symbol(tag_name_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let &decl_idx = symbol.declarations.first()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        self.ctx.arena.get(var_decl.initializer)?;
        Some(var_decl.initializer)
    }

    pub(in crate::checkers_domain::jsx) fn jsx_tag_is_logical_component_alias(
        &self,
        tag_name_idx: NodeIndex,
    ) -> bool {
        self.jsx_tag_initializer(tag_name_idx)
            .is_some_and(|init| self.jsx_expr_is_logical_choice(init))
    }

    pub(in crate::checkers_domain::jsx) fn jsx_tag_is_intrinsic_string_choice_alias(
        &self,
        tag_name_idx: NodeIndex,
    ) -> bool {
        self.jsx_tag_initializer(tag_name_idx)
            .is_some_and(|init| self.jsx_expr_is_intrinsic_string_choice(init))
    }

    fn jsx_expr_is_logical_choice(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        self.ctx.arena.get_conditional_expr(expr_node).is_some()
            || self
                .ctx
                .arena
                .get_binary_expr(expr_node)
                .is_some_and(|binary| {
                    matches!(
                        binary.operator_token,
                        x if x == SyntaxKind::BarBarToken as u16
                            || x == SyntaxKind::QuestionQuestionToken as u16
                    )
                })
    }

    fn jsx_expr_is_intrinsic_string_choice(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self.ctx.arena.get_literal(expr_node).is_some_and(|lit| {
                lit.text
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_lowercase)
            });
        }
        if let Some(cond) = self.ctx.arena.get_conditional_expr(expr_node) {
            return self.jsx_expr_is_intrinsic_string_choice(cond.when_true)
                && self.jsx_expr_is_intrinsic_string_choice(cond.when_false);
        }
        if let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) {
            return matches!(
                binary.operator_token,
                x if x == SyntaxKind::BarBarToken as u16
                    || x == SyntaxKind::QuestionQuestionToken as u16
            ) && self.jsx_expr_is_intrinsic_string_choice(binary.left)
                && self.jsx_expr_is_intrinsic_string_choice(binary.right);
        }
        false
    }
}
