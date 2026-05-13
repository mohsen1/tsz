//! Namespace-value helpers used by call-result diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn callee_identifier_has_local_instantiated_namespace_value(
        &self,
        callee_expr: NodeIndex,
    ) -> bool {
        let Some(ident) = self.ctx.arena.get_identifier_at(callee_expr) else {
            return false;
        };
        let Some(binder) = self.ctx.get_binder_for_file(self.ctx.current_file_idx) else {
            return false;
        };
        binder
            .get_symbols()
            .find_all_by_name(&ident.escaped_text)
            .iter()
            .any(|&sym_id| {
                let Some(symbol) = binder.get_symbol(sym_id) else {
                    return false;
                };
                symbol.escaped_name == ident.escaped_text
                    && symbol.has_any_flags(
                        tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE,
                    )
                    && symbol
                        .declarations
                        .iter()
                        .copied()
                        .any(|decl| self.is_namespace_declaration_instantiated(decl))
            })
    }
}
