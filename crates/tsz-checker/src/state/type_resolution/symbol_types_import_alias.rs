//! Cross-file import-alias resolution split out of `symbol_types.rs` to keep
//! that file under the architecture guard's line cap.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_import_alias_cross_file(&self, sym_id: SymbolId) -> Option<SymbolId> {
        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lc| std::sync::Arc::clone(&lc.binder))
            .collect();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return None;
        }
        let module_specifier = symbol.import_module()?;
        let import_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());

        // Local import aliases resolve relative to the current file even if a
        // same-number cross-file target has already been registered for `sym_id`.
        // SymbolIds are per binder, so imported aliases can collide numerically
        // with their target after lib merging.
        let source_file_idx = if self
            .ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|local| local.has_any_flags(symbol_flags::ALIAS))
        {
            self.ctx.current_file_idx
        } else {
            self.ctx
                .resolve_symbol_file_index(sym_id)
                .unwrap_or(self.ctx.current_file_idx)
        };

        let target_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let file_name = &target_arena.source_files.first()?.file_name;

        // Try module_exports first (keyed by filename), then file_locals.
        let target_sym_id = target_binder
            .module_exports
            .get(file_name)
            .and_then(|exports| exports.get(import_name))
            .or_else(|| target_binder.file_locals.get(import_name))?;

        self.ctx
            .register_symbol_file_target(target_sym_id, target_idx);
        Some(target_sym_id)
    }
}
