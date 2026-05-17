use crate::state::CheckerState;
use tsz_binder::SymbolId;

impl<'a> CheckerState<'a> {
    /// Get a symbol from the current binder, lib binders, or other file binders.
    /// This ensures we can resolve symbols from lib.d.ts and other files.
    pub(crate) fn get_symbol_globally(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && file_idx != self.ctx.current_file_idx
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }

        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        for lib in self.ctx.lib_contexts.iter() {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    /// Get a symbol, preferring the cross-file binder for known cross-file `SymbolIds`.
    pub(crate) fn get_cross_file_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        self.get_symbol_globally(sym_id)
    }
}
