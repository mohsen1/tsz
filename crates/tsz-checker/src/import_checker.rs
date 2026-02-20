//! Import/export declaration validation (TS2307, TS2305, TS2309, TS1202).

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Import/Export Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Helpers
    // =========================================================================

    /// Returns the appropriate "module not found" diagnostic code and message.
    /// Uses TS2792 when module resolution is "classic"-like (non-Node module kinds),
    /// otherwise TS2307.
    pub(crate) fn module_not_found_diagnostic(&self, module_name: &str) -> (String, u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            return (error.message.clone(), error.code);
        }

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
            (
                format_message(
                    diagnostic_messages::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                    &[module_name],
                ),
                diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
            )
        } else {
            (
                format_message(
                    diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                    &[module_name],
                ),
                diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
            )
        }
    }

    /// Check whether a named import can be satisfied via `export =` target members.
    fn has_named_export_via_export_equals(
        &self,
        exports_table: &tsz_binder::SymbolTable,
        import_name: &str,
    ) -> bool {
        let symbol_has_named_member = |symbol: &tsz_binder::Symbol, member_name: &str| {
            symbol
                .exports
                .as_ref()
                .is_some_and(|exports| exports.has(member_name))
                || symbol
                    .members
                    .as_ref()
                    .is_some_and(|members| members.has(member_name))
        };

        let Some(export_equals_sym) = exports_table.get("export=") else {
            return false;
        };

        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lc| std::sync::Arc::clone(&lc.binder))
            .collect();

        let resolved_export_equals = if let Some(export_sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_equals_sym, &lib_binders)
            && (export_sym.flags & symbol_flags::ALIAS) != 0
        {
            let mut visited_aliases = Vec::new();
            self.resolve_alias_symbol(export_equals_sym, &mut visited_aliases)
                .unwrap_or(export_equals_sym)
        } else {
            export_equals_sym
        };

        let Some(target_symbol) = self
            .ctx
            .binder
            .get_symbol_with_libs(resolved_export_equals, &lib_binders)
        else {
            return false;
        };

        let mut candidate_symbol_ids = vec![resolved_export_equals];

        // For `export = alias` where `alias` comes from `import alias = Namespace`,
        // resolve the namespace target explicitly so named imports can see members.
        if (target_symbol.flags & symbol_flags::ALIAS) != 0 {
            let decl_idx = if target_symbol.value_declaration.is_some() {
                target_symbol.value_declaration
            } else if let Some(&first_decl) = target_symbol.declarations.first() {
                first_decl
            } else {
                NodeIndex::NONE
            };

            if decl_idx.is_some()
                && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
            {
                let module_ref = import_decl.module_specifier;
                if let Some(module_ref_node) = self.ctx.arena.get(module_ref)
                    && module_ref_node.kind != SyntaxKind::StringLiteral as u16
                    && let Some(target_id) = self.resolve_qualified_symbol(module_ref)
                {
                    candidate_symbol_ids.push(target_id);
                }
            }
        }

        let mut seen_symbol_ids = rustc_hash::FxHashSet::default();

        for sym_id in candidate_symbol_ids {
            if !seen_symbol_ids.insert(sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                continue;
            };

            if symbol_has_named_member(symbol, import_name) {
                return true;
            }

            // Some binder paths keep function/class + namespace merges split across
            // sibling symbols with the same escaped name. Probe those namespace-shaped
            // siblings as a fallback for `export =` member lookup.
            for candidate_id in self
                .ctx
                .binder
                .get_symbols()
                .find_all_by_name(&symbol.escaped_name)
            {
                if !seen_symbol_ids.insert(candidate_id) {
                    continue;
                }
                let Some(candidate_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(candidate_id, &lib_binders)
                else {
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
                if symbol_has_named_member(candidate_symbol, import_name) {
                    return true;
                }
            }
        }

        false
    }

    // =========================================================================
    // Import Member Validation
    // =========================================================================

    /// Check that imported members exist in the module's exports.
    ///
    /// Validates that each named import from a module actually exists in that
    /// module's export table.
    pub(crate) fn check_imported_members(
        &mut self,
        import: &tsz_parser::parser::node::ImportDeclData,
        module_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if self.is_ambient_module_match(module_name)
            || self.any_ambient_module_declared(module_name)
        {
            return;
        }
        if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            if let Some(source_file) = arena.source_files.first()
                && !source_file.is_declaration_file
            {
                let file_name = source_file.file_name.as_str();
                let is_js_like = file_name.ends_with(".js")
                    || file_name.ends_with(".jsx")
                    || file_name.ends_with(".mjs")
                    || file_name.ends_with(".cjs");
                if is_js_like {
                    return;
                }
            }
        }

        let clause_node = match self.ctx.arena.get(import.import_clause) {
            Some(node) => node,
            None => return,
        };

        let clause = match self.ctx.arena.get_import_clause(clause_node) {
            Some(c) => c,
            None => return,
        };

        let has_default_import = clause.name.is_some();
        let bindings_node = self.ctx.arena.get(clause.named_bindings);
        let has_named_imports = bindings_node
            .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::NAMED_IMPORTS);
        let mut has_named_default_binding = false;

        if has_named_imports
            && let Some(bindings_node) = bindings_node
            && let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node)
        {
            for element_idx in &named_imports.elements.nodes {
                let Some(element_node) = self.ctx.arena.get(*element_idx) else {
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
                let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node) else {
                    continue;
                };

                if imported_ident.escaped_text.as_str() == "default" {
                    has_named_default_binding = true;
                    break;
                }
            }
        }

        // Nothing to check
        if !has_default_import && !has_named_imports {
            return;
        }

        // Resolve exports table (shared between default and named import checking)
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let exports_table = self.resolve_effective_module_exports(module_name);

        // Check default import: import X from "module"
        // If the module has no "default" export and allowSyntheticDefaultImports is off,
        // emit the canonical diagnostic (TS1192/TS1259) from the shared helper.
        if has_default_import && !has_named_default_binding {
            if let Some(ref table) = exports_table {
                if !table.has("default") && !self.ctx.allow_synthetic_default_imports() {
                    self.emit_no_default_export_error(module_name, clause.name);
                }
            } else if self
                .ctx
                .resolved_modules
                .as_ref()
                .is_some_and(|resolved| resolved.contains(module_name))
                && self.ctx.resolve_import_target(module_name).is_some()
                && !self.ctx.allow_synthetic_default_imports()
            {
                // Module resolved but no exports table found - still emit TS1192
                self.emit_no_default_export_error(module_name, clause.name);
            }
        }

        // Check named imports: import { X, Y } from "module"
        if has_named_imports {
            let Some(bindings_node) = bindings_node else {
                return;
            };
            let named_imports = match self.ctx.arena.get_named_imports(bindings_node) {
                Some(ni) => ni,
                None => return,
            };

            let Some(exports_table) = exports_table else {
                if self
                    .ctx
                    .resolved_modules
                    .as_ref()
                    .is_some_and(|resolved| resolved.contains(module_name))
                    && self.ctx.resolve_import_target(module_name).is_none()
                {
                    return;
                }
                for element_idx in &named_imports.elements.nodes {
                    let element_node = match self.ctx.arena.get(*element_idx) {
                        Some(node) => node,
                        None => continue,
                    };

                    let specifier = match self.ctx.arena.get_specifier(element_node) {
                        Some(s) => s,
                        None => continue,
                    };

                    let name_idx = if specifier.property_name.is_none() {
                        specifier.name
                    } else {
                        specifier.property_name
                    };

                    let name_node = match self.ctx.arena.get(name_idx) {
                        Some(node) => node,
                        None => continue,
                    };

                    let identifier = match self.ctx.arena.get_identifier(name_node) {
                        Some(id) => id,
                        None => continue,
                    };

                    let import_name = &identifier.escaped_text;

                    // Check re-export chains before emitting TS2305
                    let found_via_reexport = self
                        .ctx
                        .binder
                        .resolve_import_if_needed_public(module_name, import_name)
                        .is_some()
                        || self
                            .ctx
                            .binder
                            .resolve_import_if_needed_public(normalized, import_name)
                            .is_some()
                        || self.resolve_import_via_target_binder(module_name, import_name)
                        || self.resolve_import_via_all_binders(
                            module_name,
                            normalized,
                            import_name,
                        );

                    if !found_via_reexport {
                        // Check if the symbol exists locally in the target module
                        // to distinguish between TS2459, TS2460, and TS2305
                        let (exists_locally, exported_as) =
                            self.check_local_symbol_and_renamed_export(module_name, import_name);

                        if exists_locally {
                            if let Some(ref renamed_as) = exported_as {
                                // TS2460: Symbol exists locally and is exported under a different name
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                    &[module_name, import_name, renamed_as],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                );
                            } else {
                                // TS2459: Symbol exists locally but is not exported
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                    &[module_name, import_name],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                );
                            }
                        } else {
                            // TS2305: Symbol doesn't exist in the module at all
                            let message = format_message(
                                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                                &[module_name, import_name],
                            );
                            self.error_at_node(
                                specifier.name,
                                &message,
                                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                            );
                        }
                    }
                }
                return;
            };

            for element_idx in &named_imports.elements.nodes {
                let element_node = match self.ctx.arena.get(*element_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let specifier = match self.ctx.arena.get_specifier(element_node) {
                    Some(s) => s,
                    None => continue,
                };

                let name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };

                let name_node = match self.ctx.arena.get(name_idx) {
                    Some(node) => node,
                    None => continue,
                };

                let identifier = match self.ctx.arena.get_identifier(name_node) {
                    Some(id) => id,
                    None => continue,
                };

                let import_name = &identifier.escaped_text;

                if !exports_table.has(import_name)
                    && !self.has_named_export_via_export_equals(&exports_table, import_name)
                {
                    // Before emitting TS2305, check if this import can be resolved
                    // through re-export chains (wildcard or named re-exports).
                    let found_via_reexport = self
                        .ctx
                        .binder
                        .resolve_import_if_needed_public(module_name, import_name)
                        .is_some()
                        || self
                            .ctx
                            .binder
                            .resolve_import_if_needed_public(normalized, import_name)
                            .is_some()
                        || self.resolve_import_via_target_binder(module_name, import_name)
                        || self.resolve_import_via_all_binders(
                            module_name,
                            normalized,
                            import_name,
                        );

                    if !found_via_reexport {
                        // Check if the symbol exists locally in the target module
                        // to distinguish between TS2459, TS2460, and TS2305
                        let (exists_locally, exported_as) =
                            self.check_local_symbol_and_renamed_export(module_name, import_name);

                        if exists_locally {
                            if let Some(ref renamed_as) = exported_as {
                                // TS2460: Symbol exists locally and is exported under a different name
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                    &[module_name, import_name, renamed_as],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                );
                            } else {
                                // TS2459: Symbol exists locally but is not exported
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                    &[module_name, import_name],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                );
                            }
                        } else {
                            // TS2305: Symbol doesn't exist in the module at all
                            let message = format_message(
                                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                                &[module_name, import_name],
                            );
                            self.error_at_node(
                                specifier.name,
                                &message,
                                diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                            );
                        }
                    }
                } else {
                    // Import exists - check if it should be elided from JavaScript output
                    // Get the symbol from the exports table
                    if let Some(sym_id) = exports_table.get(import_name) {
                        use tsz_binder::symbol_flags;

                        // Get the symbol (checking lib binders for cross-file resolution)
                        let lib_binders: Vec<_> = self
                            .ctx
                            .lib_contexts
                            .iter()
                            .map(|lc| std::sync::Arc::clone(&lc.binder))
                            .collect();

                        if let Some(symbol) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        {
                            // Check if the symbol is type-only (has TYPE flags but not VALUE flags)
                            // This correctly handles:
                            // - Interfaces and type aliases (elided)
                            // - Classes (not elided - have both TYPE and VALUE flags)
                            // - Declaration merging (e.g., interface + value - not elided)
                            let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
                            let has_value = (symbol.flags & symbol_flags::VALUE) != 0;

                            if has_type && !has_value {
                                // Mark this specifier node as type-only for elision during emit
                                self.ctx.type_only_nodes.insert(*element_idx);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check if a symbol exists locally in the target module and whether it's
    /// exported under a different name.
    ///
    /// Returns (`exists_locally`, `exported_as`) where:
    /// - `exists_locally`: true if the symbol is declared in the module's scope
    /// - `exported_as`: Some(name) if the symbol is exported under a different name,
    ///                None if not exported or exported with the same name
    #[tracing::instrument(level = "debug", skip(self), fields(module = %module_name, import = %import_name))]
    fn check_local_symbol_and_renamed_export(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> (bool, Option<String>) {
        tracing::trace!("Checking if symbol exists locally and is renamed");

        // Try to get the target module's binder
        let target_binder = if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            tracing::trace!(target_idx, "Resolved import target");
            self.ctx.get_binder_for_file(target_idx)
        } else {
            tracing::trace!("Could not resolve import target");
            None
        };

        let target_binder = match target_binder {
            Some(binder) => {
                tracing::trace!("Found target binder directly");
                binder
            }
            None => {
                tracing::trace!("No direct target binder, checking all binders");
                // If we can't find the target binder, also check all binders
                if let Some(all_binders) = &self.ctx.all_binders {
                    // Try to find the module in any binder's exports
                    let normalized = module_name.trim_matches('"').trim_matches('\'');
                    tracing::trace!(num_binders = all_binders.len(), "Checking all binders");
                    for binder in all_binders.iter() {
                        if binder.module_exports.contains_key(module_name)
                            || binder.module_exports.contains_key(normalized)
                        {
                            tracing::trace!("Found matching binder via exports");
                            // Check if the symbol exists locally in this binder
                            if let Some(exists) =
                                self.check_symbol_in_binder(binder, import_name, module_name)
                            {
                                return exists;
                            }
                        }
                    }
                }
                tracing::trace!("No binder found, returning (false, None)");
                return (false, None);
            }
        };

        if let Some(result) = self.check_symbol_in_binder(target_binder, import_name, module_name) {
            tracing::trace!(exists_locally = result.0, renamed = ?result.1, "Got result from check_symbol_in_binder");
            result
        } else {
            tracing::trace!("check_symbol_in_binder returned None");
            (false, None)
        }
    }

    /// Helper to check if a symbol exists in a specific binder and whether it's renamed on export.
    #[tracing::instrument(level = "trace", skip(self, binder), fields(import = %import_name, module = %module_name))]
    fn check_symbol_in_binder(
        &self,
        binder: &tsz_binder::BinderState,
        import_name: &str,
        module_name: &str,
    ) -> Option<(bool, Option<String>)> {
        // Check if the symbol exists in the binder's file-level symbol table
        // (not just the arena, which doesn't include all declarations)
        let symbol_exists = binder.file_locals.has(import_name);
        tracing::trace!(symbol_exists, "Checked if symbol exists in binder");

        if !symbol_exists {
            return None;
        }

        // Symbol exists locally. Now check if it's exported under a different name.
        // We need to look at the module's export specifications to find renames.

        // Get the module's export table to check renamed exports
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let module_keys = [
            module_name,
            normalized,
            &format!("\"{normalized}\""),
            &format!("'{normalized}'"),
        ];

        // Also try to get the target file's name if available
        let file_name = if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            arena.source_files.first().map(|sf| sf.file_name.as_str())
        } else {
            None
        };

        for &key in &module_keys {
            if let Some(exports) = binder.module_exports.get(key) {
                // Check if the symbol is exported under a different name
                // by looking through all export names
                for (export_name, sym_id) in exports.iter() {
                    if let Some(sym) = binder.symbols.get(*sym_id) {
                        // Check if this symbol has a declaration with the import_name
                        let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                            self.declaration_name_matches_string(decl_idx, import_name)
                        });

                        if has_matching_name && export_name.as_str() != import_name {
                            // Symbol is exported under a different name
                            return Some((true, Some(export_name.clone())));
                        }
                    }
                }
            }
        }

        // Also check with file name
        if let Some(fname) = file_name
            && let Some(exports) = binder.module_exports.get(fname)
        {
            for (export_name, sym_id) in exports.iter() {
                if let Some(sym) = binder.symbols.get(*sym_id) {
                    let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                        self.declaration_name_matches_string(decl_idx, import_name)
                    });

                    if has_matching_name && export_name.as_str() != import_name {
                        return Some((true, Some(export_name.clone())));
                    }
                }
            }
        }

        // Symbol exists locally but is not exported
        Some((true, None))
    }

    /// Check if a declaration's name matches the expected string.
    fn declaration_name_matches_string(&self, decl_idx: NodeIndex, expected_name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        let name_node_idx = match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
                    var_decl.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.ctx.arena.get_function(node) {
                    func.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.ctx.arena.get_class(node) {
                    class.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(interface) = self.ctx.arena.get_interface(node) {
                    interface.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                    type_alias.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.ctx.arena.get_enum(node) {
                    enum_decl.name
                } else {
                    return false;
                }
            }
            _ => return false,
        };

        let Some(name_node) = self.ctx.arena.get(name_node_idx) else {
            return false;
        };

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        self.ctx.arena.resolve_identifier_text(ident) == expected_name
    }

    fn any_ambient_module_declared(&self, module_name: &str) -> bool {
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let Some(all_binders) = &self.ctx.all_binders else {
            return false;
        };
        for binder in all_binders.iter() {
            for pattern in binder
                .declared_modules
                .iter()
                .chain(binder.shorthand_ambient_modules.iter())
                .chain(binder.module_exports.keys())
            {
                if Self::module_name_matches_pattern_for_imports(pattern, normalized) {
                    return true;
                }
            }
        }
        false
    }

    fn module_name_matches_pattern_for_imports(pattern: &str, module_name: &str) -> bool {
        let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
        let module_name = module_name.trim().trim_matches('"').trim_matches('\'');
        if !pattern.contains('*') {
            return pattern == module_name;
        }
        if let Ok(glob) = globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
        {
            let matcher = glob.compile_matcher();
            return matcher.is_match(module_name);
        }
        false
    }

    // =========================================================================
    // Module Body Validation
    // =========================================================================

    /// Check a module body for statements and function implementations.
    pub(crate) fn check_module_body(&mut self, body_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        tracing::trace!(
            "check_module_body: body_kind={} MODULE_BLOCK={}",
            body_node.kind,
            syntax_kind_ext::MODULE_BLOCK
        );

        let mut is_ambient_external_module = false;
        if let Some(ext) = self.ctx.arena.get_extended(body_idx) {
            let parent_idx = ext.parent;
            if !parent_idx.is_none()
                && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                    && let Some(module) = self.ctx.arena.get_module(parent_node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                            && name_node.kind == SyntaxKind::StringLiteral as u16 {
                                is_ambient_external_module = true;
                            }
        }

        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                for &stmt_idx in &statements.nodes {
                    // TS1063: export assignment cannot be used in a namespace.
                    // Emit the error and skip further checking of the statement
                    // (tsc does not resolve the expression when it's invalid).
                    let is_export_assign = self
                        .ctx
                        .arena
                        .get(stmt_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::EXPORT_ASSIGNMENT);
                    if is_export_assign && !is_ambient_external_module {
                        self.error_at_node(
                            stmt_idx,
                            diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
                            diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
                        );
                        continue;
                    }
                    self.check_statement(stmt_idx);
                }
                self.check_function_implementations(&statements.nodes);
                // Check for duplicate export assignments (TS2300) and conflicts (TS2309)
                // Filter out export assignments in namespace bodies since they're already
                // flagged with TS1063 and shouldn't trigger TS2304/TS2309 follow-up errors.
                // However, they ARE checked in ambient external modules.
                let non_export_assign: Vec<NodeIndex> = if is_ambient_external_module {
                    statements.nodes.clone()
                } else {
                    statements
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_none_or(|n| n.kind != syntax_kind_ext::EXPORT_ASSIGNMENT)
                        })
                        .collect()
                };
                self.check_export_assignment(&non_export_assign);
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            self.check_statement(body_idx);
        }
    }

    // =========================================================================
    // Export Assignment Validation
    // =========================================================================

    /// Check for export assignment conflicts with other exported elements.
    ///
    /// Validates that:
    /// - `export = X` is not used when there are also other exported elements (TS2309)
    /// - There are not multiple `export = X` statements (TS2300)
    pub(crate) fn check_export_assignment(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut export_assignment_indices: Vec<NodeIndex> = Vec::new();
        let mut export_default_indices: Vec<NodeIndex> = Vec::new();
        let mut has_other_exports = false;

        // Check if we're in a declaration file (implicitly ambient)
        let is_declaration_file = self
            .ctx
            .arena
            .source_files
            .first()
            .is_some_and(|sf| sf.is_declaration_file)
            || self.ctx.file_name.contains(".d.");

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    export_assignment_indices.push(stmt_idx);

                    if let Some(export_data) = self.ctx.arena.get_export_assignment(node) {
                        // TS2714: In ambient context, export assignment expression must be
                        // an identifier or qualified name
                        let is_ambient =
                            is_declaration_file || self.is_ambient_declaration(stmt_idx);
                        if is_ambient
                            && !self.is_identifier_or_qualified_name(export_data.expression)
                        {
                            self.error_at_node(
                                export_data.expression,
                                "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                            );
                        } else {
                            self.get_type_of_node(export_data.expression);
                        }
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_data) = self.ctx.arena.get_export_decl(node) {
                        if export_data.is_default_export {
                            export_default_indices.push(stmt_idx);

                            // TS2714: In ambient context, export default expression must be
                            // an identifier or qualified name. Skip for declarations
                            // (class, function, interface, enum) which are always valid.
                            let is_ambient =
                                is_declaration_file || self.is_ambient_declaration(stmt_idx);
                            let is_declaration = self
                                .ctx
                                .arena
                                .get(export_data.export_clause)
                                .is_some_and(|n| {
                                    matches!(
                                        n.kind,
                                        k if k == syntax_kind_ext::CLASS_DECLARATION
                                            || k == syntax_kind_ext::FUNCTION_DECLARATION
                                            || k == syntax_kind_ext::INTERFACE_DECLARATION
                                            || k == syntax_kind_ext::ENUM_DECLARATION
                                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                    )
                                });
                            if is_ambient
                                && !is_declaration
                                && export_data.export_clause.is_some()
                                && !self.is_identifier_or_qualified_name(export_data.export_clause)
                            {
                                self.error_at_node(
                                    export_data.export_clause,
                                    "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                    diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                                );
                            }
                        } else {
                            has_other_exports = true;
                        }
                    } else {
                        has_other_exports = true;
                    }
                }
                _ => {
                    if self.has_export_modifier(stmt_idx) {
                        has_other_exports = true;
                    }
                }
            }
        }

        // TS1203: Check for export assignment when targeting ES modules
        // This must be checked first before TS2300/TS2309
        // Declaration files (.d.ts, .d.mts, .d.cts) are exempt: they describe
        // the shape of CJS modules and `export = X` is valid in declarations.
        // Ambient module declarations (`declare module "M" { export = X; }`) are
        // also exempt — they describe external module shapes.
        // JS files (.js, .jsx) are exempt — they get TS8003 instead.
        // CJS-extension files (.cts, .cjs) are explicitly CommonJS — export= is valid.
        let is_js_file =
            self.ctx.file_name.ends_with(".js") || self.ctx.file_name.ends_with(".jsx");
        let is_cjs_extension =
            self.ctx.file_name.ends_with(".cts") || self.ctx.file_name.ends_with(".cjs");
        if self.ctx.compiler_options.module.is_es_module()
            && !is_declaration_file
            && !is_js_file
            && !is_cjs_extension
        {
            for &export_idx in &export_assignment_indices {
                if !self.is_ambient_declaration(export_idx) {
                    self.error_at_node(
                        export_idx,
                        "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.",
                        diagnostic_codes::EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
                    );
                }
            }
        }

        // TS2300: Check for duplicate export assignments
        // TypeScript emits TS2300 on ALL export assignments if there are 2+
        if export_assignment_indices.len() > 1 {
            for &export_idx in &export_assignment_indices {
                self.error_at_node(
                    export_idx,
                    "Duplicate identifier 'export='.",
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }

        // TS2309: Check for export assignment with other exports
        // Skip if already emitting TS1203 (ES module target) or TS2300 (duplicate)
        if let Some(&export_idx) = export_assignment_indices.first()
            && has_other_exports
            && export_assignment_indices.len() == 1
        {
            self.error_at_node(
                export_idx,
                "An export assignment cannot be used in a module with other exported elements.",
                diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_MODULE_WITH_OTHER_EXPORTED_ELEMENTS,
            );
        }

        // TS2528: Check for multiple default exports
        // tsc allows declaration merging of default exports:
        // - Interface + value (function/class) can coexist
        // - Function overloads (multiple `export default function foo(...)`) are one symbol
        // Only emit TS2528 when there are truly conflicting default exports.
        if export_default_indices.len() > 1 {
            // Classify each default export
            let mut has_interface = false;
            let mut value_count = 0;
            let mut function_name: Option<String> = None;
            let mut all_same_function = true;

            for &export_idx in &export_default_indices {
                let wrapped_kind = self
                    .ctx
                    .arena
                    .get_export_decl_at(export_idx)
                    .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                    .map(|n| n.kind);

                match wrapped_kind {
                    Some(k) if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        has_interface = true;
                    }
                    Some(k) if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        // Check if all function defaults share the same name (overloads)
                        let name = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .and_then(|n| self.ctx.arena.get_function(n))
                            .map(|f| self.node_text(f.name).unwrap_or_default());
                        match (&function_name, name) {
                            (None, Some(n)) => {
                                function_name = Some(n);
                                value_count += 1;
                            }
                            (Some(existing), Some(n)) if *existing == n => {
                                // Same function name: overload, don't count again
                            }
                            _ => {
                                all_same_function = false;
                                value_count += 1;
                            }
                        }
                    }
                    _ => {
                        all_same_function = false;
                        value_count += 1;
                    }
                }
            }

            // Emit TS2528 only when there are conflicting value exports
            // (interface-only or interface + one value group is OK)
            let is_conflict = value_count > 1 || (!has_interface && !all_same_function);
            if is_conflict {
                for &export_idx in &export_default_indices {
                    self.error_at_node(
                        export_idx,
                        "A module cannot have multiple default exports.",
                        diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                    );
                }
            }
        }
    }

    /// Check if a node is an identifier or qualified name (e.g., `X` or `X.Y.Z`).
    /// Used for TS2714 validation of export assignment expressions in ambient contexts.
    fn is_identifier_or_qualified_name(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::QUALIFIED_NAME
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
    }

    /// Check if a statement has an export modifier.
    pub(crate) fn has_export_modifier(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        let Some(mods) = self.get_declaration_modifiers(node) else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.ctx
                .arena
                .get(mod_idx)
                .is_some_and(|mod_node| mod_node.kind == SyntaxKind::ExportKeyword as u16)
        })
    }

    /// Check whether a node is nested inside a namespace declaration.
    /// String-literal ambient modules (`declare module "x"`) are excluded.
    pub(crate) fn is_inside_namespace_declaration(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }

            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
                continue;
            };

            if name_node.kind != SyntaxKind::StringLiteral as u16 {
                return true;
            }
        }

        false
    }
}
