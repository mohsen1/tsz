//! Polymorphic-`this` assignment source display helpers.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn raw_this_property_assignment_rhs_display(
        &mut self,
        right_idx: NodeIndex,
    ) -> Option<String> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        if self
            .ctx
            .arena
            .get(access.expression)
            .is_none_or(|node| node.kind != SyntaxKind::ThisKeyword as u16)
        {
            return None;
        }
        if let Some(display) =
            self.this_property_initializer_new_expression_display(access.name_or_argument)
        {
            return Some(display);
        }
        let name = self
            .ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.ctx.arena.get_identifier(node))?
            .escaped_text
            .clone();
        let name = self.ctx.types.intern_string(&name);
        let raw = crate::query_boundaries::property_access::resolve_property_access_raw_this(
            self.ctx.types,
            self.current_this_type()?,
            name,
        );
        let crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } = raw
        else {
            return None;
        };
        (!matches!(type_id, TypeId::ERROR | TypeId::UNKNOWN)
            && !crate::query_boundaries::common::is_this_type(self.ctx.types, type_id))
        .then(|| self.format_type_for_assignability_message(type_id))
    }

    fn this_property_initializer_new_expression_display(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        if let Some(display) =
            self.current_class_property_initializer_new_expression_display(name_idx)
        {
            return Some(display);
        }
        let sym_id = self.ctx.binder.get_node_symbol(name_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let prop = self.ctx.arena.get_property_decl(decl_node)?;
        self.property_initializer_new_expression_display(prop.initializer)
    }

    fn current_class_property_initializer_new_expression_display(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let property_name = self
            .ctx
            .arena
            .get(name_idx)
            .and_then(|node| self.ctx.arena.get_identifier(node))?
            .escaped_text
            .as_str();
        let class_idx = self.ctx.enclosing_class.as_ref()?.class_idx;
        let class = self
            .ctx
            .arena
            .get(class_idx)
            .and_then(|node| self.ctx.arena.get_class(node))?;
        for &member_idx in &class.members.nodes {
            let Some(prop) = self
                .ctx
                .arena
                .get(member_idx)
                .and_then(|node| self.ctx.arena.get_property_decl(node))
            else {
                continue;
            };
            let Some(member_name) = self
                .ctx
                .arena
                .get(prop.name)
                .and_then(|node| self.ctx.arena.get_identifier(node))
                .map(|ident| ident.escaped_text.as_str())
            else {
                continue;
            };
            if member_name == property_name
                && let Some(display) =
                    self.property_initializer_new_expression_display(prop.initializer)
            {
                return Some(display);
            }
        }
        None
    }

    fn property_initializer_new_expression_display(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer);
        let init_node = self.ctx.arena.get(init_idx)?;
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }
        let new_expr = self.ctx.arena.get_call_expr(init_node)?;
        let ctor_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(new_expr.expression);
        self.expression_text(ctor_idx)
    }
}
