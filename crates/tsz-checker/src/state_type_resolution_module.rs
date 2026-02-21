//! Module resolution, cross-file exports, and constructor type operations
//! for `CheckerState`.

use crate::module_resolution::module_specifier_candidates;
use crate::query_boundaries::state_type_resolution as query;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
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
        _binder: &tsz_binder::BinderState,
        exports: &tsz_binder::SymbolTable,
        combined: &mut tsz_binder::SymbolTable,
    ) {
        let Some(mut export_equals_sym_id) = exports.get("export=") else {
            return;
        };

        let mut visited = Vec::new();
        if let Some(resolved) = self.resolve_alias_symbol(export_equals_sym_id, &mut visited) {
            export_equals_sym_id = resolved;
        }

        let Some(export_equals_symbol) = self.get_symbol_globally(export_equals_sym_id) else {
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
                exports_table.has("default")
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

    pub(crate) fn apply_type_arguments_to_constructor_type(
        &mut self,
        ctor_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use tsz_solver::CallableShape;

        let Some(type_arguments) = type_arguments else {
            return ctor_type;
        };

        if type_arguments.nodes.is_empty() {
            return ctor_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return ctor_type;
        }

        let Some(shape) = query::callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };
        let mut matching: Vec<&tsz_solver::CallSignature> = shape
            .construct_signatures
            .iter()
            .filter(|sig| sig.type_params.len() == type_args.len())
            .collect();

        if matching.is_empty() {
            matching = shape
                .construct_signatures
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .collect();
        }

        if matching.is_empty() {
            return ctor_type;
        }

        let instantiated_constructs: Vec<tsz_solver::CallSignature> = matching
            .iter()
            .map(|sig| {
                {
                    let app_info = query::get_application_info(self.ctx.types, sig.return_type)
                        .map(|(base, args)| format!("base={base:?} args={args:?}"))
                        .unwrap_or_default();
                    tracing::trace!(
                        ?sig.return_type,
                        %app_info,
                        type_params_count = sig.type_params.len(),
                        "apply_type_args_to_ctor: BEFORE instantiation"
                    );
                }
                let mut args = type_args.clone();
                if args.len() < sig.type_params.len() {
                    for param in sig.type_params.iter().skip(args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        args.push(fallback);
                    }
                }
                if args.len() > sig.type_params.len() {
                    args.truncate(sig.type_params.len());
                }
                let result = self.instantiate_constructor_signature(sig, &args);
                {
                    let app_info = query::get_application_info(self.ctx.types, result.return_type)
                        .map(|(base, args)| format!("base={base:?} args={args:?}"))
                        .unwrap_or_default();
                    tracing::trace!(
                        ?result.return_type,
                        %app_info,
                        "apply_type_args_to_ctor: AFTER instantiation"
                    );
                }
                result
            })
            .collect();

        let new_shape = CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: instantiated_constructs,
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: None,
        };
        let factory = self.ctx.types.factory();
        factory.callable(new_shape)
    }

    /// Apply explicit type arguments to a callable type for function calls.
    ///
    /// When a function is called with explicit type arguments like `fn<T>(x: T)`,
    /// calling it as `fn<number>("hello")` should substitute `T` with `number` and
    /// then check if `"hello"` is assignable to `number`.
    ///
    /// This function creates a new callable type with the type parameters substituted,
    /// so that argument type checking can work correctly.
    pub(crate) fn apply_type_arguments_to_callable_type(
        &mut self,
        callee_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use tsz_solver::CallableShape;

        let Some(type_arguments) = type_arguments else {
            return callee_type;
        };

        if type_arguments.nodes.is_empty() {
            return callee_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return callee_type;
        }

        // Resolve Lazy types before classification.
        let callee_type = {
            let resolved = self.resolve_lazy_type(callee_type);
            if resolved != callee_type {
                resolved
            } else {
                callee_type
            }
        };
        let factory = self.ctx.types.factory();
        match query::classify_for_signatures(self.ctx.types, callee_type) {
            query::SignatureTypeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);

                // Find call signatures that match the type argument count
                let mut matching: Vec<&tsz_solver::CallSignature> = shape
                    .call_signatures
                    .iter()
                    .filter(|sig| sig.type_params.len() == type_args.len())
                    .collect();

                // If no exact match, try signatures with type params
                if matching.is_empty() {
                    matching = shape
                        .call_signatures
                        .iter()
                        .filter(|sig| !sig.type_params.is_empty())
                        .collect();
                }

                if matching.is_empty() {
                    return callee_type;
                }

                // Instantiate each matching signature with the type arguments
                let instantiated_calls: Vec<tsz_solver::CallSignature> = matching
                    .iter()
                    .map(|sig| {
                        let mut args = type_args.clone();
                        // Fill in default type arguments if needed
                        if args.len() < sig.type_params.len() {
                            for param in sig.type_params.iter().skip(args.len()) {
                                let fallback = param
                                    .default
                                    .or(param.constraint)
                                    .unwrap_or(TypeId::UNKNOWN);
                                args.push(fallback);
                            }
                        }
                        if args.len() > sig.type_params.len() {
                            args.truncate(sig.type_params.len());
                        }
                        self.instantiate_call_signature(sig, &args)
                    })
                    .collect();

                let new_shape = CallableShape {
                    call_signatures: instantiated_calls,
                    construct_signatures: shape.construct_signatures.clone(),
                    properties: shape.properties.clone(),
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                    symbol: None,
                };
                factory.callable(new_shape)
            }
            query::SignatureTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.type_params.len() != type_args.len() {
                    return callee_type;
                }

                let instantiated_call = self.instantiate_call_signature(
                    &tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: None,
                        return_type: shape.return_type,
                        type_predicate: None,
                        is_method: shape.is_method,
                    },
                    &type_args,
                );

                // Convert single signature to callable
                let new_shape = CallableShape {
                    call_signatures: vec![instantiated_call],
                    construct_signatures: vec![],
                    properties: vec![],
                    string_index: None,
                    number_index: None,
                    symbol: None,
                };
                factory.callable(new_shape)
            }
            _ => callee_type,
        }
    }

    pub(crate) fn base_constructor_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if let Some(name) = self.heritage_name_text(expr_idx) {
            // Filter out primitive types and literals that cannot be used in class extends
            if matches!(
                name.as_str(),
                "null"
                    | "undefined"
                    | "true"
                    | "false"
                    | "void"
                    | "0"
                    | "number"
                    | "string"
                    | "boolean"
                    | "never"
                    | "unknown"
                    | "any"
            ) {
                return None;
            }
        }
        let expr_type = self.get_type_of_node(expr_idx);
        tracing::debug!(?expr_type, "base_constructor_type: expr_type");

        // Evaluate application types to get the actual intersection type
        let evaluated_type = self.evaluate_application_type(expr_type);
        tracing::debug!(?evaluated_type, "base_constructor_type: evaluated_type");

        let ctor_types = self.constructor_types_from_type(evaluated_type);
        tracing::debug!(?ctor_types, "base_constructor_type: ctor_types");
        if ctor_types.is_empty() {
            return None;
        }
        let ctor_type = if ctor_types.len() == 1 {
            ctor_types[0]
        } else {
            let factory = self.ctx.types.factory();
            factory.intersection(ctor_types)
        };
        Some(self.apply_type_arguments_to_constructor_type(ctor_type, type_arguments))
    }

    pub(crate) fn constructor_types_from_type(&mut self, type_id: TypeId) -> Vec<TypeId> {
        use rustc_hash::FxHashSet;

        self.ensure_relation_input_ready(type_id);
        let mut ctor_types = Vec::new();
        let mut visited = FxHashSet::default();
        self.collect_constructor_types_from_type_inner(type_id, &mut ctor_types, &mut visited);
        ctor_types
    }

    pub(crate) fn collect_constructor_types_from_type_inner(
        &mut self,
        type_id: TypeId,
        ctor_types: &mut Vec<TypeId>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        // Resolve Lazy types (e.g., interface references) so the classifier
        // can see the actual type structure (Callable with construct signatures)
        // rather than the opaque Lazy wrapper.
        let evaluated = {
            let resolved = self.resolve_lazy_type(evaluated);
            if resolved != evaluated {
                resolved
            } else {
                evaluated
            }
        };
        if !visited.insert(evaluated) {
            return;
        }

        let classification = query::classify_constructor_type(self.ctx.types, evaluated);
        match classification {
            query::ConstructorTypeKind::Callable => {
                ctor_types.push(evaluated);
            }
            query::ConstructorTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.is_constructor {
                    ctor_types.push(evaluated);
                }
            }
            query::ConstructorTypeKind::Members(members) => {
                for member in members {
                    self.collect_constructor_types_from_type_inner(member, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::Inner(inner) => {
                self.collect_constructor_types_from_type_inner(inner, ctor_types, visited);
            }
            query::ConstructorTypeKind::Constraint(constraint) => {
                if let Some(constraint) = constraint {
                    self.collect_constructor_types_from_type_inner(constraint, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::NeedsTypeEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::TypeQuery(sym_ref) => {
                // typeof X - get the type of the symbol X and collect constructors from it
                use tsz_binder::SymbolId;
                let sym_id = SymbolId(sym_ref.0);
                let sym_type = self.get_type_of_symbol(sym_id);
                self.collect_constructor_types_from_type_inner(sym_type, ctor_types, visited);
            }
            query::ConstructorTypeKind::NotConstructor => {}
        }
    }

    pub(crate) fn static_properties_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo> {
        use rustc_hash::{FxHashMap, FxHashSet};

        self.ensure_relation_input_ready(type_id);
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        self.collect_static_properties_from_type_inner(type_id, &mut props, &mut visited);
        props
    }

    pub(crate) fn collect_static_properties_from_type_inner(
        &mut self,
        type_id: TypeId,
        props: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        // Resolve Lazy types so the classifier sees actual type structure.
        let evaluated = {
            let resolved = self.resolve_lazy_type(evaluated);
            if resolved != evaluated {
                resolved
            } else {
                evaluated
            }
        };
        if !visited.insert(evaluated) {
            return;
        }

        match query::static_property_source(self.ctx.types, evaluated) {
            query::StaticPropertySource::Properties(properties) => {
                for prop in properties {
                    props.entry(prop.name).or_insert(prop);
                }
            }
            query::StaticPropertySource::RecurseMembers(members) => {
                for member in members {
                    self.collect_static_properties_from_type_inner(member, props, visited);
                }
            }
            query::StaticPropertySource::RecurseSingle(inner) => {
                self.collect_static_properties_from_type_inner(inner, props, visited);
            }
            query::StaticPropertySource::NeedsEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            query::StaticPropertySource::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            query::StaticPropertySource::None => {}
        }
    }

    pub(crate) fn base_instance_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        let ctor_type = self.base_constructor_type_from_expression(expr_idx, type_arguments)?;
        self.instance_type_from_constructor_type(ctor_type)
    }

    pub(crate) fn merge_constructor_properties_from_type(
        &mut self,
        ctor_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
    ) {
        let base_props = self.static_properties_from_type(ctor_type);
        for (name, prop) in base_props {
            properties.entry(name).or_insert(prop);
        }
    }

    pub(crate) fn merge_base_instance_properties(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        string_index: &mut Option<tsz_solver::IndexSignature>,
        number_index: &mut Option<tsz_solver::IndexSignature>,
    ) {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.merge_base_instance_properties_inner(
            base_instance_type,
            properties,
            string_index,
            number_index,
            &mut visited,
        );
    }

    pub(crate) fn merge_base_instance_properties_inner(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        string_index: &mut Option<tsz_solver::IndexSignature>,
        number_index: &mut Option<tsz_solver::IndexSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        // Resolve Lazy types so the classifier can see the actual structure.
        let base_instance_type = {
            let resolved = self.resolve_lazy_type(base_instance_type);
            if resolved != base_instance_type {
                resolved
            } else {
                base_instance_type
            }
        };
        if !visited.insert(base_instance_type) {
            return;
        }

        match query::classify_for_base_instance_merge(self.ctx.types, base_instance_type) {
            query::BaseInstanceMergeKind::Object(base_shape_id) => {
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                for base_prop in &base_shape.properties {
                    properties
                        .entry(base_prop.name)
                        .or_insert_with(|| base_prop.clone());
                }
                if let Some(ref idx) = base_shape.string_index {
                    Self::merge_index_signature(string_index, idx.clone());
                }
                if let Some(ref idx) = base_shape.number_index {
                    Self::merge_index_signature(number_index, idx.clone());
                }
            }
            query::BaseInstanceMergeKind::Intersection(members) => {
                for member in members {
                    self.merge_base_instance_properties_inner(
                        member,
                        properties,
                        string_index,
                        number_index,
                        visited,
                    );
                }
            }
            query::BaseInstanceMergeKind::Union(members) => {
                use rustc_hash::FxHashMap;
                let mut common_props: Option<FxHashMap<Atom, tsz_solver::PropertyInfo>> = None;
                let mut common_string_index: Option<tsz_solver::IndexSignature> = None;
                let mut common_number_index: Option<tsz_solver::IndexSignature> = None;

                for member in members {
                    let mut member_props: FxHashMap<Atom, tsz_solver::PropertyInfo> =
                        FxHashMap::default();
                    let mut member_string_index = None;
                    let mut member_number_index = None;
                    let mut member_visited = rustc_hash::FxHashSet::default();
                    member_visited.insert(base_instance_type);

                    self.merge_base_instance_properties_inner(
                        member,
                        &mut member_props,
                        &mut member_string_index,
                        &mut member_number_index,
                        &mut member_visited,
                    );

                    if common_props.is_none() {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        continue;
                    }

                    let mut props = match common_props.take() {
                        Some(props) => props,
                        None => {
                            // This should never happen due to the check above, but handle gracefully
                            common_props = Some(member_props);
                            common_string_index = member_string_index;
                            common_number_index = member_number_index;
                            continue;
                        }
                    };
                    props.retain(|name, prop| {
                        let Some(member_prop) = member_props.get(name) else {
                            return false;
                        };
                        let merged_type = if prop.type_id == member_prop.type_id {
                            prop.type_id
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.type_id, member_prop.type_id])
                        };
                        let merged_write_type = if prop.write_type == member_prop.write_type {
                            prop.write_type
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.write_type, member_prop.write_type])
                        };
                        prop.type_id = merged_type;
                        prop.write_type = merged_write_type;
                        prop.optional |= member_prop.optional;
                        prop.readonly &= member_prop.readonly;
                        prop.is_method &= member_prop.is_method;
                        true
                    });
                    common_props = Some(props);

                    common_string_index = match (common_string_index.take(), member_string_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };
                    common_number_index = match (common_number_index.take(), member_number_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };

                    if common_props
                        .as_ref()
                        .is_none_or(std::collections::HashMap::is_empty)
                        && common_string_index.is_none()
                        && common_number_index.is_none()
                    {
                        break;
                    }
                }

                if let Some(props) = common_props {
                    for prop in props.into_values() {
                        properties.entry(prop.name).or_insert(prop);
                    }
                }
                if let Some(idx) = common_string_index {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = common_number_index {
                    Self::merge_index_signature(number_index, idx);
                }
            }
            query::BaseInstanceMergeKind::Other => {}
        }
    }

    /// Check if a node is inside a type parameter declaration (constraint or default).
    /// Used to skip TS2344 validation for type args in type parameter constraints/defaults.
    pub(crate) fn is_inside_type_parameter_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        for _ in 0..10 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |e| e.parent);
            if parent.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                if parent_node.kind == syntax_kind_ext::TYPE_PARAMETER {
                    return true;
                }
                // Stop at declaration-level nodes
                if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                {
                    return false;
                }
            }
            current = parent;
        }
        false
    }

    /// Check if a class extends a type parameter and is "transparent" (adds no new instance members).
    ///
    /// When a class expression extends a generic type parameter but adds no new instance properties
    /// or methods (only has a constructor), it should be typed as that type parameter to maintain
    /// generic compatibility. This is common in simple wrapper patterns.
    ///
    /// # Returns
    /// - `Some(TypeId)` if the class extends a type parameter and has no additional instance members
    /// - `None` otherwise
    pub(crate) fn get_extends_type_parameter_if_transparent(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<TypeId> {
        // Check if class has an extends clause with a type parameter
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        let mut extends_type_param = None;
        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;

            // Only process extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;

            // Handle ExpressionWithTypeArguments
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            // Get the type of the extends expression
            let base_type = self.get_type_of_node(expr_idx);

            // Check if this is a type parameter
            if query::is_type_parameter(self.ctx.types, base_type) {
                extends_type_param = Some(base_type);
                break;
            }
        }

        let base_type_param = extends_type_param?;

        // Check if class adds any new instance members (excluding constructor)
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Skip constructors and static members
            match member_node.kind {
                k if k == syntax_kind_ext::CONSTRUCTOR => continue,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                        // Skip static properties
                        if self.has_static_modifier(&prop.modifiers) {
                            continue;
                        }
                        // Found an instance property - class is not transparent
                        return None;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                        // Skip static methods
                        if self.has_static_modifier(&method.modifiers) {
                            continue;
                        }
                        // Found an instance method - class is not transparent
                        return None;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                        // Skip static accessors
                        if self.has_static_modifier(&accessor.modifiers) {
                            continue;
                        }
                        // Found an instance accessor - class is not transparent
                        return None;
                    }
                }
                _ => {
                    // Other member types - be conservative
                    continue;
                }
            }
        }

        // Class is transparent - return the type parameter
        Some(base_type_param)
    }
}
