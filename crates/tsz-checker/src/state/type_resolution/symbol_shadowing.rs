//! Symbol shadowing predicates for type-reference resolution.

use crate::state::CheckerState;
use tsz_binder::SymbolId;

impl CheckerState<'_> {
    pub(crate) fn symbol_shadows_file_local_lib_type(
        &self,
        sym_id: SymbolId,
        escaped_name: &str,
    ) -> bool {
        self.ctx
            .resolve_symbol_file_index(sym_id)
            .is_some_and(|file_idx| file_idx == self.ctx.current_file_idx)
            && self
                .ctx
                .binder
                .file_locals
                .get(escaped_name)
                .is_some_and(|candidate| {
                    candidate != sym_id
                        && (self.ctx.binder.lib_symbol_ids.contains(&candidate)
                            || self.ctx.symbol_is_from_lib(candidate)
                            || self
                                .ctx
                                .binder
                                .get_symbol(candidate)
                                .is_some_and(|symbol| symbol.decl_file_idx == u32::MAX))
                })
    }
}
