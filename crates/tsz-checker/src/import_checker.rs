//! Import/Export Checking Module
//!
//! This module contains methods for validating import and export declarations.
//! It handles:
//! - Import declaration validation (TS2307, TS2305)
//! - Export assignment validation (TS2309)
//! - Import equals declaration validation (TS1202)
//! - Re-export chain cycle detection
//! - Module body validation
//!
//! This module extends CheckerState with import/export methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
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
    fn module_not_found_diagnostic(&self, module_name: &str) -> (String, u32) {
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
            let decl_idx = if !target_symbol.value_declaration.is_none() {
                target_symbol.value_declaration
            } else if let Some(&first_decl) = target_symbol.declarations.first() {
                first_decl
            } else {
                NodeIndex::NONE
            };

            if !decl_idx.is_none()
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

        let has_default_import = !clause.name.is_none();
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
            let bindings_node = bindings_node.unwrap();
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
    /// Returns (exists_locally, exported_as) where:
    /// - exists_locally: true if the symbol is declared in the module's scope
    /// - exported_as: Some(name) if the symbol is exported under a different name,
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
            &format!("\"{}\"", normalized),
            &format!("'{}'", normalized),
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
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                for &stmt_idx in &statements.nodes {
                    self.check_statement(stmt_idx);
                }
                self.check_function_implementations(&statements.nodes);
                // Check for duplicate export assignments (TS2300) and conflicts (TS2309)
                self.check_export_assignment(&statements.nodes);
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
                            // an identifier or qualified name
                            let is_ambient =
                                is_declaration_file || self.is_ambient_declaration(stmt_idx);
                            if is_ambient
                                && !export_data.export_clause.is_none()
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
        if self.ctx.compiler_options.module.is_es_module() && !is_declaration_file {
            for &export_idx in &export_assignment_indices {
                self.error_at_node(
                    export_idx,
                    "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.",
                    diagnostic_codes::EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
                );
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

        // TS2323/TS2528: Check for multiple default exports
        // TypeScript prefers TS2528 for duplicate defaults when a default export
        // targets a type-only symbol (e.g. `export default Foo` where `Foo` is
        // interface/type alias). Otherwise it reports TS2323 in this checker path.
        if export_default_indices.len() > 1 {
            let prefer_multiple_default_exports = export_default_indices
                .iter()
                .copied()
                .any(|export_idx| self.default_export_targets_type_only_symbol(export_idx));

            for &export_idx in &export_default_indices {
                if prefer_multiple_default_exports {
                    self.error_at_node(
                        export_idx,
                        "A module cannot have multiple default exports.",
                        diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                    );
                } else {
                    self.error_at_node(
                        export_idx,
                        "Cannot redeclare exported variable 'default'.",
                        diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                    );
                }
            }
        }
    }

    fn default_export_targets_type_only_symbol(&self, export_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(export_idx) else {
            return false;
        };
        let Some(export_data) = self.ctx.arena.get_export_decl(node) else {
            return false;
        };
        if !export_data.is_default_export || export_data.export_clause.is_none() {
            return false;
        }

        let Some(target_node) = self.ctx.arena.get(export_data.export_clause) else {
            return false;
        };
        if target_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            || target_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
        {
            return true;
        }

        if let Some(sym_id) = self.resolve_qualified_symbol(export_data.export_clause)
            && self.symbol_is_type_only(sym_id, None)
        {
            return true;
        }

        // Fallback for default-exported identifiers that bind to local type-only
        // declarations not resolved through the current symbol path yet.
        let Some(target_ident) = self.ctx.arena.get_identifier(target_node) else {
            return false;
        };
        let target_name = self.ctx.arena.resolve_identifier_text(target_ident);
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let is_type_only_decl = stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION;
            if is_type_only_decl && self.declaration_name_matches_string(stmt_idx, &target_name) {
                return true;
            }
        }

        false
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

        while !current.is_none() {
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

    // =========================================================================
    // Import Alias Duplicate Checking
    // =========================================================================

    /// Check for duplicate import alias declarations within a scope.
    ///
    /// TS2300: Emitted when multiple `import X = ...` declarations have the same name
    /// within the same scope (namespace, module, or file).
    pub(crate) fn check_import_alias_duplicates(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Map from import alias name to list of declaration indices
        let mut alias_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };

            // Get the import alias name from import_clause (e.g., 'M' in 'import M = Z.I')
            let Some(alias_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(alias_id) = self.ctx.arena.get_identifier(alias_node) else {
                continue;
            };
            let alias_name = alias_id.escaped_text.to_string();

            alias_map.entry(alias_name).or_default().push(stmt_idx);
        }

        // TS2300: Emit for all declarations with duplicate names
        for (alias_name, indices) in alias_map {
            if indices.len() > 1 {
                for &import_idx in &indices {
                    let Some(import_node) = self.ctx.arena.get(import_idx) else {
                        continue;
                    };
                    let Some(import_decl) = self.ctx.arena.get_import_decl(import_node) else {
                        continue;
                    };

                    // Report error on the alias name (import_clause)
                    self.error_at_node(
                        import_decl.import_clause,
                        &format!("Duplicate identifier '{}'.", alias_name),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    // =========================================================================
    // Import Equals Declaration Validation
    // =========================================================================

    /// Check an import equals declaration for ESM compatibility, unresolved modules,
    /// and conflicts with local declarations.
    ///
    /// Validates `import x = require()` and `import x = Namespace` style imports:
    /// - TS1202 when import assignment is used in ES modules
    /// - TS2307 when the module cannot be found
    /// - TS2440 when import conflicts with a local declaration
    pub(crate) fn check_import_equals_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // TS1147/TS2439/TS2303 checks for import = require("...") forms.
        // Use get_require_module_specifier so both StringLiteral and recovered require-call
        // representations are handled consistently.
        let require_module_specifier = self.get_require_module_specifier(import.module_specifier);
        let mut force_module_not_found = false;
        if require_module_specifier.is_some()
            && self.ctx.arena.get(import.module_specifier).is_some()
        {
            // This is an external module reference (require("..."))
            // Check if we're inside a MODULE_DECLARATION (namespace/module)
            let mut current = stmt_idx;
            let mut inside_namespace = false;
            let mut namespace_is_exported = false;
            let mut containing_module_name: Option<String> = None;

            while !current.is_none() {
                if let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        // Check if this is an ambient module (declare module "...") or namespace
                        if let Some(module_decl) = self.ctx.arena.get_module(node)
                            && let Some(name_node) = self.ctx.arena.get(module_decl.name)
                        {
                            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                                // This is an ambient module: declare module "foo"
                                if let Some(name_literal) = self.ctx.arena.get_literal(name_node) {
                                    containing_module_name = Some(name_literal.text.clone());
                                }
                            } else {
                                // This is a namespace: namespace Foo
                                inside_namespace = true;
                                // Check if this namespace is exported
                                namespace_is_exported = self.has_export_modifier(current);
                            }
                        }
                        break;
                    }
                    // Move to parent
                    if let Some(ext) = self.ctx.arena.get_extended(current) {
                        current = ext.parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // TS1147: Only emit for namespaces (not ambient modules)
            if inside_namespace {
                self.error_at_node(
                        import.module_specifier,
                        diagnostic_messages::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                        diagnostic_codes::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                    );
                // Only return early for non-exported namespaces
                // TypeScript emits both TS1147 and TS2307 for exported namespaces
                if !namespace_is_exported {
                    return;
                }
                force_module_not_found = true;
            }

            // TS2439: Ambient modules cannot use relative imports
            if containing_module_name.is_some()
                && let Some(imported_module) = require_module_specifier.as_deref()
            {
                // Check if this is a relative import (starts with ./ or ../)
                if imported_module.starts_with("./") || imported_module.starts_with("../") {
                    self.error_at_node(
                                import.module_specifier,
                                diagnostic_messages::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
                                diagnostic_codes::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
                            );
                    // Keep TS2439 and also force TS2307 in this ambient-relative import case.
                    force_module_not_found = true
                }
            }

            // TS2303: Check for circular import in ambient modules
            if let Some(ref ambient_module_name) = containing_module_name
                && let Some(imported_module) = require_module_specifier.as_deref()
            {
                // Check if the imported module matches the containing module
                if ambient_module_name == imported_module {
                    // Emit TS2303: Circular definition of import alias
                    if let Some(import_name) = self
                        .ctx
                        .arena
                        .get(import.import_clause)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.clone())
                    {
                        let message = format_message(
                            diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                            &[&import_name],
                        );
                        self.error_at_node(
                            import.import_clause,
                            &message,
                            diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                        );
                        return;
                    }
                }
            }
        }

        // Get the import alias name (e.g., 'a' in 'import a = M')
        let import_name = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.clone());

        // Check for TS2440: Import declaration conflicts with local declaration
        // This error is specific to ImportEqualsDeclaration (not ES6 imports).
        // It occurs when:
        // 1. The import introduces a name that already has a value declaration
        // 2. The value declaration is in the same file (local)
        //
        // Note: The binder does NOT merge import equals declarations - it creates
        // a new symbol and overwrites the scope. So we need to find ALL symbols
        // with the same name and check if any non-import has VALUE flags.
        if let Some(ref name) = import_name {
            // Get the symbol for this import
            let import_sym_id = self.ctx.binder.node_symbols.get(&stmt_idx.0).copied();
            // Find the enclosing scope of the import statement
            let import_scope = self
                .ctx
                .binder
                .find_enclosing_scope(self.ctx.arena, stmt_idx);

            // TS2440: Import declaration conflicts with local declaration.
            // The binder can merge non-mergeable declarations into the import symbol,
            // so detect conflicts directly on the import symbol's declarations first.
            if let Some(import_sym_id) = import_sym_id
                && let Some(import_sym) = self.ctx.binder.symbols.get(import_sym_id)
            {
                let has_merged_local_non_import_decl =
                    import_sym.declarations.iter().any(|&decl_idx| {
                        if decl_idx == stmt_idx {
                            return false;
                        }
                        let in_same_scope = if let Some(import_scope_id) = import_scope {
                            self.ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, decl_idx)
                                == Some(import_scope_id)
                        } else {
                            true
                        };
                        if !in_same_scope {
                            return false;
                        }

                        self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                            decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        })
                    });

                if has_merged_local_non_import_decl {
                    let message = format_message(
                        diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                        &[name],
                    );
                    self.error_at_node(
                        stmt_idx,
                        &message,
                        diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                    );
                    return;
                }
            }

            // Find all symbols with this name (there may be multiple due to shadowing)
            let all_symbols = self.ctx.binder.symbols.find_all_by_name(name);

            for sym_id in all_symbols {
                // Skip the import's own symbol
                if Some(sym_id) == import_sym_id {
                    continue;
                }

                if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                    // Check if this symbol has value semantics
                    let is_value = (sym.flags & symbol_flags::VALUE) != 0;
                    let is_alias = (sym.flags & symbol_flags::ALIAS) != 0;
                    let is_namespace = (sym.flags & symbol_flags::NAMESPACE_MODULE) != 0;

                    // TS2300: duplicate `import =` aliases with the same name in the same scope.
                    // TypeScript reports this as duplicate identifier (not TS2440).
                    if is_alias {
                        let alias_in_same_scope = if let Some(import_scope_id) = import_scope {
                            sym.declarations.iter().any(|&decl_idx| {
                                self.ctx
                                    .binder
                                    .find_enclosing_scope(self.ctx.arena, decl_idx)
                                    == Some(import_scope_id)
                            })
                        } else {
                            true
                        };

                        let has_local_alias_decl = sym.declarations.iter().any(|&decl_idx| {
                            self.ctx.binder.node_symbols.get(&decl_idx.0) == Some(&sym_id)
                        });

                        if alias_in_same_scope && has_local_alias_decl {
                            let message =
                                format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[name]);
                            self.error_at_node(
                                import.import_clause,
                                &message,
                                diagnostic_codes::DUPLICATE_IDENTIFIER,
                            );
                            return;
                        }
                        continue;
                    }

                    // Special case: If this is a namespace module, check if it's the enclosing scope
                    // itself. In TypeScript, `namespace A.M { import M = Z.M; }` is allowed - the
                    // import alias `M` shadows the namespace container name `M`.
                    if is_namespace && let Some(import_scope_id) = import_scope {
                        // Get the scope that contains the import
                        if let Some(scope) = self.ctx.binder.scopes.get(import_scope_id.0 as usize)
                        {
                            // Check if any of this namespace's declarations match the container node
                            // of the import's enclosing scope
                            let is_enclosing_namespace =
                                sym.declarations.contains(&scope.container_node);
                            if is_enclosing_namespace {
                                // This namespace is the enclosing context, not a conflicting declaration
                                continue;
                            }
                        }
                    }

                    // Only check for conflicts within the same scope.
                    // A symbol in a different namespace/module should not conflict.
                    if let Some(import_scope_id) = import_scope {
                        let decl_in_same_scope = sym.declarations.iter().any(|&decl_idx| {
                            self.ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, decl_idx)
                                == Some(import_scope_id)
                        });
                        if !decl_in_same_scope {
                            continue;
                        }
                    }

                    // Check if this symbol has any declaration in the CURRENT file
                    // A declaration is in the current file if it's in node_symbols
                    let has_local_declaration = sym.declarations.iter().any(|&decl_idx| {
                        // The declaration is local if its node_symbols entry points to this symbol
                        self.ctx.binder.node_symbols.get(&decl_idx.0) == Some(&sym_id)
                    });

                    if is_value && has_local_declaration {
                        let message = format_message(
                            diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            &[name],
                        );
                        self.error_at_node(
                            stmt_idx,
                            &message,
                            diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                        );
                        return; // Don't emit further errors for this import
                    }
                }
            }
        }

        let module_specifier_idx = import.module_specifier;
        let Some(ref_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = ref_node.pos;
        let spec_length = ref_node.end.saturating_sub(ref_node.pos);

        // Handle namespace imports: import x = Namespace or import x = Namespace.Member
        // These need to emit TS2503 ("Cannot find namespace") if not found
        if require_module_specifier.is_none() {
            self.check_namespace_import(stmt_idx, module_specifier_idx);
            return;
        }

        // TS1202: Import assignment cannot be used when targeting ECMAScript modules.
        // Check if module kind is explicitly ESM (CommonJS modules support import = require)
        let is_ambient_context =
            self.ctx.file_name.ends_with(".d.ts") || self.is_ambient_declaration(stmt_idx);
        if self.ctx.compiler_options.module.is_es_module() && !is_ambient_context {
            self.error_at_node(
                stmt_idx,
                "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
                diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
            );
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(module_name) = require_module_specifier.as_deref() else {
            return;
        };

        if force_module_not_found {
            let (message, code) = self.module_not_found_diagnostic(module_name);
            self.ctx.push_diagnostic(crate::diagnostics::Diagnostic {
                code,
                category: crate::diagnostics::DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: spec_start,
                length: spec_length,
                related_information: Vec::new(),
            });
            return;
        }

        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            return;
        }

        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return;
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            {
                let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                error_code = fallback_code;
                error_message = fallback_message;
            }
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            return;
        }

        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return;
        }

        // Use TS2792 when module resolution is "classic" (system/amd/umd modules),
        // suggesting the user switch to nodenext or configure paths.
        let (message, code) = self.module_not_found_diagnostic(module_name);
        self.ctx
            .modules_with_ts2307_emitted
            .insert(module_key.clone());
        self.error_at_position(spec_start, spec_length, &message, code);
    }

    // =========================================================================
    // Namespace Import Validation (TS2503)
    // =========================================================================

    /// Check a namespace import (import x = Namespace or import x = Namespace.Member).
    /// Emits TS2503 "Cannot find namespace" if the namespace cannot be resolved.
    /// Emits TS2708 "Cannot use namespace as a value" if exporting a type-only member.
    fn check_namespace_import(&mut self, stmt_idx: NodeIndex, module_ref: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_binder::symbol_flags;

        let Some(ref_node) = self.ctx.arena.get(module_ref) else {
            return;
        };

        // Handle simple identifier: import x = Namespace
        if ref_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(ref_node) {
                let name = &ident.escaped_text;
                // Skip if identifier is empty (parse error created a placeholder)
                // or if it's a reserved word that should be handled by TS1359
                if name.is_empty() || name == "null" {
                    return;
                }
                // Try to resolve the identifier as a namespace/module
                if self.resolve_identifier_symbol(module_ref).is_none() {
                    self.error_at_node_msg(
                        module_ref,
                        diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        &[name],
                    );
                }
            }
            return;
        }

        // Handle qualified name: import x = Namespace.Member
        if ref_node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.ctx.arena.get_qualified_name(ref_node)
        {
            // Check the leftmost part first - this is what determines TS2503 vs TS2694
            let left_name = self.get_leftmost_identifier_name(qn.left);
            if let Some(name) = left_name {
                // Try to resolve the left identifier
                let left_resolved = self.resolve_leftmost_qualified_name(qn.left);
                if left_resolved.is_none() {
                    self.error_at_node_msg(
                        qn.left,
                        diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        &[&name],
                    );
                    return; // Don't check for TS2694 if left doesn't exist
                }

                // If left is resolved, check if right member exists (TS2694)
                // Use the existing report_type_query_missing_member which handles this correctly
                self.report_type_query_missing_member(module_ref);

                // TS2708: Check if export import is used with a namespace member
                // When you have `export import a = NS.Member`, if NS contains only types,
                // you cannot export it as a value.
                // Check if the parent node is an EXPORT_DECLARATION (for `export import`)
                let mut is_exported = self.has_export_modifier(stmt_idx);
                if !is_exported
                    && let Some(ext) = self.ctx.arena.get_extended(stmt_idx)
                    && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                    && parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                {
                    is_exported = true;
                }
                if is_exported {
                    if self.is_unresolved_import_symbol(qn.left) {
                        return;
                    }
                    // Check if the left (namespace) is type-only by checking if it has
                    // any value members. For now, emit TS2708 if we're exporting an import
                    // from a namespace that contains a type member.
                    // Try to resolve the qualified name to check if it's type-only
                    if let Some(resolved_sym) = self.resolve_qualified_symbol(module_ref) {
                        let lib_binders = self.get_lib_binders();
                        if let Some(symbol) = self
                            .ctx
                            .binder
                            .get_symbol_with_libs(resolved_sym, &lib_binders)
                        {
                            // Check if this is a type-only symbol (interface or type alias)
                            let is_type_only = (symbol.flags
                                & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS))
                                != 0;
                            if is_type_only {
                                if self.should_suppress_namespace_value_error_for_failed_import(
                                    qn.left,
                                ) {
                                    return;
                                }
                                // Emit TS2708: Cannot use namespace as a value
                                // The error message mentions the namespace, not the member
                                self.error_namespace_used_as_value_at(&name, qn.left);
                            }
                        }
                    } else {
                        // Even if we can't resolve the full qualified name (TS2694 case),
                        // check if the namespace contains only types
                        if let Some(left_sym) = self.resolve_qualified_symbol(qn.left) {
                            let lib_binders = self.get_lib_binders();
                            if let Some(ns_symbol) =
                                self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders)
                            {
                                // Check if namespace exports table contains any value symbols
                                let has_value_exports = if let Some(exports) = &ns_symbol.exports {
                                    exports.as_ref().iter().any(|(_, &sym_id)| {
                                        if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                                            (sym.flags & symbol_flags::VALUE) != 0
                                        } else {
                                            false
                                        }
                                    })
                                } else {
                                    false
                                };

                                // If namespace has no value exports, emit TS2708
                                if !has_value_exports {
                                    if self.should_suppress_namespace_value_error_for_failed_import(
                                        qn.left,
                                    ) {
                                        return;
                                    }
                                    self.error_namespace_used_as_value_at(&name, qn.left);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get the leftmost identifier name from a node (handles nested QualifiedNames).
    fn get_leftmost_identifier_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            return Some(ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.get_leftmost_identifier_name(qn.left);
        }
        None
    }

    /// Resolve the leftmost identifier in a potentially nested QualifiedName.
    fn resolve_leftmost_qualified_name(&self, idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.resolve_leftmost_qualified_name(qn.left);
        }
        None
    }

    fn should_suppress_namespace_value_error_for_failed_import(&self, left_idx: NodeIndex) -> bool {
        let Some(left_sym) = self.resolve_leftmost_qualified_name(left_idx) else {
            return false;
        };

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) else {
            return false;
        };

        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return false;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            return false;
        };

        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        let module_name = if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node) else {
                return false;
            };
            let Some(module_node) = self.ctx.arena.get(import_decl.module_specifier) else {
                return false;
            };
            if module_node.kind != SyntaxKind::StringLiteral as u16 {
                return false;
            }
            let Some(literal) = self.ctx.arena.get_literal(module_node) else {
                return false;
            };
            literal.text.as_str()
        } else if decl_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
            || decl_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
            || decl_node.kind == syntax_kind_ext::IMPORT_CLAUSE
        {
            let mut current = decl_idx;
            let mut import_decl_idx = None;
            for _ in 0..4 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                let parent = ext.parent;
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    break;
                };
                if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    import_decl_idx = Some(parent);
                    break;
                }
                current = parent;
            }

            let Some(import_decl_idx) = import_decl_idx else {
                return false;
            };
            let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx) else {
                return false;
            };
            let Some(import_decl) = self.ctx.arena.get_import_decl(import_decl_node) else {
                return false;
            };
            let Some(module_node) = self.ctx.arena.get(import_decl.module_specifier) else {
                return false;
            };
            if module_node.kind != SyntaxKind::StringLiteral as u16 {
                return false;
            }
            let Some(literal) = self.ctx.arena.get_literal(module_node) else {
                return false;
            };
            literal.text.as_str()
        } else {
            return false;
        };

        self.ctx.modules_with_ts2307_emitted.contains(module_name)
            || (!self.module_exists_cross_file(module_name)
                && !self.is_ambient_module_match(module_name))
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // Extract module specifier data eagerly to avoid borrow issues later
        let module_specifier_idx = import.module_specifier;
        let import_clause_idx = import.import_clause;

        let Some(spec_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = spec_node.pos;
        let spec_length = spec_node.end.saturating_sub(spec_node.pos);

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;
        let is_type_only_import = self
            .ctx
            .arena
            .get(import_clause_idx)
            .and_then(|clause_node| self.ctx.arena.get_import_clause(clause_node))
            .map(|clause| clause.is_type_only)
            .unwrap_or(false);
        let mut emitted_dts_import_error = false;
        if module_name.ends_with(".d.ts") && !is_type_only_import {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let suggested = module_name.trim_end_matches(".d.ts");
            let message = format_message(
                diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                &[suggested],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
            );
            emitted_dts_import_error = true;
        }

        if let Some(binders) = &self.ctx.all_binders
            && binders.iter().any(|binder| {
                binder.declared_modules.contains(module_name)
                    || binder.shorthand_ambient_modules.contains(module_name)
            })
        {
            tracing::trace!(%module_name, "check_import_declaration: found in declared/shorthand modules, returning");
            return;
        }

        if self.would_create_cycle(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: cycle detected");
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(|s| s.as_str())
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular import detected: {}", cycle_str);

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        self.ctx.import_resolution_stack.push(module_name.clone());

        // Check ambient modules BEFORE resolution errors.
        // `declare module "x"` in .d.ts files should suppress TS2307 even when
        // file-based resolution fails (matching check_import_equals_declaration).
        if self.is_ambient_module_match(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: ambient module match, returning");
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            {
                let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                error_code = fallback_code;
                error_message = fallback_message;
            }
            tracing::trace!(%module_name, error_code, "check_import_declaration: resolution error found");
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            if error_code
                != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // Check if module was successfully resolved
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
                let mut skip_export_checks = false;
                // Extract data we need before any mutable borrows
                let (is_declaration_file_flag, file_info) = {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let skip_exports = is_js_like && !source_file.is_declaration_file;
                        let target_is_esm =
                            file_name.ends_with(".mjs") || file_name.ends_with(".mts");
                        let is_dts = source_file.is_declaration_file;
                        (is_dts, Some((skip_exports, target_is_esm)))
                    } else {
                        (false, None)
                    }
                };

                if let Some((should_skip_exports, target_is_esm)) = file_info {
                    if should_skip_exports {
                        skip_export_checks = true;
                    }

                    // TS1479: Check if CommonJS file is importing an ES module
                    // This error occurs when the current file will emit require() calls
                    // but the target file is an ES module (which cannot be required)
                    let current_is_commonjs = {
                        let current_file = &self.ctx.file_name;
                        // .cts files are always CommonJS
                        let is_cts = current_file.ends_with(".cts");
                        // .mts files are always ESM
                        let is_mts = current_file.ends_with(".mts");
                        // For other files, check if module system will emit require() calls
                        is_cts || (!is_mts && !self.ctx.compiler_options.module.is_es_module())
                    };

                    if current_is_commonjs && target_is_esm && !is_type_only_import {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                            &[module_name],
                        );
                        self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                        );
                    }
                }

                if is_declaration_file_flag && !is_type_only_import && !emitted_dts_import_error {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let suggested = if module_name.ends_with(".d.ts") {
                        module_name.trim_end_matches(".d.ts")
                    } else {
                        module_name.as_str()
                    };
                    let message = format_message(
                            diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                            &[suggested],
                        );
                    self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                        );
                }
                if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                    let normalized_module_name = module_name.trim_matches('"').trim_matches('\'');
                    if !binder.is_external_module
                        && !self.is_ambient_module_match(module_name)
                        && !binder.declared_modules.contains(normalized_module_name)
                    {
                        let arena = self.ctx.get_arena_for_file(target_idx as u32);
                        if let Some(source_file) = arena.source_files.first()
                            && !source_file.is_declaration_file
                        {
                            let file_name = source_file.file_name.as_str();
                            let is_js_like = file_name.ends_with(".js")
                                || file_name.ends_with(".jsx")
                                || file_name.ends_with(".mjs")
                                || file_name.ends_with(".cjs");
                            if !is_js_like {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::FILE_IS_NOT_A_MODULE,
                                    &[&source_file.file_name],
                                );
                                self.error_at_position(
                                    spec_start,
                                    spec_length,
                                    &message,
                                    diagnostic_codes::FILE_IS_NOT_A_MODULE,
                                );
                                self.ctx.import_resolution_stack.pop();
                                return;
                            }
                        }
                    }
                }
                if !skip_export_checks {
                    self.check_imported_members(import, module_name);
                }
            } else {
                self.check_imported_members(import, module_name);
            }

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: found in module_exports, checking members");
            self.check_imported_members(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        tracing::trace!(%module_name, "check_import_declaration: fallback - emitting module-not-found error");
        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx
                .modules_with_ts2307_emitted
                .insert(module_key.clone());
            let (message, code) = self.module_not_found_diagnostic(module_name);
            // Use pre-extracted position instead of error_at_node to avoid
            // silent failures when get_node_span returns None
            self.error_at_position(spec_start, spec_length, &message, code);
        }

        self.ctx.import_resolution_stack.pop();
    }

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Check re-export chains for circular dependencies.
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut FxHashSet<String>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if visited.contains(module_name) {
            let cycle_path: Vec<&str> = visited
                .iter()
                .map(|s| s.as_str())
                .chain(std::iter::once(module_name))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!(
                "{}: {}",
                diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                cycle_str
            );

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error(
                    0,
                    0,
                    message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        visited.insert(module_name.to_string());

        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        visited.remove(module_name);
    }

    /// Check if adding a module to the resolution path would create a cycle.
    pub(crate) fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }

    // =========================================================================
    // Re-export Resolution Helpers
    // =========================================================================

    /// Try to resolve an import through the target module's binder re-export chains.
    /// Traverses across binder boundaries by resolving each re-export source
    /// to its target file and checking that file's binder.
    fn resolve_import_via_target_binder(&self, module_name: &str, import_name: &str) -> bool {
        if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_in_file(target_idx, import_name, &mut visited);
        }
        false
    }

    /// Try to resolve an import by searching all binders' re-export chains.
    fn resolve_import_via_all_binders(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .resolve_import_if_needed_public(module_name, import_name)
                    .is_some()
                    || binder
                        .resolve_import_if_needed_public(normalized, import_name)
                        .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Resolve an import by checking a specific file's exports and following
    /// re-export chains across binder boundaries. Each file has its own binder
    /// in multi-file mode, so we traverse wildcard/named re-exports by resolving
    /// each source specifier to its target file and checking that file's binder.
    fn resolve_import_in_file(
        &self,
        file_idx: usize,
        import_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> bool {
        if !visited.insert(file_idx) {
            return false; // Cycle detection
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && exports.has(import_name)
        {
            return true;
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = reexports.get(import_name)
        {
            let name = original_name.as_deref().unwrap_or(import_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && self.resolve_import_in_file(source_idx, name, visited)
            {
                return true;
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && self.resolve_import_in_file(source_idx, import_name, visited)
                {
                    return true;
                }
            }
        }

        false
    }
}
