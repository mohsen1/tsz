//! Module resolution and cross-file exports for `CheckerState`.
//!
//! Constructor type operations have been extracted to
//! `state_type_resolution_constructors.rs`.

use crate::module_resolution::module_specifier_candidates;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Resolve a named type reference to its `TypeId`.
    ///
    /// This is a core function for resolving type names like `User`, `Array`, `Promise`,
    /// etc. to their actual type representations. It handles multiple resolution strategies.
    ///
    /// ## Resolution Strategy (in order):
    /// 1. **Type Parameters**: Check if name is a type parameter in current scope
    /// 2. **Global Augmentations**: Check if name is declared in `declare global` blocks
    /// 3. **Local Symbols**: Resolve to interface/class/type alias in current file
    /// 4. **Lib Types**: Fall back to lib.d.ts and library contexts
    ///
    /// ## Type Parameter Lookup:
    /// - Checks current type parameter scope first
    /// - Allows generic type parameters to shadow global types
    ///
    /// ## Global Augmentations:
    /// - Merges user's global declarations with lib.d.ts
    /// - Ensures augmentation properly extends base types
    ///
    /// ## Lib Context Resolution:
    /// - Searches through loaded library contexts
    /// - Handles built-in types (Object, Array, Promise, etc.)
    /// - Merges multiple declarations (interface merging)
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Type parameter lookup
    /// function identity<T>(value: T): T {
    ///   // resolve_named_type_reference("T") → type parameter T
    ///   return value;
    /// }
    ///
    /// // Local interface
    /// interface User {}
    /// // resolve_named_type_reference("User") → User interface type
    ///
    /// // Global type (from lib.d.ts)
    /// let arr: Array<string>;
    /// // resolve_named_type_reference("Array") → Array global type
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProp: string;
    ///   }
    /// }
    /// // resolve_named_type_reference("Window") → merged Window type
    ///
    /// // Type alias
    /// type UserId = number;
    /// // resolve_named_type_reference("UserId") → number
    /// ```
    pub(crate) fn resolve_named_type_reference(
        &mut self,
        name: &str,
        name_idx: NodeIndex,
    ) -> Option<TypeId> {
        if let Some(type_id) = self.lookup_type_parameter(name) {
            return Some(type_id);
        }
        // Check if this is a global augmentation (interface declared in `declare global` block)
        // If so, use resolve_lib_type_by_name to merge with lib.d.ts declarations
        let is_global_augmentation = self.ctx.binder.global_augmentations.contains_key(name);
        if is_global_augmentation {
            // For global augmentations, we must use resolve_lib_type_by_name to get
            // the proper merge of lib.d.ts + user augmentation
            if let Some(type_id) = self.resolve_lib_type_by_name(name) {
                return Some(type_id);
            }
        }
        if let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position(name_idx)
        {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Fall back to lib contexts for global type resolution
        // BUT only if lib files are actually loaded (noLib is false)
        if self.ctx.has_lib_loaded()
            && let Some(type_id) = self.resolve_lib_type_by_name(name)
        {
            return Some(type_id);
        }
        None
    }

    /// Resolve an export from another file using cross-file resolution.
    ///
    /// This method uses `all_binders` and `resolved_module_paths` to look up an export
    /// from a different file in multi-file mode. Returns the `SymbolId` of the export
    /// if found, or None if cross-file resolution is not available or the export is not found.
    ///
    /// This is the core of Phase 1.1: `ModuleResolver` ↔ Checker Integration.
    pub(crate) fn resolve_cross_file_export(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(sym_id) = self.resolve_ambient_module_export(module_specifier, export_name) {
            return Some(sym_id);
        }

        // First, try to resolve the module specifier to a target file index
        let target_file_idx = self.ctx.resolve_import_target(module_specifier)?;

        // Get the target file's binder
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;

        // Resolve the target file's canonical module key (source file path)
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Helper: record the cross-file origin so delegate_cross_arena_symbol_resolution
        // can find the correct arena for this SymbolId.
        let record_and_return = |sym_id: tsz_binder::SymbolId| -> Option<tsz_binder::SymbolId> {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, target_file_idx);
            Some(sym_id)
        };

        // Look up the export in the target binder's module_exports.
        // Prefer canonical file key, then module specifier fallback.
        if let Some(exports_table) = target_binder.module_exports.get(&target_file_name)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports_table, export_name)
        {
            return record_and_return(sym_id);
        }

        if let Some(exports_table) = target_binder.module_exports.get(module_specifier)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports_table, export_name)
        {
            return record_and_return(sym_id);
        }

        // Fall back to checking file_locals in the target binder
        if let Some(sym_id) = target_binder.file_locals.get(export_name) {
            return record_and_return(sym_id);
        }

        // Follow re-export chains (wildcard and named re-exports)
        let mut visited = rustc_hash::FxHashSet::default();
        let result = self.resolve_export_in_file(target_file_idx, export_name, &mut visited);
        if let Some((sym_id, actual_file_idx)) = result {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, actual_file_idx);
            return Some(sym_id);
        }
        None
    }

    fn resolve_export_from_table(
        &self,
        binder: &tsz_binder::BinderState,
        exports_table: &tsz_binder::SymbolTable,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(sym_id) = exports_table.get(export_name) {
            return Some(sym_id);
        }

        let export_equals_sym_id = exports_table.get("export=")?;
        let export_equals_symbol = binder.get_symbol(export_equals_sym_id)?;

        if let Some(exports) = export_equals_symbol.exports.as_ref()
            && let Some(sym_id) = exports.get(export_name)
        {
            return Some(sym_id);
        }

        if let Some(members) = export_equals_symbol.members.as_ref()
            && let Some(sym_id) = members.get(export_name)
        {
            return Some(sym_id);
        }

        // Some binder paths keep the namespace merge partner as a distinct symbol.
        // Probe symbols with the same name and a module namespace shape.
        for candidate_id in binder
            .get_symbols()
            .find_all_by_name(&export_equals_symbol.escaped_name)
        {
            let Some(candidate_symbol) = binder.get_symbol(candidate_id) else {
                continue;
            };
            if (candidate_symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                == 0
            {
                continue;
            }
            if let Some(exports) = candidate_symbol.exports.as_ref()
                && let Some(sym_id) = exports.get(export_name)
            {
                return Some(sym_id);
            }
            if let Some(members) = candidate_symbol.members.as_ref()
                && let Some(sym_id) = members.get(export_name)
            {
                return Some(sym_id);
            }
        }

        None
    }

    fn resolve_ambient_module_export(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let binders = self.ctx.all_binders.as_ref()?;
        for binder in binders.iter() {
            if let Some(exports_table) = binder.module_exports.get(module_specifier)
                && let Some(sym_id) =
                    self.resolve_export_from_table(binder, exports_table, export_name)
            {
                return Some(sym_id);
            }
        }
        None
    }

    /// Follow re-export chains across binder boundaries to find an exported symbol.
    /// Returns the `SymbolId` if the export is found via named or wildcard re-exports.
    /// Follow re-export chains across binder boundaries to find an exported symbol.
    /// Returns `(SymbolId, file_idx)` where `file_idx` is the actual file that owns
    /// the symbol, so callers can record the correct cross-file origin.
    fn resolve_export_in_file(
        &self,
        file_idx: usize,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        if !visited.insert(file_idx) {
            return None; // Cycle detection
        }

        let target_binder = self.ctx.get_binder_for_file(file_idx)?;

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Check direct exports
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports, export_name)
        {
            return Some((sym_id, file_idx));
        }

        // Check file_locals
        if let Some(sym_id) = target_binder.file_locals.get(export_name) {
            return Some((sym_id, file_idx));
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = reexports.get(export_name)
        {
            let name = original_name.as_deref().unwrap_or(export_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && let Some(result) = self.resolve_export_in_file(source_idx, name, visited)
            {
                return Some(result);
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(result) =
                        self.resolve_export_in_file(source_idx, export_name, visited)
                {
                    return Some(result);
                }
            }
        }

        None
    }

    /// Resolve a namespace import (import * as ns) from another file using cross-file resolution.
    ///
    /// Returns a `SymbolTable` containing all exports from the target module.
    pub(crate) fn resolve_cross_file_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        if let Some(exports) = self.resolve_ambient_module_namespace_exports(module_specifier) {
            return Some(exports);
        }

        let target_file_idx = self.ctx.resolve_import_target(module_specifier)?;
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        // Helper: record cross-file origin for all symbols in a table.
        let record_symbols = |table: &tsz_binder::SymbolTable| {
            let mut targets = self.ctx.cross_file_symbol_targets.borrow_mut();
            for (_, &sym_id) in table.iter() {
                targets.insert(sym_id, target_file_idx);
            }
        };

        // Try to find exports in the target binder's module_exports.
        // Prefer canonical file key first.
        if let Some(exports) = target_binder.module_exports.get(&target_file_name) {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            record_symbols(&combined);
            return Some(combined);
        }

        // Fallback to module specifier key.
        if let Some(exports) = target_binder.module_exports.get(module_specifier) {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            record_symbols(&combined);
            return Some(combined);
        }

        // No target-driven export surface found for this module specifier.
        None
    }

    /// Resolve a module's effective export surface.
    ///
    /// This canonicalizes module-specifier variants and ensures `export =` target
    /// members are merged into the result. Prefer this over ad-hoc lookups against
    /// `binder.module_exports`.
    pub(crate) fn resolve_effective_module_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        for candidate in module_specifier_candidates(module_specifier) {
            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate) {
                let mut combined = exports.clone();
                self.merge_export_equals_members(self.ctx.binder, exports, &mut combined);
                return Some(combined);
            }

            if let Some(exports) = self.resolve_cross_file_namespace_exports(&candidate) {
                return Some(exports);
            }
        }

        None
    }

    fn resolve_ambient_module_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        let binders = self.ctx.all_binders.as_ref()?;
        for binder in binders.iter() {
            if let Some(exports) = binder.module_exports.get(module_specifier) {
                let mut combined = exports.clone();
                self.merge_export_equals_members(binder, exports, &mut combined);
                return Some(combined);
            }
        }
        None
    }

    fn merge_export_equals_members(
        &self,
        binder: &tsz_binder::BinderState,
        exports: &tsz_binder::SymbolTable,
        combined: &mut tsz_binder::SymbolTable,
    ) {
        let Some(export_equals_sym_id) = exports.get("export=") else {
            return;
        };
        let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id) else {
            return;
        };

        if let Some(symbol_exports) = export_equals_symbol.exports.as_ref() {
            for (name, sym_id) in symbol_exports.iter() {
                if !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }

        if let Some(symbol_members) = export_equals_symbol.members.as_ref() {
            for (name, sym_id) in symbol_members.iter() {
                if !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }
    }

    /// Collect all symbols reachable through re-export chains into the given `SymbolTable`.
    fn collect_reexported_symbols(
        &self,
        file_idx: usize,
        result: &mut tsz_binder::SymbolTable,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) {
        if !visited.insert(file_idx) {
            return; // Cycle detection
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return;
        };
        let Some(target_file_name) = self
            .ctx
            .get_arena_for_file(file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return;
        };

        // Collect from wildcard re-exports (export * from './module')
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(source_binder) = self.ctx.get_binder_for_file(source_idx)
                {
                    // Add all exports from the source module
                    if let Some((_, exports)) = source_binder.module_exports.iter().next() {
                        for (name, sym_id) in exports.iter() {
                            if !result.has(name) {
                                result.set(name.to_string(), *sym_id);
                            }
                        }
                    }
                    // Recursively collect from the source's re-exports
                    self.collect_reexported_symbols(source_idx, result, visited);
                }
            }
        }

        // Collect from named re-exports (export { X } from './module')
        if let Some(reexports) = target_binder.reexports.get(&target_file_name) {
            let reexports = reexports.clone();
            for (exported_name, (source_module, original_name)) in &reexports {
                if !result.has(exported_name) {
                    let name = original_name.as_deref().unwrap_or(exported_name);
                    if let Some(source_idx) = self
                        .ctx
                        .resolve_import_target_from_file(file_idx, source_module)
                    {
                        let mut inner_visited = visited.clone();
                        if let Some((sym_id, _actual_file_idx)) =
                            self.resolve_export_in_file(source_idx, name, &mut inner_visited)
                        {
                            result.set(exported_name.to_string(), sym_id);
                        }
                    }
                }
            }
        }
    }

    /// Emit TS2307 error for a module that cannot be found.
    ///
    /// This function emits a "Cannot find module" error with the module specifier
    /// and attempts to report the error at the import declaration node if available.
    pub(crate) fn emit_module_not_found_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::syntax_kind_ext;

        // Only emit if report_unresolved_imports is enabled
        // (CLI driver handles module resolution in multi-file mode)
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // For import declarations, defer to check_import_declaration / check_import_equals_declaration
        // which have accurate module specifier positions and handle special cases (e.g., TS1147 for
        // imports in namespaces). This function may be called during type resolution with incorrect
        // position information (or no node at all).
        if let Some(node) = self.ctx.arena.get(decl_node) {
            match node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_SPECIFIER
                | syntax_kind_ext::NAMESPACE_IMPORT
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    return;
                }
                _ => {}
            }
        } else if self.ctx.report_unresolved_imports {
            // No declaration node available — check_import_declaration will handle this
            // with correct module specifier positions from the import statement
            return;
        }

        // Don't emit TS2307 for modules in the resolved_modules set.
        // The CLI driver populates this set for modules that have been resolved
        // but whose exports might not yet be available in the binder.
        if self.module_exists_cross_file(module_specifier) {
            return;
        }

        // Don't emit for ambient module matches (declared modules, shorthand modules)
        if self.is_ambient_module_match(module_specifier) {
            return;
        }

        // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
        // IMPORTANT: Mark as emitted BEFORE calling self.error() to prevent race conditions
        // where multiple code paths check the set simultaneously
        let module_key = module_specifier.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return; // Already emitted - skip duplicate
        }
        self.ctx.modules_with_ts2307_emitted.insert(module_key);

        // Try to find the import declaration node to get the module specifier span
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                // For import equals declarations, try to get the module specifier node
                if node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    // For ES6 import declarations, the module specifier should be available
                    if let Some(import) = self.ctx.arena.get_import_decl(node) {
                        if let Some(module_node) = self.ctx.arena.get(import.module_specifier) {
                            // Found the module specifier node - use its span
                            (module_node.pos, module_node.end - module_node.pos)
                        } else {
                            // Fall back to the declaration node span
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else if node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
                    // For import specifiers, try to find the parent import declaration
                    if let Some(ext) = self.ctx.arena.get_extended(decl_node) {
                        let parent = ext.parent;
                        if let Some(parent_node) = self.ctx.arena.get(parent) {
                            if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                                if let Some(import) = self.ctx.arena.get_import_decl(parent_node) {
                                    if let Some(module_node) =
                                        self.ctx.arena.get(import.module_specifier)
                                    {
                                        // Found the module specifier node - use its span
                                        (module_node.pos, module_node.end - module_node.pos)
                                    } else {
                                        // Fall back to the parent declaration node span
                                        (parent_node.pos, parent_node.end - parent_node.pos)
                                    }
                                } else {
                                    (parent_node.pos, parent_node.end - parent_node.pos)
                                }
                            } else {
                                (node.pos, node.end - node.pos)
                            }
                        } else {
                            (node.pos, node.end - node.pos)
                        }
                    } else {
                        (node.pos, node.end - node.pos)
                    }
                } else {
                    // Use the declaration node span for other cases
                    (node.pos, node.end - node.pos)
                }
            } else {
                // No node available - use position 0
                (0, 0)
            }
        } else {
            // No declaration node - use position 0
            (0, 0)
        };

        // Note: We use self.error() which already checks emitted_diagnostics for deduplication
        // The key is (start, code), so we won't emit duplicate errors at the same location

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // The driver's ModuleResolver may have a more specific error code than TS2307.
        if let Some(error) = self.ctx.get_resolution_error(module_specifier) {
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            {
                let module_kind_prefers_2792 = matches!(
                    self.ctx.compiler_options.module,
                    tsz_common::common::ModuleKind::System
                        | tsz_common::common::ModuleKind::AMD
                        | tsz_common::common::ModuleKind::UMD
                        | tsz_common::common::ModuleKind::ES2015
                        | tsz_common::common::ModuleKind::ES2020
                        | tsz_common::common::ModuleKind::ES2022
                        | tsz_common::common::ModuleKind::ESNext
                        | tsz_common::common::ModuleKind::Preserve
                );
                if module_kind_prefers_2792 {
                    let fallback_message = format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                        &[module_specifier],
                    );
                    error_code = diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O;
                    error_message = fallback_message;
                }
            }
            self.error(start, length, error_message, error_code);
            return;
        }

        // Use TS2792 when module resolution is "classic"-like (non-Node module kinds),
        // otherwise TS2307.
        use crate::diagnostics::{diagnostic_messages, format_message};
        use tsz_common::common::ModuleKind;

        let module_kind_prefers_2792 = matches!(
            self.ctx.compiler_options.module,
            ModuleKind::System
                | ModuleKind::AMD
                | ModuleKind::UMD
                | ModuleKind::ES2015
                | ModuleKind::ES2020
                | ModuleKind::ES2022
                | ModuleKind::ESNext
                | ModuleKind::Preserve
        );
        let use_2792 = module_kind_prefers_2792;

        if use_2792 {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                &[module_specifier],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
            );
        } else {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                &[module_specifier],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
            );
        }
    }

    /// Emit TS1192 error when a module has no default export, or TS2732 for JSON files.
    ///
    /// This is emitted when trying to use a default import (`import X from 'mod'`)
    /// but the module doesn't export a default binding.
    ///
    /// For JSON files (.json extension), emits TS2732 when `resolveJsonModule` is disabled,
    /// suggesting to enable the flag. This takes precedence over TS1192.
    ///
    /// Note: TS1192 is suppressed when `allowSyntheticDefaultImports` or
    /// `esModuleInterop` is enabled, as those flags allow importing modules
    /// without explicit default exports.
    pub(crate) fn emit_no_default_export_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let mut named_default_specifier_node: Option<NodeIndex> = None;

        if let Some(node) = self.ctx.arena.get(decl_node)
            && node.kind == syntax_kind_ext::IMPORT_SPECIFIER
            && let Some(specifier) = self.ctx.arena.get_specifier(node)
        {
            let imported_name_idx = if specifier.property_name.is_none() {
                specifier.name
            } else {
                specifier.property_name
            };
            if let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx)
                && let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node)
                && imported_ident.escaped_text.as_str() == "default"
            {
                named_default_specifier_node = Some(decl_node);
            }
        }

        if named_default_specifier_node.is_none() {
            let mut current = decl_node;
            let mut import_decl_idx = NodeIndex::NONE;
            for _ in 0..8 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    import_decl_idx = parent;
                    break;
                }
                current = parent;
            }

            if import_decl_idx.is_some()
                && let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx)
                && let Some(import_decl) = self.ctx.arena.get_import_decl(import_decl_node)
                && let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause)
                && let Some(clause) = self.ctx.arena.get_import_clause(clause_node)
                && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
                && bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
                && let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node)
            {
                for &element_idx in &named_imports.elements.nodes {
                    let Some(element_node) = self.ctx.arena.get(element_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                        continue;
                    };
                    let imported_name_idx = if specifier.property_name.is_none() {
                        specifier.name
                    } else {
                        specifier.property_name
                    };
                    let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx) else {
                        continue;
                    };
                    let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node)
                    else {
                        continue;
                    };
                    if imported_ident.escaped_text.as_str() == "default" {
                        named_default_specifier_node = Some(element_idx);
                        break;
                    }
                }
            }
        }

        if let Some(specifier_node) = named_default_specifier_node {
            self.emit_no_exported_member_error(module_specifier, "default", specifier_node);
            return;
        }

        // Check if this is a JSON file import without resolveJsonModule enabled
        // TS2732 takes precedence over TS1192 for JSON files
        // IMPORTANT: This check must come BEFORE report_unresolved_imports guard
        // because TS2732 should be emitted even in single-file mode
        if module_specifier.ends_with(".json") && !self.ctx.compiler_options.resolve_json_module {
            // Get span from declaration node
            let (start, length) = if decl_node.is_some() {
                if let Some(node) = self.ctx.arena.get(decl_node) {
                    (node.pos, node.end - node.pos)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            };

            use crate::diagnostics::{diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E,
                &[module_specifier],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::CANNOT_FIND_MODULE_CONSIDER_USING_RESOLVEJSONMODULE_TO_IMPORT_MODULE_WITH_JSON_E,
            );
            return;
        }

        // Only emit TS1192 if report_unresolved_imports is enabled
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // allowSyntheticDefaultImports allows default imports without explicit default export
        // This is implied by esModuleInterop
        if self.ctx.allow_synthetic_default_imports() {
            return;
        }

        // Get span from declaration node
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::diagnostics::{diagnostic_messages, format_message};

        let has_export_equals = self.module_has_export_equals(module_specifier)
            || self.module_has_export_assignment_declaration(module_specifier);

        if has_export_equals {
            let message = format_message(
                diagnostic_messages::MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG,
                &[module_specifier, "allowSyntheticDefaultImports"],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_CAN_ONLY_BE_DEFAULT_IMPORTED_USING_THE_FLAG,
            );
            return;
        }

        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_DEFAULT_EXPORT,
            &[module_specifier],
        );
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT,
        );
    }

    pub(crate) fn module_has_export_equals(&self, module_specifier: &str) -> bool {
        if self
            .ctx
            .binder
            .module_exports
            .get(module_specifier)
            .is_some_and(|exports| exports.has("export="))
        {
            return true;
        }

        if self
            .resolve_cross_file_namespace_exports(module_specifier)
            .is_some_and(|exports| exports.has("export="))
        {
            return true;
        }

        false
    }

    /// Resolve a named export through an `export =` target's members.
    ///
    /// This supports declaration patterns like:
    /// `declare module "m" { namespace e { interface X {} } export = e }`
    /// where `import { X } from "m"` should resolve via the export-assignment target.
    pub(crate) fn resolve_named_export_via_export_equals(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol_export_named_member =
            |symbol: &tsz_binder::Symbol, member_name: &str| -> Option<tsz_binder::SymbolId> {
                if let Some(exports) = symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    return Some(sym_id);
                }
                if let Some(members) = symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    return Some(sym_id);
                }
                None
            };

        let lookup_symbol = |sym_id: tsz_binder::SymbolId| -> Option<&tsz_binder::Symbol> {
            if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                return Some(sym);
            }
            self.ctx
                .all_binders
                .as_ref()
                .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
        };

        let lookup_by_name = |name: &str| -> Vec<tsz_binder::SymbolId> {
            let mut result = self.ctx.binder.get_symbols().find_all_by_name(name);
            if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                for binder in all_binders.iter() {
                    for sym_id in binder.get_symbols().find_all_by_name(name) {
                        if !result.contains(&sym_id) {
                            result.push(sym_id);
                        }
                    }
                }
            }
            result
        };

        let resolve_from_exports =
            |exports: &tsz_binder::SymbolTable| -> Option<tsz_binder::SymbolId> {
                let export_equals_sym = exports.get("export=")?;
                let export_equals_symbol = lookup_symbol(export_equals_sym)?;

                if let Some(sym_id) = symbol_export_named_member(export_equals_symbol, export_name)
                {
                    return Some(sym_id);
                }

                // Namespace-merge fallback (function/class + namespace split symbols).
                let candidates = lookup_by_name(&export_equals_symbol.escaped_name);
                for candidate_id in candidates {
                    let Some(candidate_symbol) = lookup_symbol(candidate_id) else {
                        continue;
                    };
                    if (candidate_symbol.flags
                        & (symbol_flags::MODULE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE))
                        == 0
                    {
                        continue;
                    }
                    if let Some(sym_id) = symbol_export_named_member(candidate_symbol, export_name)
                    {
                        return Some(sym_id);
                    }
                }

                None
            };

        for candidate in module_specifier_candidates(module_specifier) {
            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate)
                && let Some(sym_id) = resolve_from_exports(exports)
            {
                return Some(sym_id);
            }
            if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                for binder in all_binders.iter() {
                    if let Some(exports) = binder.module_exports.get(&candidate)
                        && let Some(sym_id) = resolve_from_exports(exports)
                    {
                        return Some(sym_id);
                    }
                }
            }
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            if let Some(target_file_name) = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
                && let Some(exports) = target_binder.module_exports.get(&target_file_name)
                && let Some(sym_id) = resolve_from_exports(exports)
            {
                self.ctx
                    .cross_file_symbol_targets
                    .borrow_mut()
                    .insert(sym_id, target_idx);
                return Some(sym_id);
            }

            if let Some(exports) = target_binder.module_exports.get(module_specifier)
                && let Some(sym_id) = resolve_from_exports(exports)
            {
                self.ctx
                    .cross_file_symbol_targets
                    .borrow_mut()
                    .insert(sym_id, target_idx);
                return Some(sym_id);
            }
        }

        if let Some(exports) = self.resolve_cross_file_namespace_exports(module_specifier)
            && let Some(sym_id) = resolve_from_exports(&exports)
        {
            return Some(sym_id);
        }

        None
    }

    fn module_has_export_assignment_declaration(&self, module_specifier: &str) -> bool {
        self.ctx
            .resolve_import_target(module_specifier)
            .and_then(|file_idx| {
                self.ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(file_idx))
            })
            .is_some_and(|arena| {
                (0..arena.len()).any(|i| {
                    arena
                        .get(NodeIndex(i as u32))
                        .is_some_and(|node| node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT)
                })
            })
    }

    /// Emit TS2305 error when a module has no exported member with the given name.
    ///
    /// This is emitted when trying to use a named import (`import { X } from 'mod'`)
    /// but the module doesn't export a member named 'X'.
    pub(crate) fn emit_no_exported_member_error(
        &mut self,
        module_specifier: &str,
        member_name: &str,
        decl_node: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        // Only emit if report_unresolved_imports is enabled
        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Get span from declaration node
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let has_default =
            if let Some(exports_table) = self.resolve_effective_module_exports(module_specifier) {
                exports_table.has("default") || exports_table.has("export=")
            } else {
                false
            };

        use crate::diagnostics::{diagnostic_messages, format_message};
        if has_default && member_name != "default" {
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                &[module_specifier, member_name],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
            );
        } else {
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                &[module_specifier, member_name],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
            );
        }
    }

    /// Check if a module exists for cross-file resolution.
    ///
    /// Returns true if the module can be found via `resolved_modules` or through
    /// the context's cross-file resolution mechanism.
    pub(crate) fn module_exists_cross_file(&self, module_name: &str) -> bool {
        if self.ctx.resolve_import_target(module_name).is_some() {
            return true;
        }

        // Check if it's in resolved_modules (set by the driver for multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return true;
        }
        // Could add additional cross-file resolution checks here in the future
        false
    }
}
