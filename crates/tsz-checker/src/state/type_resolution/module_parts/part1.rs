impl<'a> CheckerState<'a> {
    pub(crate) fn emit_module_has_no_default_export_at(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

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

    pub(crate) fn source_file_idx_is_js_with_esm_syntax(&self, target_idx: usize) -> bool {
        if !self.source_file_idx_has_esm_syntax(target_idx) {
            return false;
        }
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        if source_file.is_declaration_file {
            return false;
        }
        let file_name = source_file.file_name.as_str();
        file_name.ends_with(".js")
            || file_name.ends_with(".jsx")
            || file_name.ends_with(".mjs")
            || file_name.ends_with(".cjs")
    }

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
                (sym.declarations.contains(&aug.node) && sym.has_any_flags(preferred_flags))
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
                for (module_spec, augmentations) in binder.module_augmentations.iter() {
                    for aug in augmentations {
                        consider_augmentation(module_spec, augmenting_file_idx, aug);
                    }
                }
            }
            return resolved;
        }

        for (module_spec, augmentations) in self.ctx.binder.module_augmentations.iter() {
            for aug in augmentations {
                consider_augmentation(module_spec, augmentation_owner_file_idx(aug), aug);
            }
        }

        resolved
    }

    fn module_augmentation_export_preempts_reexport_alias(
        &self,
        sym_id: tsz_binder::SymbolId,
        augmenting_file_idx: usize,
    ) -> bool {
        let Some(binder) = self.ctx.get_binder_for_file(augmenting_file_idx) else {
            return false;
        };
        let Some(symbol) = binder.get_symbol(sym_id) else {
            return false;
        };
        let arena = self.ctx.get_arena_for_file(augmenting_file_idx as u32);

        symbol.declarations.iter().any(|decl| {
            arena
                .get(*decl)
                .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION)
        })
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
            // For named imports from export= modules, tsc resolves through
            // getPropertyOfType(getTypeOfSymbol(exportValue), name) and combines
            // the value meaning (property) with the type meaning (namespace member).
            // When both exist and the property type differs from the interface,
            // the merged symbol has a different this-type binding, causing structural
            // subtyping differences. Match this by using the property type.
            let prop_result =
                self.resolve_export_equals_property_type_for_named_import(name_idx, name);
            if let Some(prop_type) = prop_result {
                return Some(prop_type);
            }
            let mut result = self.type_reference_symbol_type(sym_id);
            if matches!(result, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                && self.ctx.has_lib_loaded()
                && self.is_well_known_lib_type_name(name)
                && let Some(lib_type) = self.resolve_lib_type_by_name(name)
            {
                result = lib_type;
            }
            if let Some(module_specifier) = self.resolve_named_import_module_for_local_name(name) {
                result = self.apply_module_augmentations(&module_specifier, name, result);
                // In type-reference position, a class name means the instance
                // type, not the constructor. If augmentation produced a Callable
                // with construct signatures (constructor type), extract the
                // prototype's type (instance type) so the reference resolves
                // correctly.
                if let Some(shape) =
                    crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, result)
                    && !shape.construct_signatures.is_empty()
                {
                    let prototype_name = self.ctx.types.intern_string("prototype");
                    if let Some(proto_prop) =
                        shape.properties.iter().find(|p| p.name == prototype_name)
                    {
                        result = proto_prop.type_id;
                    }
                }
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

    /// For named imports from `export =` modules, check if the exported value's
    /// type has a property matching the import name. Returns the property type
    /// when a conflict exists between a namespace type member and a value property.
    fn resolve_export_equals_property_type_for_named_import(
        &mut self,
        name_idx: NodeIndex,
        _name: &str,
    ) -> Option<TypeId> {
        use crate::module_resolution::module_specifier_candidates;
        use crate::query_boundaries::common::PropertyAccessResult;

        // Find the original import alias symbol by name in file_locals.
        // We can't use resolve_identifier because it resolves through
        // aliases and returns the target symbol.
        let name_str = self
            .ctx
            .arena
            .get(name_idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.as_str())?;
        let alias_sym_id = self.ctx.binder.file_locals.get(name_str)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(alias_sym_id, &lib_binders)?;

        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return None;
        }
        let module_name = symbol.import_module.as_ref()?;
        let import_name = symbol.import_name.as_ref()?;
        if import_name == "default" {
            return None;
        }

        // Find the export= symbol in the module
        let export_equals_sym = {
            let mut found = None;
            for candidate in module_specifier_candidates(module_name) {
                if let Some(exports) = self
                    .ctx
                    .module_exports_for_module(self.ctx.binder, &candidate)
                    && let Some(sym_id) = exports.get("export=")
                {
                    found = Some(sym_id);
                    break;
                }
            }
            if found.is_none()
                && let Some(all_binders) = &self.ctx.all_binders
            {
                for binder in all_binders.iter() {
                    for candidate in module_specifier_candidates(module_name) {
                        if let Some(exports) =
                            self.ctx.module_exports_for_module(binder, &candidate)
                            && let Some(sym_id) = exports.get("export=")
                        {
                            found = Some(sym_id);
                            break;
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
            found?
        };

        let export_type = self.get_type_of_symbol(export_equals_sym);
        if export_type == TypeId::ERROR || export_type == TypeId::ANY {
            return None;
        }

        // Check if the exported value's type has a property matching the import name.
        // In a type-reference position, a constructor-valued property denotes the
        // constructed instance type (`import { C } ...; let x: C`), not the
        // constructor object type itself.
        match self.resolve_property_access_with_env(export_type, import_name) {
            PropertyAccessResult::Success { type_id, .. } => {
                if let Some(construct_sigs) =
                    crate::query_boundaries::common::construct_signatures_for_type(
                        self.ctx.types,
                        type_id,
                    )
                    && let Some(sig) = construct_sigs.first()
                {
                    return Some(sig.return_type);
                }
                Some(type_id)
            }
            _ => None,
        }
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

    pub(crate) fn resolve_namespace_import_module_for_local_name(
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
            let named_bindings_idx = clause.named_bindings;
            let Some(bindings_node) = self.ctx.arena.get(named_bindings_idx) else {
                continue;
            };
            if bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                continue;
            }
            let Some(namespace_import) = self.ctx.arena.get_named_imports(bindings_node) else {
                continue;
            };
            let Some(local_ident) = self
                .ctx
                .arena
                .get(namespace_import.name)
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
        };

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
            if let Some(exports_table) =
                self.ctx.module_exports_for_module(binder, module_specifier)
                && let Some(sym_id) =
                    self.resolve_export_from_table(binder, exports_table, export_name)
            {
                return Some((sym_id, idx));
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
        let cache_key = (self.ctx.current_file_idx, module_specifier.to_string());
        if let Some(cached) = self
            .ctx
            .namespace_exports_cache
            .borrow()
            .get(&cache_key)
            .cloned()
        {
            return cached;
        }

        if let Some(exports) = self.resolve_ambient_module_namespace_exports(module_specifier) {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(exports.clone()));
            return Some(exports);
        }

        let Some(target_file_idx) = self.ctx.resolve_import_target(module_specifier) else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|source_file| source_file.file_name.clone())
        else {
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, None);
            return None;
        };

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
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .or_else(|| {
                self.ctx
                    .module_exports_for_module(target_binder, module_specifier)
            });

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            if let Some(export_equals_sym_id) = exports.get("export=")
                && let Some(export_equals_symbol) = target_binder.get_symbol(export_equals_sym_id)
            {
                let _ = self.merge_export_equals_import_type_members(
                    export_equals_symbol,
                    Some(target_file_idx),
                    &mut combined,
                );
            }
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                Some(module_specifier),
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                Some(module_specifier),
            );
            record_symbols(&combined);
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(combined.clone()));
            return Some(combined);
        }

        // No direct exports found, but the module may still re-export symbols
        // via `export * from './other'` or `export { X } from './other'`.
        // Collect re-exported symbols even when there are no direct exports.
        let has_reexports = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .is_some()
            || self
                .ctx
                .reexports_for_file(target_binder, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                Some(module_specifier),
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                Some(module_specifier),
            );
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            // Return the table even if empty — the module exists but may have only
            // type-only exports (e.g., `export type * from '...'`). An empty namespace
            // object type is correct and will produce TS2339 for value access, instead
            // of falling through to "module not found" → TypeId::ANY.
            self.ctx
                .namespace_exports_cache
                .borrow_mut()
                .insert(cache_key, Some(combined.clone()));
            return Some(combined);
        }

        self.ctx
            .namespace_exports_cache
            .borrow_mut()
            .insert(cache_key, None);
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
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
            .or_else(|| {
                module_specifier.and_then(|specifier| {
                    self.ctx.module_exports_for_module(target_binder, specifier)
                })
            });

        if let Some(exports) = direct_exports {
            let mut combined = exports.clone();
            self.merge_export_equals_members(target_binder, exports, &mut combined);
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                module_specifier,
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                module_specifier,
            );
            record_symbols(&combined);
            return Some(combined);
        }

        let has_reexports = self
            .ctx
            .wildcard_reexports_for_file(target_binder, &target_file_name)
            .is_some()
            || self
                .ctx
                .reexports_for_file(target_binder, &target_file_name)
                .is_some();
        if has_reexports {
            let mut combined = tsz_binder::SymbolTable::new();
            let mut visited = rustc_hash::FxHashSet::default();
            self.collect_reexported_symbols(
                target_file_idx,
                module_specifier,
                &mut combined,
                &mut visited,
            );
            self.merge_module_augmentation_namespace_exports(
                &mut combined,
                target_file_idx,
                module_specifier,
            );
            if !combined.is_empty() {
                record_symbols(&combined);
            }
            return Some(combined);
        }

        // The target file is a real ES module (has top-level `import`/`export`
        // statements or a module file extension) but its public surface is
        // empty — e.g. `main.mts` only declares `import` aliases, no exports.
        // tsc still types `import * as ns from "./main.mjs"` as the empty
        // module namespace `{}`, so `ns.default` / `ns.imported` correctly
        // report TS2339 instead of leaking the local imports as members.
        // Returning an empty table here matches that behavior; falling
        // through to `None` would let the caller widen the namespace to
        // `any`, which silently accepts any property access.
        if target_binder.is_external_module {
            return Some(tsz_binder::SymbolTable::new());
        }

        None
    }

    pub(crate) fn merge_module_augmentation_namespace_exports(
        &self,
        exports: &mut tsz_binder::SymbolTable,
        target_file_idx: usize,
        module_specifier: Option<&str>,
    ) {
        // Skip the wildcard-chain helper cost when no augmentations exist.
        if !self.ctx.program_has_module_augmentations() {
            return;
        }
        let mut names: Vec<String> = Vec::new();

        if let Some(module_specifier) = module_specifier {
            for name in self.collect_module_augmentation_names(module_specifier) {
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        if let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
        {
            for name in self.collect_module_augmentation_names(target_file_name) {
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        for name in names {
            if exports.get(name.as_str()).is_some() {
                continue;
            }
            if let Some((sym_id, owner_file_idx)) =
                self.resolve_module_augmentation_export_for_file(target_file_idx, &name)
            {
                exports.set(name, sym_id);
                self.ctx.register_symbol_file_target(sym_id, owner_file_idx);
            }
        }
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
        if let Some(mode) = resolution_mode
            && let Some(target_idx) = self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_specifier,
                Some(mode),
            )
        {
            if let Some(exports) = self
                .resolve_cross_file_namespace_exports_for_file(target_idx, Some(module_specifier))
            {
                return Some(exports);
            }
            return Some(tsz_binder::SymbolTable::new());
        }
        self.resolve_effective_module_exports_from_file(
            module_specifier,
            Some(self.ctx.current_file_idx),
        )
        .or_else(|| self.resolve_effective_module_exports(module_specifier))
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

            if let Some(exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, &candidate)
            {
                let mut combined = exports.clone();
                self.merge_export_equals_members(self.ctx.binder, exports, &mut combined);
                if let Some(export_equals_sym_id) = exports.get("export=")
                    && let Some(export_equals_symbol) =
                        self.ctx.binder.get_symbol(export_equals_sym_id)
                {
                    let _ = self.merge_export_equals_import_type_members(
                        export_equals_symbol,
                        source_file_idx.or_else(|| self.ctx.resolve_import_target(&candidate)),
                        &mut combined,
                    );
                }
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
                    && let Some(exports) =
                        self.ctx.module_exports_for_module(binder, module_specifier)
                {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    if let Some(export_equals_sym_id) = exports.get("export=")
                        && let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id)
                    {
                        let _ = self.merge_export_equals_import_type_members(
                            export_equals_symbol,
                            Some(file_idx),
                            &mut combined,
                        );
                    }
                    return Some(combined);
                }
            }
        } else {
            for (file_idx, binder) in binders.iter().enumerate() {
                if let Some(exports) = self.ctx.module_exports_for_module(binder, module_specifier)
                {
                    let mut combined = exports.clone();
                    self.merge_export_equals_members(binder, exports, &mut combined);
                    if let Some(export_equals_sym_id) = exports.get("export=")
                        && let Some(export_equals_symbol) = binder.get_symbol(export_equals_sym_id)
                    {
                        let _ = self.merge_export_equals_import_type_members(
                            export_equals_symbol,
                            Some(file_idx),
                            &mut combined,
                        );
                    }
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

        // The `.members` table on a class symbol holds INSTANCE members (e.g. `bar`
        // from `class D { bar: string; }`). Those live on D's prototype and on
        // instances of D — they are never accessible at the module-namespace level.
        // Merging them here would synthesize a phantom `{ bar }` namespace surface
        // and force the import type to be `typeof D & { bar }` instead of `typeof D`.
        // tsc treats `import x = require()` of an `export = D` module as `typeof D`
        // directly. Static members and namespace augmentations live in `.exports`,
        // which we already merged above.
        let is_class = export_equals_symbol.has_any_flags(tsz_binder::symbol_flags::CLASS);
        if !is_class && let Some(symbol_members) = export_equals_symbol.members.as_ref() {
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
        fallback_decl_file_idx: Option<usize>,
        combined: &mut tsz_binder::SymbolTable,
    ) -> Option<String> {
        let decl_file_idx = if export_equals_symbol.decl_file_idx == u32::MAX {
            fallback_decl_file_idx?
        } else {
            export_equals_symbol.decl_file_idx as usize
        };
        let binder = self.ctx.get_binder_for_file(decl_file_idx)?;
        let arena = self.ctx.get_arena_for_file(decl_file_idx as u32);

        let module_specifier_from_decl = |decl_idx: NodeIndex| -> Option<String> {
            let node = arena.get(decl_idx)?;
            if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return None;
            }
            let var_decl = arena.get_variable_declaration(node)?;
            if !var_decl.type_annotation.is_some() {
                return None;
            }
            self.import_type_module_specifier_from_type_node(arena, var_decl.type_annotation)
        };

        let mut module_specifier = export_equals_symbol
            .value_declaration
            .into_option()
            .and_then(module_specifier_from_decl)
            .or_else(|| {
                export_equals_symbol
                    .declarations
                    .iter()
                    .find_map(|&decl_idx| module_specifier_from_decl(decl_idx))
            });

        // Handle `export = x` where `x` carries the import-type annotation.
        if module_specifier.is_none() {
            let export_assign_decl = export_equals_symbol
                .value_declaration
                .into_option()
                .and_then(|decl_idx| {
                    arena.get(decl_idx).and_then(|node| {
                        (node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT).then_some(decl_idx)
                    })
                })
                .or_else(|| {
                    export_equals_symbol
                        .declarations
                        .iter()
                        .find_map(|&decl_idx| {
                            arena.get(decl_idx).and_then(|node| {
                                (node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT)
                                    .then_some(decl_idx)
                            })
                        })
                });

            if let Some(export_assign_idx) = export_assign_decl
                && let Some(assign) = arena
                    .get(export_assign_idx)
                    .and_then(|node| arena.get_export_assignment(node))
                && let Some(target_sym_id) = binder
                    .get_node_symbol(assign.expression)
                    .or_else(|| binder.resolve_identifier(arena, assign.expression))
            {
                let resolved_target = {
                    let mut visited = AliasCycleTracker::new();
                    self.resolve_alias_symbol(target_sym_id, &mut visited)
                        .unwrap_or(target_sym_id)
                };
                let target_symbol = binder
                    .get_symbol(resolved_target)
                    .or_else(|| self.get_symbol_globally(resolved_target))
                    .or_else(|| self.get_cross_file_symbol(resolved_target));
                if let Some(target_symbol) = target_symbol {
                    module_specifier = target_symbol
                        .value_declaration
                        .into_option()
                        .and_then(module_specifier_from_decl)
                        .or_else(|| {
                            target_symbol
                                .declarations
                                .iter()
                                .find_map(|&decl_idx| module_specifier_from_decl(decl_idx))
                        });
                }
            }
        }

        let module_specifier = module_specifier?;

        let Some(nested_exports) =
            self.resolve_effective_module_exports_from_file(&module_specifier, Some(decl_file_idx))
        else {
            return Some(module_specifier);
        };
        let nested_target_idx = nested_exports
            .iter()
            .find_map(|(_, &sym_id)| self.ctx.resolve_symbol_file_index_stable(sym_id))
            .or_else(|| {
                self.ctx
                    .resolve_import_target_from_file(decl_file_idx, &module_specifier)
            })
            .or_else(|| self.ctx.resolve_import_target(&module_specifier));

        for (name, sym_id) in nested_exports.iter() {
            if let Some(target_idx) = nested_target_idx {
                self.ctx.register_symbol_file_target(*sym_id, target_idx);
            }
            if name != "export=" && !combined.has(name) {
                combined.set(name.to_string(), *sym_id);
            }
        }
        Some(module_specifier)
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
}
