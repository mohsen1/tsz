//! Mode-aware cross-file export resolution for `CheckerState`.
//!
//! Houses `resolve_cross_file_export_from_file_with_mode`, the export-lookup
//! kernel that honors an explicit `resolution-mode` override when picking the
//! target file. The thin default-mode wrappers
//! (`resolve_cross_file_export` / `resolve_cross_file_export_from_file`) live in
//! `type_resolution/module.rs` and delegate here.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Like `resolve_cross_file_export_from_file` but honors an explicit
    /// `resolution-mode` override when picking the target file. This routes a
    /// specifier through the requested package `exports`/`imports` condition
    /// (ESM `import` vs CommonJS `require`) so, e.g., a JSDoc
    /// `@import { X } from "pkg" with { "resolution-mode": "import" }` resolves
    /// `X` from the package's ESM-condition declaration file even when the
    /// importing file is CommonJS. With `None` the behavior is identical to the
    /// default-mode entry point.
    pub(crate) fn resolve_cross_file_export_from_file_with_mode(
        &self,
        module_specifier: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
        resolution_mode_override: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<tsz_binder::SymbolId> {
        // First, try to resolve the module specifier to a target file index.
        // When source_file_idx is provided, resolve from that file's perspective
        // (for following re-export chains where specifiers are relative to the
        // declaring file, not the current file). `resolve_import_target_from_file_with_mode`
        // falls back to the default-mode resolution when the override is `None`,
        // so this single call covers both the default and mode-overridden paths.
        let from_file = source_file_idx.unwrap_or(self.ctx.current_file_idx);
        let target_file_idx = self.ctx.resolve_import_target_from_file_with_mode(
            from_file,
            module_specifier,
            resolution_mode_override,
        );

        let Some(target_file_idx) = target_file_idx else {
            if let Some((sym_id, binder_idx)) =
                self.resolve_ambient_module_export(module_specifier, export_name)
            {
                // Record cross-file origin so delegate_cross_arena_symbol_resolution
                // can find the correct arena/binder for this symbol.
                if !self.ctx.has_symbol_file_index(sym_id) {
                    self.ctx.register_symbol_file_target(sym_id, binder_idx);
                }
                return Some(sym_id);
            }
            return None;
        };

        // Get the target file's binder
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;

        // Resolve the target file's canonical module key (source file path)
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Helper: record the cross-file origin so delegate_cross_arena_symbol_resolution
        // can find the correct arena for this SymbolId.
        let record_and_return = |sym_id: tsz_binder::SymbolId| -> Option<tsz_binder::SymbolId> {
            self.ctx
                .register_symbol_file_target(sym_id, target_file_idx);
            Some(sym_id)
        };

        let is_reexport_alias = |sym_id: tsz_binder::SymbolId| {
            target_binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.import_module.is_some())
        };

        if let Some(exports_table) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports_table, export_name)
            && !is_reexport_alias(sym_id)
        {
            return record_and_return(sym_id);
        }

        if let Some(exports_table) = self
            .ctx
            .module_exports_for_module(target_binder, module_specifier)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports_table, export_name)
            && !is_reexport_alias(sym_id)
        {
            return record_and_return(sym_id);
        }

        let augmentation_export =
            self.resolve_module_augmentation_export_for_file(target_file_idx, export_name);
        if let Some((sym_id, augmenting_file_idx)) = augmentation_export
            && self.module_augmentation_export_preempts_reexport_alias(sym_id, augmenting_file_idx)
        {
            self.ctx
                .register_symbol_file_target(sym_id, augmenting_file_idx);
            return Some(sym_id);
        }

        if let Some(source_binder) = self.ctx.get_binder_for_file(from_file)
            && let Some((sym_id, _is_type_only)) =
                source_binder.resolve_import_with_reexports_type_only(module_specifier, export_name)
        {
            return record_and_return(sym_id);
        }

        // Prefer the binder's type-aware export resolver so interface/type-only
        // exports reached through `import("./x").T` behave the same way as
        // regular type-node resolution.
        if let Some((sym_id, _is_type_only)) =
            target_binder.resolve_import_with_reexports_type_only(&target_file_name, export_name)
        {
            return record_and_return(sym_id);
        }

        // Follow re-export chains (wildcard and named re-exports) BEFORE
        // falling back to file_locals. file_locals may contain merged globals
        // that shadow the actual re-exported symbols.
        let mut visited = rustc_hash::FxHashSet::default();
        if let Some((sym_id, actual_file_idx)) =
            self.resolve_export_in_file(target_file_idx, export_name, &mut visited)
        {
            self.ctx
                .register_symbol_file_target(sym_id, actual_file_idx);
            return Some(sym_id);
        }

        if let Some((sym_id, augmenting_file_idx)) = augmentation_export {
            self.ctx
                .register_symbol_file_target(sym_id, augmenting_file_idx);
            return Some(sym_id);
        }

        // Last resort: check file_locals (for script files or binding edge cases
        // where module_exports wasn't populated).
        //
        // IMPORTANT: Only use file_locals as a fallback when module_exports is
        // empty or unavailable AND the target file is a script (not an external
        // module). For real ES modules — files with `import`/`export` syntax or
        // module file extensions like `.mts`/`.cts` — `file_locals` may hold
        // imported aliases (`import x from "./other"`) that are NOT part of the
        // module's public surface. Returning those here would let
        // `import * as ns from "./self"` see the file's local imports through
        // `ns.x`, which `tsc` rejects with TS2339 (issue #3585).
        let has_module_exports = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .is_some_and(|e| !e.is_empty());
        if !target_binder.is_external_module
            && !has_module_exports
            && let Some(sym_id) = target_binder.file_locals.get(export_name)
        {
            return record_and_return(sym_id);
        }

        None
    }
}
