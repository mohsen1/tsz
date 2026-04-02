//! Module resolution and cross-file exports for `CheckerState`.
//!
//! Constructor type operations have been extracted to
//! `type_resolution/constructors.rs`.

use crate::module_resolution::module_specifier_candidates;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_binder::symbol_flags;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn module_export_file_key_candidates(&self, file_name: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        let mut push_unique = |value: String| {
            if !candidates.contains(&value) {
                candidates.push(value);
            }
        };

        push_unique(file_name.to_string());

        let normalized = file_name.replace('\\', "/");
        if normalized != file_name {
            push_unique(normalized.clone());
        }

        for candidate in [file_name, normalized.as_str()] {
            if let Some(stripped) = candidate.strip_prefix("./") {
                push_unique(stripped.to_string());
            } else if !candidate.starts_with("../")
                && !candidate.starts_with('/')
                && !candidate.starts_with(".\\")
                && !candidate.starts_with("..\\")
            {
                push_unique(format!("./{candidate}"));
            }
        }

        candidates
    }

    pub(crate) fn module_exports_for_file<'b>(
        &self,
        binder: &'b tsz_binder::BinderState,
        file_name: &str,
    ) -> Option<&'b tsz_binder::SymbolTable> {
        self.module_export_file_key_candidates(file_name)
            .into_iter()
            .find_map(|candidate| binder.module_exports.get(&candidate))
    }

    fn keyed_exports_for_file<'b, T>(
        &self,
        map: &'b rustc_hash::FxHashMap<String, T>,
        file_name: &str,
    ) -> Option<&'b T> {
        self.module_export_file_key_candidates(file_name)
            .into_iter()
            .find_map(|candidate| map.get(&candidate))
    }

    fn resolve_module_augmentation_export_for_file(
        &self,
        file_idx: usize,
        export_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let resolve_augmentation_symbol = |binder: &tsz_binder::BinderState,
                                           aug: &tsz_binder::ModuleAugmentation|
         -> Option<tsz_binder::SymbolId> {
            let preferred_flags =
                symbol_flags::TYPE | symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE;

            let matches_augmentation_decl = |sym_id: tsz_binder::SymbolId| {
                let sym = binder.get_symbol(sym_id)?;
                (sym.declarations.contains(&aug.node) && (sym.flags & preferred_flags) != 0)
                    .then_some(sym_id)
            };

            if let Some(sym_id) = binder.get_node_symbol(aug.node)
                && let Some(preferred) = matches_augmentation_decl(sym_id)
            {
                return Some(preferred);
            }

            for candidate_id in binder.get_symbols().find_all_by_name(&aug.name) {
                if let Some(preferred) = matches_augmentation_decl(*candidate_id) {
                    return Some(preferred);
                }
            }

            binder.get_node_symbol(aug.node)
        };

        let mut resolved = None;
        let mut consider_augmentation =
            |module_spec: &str,
             augmenting_file_idx: usize,
             aug: &tsz_binder::ModuleAugmentation| {
                if aug.name != export_name {
                    return;
                }
                if self
                    .ctx
                    .resolve_import_target_from_file(augmenting_file_idx, module_spec)
                    != Some(file_idx)
                {
                    return;
                }
                let Some(binder) = self.ctx.get_binder_for_file(augmenting_file_idx) else {
                    return;
                };
                let Some(sym_id) = resolve_augmentation_symbol(binder, aug) else {
                    return;
                };
                if binder.get_symbol(sym_id).is_some() {
                    resolved = Some((sym_id, augmenting_file_idx));
                }
            };

        let augmentation_owner_file_idx = |aug: &tsz_binder::ModuleAugmentation| {
            aug.arena
                .as_deref()
                .and_then(|arena| self.ctx.get_file_idx_for_arena(arena))
                .unwrap_or(self.ctx.current_file_idx)
        };

        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for (module_spec, entries) in aug_index.iter() {
                for (augmenting_file_idx, aug) in entries {
                    consider_augmentation(module_spec, *augmenting_file_idx, aug);
                }
            }
            return resolved;
        }

        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (augmenting_file_idx, binder) in all_binders.iter().enumerate() {
                for (module_spec, augmentations) in &binder.module_augmentations {
                    for aug in augmentations {
                        consider_augmentation(module_spec, augmenting_file_idx, aug);
                    }
                }
            }
            return resolved;
        }

        for (module_spec, augmentations) in &self.ctx.binder.module_augmentations {
            for aug in augmentations {
                consider_augmentation(module_spec, augmentation_owner_file_idx(aug), aug);
            }
        }

        resolved
    }

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
                // Register TypeId → DefId so the TypeFormatter can display the
                // interface name (e.g., "Boolean") instead of its structural
                // expansion in TS2322 diagnostics. User-augmented global interfaces
                // have a different shape from the original lib type, so the
                // formatter's structural fallback (find_def_by_shape) can't find them.
                if type_id != TypeId::ERROR
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(name)
                {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    self.ctx
                        .definition_store
                        .register_type_to_def(type_id, def_id);
                }
                return Some(type_id);
            }
        }
        if let TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position(name_idx)
        {
            let mut result = self.type_reference_symbol_type(sym_id);
            if let Some(module_specifier) = self.resolve_named_import_module_for_local_name(name) {
                result = self.apply_module_augmentations(&module_specifier, name, result);
            }
            return Some(result);
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

    pub(crate) fn resolve_named_import_module_for_local_name(
        &self,
        local_name: &str,
    ) -> Option<String> {
        let source_file = self.ctx.arena.source_files.first()?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if import_decl.import_clause.is_none() {
                continue;
            }
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                continue;
            }
            let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &element_idx in &named_imports.elements.nodes {
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                    continue;
                };
                let Some(local_ident) = self
                    .ctx
                    .arena
                    .get(specifier.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                else {
                    continue;
                };
                if local_ident.escaped_text.as_str() != local_name {
                    continue;
                }
                let Some(module_node) = self.ctx.arena.get(import_decl.module_specifier) else {
                    continue;
                };
                let Some(module_literal) = self.ctx.arena.get_literal(module_node) else {
                    continue;
                };
                return Some(module_literal.text.clone());
            }
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
        self.resolve_cross_file_export_from_file(module_specifier, export_name, None)
    }

    /// Like `resolve_cross_file_export` but resolves the module specifier from
    /// a specific source file's perspective. This is needed when following
    /// cross-file re-export chains: symbol C from b.ts has `import_module = "./a"`,
    /// which is relative to b.ts, not the current file.
    pub(crate) fn resolve_cross_file_export_from_file(
        &self,
        module_specifier: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<tsz_binder::SymbolId> {
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

        // First, try to resolve the module specifier to a target file index.
        // When source_file_idx is provided, resolve from that file's perspective
        // (for following re-export chains where specifiers are relative to the
        // declaring file, not the current file).
        let from_file = source_file_idx.unwrap_or(self.ctx.current_file_idx);
        let target_file_idx = if let Some(from_file) = source_file_idx {
            self.ctx
                .resolve_import_target_from_file(from_file, module_specifier)
        } else {
            self.ctx.resolve_import_target(module_specifier)
        }?;

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

        if let Some(source_binder) = self.ctx.get_binder_for_file(from_file)
            && let Some((sym_id, _)) =
                source_binder.resolve_import_with_reexports_type_only(module_specifier, export_name)
        {
            return record_and_return(sym_id);
        }

        // Prefer the binder's type-aware export resolver so interface/type-only
        // exports reached through `import("./x").T` behave the same way as
        // regular type-node resolution.
        if let Some((sym_id, _)) =
            target_binder.resolve_import_with_reexports_type_only(&target_file_name, export_name)
        {
            return record_and_return(sym_id);
        }

        // Look up the export in the target binder's module_exports.
        // Prefer canonical file key, then module specifier fallback.
        if let Some(exports_table) = self.module_exports_for_file(target_binder, &target_file_name)
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

        if let Some((sym_id, augmenting_file_idx)) =
            self.resolve_module_augmentation_export_for_file(target_file_idx, export_name)
        {
            self.ctx
                .register_symbol_file_target(sym_id, augmenting_file_idx);
            return Some(sym_id);
        }

        // Last resort: check file_locals (for script files or binding edge cases
        // where module_exports wasn't populated).
        if let Some(sym_id) = target_binder.file_locals.get(export_name) {
            return record_and_return(sym_id);
        }

        None
    }

    pub(crate) fn resolve_export_from_table(
        &self,
        binder: &tsz_binder::BinderState,
        exports_table: &tsz_binder::SymbolTable,
        export_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        // When the module has `export =`, the default import MUST resolve to
        // the `export =` target, NOT to a member named "default" that may have
        // been copied from the target's static exports (e.g. `static default: "foo"`).
        // Check `export =` first for "default" lookups.
        if export_name == "default"
            && let Some(export_equals_sym_id) = exports_table.get("export=")
            && binder.get_symbol(export_equals_sym_id).is_some()
        {
            return Some(export_equals_sym_id);
        }

        if let Some(sym_id) = exports_table.get(export_name)
            && binder.get_symbol(sym_id).is_some()
        {
            return Some(sym_id);
        }

        let export_equals_sym_id = exports_table.get("export=")?;
        let export_equals_symbol = binder.get_symbol(export_equals_sym_id)?;

        // For non-"default" exports, the `export =` target's members are
        // searched below to support named import compatibility.
        // (The "default" case was already handled above.)

        if let Some(exports) = export_equals_symbol.exports.as_ref()
            && let Some(sym_id) = exports.get(export_name)
            && binder.get_symbol(sym_id).is_some()
        {
            return Some(sym_id);
        }

        if let Some(members) = export_equals_symbol.members.as_ref()
            && let Some(sym_id) = members.get(export_name)
            && binder.get_symbol(sym_id).is_some()
        {
            return Some(sym_id);
        }

        // Some binder paths keep the namespace merge partner as a distinct symbol.
        // Probe symbols with the same name and a module namespace shape.
        for &candidate_id in binder
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
                && binder.get_symbol(sym_id).is_some()
            {
                return Some(sym_id);
            }
            if let Some(members) = candidate_symbol.members.as_ref()
                && let Some(sym_id) = members.get(export_name)
                && binder.get_symbol(sym_id).is_some()
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
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        let binders = self.ctx.all_binders.as_ref()?;
        for (idx, binder) in binders.iter().enumerate() {
            if let Some(exports_table) = binder.module_exports.get(module_specifier)
                && let Some(sym_id) =
                    self.resolve_export_from_table(binder, exports_table, export_name)
            {
                return Some((sym_id, idx));
            }
        }
        None
    }

    /// Follow re-export chains across binder boundaries to find an exported symbol.
    /// Returns the `SymbolId` if the export is found via named or wildcard re-exports.
    /// Follow re-export chains across binder boundaries to find an exported symbol.
    /// Returns `(SymbolId, file_idx)` where `file_idx` is the actual file that owns
    /// the symbol, so callers can record the correct cross-file origin.
    pub(crate) fn resolve_export_in_file(
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

        // Check direct exports (module_exports)
        if let Some(exports) = self.module_exports_for_file(target_binder, &target_file_name)
            && let Some(sym_id) =
                self.resolve_export_from_table(target_binder, exports, export_name)
        {
            return Some((sym_id, file_idx));
        }

        // Check named re-exports before file_locals so that
        // `export { X } from './other'` is resolved through the chain.
        if let Some(reexports) =
            self.keyed_exports_for_file(&target_binder.reexports, &target_file_name)
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

        // Check wildcard re-exports before file_locals so that
        // `export * from './other'` is followed to the actual declaring file.
        // file_locals may contain merged globals that shadow re-exported symbols.
        if let Some(source_modules) =
            self.keyed_exports_for_file(&target_binder.wildcard_reexports, &target_file_name)
        {
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

        // Module augmentations should apply after direct exports and re-export chains,
        // so an augmentation does not mask a concrete exported declaration.
        if let Some((sym_id, augmenting_file_idx)) =
            self.resolve_module_augmentation_export_for_file(file_idx, export_name)
        {
            return Some((sym_id, augmenting_file_idx));
        }

        // Last resort: check file_locals (for script files or binding edge cases
        // where module_exports wasn't populated).
        // When looking for "default" and the module has `export =`, prefer the
        // `export =` target over a static member named "default".
        if export_name == "default"
            && let Some(sym_id) = target_binder.file_locals.get("export=")
        {
            return Some((sym_id, file_idx));
        }
        if let Some(sym_id) = target_binder.file_locals.get(export_name) {
            return Some((sym_id, file_idx));
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
            for (_, &sym_id) in table.iter() {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
        };

        // Try to find exports in the target binder's module_exports.
        // Prefer canonical file key first, then module specifier fallback.
        let direct_exports = self
            .module_exports_for_file(target_binder, &target_file_name)
            .or_else(|| target_binder.module_exports.get(module_specifier));

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            record_symbols(&combined);
            return Some(combined);
        }

        // No direct exports found, but the module may still re-export symbols
        // via `export * from './other'` or `export { X } from './other'`.
        // Collect re-exported symbols even when there are no direct exports.
        let has_reexports = self
            .keyed_exports_for_file(&target_binder.wildcard_reexports, &target_file_name)
            .is_some()
            || self
                .keyed_exports_for_file(&target_binder.reexports, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            // Return the table even if empty — the module exists but may have only
            // type-only exports (e.g., `export type * from '...'`). An empty namespace
            // object type is correct and will produce TS2339 for value access, instead
            // of falling through to "module not found" → TypeId::ANY.
            return Some(combined);
        }

        None
    }

    /// Like `resolve_cross_file_namespace_exports` but with a pre-resolved target file index.
    /// Used when the module specifier was already resolved from a different source file.
    fn resolve_cross_file_namespace_exports_for_file(
        &self,
        target_file_idx: usize,
        module_specifier: Option<&str>,
    ) -> Option<tsz_binder::SymbolTable> {
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        let record_symbols = |table: &tsz_binder::SymbolTable| {
            for (_, &sym_id) in table.iter() {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
        };

        let direct_exports = self
            .module_exports_for_file(target_binder, &target_file_name)
            .or_else(|| {
                module_specifier.and_then(|specifier| target_binder.module_exports.get(specifier))
            });

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            record_symbols(&combined);
            return Some(combined);
        }

        let has_reexports = self
            .keyed_exports_for_file(&target_binder.wildcard_reexports, &target_file_name)
            .is_some()
            || self
                .keyed_exports_for_file(&target_binder.reexports, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(target_file_idx, &mut combined, &mut visited);
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            return Some(combined);
        }

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
        self.resolve_effective_module_exports_from_file(module_specifier, None)
    }

    /// Like `resolve_effective_module_exports` but uses an explicit `resolution-mode`
    /// override from import attributes (e.g., `with { "resolution-mode": "require" }`).
    /// Falls back to the non-mode-aware path when no override is provided.
    pub(crate) fn resolve_effective_module_exports_with_mode(
        &self,
        module_specifier: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<tsz_binder::SymbolTable> {
        if let Some(mode) = resolution_mode {
            if let Some(target_idx) = self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_specifier,
                Some(mode),
            ) {
                if let Some(exports) = self.resolve_cross_file_namespace_exports_for_file(
                    target_idx,
                    Some(module_specifier),
                ) {
                    return Some(exports);
                }
                return Some(tsz_binder::SymbolTable::new());
            }
        }
        self.resolve_effective_module_exports(module_specifier)
    }

    /// Like `resolve_effective_module_exports` but optionally resolves relative paths
    /// from a specific source file. This is needed for cross-file namespace re-exports
    /// where the module specifier (e.g., `"./b"`) is relative to the declaring file,
    /// not the current file being checked.
    pub(crate) fn resolve_effective_module_exports_from_file(
        &self,
        module_specifier: &str,
        source_file_idx: Option<usize>,
    ) -> Option<tsz_binder::SymbolTable> {
        if let Some(source_idx) = source_file_idx
            && let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_idx, module_specifier)
            && let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
        {
            return Some(exports);
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
            && let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
        {
            return Some(exports);
        }

        for candidate in module_specifier_candidates(module_specifier) {
            // When resolving from a specific source file (cross-file symbol),
            // also try resolving the module specifier from that file's perspective
            if let Some(source_idx) = source_file_idx
                && let Some(target_idx) = self
                    .ctx
                    .resolve_import_target_from_file(source_idx, &candidate)
                && let Some(exports) =
                    self.resolve_cross_file_namespace_exports_for_file(target_idx, Some(&candidate))
            {
                return Some(exports);
            }

            if let Some(exports) = self.resolve_cross_file_namespace_exports(&candidate) {
                return Some(exports);
            }

            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate) {
                let mut combined = exports.clone();
                self.merge_export_equals_members(self.ctx.binder, exports, &mut combined);
                return Some(combined);
            }
        }

        None
    }

    fn resolve_ambient_module_namespace_exports(
        &self,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolTable> {
        let binders = self.ctx.all_binders.as_ref()?;
        // Use O(1) module binder index when available.
        if let Some(file_indices) = self.ctx.files_for_module_specifier(module_specifier) {
            for &file_idx in file_indices {
                if let Some(binder) = binders.get(file_idx)
                    && let Some(exports) = binder.module_exports.get(module_specifier)
                {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    return Some(combined);
                }
            }
        } else {
            for binder in binders.iter() {
                if let Some(exports) = binder.module_exports.get(module_specifier) {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    return Some(combined);
                }
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
                if name != "default" && !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }

        if let Some(symbol_members) = export_equals_symbol.members.as_ref() {
            for (name, sym_id) in symbol_members.iter() {
                if name != "default" && !combined.has(name) {
                    combined.set(name.to_string(), *sym_id);
                }
            }
        }
    }

    /// When `export =` targets a `typeof import("./...")` declaration, the binder symbol
    /// itself has no exports table. Re-hydrate the referenced module's named exports so
    /// namespace imports see the same surface as the imported module.
    pub(crate) fn merge_export_equals_import_type_members(
        &self,
        export_equals_symbol: &tsz_binder::Symbol,
        combined: &mut tsz_binder::SymbolTable,
    ) {
        if export_equals_symbol.decl_file_idx == u32::MAX {
            return;
        }
        let decl_file_idx = export_equals_symbol.decl_file_idx as usize;
        if self.ctx.get_binder_for_file(decl_file_idx).is_none() {
            return;
        }
        let arena = self.ctx.get_arena_for_file(decl_file_idx as u32);

        let decl_idx = if export_equals_symbol.value_declaration.is_some() {
            export_equals_symbol.value_declaration
        } else if let Some(&decl_idx) = export_equals_symbol.declarations.first() {
            decl_idx
        } else {
            return;
        };
        let Some(node) = arena.get(decl_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return;
        }
        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return;
        };
        let type_idx = var_decl.type_annotation;
        if !type_idx.is_some() {
            return;
        }
        let Some(module_specifier) =
            self.import_type_module_specifier_from_type_node(arena, type_idx)
        else {
            return;
        };

        let Some(nested_exports) =
            self.resolve_effective_module_exports_from_file(&module_specifier, Some(decl_file_idx))
        else {
            return;
        };

        for (name, sym_id) in nested_exports.iter() {
            if name != "export=" && !combined.has(name) {
                combined.set(name.to_string(), *sym_id);
            }
        }
    }

    fn import_type_module_specifier_from_type_node(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(type_idx)?;
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let type_query = arena.get_type_query(node)?;
        let call_idx = self.leftmost_import_call_in_entity_name(arena, type_query.expr_name)?;
        let call = arena.get_call_expr(arena.get(call_idx)?)?;
        let args = call.arguments.as_ref()?;
        let &first_arg = args.nodes.first()?;
        let arg_node = arena.get(first_arg)?;
        let literal = arena.get_literal(arg_node)?;
        Some(literal.text.clone())
    }

    fn leftmost_import_call_in_entity_name(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        mut idx: NodeIndex,
    ) -> Option<NodeIndex> {
        const MAX_DEPTH: usize = 64;
        for _ in 0..MAX_DEPTH {
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = arena.get_qualified_name(node)?;
                idx = qn.left;
                continue;
            }
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }
            let call = arena.get_call_expr(node)?;
            let expr_node = arena.get(call.expression)?;
            return (expr_node.kind == SyntaxKind::ImportKeyword as u16).then_some(idx);
        }
        None
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
        if let Some(source_modules) =
            self.keyed_exports_for_file(&target_binder.wildcard_reexports, &target_file_name)
        {
            let source_modules = source_modules.clone();
            // Get type-only flags for wildcard re-exports to skip `export type *` members
            let type_only_flags = self
                .keyed_exports_for_file(
                    &target_binder.wildcard_reexports_type_only,
                    &target_file_name,
                )
                .cloned();
            for (i, source_module) in source_modules.iter().enumerate() {
                // Skip `export type * from '...'` — these exports should not appear as
                // value properties on the namespace object. They are only accessible in
                // type position via symbol-based resolution.
                let is_type_only = type_only_flags
                    .as_ref()
                    .and_then(|flags| flags.get(i).map(|(_, is_to)| *is_to))
                    .unwrap_or(false);
                if is_type_only {
                    continue;
                }
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && let Some(source_binder) = self.ctx.get_binder_for_file(source_idx)
                {
                    // Add all exports from the source module
                    let source_file_name = self
                        .ctx
                        .get_arena_for_file(source_idx as u32)
                        .source_files
                        .first()
                        .map(|sf| sf.file_name.clone());
                    if let Some(source_file_name) = source_file_name
                        && let Some(exports) =
                            self.module_exports_for_file(source_binder, &source_file_name)
                    {
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
        if let Some(reexports) =
            self.keyed_exports_for_file(&target_binder.reexports, &target_file_name)
        {
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
            // For Node.js built-in modules, use TS2591 instead of TS2307
            let (error_message, error_code) = {
                let (msg, code) = self.module_not_found_diagnostic(module_specifier);
                if code != error.code {
                    (msg, code) // module_not_found_diagnostic upgraded to TS2591
                } else if error.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                    && self.ctx.compiler_options.implied_classic_resolution
                {
                    use crate::diagnostics::{diagnostic_messages, format_message};
                    (
                        format_message(
                            diagnostic_messages::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                            &[module_specifier],
                        ),
                        diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                    )
                } else {
                    (error.message.clone(), error.code)
                }
            };
            self.error(start, length, error_message, error_code);
            return;
        }

        // Fallback: use centralized module_not_found_diagnostic which handles
        // Node.js built-in module substitution (TS2591) and Classic resolution (TS2792).
        let (message, code) = self.module_not_found_diagnostic(module_specifier);
        self.error(start, length, message, code);
    }

    /// Emit TS1192 error when a module has no default export, or TS2732 for JSON files.
    ///
    /// This is emitted when trying to use a default import (`import X from 'mod'`)
    /// but the module doesn't export a default binding.
    ///
    /// For JSON files (.json extension), emits TS2732 when `resolveJsonModule` is disabled,
    /// suggesting to enable the flag. This takes precedence over TS1192.
    ///
    /// Note: TS1192 is only suppressed when synthetic default imports are
    /// enabled for CommonJS-shaped modules. Pure ESM modules still require an
    /// explicit `default` export.
    pub(crate) fn emit_no_default_export_error(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
        is_source_file_import: bool,
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

        let has_json_default_export =
            self.module_has_json_default_export(module_specifier, Some(self.ctx.current_file_idx));

        if let Some(specifier_node) = named_default_specifier_node {
            if has_json_default_export {
                return;
            }
            self.emit_no_exported_member_error(module_specifier, "default", specifier_node);
            return;
        }

        // Check if this is a JSON file import.
        // - Without resolveJsonModule: TS2732 takes precedence over TS1192.
        // - With resolveJsonModule: JSON modules always have a default export
        //   (the parsed JSON content), so TS1192 must be suppressed.
        // IMPORTANT: This check must come BEFORE report_unresolved_imports guard
        // because TS2732 should be emitted even in single-file mode.
        if has_json_default_export {
            return;
        }
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

        // In `module: system`, source `.ts` files can still be default-imported
        // through the module namespace object even when
        // `allowSyntheticDefaultImports` is explicitly false.
        if is_source_file_import
            && self.ctx.compiler_options.module == tsz_common::common::ModuleKind::System
            && !self.module_has_export_equals(module_specifier)
            && !self.module_has_export_assignment_declaration(module_specifier)
        {
            return;
        }

        // allowSyntheticDefaultImports suppresses TS1192 for non-source-file modules
        // (.d.ts, .js) that can use synthetic default imports. For .ts source files,
        // tsc always emits TS1192 when there is no default export — the developer
        // should add an explicit `export default`.
        //
        // When esModuleInterop is true, tsc always suppresses TS1192 for .d.ts
        // imports because the interop helper synthesizes default exports for all
        // module formats. The file_is_esm_map marks all files as ESM when the
        // compiler module is ES2015+, but this should not prevent suppression
        // when esModuleInterop explicitly enables synthetic defaults.
        //
        // When only allowSyntheticDefaultImports is true (without esModuleInterop),
        // suppression applies to CJS-shaped modules. ESM .d.ts files (from packages
        // with "type": "module") still require an explicit default export.
        if self.ctx.allow_synthetic_default_imports() && !is_source_file_import {
            // esModuleInterop: suppress TS1192 for non-source-file imports unless
            // the module is from a genuine ESM package (e.g., node_modules with
            // package.json "type": "module"). The file_is_esm_map marks all files
            // as ESM when the compiler module is ES2015+, so module_is_esm alone
            // cannot distinguish "ESM because of package" vs "ESM because of
            // compiler mode". We additionally check if the file is in node_modules
            // to identify genuine package ESM.
            if self.ctx.compiler_options.es_module_interop {
                let is_package_esm = self.module_is_esm(module_specifier)
                    && self.module_file_is_in_node_modules(module_specifier);
                if !is_package_esm {
                    return;
                }
            }
            if self.module_can_use_synthetic_default_import(module_specifier) {
                return;
            }
            // For non-source-file imports (.d.ts), also suppress when the module is
            // not positively identified as ESM. Plain .d.ts files without a "type":
            // "module" package.json are assumed to be CJS-compatible.
            if !self.module_is_esm(module_specifier) {
                return;
            }
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
            // TS1259: "Module X can only be default-imported using the 'allowSyntheticDefaultImports' flag"
            // Only emitted for export= modules when allowSyntheticDefaultImports is false.
            if !self.ctx.allow_synthetic_default_imports() {
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
            }
            return;
        }

        // TS1192: "Module X has no default export"
        // tsc formats the module name as the symbol name (without ./ prefix),
        // wrapped in double quotes, e.g., Module '"server"' has no default export.
        let display_name = self.imported_namespace_display_module_name(module_specifier);
        let quoted_name = format!("\"{display_name}\"");
        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_DEFAULT_EXPORT,
            &[&quoted_name],
        );
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT,
        );
    }

    pub(crate) fn module_has_json_default_export(
        &mut self,
        module_specifier: &str,
        source_file_idx: Option<usize>,
    ) -> bool {
        self.json_module_type_for_module(module_specifier, source_file_idx)
            .is_some()
    }

    pub(crate) fn module_can_use_synthetic_default_import(
        &mut self,
        module_specifier: &str,
    ) -> bool {
        if self.module_has_export_equals(module_specifier)
            || self.module_has_export_assignment_declaration(module_specifier)
        {
            return true;
        }

        if self
            .resolve_js_export_surface_for_module(module_specifier, Some(self.ctx.current_file_idx))
            .is_some_and(|surface| surface.has_commonjs_exports)
        {
            return true;
        }

        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();

        if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            return true;
        }
        if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            return false;
        }

        self.ctx
            .file_is_esm_map
            .as_ref()
            .and_then(|map| map.get(file_name))
            .is_some_and(|is_esm| !*is_esm)
    }

    /// Check if the target module's resolved file is in a `node_modules` directory.
    /// This helps distinguish between files that are ESM because of their package
    /// context vs files that are ESM because of the compiler's module setting.
    fn module_file_is_in_node_modules(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        source_file.file_name.contains("node_modules")
    }

    /// Check if the target module is a pure ESM module (from a package with
    /// `"type": "module"` or using `.mjs`/`.mts` extension).
    fn module_is_esm(&self, module_specifier: &str) -> bool {
        let Some(target_idx) = self.ctx.resolve_import_target(module_specifier) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();

        if file_name.ends_with(".mjs") || file_name.ends_with(".mts") {
            return true;
        }
        if file_name.ends_with(".cjs") || file_name.ends_with(".cts") {
            return false;
        }

        self.ctx
            .file_is_esm_map
            .as_ref()
            .and_then(|map| map.get(file_name))
            .copied()
            .unwrap_or(false)
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
            // O(1) fast-path: check resolve_symbol_file_index before O(N) binder scan
            {
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
            }
            self.ctx
                .all_binders
                .as_ref()
                .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
        };

        let lookup_by_name = |name: &str| -> Vec<tsz_binder::SymbolId> {
            let mut result: Vec<tsz_binder::SymbolId> = self
                .ctx
                .binder
                .get_symbols()
                .find_all_by_name(name)
                .to_vec();
            if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                for binder in all_binders.iter() {
                    for &sym_id in binder.get_symbols().find_all_by_name(name) {
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
                if export_name == "default" {
                    return Some(export_equals_sym);
                }
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
                if let Some(file_indices) = self.ctx.files_for_module_specifier(&candidate) {
                    for &file_idx in file_indices {
                        if let Some(binder) = all_binders.get(file_idx)
                            && let Some(exports) = binder.module_exports.get(&candidate)
                            && let Some(sym_id) = resolve_from_exports(exports)
                        {
                            return Some(sym_id);
                        }
                    }
                } else {
                    for binder in all_binders.iter() {
                        if let Some(exports) = binder.module_exports.get(&candidate)
                            && let Some(sym_id) = resolve_from_exports(exports)
                        {
                            return Some(sym_id);
                        }
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
                self.ctx.register_symbol_file_target(sym_id, target_idx);
                return Some(sym_id);
            }

            if let Some(exports) = target_binder.module_exports.get(module_specifier)
                && let Some(sym_id) = resolve_from_exports(exports)
            {
                self.ctx.register_symbol_file_target(sym_id, target_idx);
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
        // TSC includes source-level quotes in module diagnostic messages
        let quoted_module = format!("\"{module_specifier}\"");
        if has_default && member_name != "default" {
            let message = format_message(
                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                &[&quoted_module, member_name],
            );
            self.error(
                start,
                length,
                message,
                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
            );
        } else {
            // Check for spelling suggestions (TS2724) before TS2305
            let suggestion = self
                .resolve_effective_module_exports(module_specifier)
                .and_then(|exports| {
                    let export_names: Vec<&str> =
                        exports.iter().map(|(name, _)| name.as_str()).collect();
                    tsz_parser::parser::spelling::get_spelling_suggestion(
                        member_name,
                        &export_names,
                    )
                    .map(|s| s.to_string())
                });

            if let Some(suggestion) = suggestion {
                let message = format_message(
                    diagnostic_messages::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                    &[&quoted_module, member_name, &suggestion],
                );
                self.error(
                    start,
                    length,
                    message,
                    diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                );
            } else {
                let message = format_message(
                    diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                    &[&quoted_module, member_name],
                );
                self.error(
                    start,
                    length,
                    message,
                    diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                );
            }
        }
    }

    /// Check if a module exists for cross-file resolution.
    ///
    /// Returns true if the module can be found via `resolved_modules`, through
    /// the context's cross-file resolution mechanism, or via global binder indices.
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

        // O(1) check via global_module_binder_index: any binder with module_exports
        // for this specifier means the module exists as an ambient declaration.
        if self.ctx.files_for_module_specifier(module_name).is_some() {
            return true;
        }

        // O(1) check via global_declared_modules: covers `declare module "X"` and
        // shorthand ambient modules across all files.
        if let Some(declared) = &self.ctx.global_declared_modules {
            let normalized = module_name.trim().trim_matches('"').trim_matches('\'');
            if declared.exact.contains(normalized) {
                return true;
            }
            // Small linear scan over wildcard patterns only
            for pattern in &declared.patterns {
                let p = pattern.trim().trim_matches('"').trim_matches('\'');
                if let Some(prefix) = p.strip_suffix('*')
                    && normalized.starts_with(prefix)
                {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::CheckerState;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::module_resolution::build_module_resolution_maps;
    use crate::query_boundaries::common::TypeInterner;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    #[test]
    fn module_augmentation_export_resolution_prefers_local_alias_over_named_reexport() {
        let files = [
            (
                "/main.ts",
                r#"
import { Row2 } from "./index";
type Use = Row2;
"#,
            ),
            (
                "/a.d.ts",
                r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
export type { Row2 } from "./common";
"#,
            ),
            (
                "/common.d.ts",
                r#"
export interface Row2 { b: string }
"#,
            ),
        ];

        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

        for (name, source) in &files {
            let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let entry_idx = file_names
            .iter()
            .position(|name| name == "/main.ts")
            .expect("entry file should exist");
        let augmentation_idx = file_names
            .iter()
            .position(|name| name == "/a.d.ts")
            .expect("augmentation file should exist");
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[entry_idx].as_ref(),
            all_binders[entry_idx].as_ref(),
            &types,
            file_names[entry_idx].clone(),
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );

        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(entry_idx);
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);
        checker.check_source_file(roots[entry_idx]);

        let sym_id = checker
            .resolve_cross_file_export("./index", "Row2")
            .expect("Row2 should resolve through the module augmentation export");

        assert_eq!(
            checker.ctx.resolve_symbol_file_index(sym_id),
            Some(augmentation_idx),
            "Expected Row2 to resolve to the augmentation file, not the reexport source"
        );
    }
}
