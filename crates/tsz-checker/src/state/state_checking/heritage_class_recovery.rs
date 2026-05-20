use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_class_heritage_reserved_leftmost_name(&mut self, expr_idx: NodeIndex) {
        let Some(leftmost_idx) = self.leftmost_identifier_of_property_access(expr_idx) else {
            return;
        };
        let Some(name_node) = self.ctx.arena.get(leftmost_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        if crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text) {
            let name = ident.escaped_text.clone();
            self.emit_strict_mode_reserved_word_error(leftmost_idx, &name, true);
        }
    }

    pub(crate) fn check_class_heritage_type_only_namespace_left(&mut self, expr_idx: NodeIndex) {
        let Some(leftmost_idx) = self.leftmost_identifier_of_qualified_heritage(expr_idx) else {
            return;
        };
        let Some(name_node) = self.ctx.arena.get(leftmost_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let name = ident.escaped_text.clone();
        let Some(sym_id) = self.resolve_identifier_symbol(leftmost_idx) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        use tsz_binder::symbol_flags;
        let has_namespace = symbol.has_any_flags(symbol_flags::MODULE);
        let has_type = symbol.has_any_flags(symbol_flags::TYPE);
        if has_type && !has_namespace {
            self.error_type_used_as_namespace_at(&name, leftmost_idx);
        }
    }

    fn leftmost_identifier_of_qualified_heritage(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        self.leftmost_identifier_of_property_access(expr_idx)
    }

    fn leftmost_identifier_of_property_access(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                return Some(current);
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.ctx.arena.get_access_expr(node)?;
                current = access.expression;
                continue;
            }
            return None;
        }
    }
}
