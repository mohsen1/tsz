//! Identifier-based property access fallbacks for namespace and const aliases.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn retry_property_access_from_const_identifier_initializer(
        &mut self,
        expression: NodeIndex,
        property_name: &str,
    ) -> Option<(
        TypeId,
        crate::query_boundaries::common::PropertyAccessResult,
    )> {
        let expr_idx = self.ctx.arena.skip_parenthesized(expression);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.escaped_name != ident.escaped_text || symbol.value_declaration.is_none() {
            return None;
        }
        let decl_idx = symbol.value_declaration;
        if !self.ctx.arena.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let initializer = decl.initializer;
        if initializer.is_none() {
            return None;
        }
        let init_type = self.get_type_of_node(initializer);
        if matches!(init_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(init_type);
        let resolved = self.resolve_type_for_property_access(evaluated);
        let retry = self.resolve_property_access_with_env(resolved, property_name);
        match retry {
            crate::query_boundaries::common::PropertyAccessResult::Success { .. }
            | crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(_),
                ..
            } => Some((resolved, retry)),
            _ => None,
        }
    }

    pub(crate) fn same_file_namespace_value_member_for_identifier(
        &mut self,
        expression: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized(expression);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let resolved_sym_id = self
            .resolve_identifier_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))?;
        let resolved_symbol = self.ctx.binder.get_symbol(resolved_sym_id)?;
        if !resolved_symbol.has_any_flags(symbol_flags::MODULE) {
            return None;
        }
        let candidates: Vec<_> = self
            .ctx
            .binder
            .symbols
            .find_all_by_name(ident.escaped_text.as_str())
            .to_vec();
        for sym_id in candidates {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            if !symbol.has_any_flags(symbol_flags::MODULE) {
                continue;
            }
            let Some(exports) = symbol.exports.as_ref() else {
                continue;
            };
            let Some(member_sym_id) = exports.get(property_name) else {
                continue;
            };
            let Some(member_symbol) = self.ctx.binder.get_symbol(member_sym_id) else {
                continue;
            };
            if member_symbol.is_type_only
                || !member_symbol.is_exported
                || !member_symbol.has_any_flags(symbol_flags::VALUE)
            {
                continue;
            }
            let member_type = self.get_type_of_symbol(member_sym_id);
            if !matches!(member_type, TypeId::UNKNOWN | TypeId::ERROR) {
                return Some(member_type);
            }
        }
        None
    }
}
