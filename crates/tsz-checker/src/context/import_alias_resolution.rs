//! Import-alias / re-export resolution for `CheckerContext`.
//!
//! Follows `import { X } from "./m"` aliases to their declaring symbol across
//! file boundaries — a single hop, the full re-export chain, and ambient
//! `declare module "X"` exports. Per-file binders mint colliding raw
//! `SymbolId`s, so each hop is read from the binder of the file that declares
//! it and pinned into the cross-file target overlay for cross-arena delegation.

use tsz_binder::{BinderState, SymbolId};

use super::CheckerContext;

/// Look up `import_name` among a binder's exports for `file_name`, falling back
/// to its file-local declarations. Shared by the single-hop and chain import
/// alias resolvers so both agree on where a re-exported name resolves.
fn binder_named_export_or_local(
    binder: &BinderState,
    file_name: &str,
    import_name: &str,
) -> Option<SymbolId> {
    binder
        .module_exports
        .get(file_name)
        .and_then(|exports| exports.get(import_name))
        .or_else(|| binder.file_locals.get(import_name))
}

impl CheckerContext<'_> {
    /// Follow an import alias to its actual target symbol across file boundaries.
    ///
    /// For ALIAS symbols (created by `import {A} from "./file"`), resolves
    /// the module specifier from the alias's source file, then looks up the
    /// exported name in the target file's binder. Returns None if the symbol
    /// is not an alias or resolution fails.
    ///
    /// This is a pure lookup — it does NOT register the result in
    /// `cross_file_symbol_targets`. Callers that need cross-arena delegation
    /// (e.g., lazy type resolution) should call [`resolve_import_alias_and_register`]
    /// instead.
    pub fn resolve_import_alias(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module()?;
        let import_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());

        let source_file_idx = if self
            .binder
            .get_symbol(sym_id)
            .is_some_and(|local| local.flags & tsz_binder::symbol_flags::ALIAS != 0)
        {
            self.current_file_idx
        } else {
            symbol.decl_file_idx as usize
        };
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            return target_binder.file_locals.get(import_name);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        self.resolve_import_from_ambient_module(module_specifier, import_name)
    }

    /// Like [`resolve_import_alias`], but also registers the resolved symbol in
    /// `cross_file_symbol_targets` so that `delegate_cross_arena_symbol_resolution`
    /// can create a child checker with the correct arena when computing its type.
    pub fn resolve_import_alias_and_register(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module()?;
        let import_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());

        let source_file_idx = if self
            .binder
            .get_symbol(sym_id)
            .is_some_and(|local| local.flags & tsz_binder::symbol_flags::ALIAS != 0)
        {
            self.current_file_idx
        } else {
            symbol.decl_file_idx as usize
        };
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            let target_arena = self.get_arena_for_file(target_idx as u32);
            let file_name = &target_arena.source_files.first()?.file_name;
            let result = binder_named_export_or_local(target_binder, file_name, import_name)?;
            self.register_symbol_file_target(result, target_idx);
            return Some(result);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        // For ambient modules, the symbol lives in the same binder that declared
        // the module, so we also register it in cross_file_symbol_targets with
        // the declaring file's index for proper cross-arena delegation.
        if let Some((result, file_idx)) =
            self.resolve_import_from_ambient_module_with_file_idx(module_specifier, import_name)
        {
            self.register_symbol_file_target(result, file_idx);
            return Some(result);
        }
        None
    }

    /// Follow an import-alias / re-export chain to its terminal target symbol,
    /// registering every hop's owning file so cross-arena delegation can locate
    /// it.
    ///
    /// [`resolve_import_alias_and_register`] resolves a *single* hop using the
    /// current file's binder. A barrel/re-export chain
    /// (`import { x } from "./re"` where `re.ts` does `export { x } from
    /// "./src"`, or `import { x } from "./re"; export { x }`) lands on an
    /// intermediate alias that is itself imported from a third module. Each
    /// intermediate must be read from the binder of the file that *declares* it:
    /// per-file binders mint colliding raw `SymbolId`s, so reading a re-exported
    /// intermediate through the current file's binder would follow an unrelated
    /// same-id symbol. Bounded against re-export cycles.
    pub fn resolve_import_alias_chain_and_register(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        // The first hop is a local alias, resolved through the current binder.
        // `export *` barrels and other chains the single-hop named lookup misses
        // are followed by the binder's full re-export resolver instead; pin the
        // terminal's declaring file so cross-arena delegation can locate it.
        let Some(mut current) = self.resolve_import_alias_and_register(sym_id) else {
            let terminal = self.binder.resolve_import_symbol(sym_id)?;
            if let Some(file_idx) = self.resolve_symbol_file_index(terminal) {
                self.register_symbol_file_target(terminal, file_idx);
            }
            return Some(terminal);
        };
        const MAX_REEXPORT_DEPTH: usize = 64;
        for _ in 0..MAX_REEXPORT_DEPTH {
            // Read the intermediate from the binder that actually owns it.
            let Some(file_idx) = self.resolve_symbol_file_index(current) else {
                break;
            };
            let Some(binder) = self.get_binder_for_file(file_idx) else {
                break;
            };
            let Some(symbol) = binder.get_symbol(current) else {
                break;
            };
            if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
                break;
            }
            let Some(module_specifier) = symbol.import_module() else {
                break;
            };
            let import_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());

            let Some(target_idx) = self.resolve_import_target_from_file(file_idx, module_specifier)
            else {
                break;
            };
            let Some(target_binder) = self.get_binder_for_file(target_idx) else {
                break;
            };
            let target_arena = self.get_arena_for_file(target_idx as u32);
            let Some(file_name) = target_arena.source_files.first().map(|sf| &sf.file_name) else {
                break;
            };
            let Some(next) = binder_named_export_or_local(target_binder, file_name, import_name)
            else {
                break;
            };
            if next == current {
                break;
            }
            self.register_symbol_file_target(next, target_idx);
            current = next;
        }
        Some(current)
    }

    /// Resolve an import name from ambient module exports (`declare module "X" { ... }`).
    ///
    /// When file-based module resolution fails (the module specifier doesn't correspond
    /// to any file), this fallback checks `module_exports` in the current binder and
    /// all cross-file binders. Ambient module declarations populate `module_exports`
    /// keyed by their string-literal module specifier (e.g., `"A"` for `declare module "A"`).
    fn resolve_import_from_ambient_module(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        // Check current binder first
        if let Some(exports) = self.module_exports_for_module(self.binder, module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some(sym_id);
        }
        // Use the pre-built global module_exports index for O(1) lookup (no allocation)
        if let Some(entries) = self
            .global_module_exports_index
            .as_ref()
            .and_then(|idx| idx.get(module_specifier))
            .and_then(|inner| inner.get(import_name))
            && let Some(&(_file_idx, sym_id)) = entries.first()
        {
            return Some(sym_id);
        }
        None
    }

    /// Like [`resolve_import_from_ambient_module`] but also returns the file index
    /// of the binder that owns the resolved symbol, for `cross_file_symbol_targets`
    /// registration.
    fn resolve_import_from_ambient_module_with_file_idx(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        // Check current binder first
        if let Some(exports) = self.module_exports_for_module(self.binder, module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some((sym_id, self.current_file_idx));
        }
        // Use the pre-built global module_exports index for O(1) lookup (no allocation)
        if let Some(entries) = self
            .global_module_exports_index
            .as_ref()
            .and_then(|idx| idx.get(module_specifier))
            .and_then(|inner| inner.get(import_name))
            && let Some(&(file_idx, sym_id)) = entries.first()
        {
            return Some((sym_id, file_idx));
        }
        None
    }
}
