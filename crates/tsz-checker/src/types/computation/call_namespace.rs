//! Namespace helpers for call expression diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn same_file_namespace_value_type_for_call(
        &mut self,
        callee_expr: NodeIndex,
    ) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized(callee_expr);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let candidates = self
            .ctx
            .binder
            .symbols
            .find_all_by_name(ident.escaped_text.as_str())
            .to_vec();
        for sym_id in candidates {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            if !symbol.has_any_flags(tsz_binder::symbol_flags::MODULE) {
                continue;
            }
            let namespace_type = self.get_type_of_symbol(sym_id);
            if !matches!(
                namespace_type,
                TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR | TypeId::UNDEFINED
            ) {
                return Some(namespace_type);
            }
        }
        None
    }
}
