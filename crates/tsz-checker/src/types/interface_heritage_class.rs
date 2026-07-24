//! Class-declaration recovery for interface heritage.
//!
//! Keeps owner-aware import/default-export resolution out of the interface
//! shape-merging shard while preserving the checker/solver boundary.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};

impl CheckerState<'_> {
    /// Resolve an interface-heritage base symbol to the underlying `class`
    /// symbol — directly, through an import alias, or through a cross-file
    /// re-export — or `None` when the base is not a class. Used to route a
    /// class base through the class-instance resolver (so instance members are
    /// inherited) instead of the symbol's constructor type.
    pub(super) fn heritage_base_class_symbol(&mut self, base_sym_id: SymbolId) -> Option<SymbolId> {
        let symbol_is_class = |this: &Self, sym: SymbolId| {
            this.get_cross_file_symbol(sym)
                .or_else(|| this.ctx.binder.get_symbol(sym))
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS))
        };

        if symbol_is_class(self, base_sym_id) {
            return Some(base_sym_id);
        }

        // A default import of `class Base; export default Base` resolves first
        // to the exporting file's synthetic `default` symbol. That symbol is an
        // alias/value surface, not the local class declaration, so the ordinary
        // alias paths below cannot classify it as a class and heritage falls
        // through to `get_type_of_symbol` (the constructor/static side).
        //
        // Recover the exact declaration named by the explicit default-export
        // clause before that fallback. The target is resolved from the
        // export-clause identifier through its owning binder, which avoids raw
        // cross-binder `SymbolId` collisions and also preserves class/namespace
        // merges. Property-access defaults, re-exports, `export =`, and
        // non-class defaults remain on their existing paths.
        if let Some(target) = self.explicit_default_import_class_symbol(base_sym_id) {
            return Some(target);
        }

        let mut visited_aliases = AliasCycleTracker::new();
        if let Some(target) = self
            .resolve_alias_symbol(base_sym_id, &mut visited_aliases)
            .filter(|&target| target != base_sym_id)
            && symbol_is_class(self, target)
        {
            return Some(target);
        }

        self.resolve_import_alias_cross_file(base_sym_id)
            .filter(|&target| target != base_sym_id && symbol_is_class(self, target))
    }

    /// Resolve the local class named by an explicit
    /// `export default <identifier>` reached through a requester-local default
    /// import.
    fn explicit_default_import_class_symbol(&self, base_sym_id: SymbolId) -> Option<SymbolId> {
        let alias_symbol = self.local_import_alias(base_sym_id)?;
        if alias_symbol.import_name()? != "default" {
            return None;
        }
        let module_specifier = alias_symbol.import_module()?;
        let source_file_idx = if alias_symbol.decl_file_idx == u32::MAX {
            self.ctx.current_file_idx
        } else {
            alias_symbol.decl_file_idx as usize
        };
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = &target_arena.source_files.first()?.file_name;
        let exports = self
            .ctx
            .module_exports_for_module(target_binder, target_file_name)?;
        if exports.get("export=").is_some() {
            return None;
        }
        let default_sym_id = exports.get("default")?;
        let target_sym_id =
            self.default_export_identifier_target_in_file(default_sym_id, target_file_idx)?;
        target_binder
            .get_symbol(target_sym_id)
            .filter(|symbol| symbol.has_any_flags(symbol_flags::CLASS))
            .map(|_| target_sym_id)
    }
}
