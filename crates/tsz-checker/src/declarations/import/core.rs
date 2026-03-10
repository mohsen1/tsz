//! Core import/export checking implementation.

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
    /// Uses TS2792 when the effective module resolution is "Classic", otherwise TS2307.
    ///
    /// tsc uses `getEmitModuleResolutionKind(compilerOptions) === Classic` to decide.
    /// The `implied_classic_resolution` flag is computed at config resolution time from
    /// the effective module resolution (considering both `module` and `moduleResolution`
    /// options), matching tsc's `getEmitModuleResolutionKind()`.
    pub(crate) fn module_not_found_diagnostic(&self, module_name: &str) -> (String, u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            return (error.message.clone(), error.code);
        }

        let use_2792 = self.ctx.compiler_options.implied_classic_resolution;

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

    /// Check whether the `export =` target symbol is NOT a module or variable.
    /// Used for TS2497: modules whose `export =` targets a class/function/interface
    /// (not Module | Variable) cannot be namespace-imported or named-imported
    /// without `esModuleInterop` / `allowSyntheticDefaultImports`.
    fn export_equals_target_is_not_module_or_variable(
        &self,
        exports_table: &tsz_binder::SymbolTable,
    ) -> bool {
        let Some(export_equals_sym) = exports_table.get("export=") else {
            return false;
        };

        let lib_binders: Vec<_> = self
            .ctx
            .lib_contexts
            .iter()
            .map(|lc| std::sync::Arc::clone(&lc.binder))
            .collect();

        // If the export= target is a type-only import (e.g., `import type * as X`),
        // it's erased at runtime, so TS2497 should not apply.
        if let Some(sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_equals_sym, &lib_binders)
            && sym.is_type_only
        {
            return false;
        }

        // Resolve aliases to find the actual target symbol
        let resolved = if let Some(sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_equals_sym, &lib_binders)
            && (sym.flags & symbol_flags::ALIAS) != 0
        {
            let mut visited = Vec::new();
            self.resolve_alias_symbol(export_equals_sym, &mut visited)
                .unwrap_or(export_equals_sym)
        } else {
            export_equals_sym
        };

        let Some(target) = self.ctx.binder.get_symbol_with_libs(resolved, &lib_binders) else {
            return false;
        };

        // tsc checks: !(symbol.flags & (SymbolFlags.Module | SymbolFlags.Variable))
        let is_module_or_variable = (target.flags
            & (symbol_flags::MODULE
                | symbol_flags::FUNCTION_SCOPED_VARIABLE
                | symbol_flags::BLOCK_SCOPED_VARIABLE))
            != 0;
        !is_module_or_variable
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

    /// Type-level fallback for `has_named_export_via_export_equals`.
    ///
    /// When the `export =` target is a typed value (e.g., `const x: T` where
    /// `T = { [P in 'NotFound']: unknown }`), `NotFound` is invisible to symbol-table
    /// lookups but accessible via type-level property access.
    fn has_named_export_via_export_equals_type(
        &mut self,
        exports_table: &tsz_binder::SymbolTable,
        import_name: &str,
    ) -> bool {
        use tsz_solver::operations::property::PropertyAccessResult;

        let Some(export_equals_sym) = exports_table.get("export=") else {
            return false;
        };

        let export_type = self.get_type_of_symbol(export_equals_sym);
        if export_type == tsz_solver::TypeId::ERROR || export_type == tsz_solver::TypeId::ANY {
            return false;
        }

        matches!(
            self.resolve_property_access_with_env(export_type, import_name),
            PropertyAccessResult::Success { .. }
        )
    }

    /// Check if an import resolves to a `.ts`/`.tsx` source file (not a `.d.ts` declaration).
    /// For source files, TS1192 always fires regardless of `allowSyntheticDefaultImports`,
    /// because the developer controls the module and should add `export default`.
    ///
    /// Exception: Node module kinds (Node16/Node18/Node20/NodeNext) where CJS-format
    /// `.ts` files always have a synthetic default. Since detecting CJS format requires
    /// checking package.json, we conservatively return `false` for all Node module modes.
    fn is_source_file_import(&self, module_name: &str) -> bool {
        // In Node module resolution, CJS-format .ts files always have a default export.
        // Since format depends on package.json "type" field, we can't easily determine
        // it here. Conservatively respect allowSyntheticDefaultImports for all Node modes.
        if self.ctx.compiler_options.module.is_node_module() {
            return false;
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            if let Some(sf) = arena.source_files.first() {
                let name = sf.file_name.as_str();
                // .ts/.tsx but NOT .d.ts/.d.tsx
                return (name.ends_with(".ts") || name.ends_with(".tsx"))
                    && !name.ends_with(".d.ts")
                    && !name.ends_with(".d.tsx");
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

        let has_namespace_import =
            bindings_node.is_some_and(|n| n.kind == syntax_kind_ext::NAMESPACE_IMPORT);

        // Nothing to check
        if !has_default_import && !has_named_imports && !has_namespace_import {
            return;
        }

        // Resolve exports table (shared between default and named import checking)
        let normalized = module_name.trim_matches('"').trim_matches('\'');
        // TSC includes source-level quotes in module diagnostic messages:
        // Module '"./foo"' has no exported member 'X'
        let quoted_module = format!("\"{module_name}\"");
        let exports_table = self.resolve_effective_module_exports(module_name);

        // TS2497: Module with `export =` targeting a non-module/non-variable symbol
        // can only be referenced via default import. Applies to namespace imports
        // (`import * as X`) and named imports (`import { X }`), regardless of
        // esModuleInterop / allowSyntheticDefaultImports.
        // Even with esModuleInterop enabled, namespace/named imports on `export =`
        // targeting a class/function/interface are invalid — the user must use a
        // default import (`import X from "mod"`) instead.
        if (has_namespace_import || has_named_imports)
            && !clause.is_type_only
            && let Some(ref table) = exports_table
            && table.has("export=")
            && self.export_equals_target_is_not_module_or_variable(table)
        {
            let flag_name = if (self.ctx.compiler_options.module as u32)
                >= (tsz_common::ModuleKind::ES2015 as u32)
            {
                "allowSyntheticDefaultImports"
            } else {
                "esModuleInterop"
            };
            let message = format_message(
                    diagnostic_messages::THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON,
                    &[flag_name],
                );
            self.error_at_node(
                    import.module_specifier,
                    &message,
                    diagnostic_codes::THIS_MODULE_CAN_ONLY_BE_REFERENCED_WITH_ECMASCRIPT_IMPORTS_EXPORTS_BY_TURNING_ON,
                );
        }

        // Check default import: import X from "module"
        // If the module has no "default" export, emit the canonical diagnostic
        // (TS1192 for no-default modules, TS1259 for export= modules).
        // For .ts source files, TS1192 always fires regardless of allowSyntheticDefaultImports.
        // For .d.ts/.js/.json files, allowSyntheticDefaultImports suppresses TS1192.
        if has_default_import && !has_named_default_binding {
            let is_source_file = self.is_source_file_import(module_name);
            if let Some(ref table) = exports_table {
                if !table.has("default") {
                    self.emit_no_default_export_error(module_name, clause.name, is_source_file);
                }
            } else if self
                .ctx
                .resolved_modules
                .as_ref()
                .is_some_and(|resolved| resolved.contains(module_name))
                && self.ctx.resolve_import_target(module_name).is_some()
            {
                // Module resolved but no exports table found - still emit TS1192
                self.emit_no_default_export_error(module_name, clause.name, is_source_file);
            }
        }

        // Check named imports: import { X, Y } from "module"
        // Note: tsc validates named imports even when TS1192 fires for the default import.
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

                    if import_name == "default" && self.ctx.allow_synthetic_default_imports() {
                        continue;
                    }

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
                                    &[&quoted_module, import_name, renamed_as],
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
                                    &[&quoted_module, import_name],
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
                                &[&quoted_module, import_name],
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

                if import_name == "default" && self.ctx.allow_synthetic_default_imports() {
                    continue;
                }

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
                                    &[&quoted_module, import_name, renamed_as],
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
                                    &[&quoted_module, import_name],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                );
                            }
                        } else if exports_table.has("default") || exports_table.has("export=") {
                            // Before emitting TS2614, try a type-level resolution for
                            // `export =` modules where the member may be a key of a
                            // mapped type stored as the type of the `export =` target.
                            let found_via_type = exports_table.has("export=")
                                && self.has_named_export_via_export_equals_type(
                                    &exports_table,
                                    import_name,
                                );

                            if !found_via_type {
                                // TS2614: Symbol doesn't exist but a default export does
                                let message = format_message(
                                    diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                                    &[&quoted_module, import_name],
                                );
                                self.error_at_node(
                                    specifier.name,
                                    &message,
                                    diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                                );
                            }
                        } else {
                            // TS2305: Symbol doesn't exist in the module at all
                            let message = format_message(
                                diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                                &[&quoted_module, import_name],
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
        let mut symbol_exists = binder.file_locals.has(import_name);
        if symbol_exists
            && let Some(sym_id) = binder.file_locals.get(import_name)
            && let Some(sym) = self.get_symbol_globally(sym_id)
            && let Some(augs) = self.ctx.binder.global_augmentations.get(import_name)
        {
            let all_are_global = sym
                .declarations
                .iter()
                .all(|d| augs.iter().any(|a| a.node == *d));
            if all_are_global {
                symbol_exists = false;
            }
        }
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
            if parent_idx.is_some()
                && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
            {
                is_ambient_external_module = true;
            }
        }

        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                let is_ambient_body = self.ctx.is_ambient_declaration(body_idx);
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
                    if is_ambient_body
                        && let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                        && !stmt_node.is_declaration()
                        && stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT
                    {
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
                        // TS1282/TS1283: VMS checks for export = <type>
                        if export_data.is_export_equals
                            && self.ctx.compiler_options.verbatim_module_syntax
                            && !is_declaration_file
                        {
                            self.check_vms_export_equals(export_data.expression);
                        }

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
                        } else if let Some(expected_type) =
                            self.jsdoc_type_annotation_for_node(stmt_idx)
                        {
                            let prev_context = self.ctx.contextual_type;
                            self.ctx.contextual_type = Some(expected_type);
                            let actual_type = self.get_type_of_node(export_data.expression);
                            self.ctx.contextual_type = prev_context;
                            self.check_assignable_or_report(
                                actual_type,
                                expected_type,
                                export_data.expression,
                            );
                            if let Some(expr_node) = self.ctx.arena.get(export_data.expression)
                                && expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            {
                                self.check_object_literal_excess_properties(
                                    actual_type,
                                    expected_type,
                                    export_data.expression,
                                );
                            }
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
        // JS files (.js, .jsx, .mjs, .cjs) are exempt — they get TS8003 instead.
        // CJS-extension files (.cts) are explicitly CommonJS — export= is valid.
        let is_cjs_extension = self.ctx.file_name.ends_with(".cts");

        let is_system_module = matches!(
            self.ctx.compiler_options.module,
            tsz_common::common::ModuleKind::System
        );
        let is_es_module = self.ctx.compiler_options.module.is_es_module();
        // `module: preserve` allows both CJS (`export =`) and ESM (`export default`)
        // syntax — it preserves the module format as-written. TS1203 should not fire.
        let is_preserve = matches!(
            self.ctx.compiler_options.module,
            tsz_common::common::ModuleKind::Preserve
        );
        // For node module modes (node16/node18/node20/nodenext), the module format
        // is per-file: .mts → ESM, .cts → CJS, .ts → depends on nearest package.json
        // "type" field. Use `file_is_esm` from the driver to determine this.
        let is_node_esm_file =
            self.ctx.compiler_options.module.is_node_module() && self.ctx.file_is_esm == Some(true);

        if (is_es_module || is_system_module || is_node_esm_file)
            && !is_preserve
            && !is_declaration_file
            && !self.is_js_file()
            && !is_cjs_extension
        {
            for &export_idx in &export_assignment_indices {
                if !self.is_ambient_declaration(export_idx) {
                    if is_system_module {
                        self.error_at_node(
                            export_idx,
                            "Export assignment is not supported when '--module' flag is 'system'.",
                            diagnostic_codes::EXPORT_ASSIGNMENT_IS_NOT_SUPPORTED_WHEN_MODULE_FLAG_IS_SYSTEM,
                        );
                    } else {
                        self.error_at_node(
                            export_idx,
                            "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.",
                            diagnostic_codes::EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
                        );
                    }
                }
            }
        }

        // TS2300: Check for duplicate export assignments
        // TypeScript emits TS2300 on ALL export assignments if there are 2+
        // tsc points the error at the expression (e.g., `x` in `export = x;`),
        // not at the `export` keyword.
        if export_assignment_indices.len() > 1 {
            for &export_idx in &export_assignment_indices {
                let error_node = self
                    .ctx
                    .arena
                    .get(export_idx)
                    .and_then(|node| self.ctx.arena.get_export_assignment(node))
                    .map(|data| data.expression)
                    .filter(|idx| idx.is_some())
                    .unwrap_or(export_idx);
                self.error_at_node(
                    error_node,
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
                    // tsc points TS2528 at the declaration name for named exports
                    // (e.g., `Foo` in `export default function Foo()`), or at `default`
                    // for anonymous exports. Find the best anchor node.
                    let anchor = self
                        .ctx
                        .arena
                        .get_export_decl_at(export_idx)
                        .and_then(|ed| {
                            let clause = self.ctx.arena.get(ed.export_clause)?;
                            // For function/class declarations, point at the name
                            if clause.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                self.ctx.arena.get_function(clause).and_then(|f| {
                                    let n = self.ctx.arena.get(f.name)?;
                                    if n.kind == SyntaxKind::Identifier as u16 {
                                        Some(f.name)
                                    } else {
                                        None
                                    }
                                })
                            } else if clause.kind == syntax_kind_ext::CLASS_DECLARATION {
                                self.ctx.arena.get_class(clause).and_then(|c| {
                                    let n = self.ctx.arena.get(c.name)?;
                                    if n.kind == SyntaxKind::Identifier as u16 {
                                        Some(c.name)
                                    } else {
                                        None
                                    }
                                })
                            } else if clause.kind == SyntaxKind::Identifier as u16 {
                                // For `export default Bar`, point at the identifier
                                Some(ed.export_clause)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(export_idx);
                    self.error_at_node(
                        anchor,
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

        self.ctx
            .arena
            .has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
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

    /// Check if a node is inside a module augmentation
    /// (`declare module "string" { ... }`).  Module augmentations have a
    /// `MODULE_DECLARATION` ancestor whose name is a string literal.
    pub(crate) fn is_inside_module_augmentation(&self, node_idx: NodeIndex) -> bool {
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
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(mod_data) = self.ctx.arena.get_module_at(current)
                && let Some(name_node) = self.ctx.arena.get(mod_data.name)
                && name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a node is inside a `declare global { ... }` augmentation block.
    pub(crate) fn is_inside_global_augmentation(&self, node_idx: NodeIndex) -> bool {
        use tsz_parser::parser::flags::node_flags;

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
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
            {
                return true;
            }
        }
        false
    }

    // =========================================================================
    // verbatimModuleSyntax Import Checks (TS1484, TS1485)
    // =========================================================================

    /// Check named import specifiers under `verbatimModuleSyntax`.
    pub(crate) fn check_verbatim_module_syntax_imports(
        &mut self,
        import: &tsz_parser::parser::node::ImportDeclData,
        module_name: &str,
    ) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if !self.ctx.compiler_options.verbatim_module_syntax {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };
        if clause.is_type_only {
            return;
        }

        // TS1295: In CJS+VMS mode, ESM import syntax is forbidden entirely.
        // Emit TS1295 on the import clause and skip ESM-specific checks.
        // TSC skips this check for .d.ts files.
        if self.is_current_file_commonjs_for_vms() && !self.ctx.is_declaration_file() {
            // TSC positions the error at the binding NAME:
            // - Default import `import X from ...` → at X
            // - Namespace import `import * as X from ...` → at X
            // - Named imports `import { X } from ...` → at X (first specifier name)
            let error_node = if clause.named_bindings.is_some() {
                if let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) {
                    if let Some(ns_import) = self.ctx.arena.get_named_imports(bindings_node) {
                        if ns_import.name.is_some() {
                            // Namespace import: use the name (esmy2 in `* as esmy2`)
                            ns_import.name
                        } else if let Some(&first_spec) = ns_import.elements.nodes.first() {
                            // Named imports: use first specifier's local name
                            if let Some(spec_node) = self.ctx.arena.get(first_spec)
                                && let Some(spec) = self.ctx.arena.get_specifier(spec_node)
                            {
                                if spec.name.is_some() {
                                    spec.name
                                } else {
                                    spec.property_name
                                }
                            } else {
                                clause.named_bindings
                            }
                        } else {
                            clause.named_bindings
                        }
                    } else {
                        clause.named_bindings
                    }
                } else {
                    clause.named_bindings
                }
            } else if clause.name.is_some() {
                clause.name
            } else {
                import.import_clause
            };
            self.error_at_node(
                error_node,
                diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
            );
            return;
        }

        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };
        if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return;
        }
        let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node) else {
            return;
        };

        for element_idx in &named_imports.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(*element_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                continue;
            };
            if specifier.is_type_only {
                continue;
            }

            let imported_name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            let Some(imported_name_node) = self.ctx.arena.get(imported_name_idx) else {
                continue;
            };
            let Some(imported_ident) = self.ctx.arena.get_identifier(imported_name_node) else {
                continue;
            };
            let import_name = imported_ident.escaped_text.clone();

            let local_name_idx = specifier.name;
            let local_name = if let Some(local_node) = self.ctx.arena.get(local_name_idx)
                && let Some(local_ident) = self.ctx.arena.get_identifier(local_node)
            {
                local_ident.escaped_text.clone()
            } else {
                import_name.clone()
            };

            // TS1485: type-only export chain
            if self.is_export_type_only_across_binders(module_name, &import_name) {
                let message = format_message(
                    diagnostic_messages::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR,
                    &[&local_name],
                );
                self.error_at_node(
                    local_name_idx,
                    &message,
                    diagnostic_codes::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR,
                );
                continue;
            }

            // TS1484: inherently a type
            if self.is_import_specifier_type_only(module_name, &import_name) {
                let message = format_message(
                    diagnostic_messages::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                    &[&local_name],
                );
                self.error_at_node(
                    local_name_idx,
                    &message,
                    diagnostic_codes::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                );
            }
        }
    }

    /// Check if a named import refers to a purely type-only entity.
    pub(crate) fn is_import_specifier_type_only(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let normalized = module_name.trim_matches('"').trim_matches('\'');

        if let Some(target_idx) = self.ctx.resolve_import_target(normalized)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
            && let Some(sym_id) = target_binder.file_locals.get(import_name)
            && let Some(sym) = target_binder.get_symbol(sym_id)
        {
            let flags = sym.flags;
            return (flags & PURE_TYPE) != 0 && (flags & VALUE) == 0;
        }

        for candidate in crate::module_resolution::module_specifier_candidates(module_name) {
            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate)
                && let Some(sym_id) = exports.get(import_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
            {
                let flags = sym.flags;
                return (flags & PURE_TYPE) != 0 && (flags & VALUE) == 0;
            }
        }

        false
    }

    /// TS1282/TS1283: VMS check for `export = X`.
    /// TS1282: X only refers to a type (interface/type alias, no value).
    /// TS1283: X resolves to a type-only declaration (import type).
    fn check_vms_export_equals(&mut self, expression: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        // Get the name of the exported identifier
        let Some(expr_node) = self.ctx.arena.get(expression) else {
            return;
        };
        let name = if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            ident.escaped_text.clone()
        } else {
            return;
        };

        // Look up the symbol in file_locals
        let Some(sym_id) = self.ctx.binder.file_locals.get(&name) else {
            return;
        };
        let Some(sym) = self.ctx.binder.symbols.get(sym_id) else {
            return;
        };

        // Check if this is a type-only import (TS1283)
        // Only if the symbol doesn't also have VALUE flags — a local `const I = {}`
        // alongside `import type I = ...` makes `export = I` valid.
        let value_flags = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::VALUE_MODULE;
        if sym.is_type_only && (sym.flags & value_flags) == 0 {
            let msg = format_message(
                diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
                &[&name],
            );
            self.error_at_node(
                expression,
                &msg,
                diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
            );
            return;
        }

        // Check if this is a pure type (TS1282)
        let pure_type = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        if (sym.flags & pure_type) != 0 && (sym.flags & value_flags) == 0 {
            let msg = format_message(
                diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
                &[&name],
            );
            self.error_at_node(
                expression,
                &msg,
                diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
            );
        }
    }

    /// Determine if the current file is treated as CommonJS for VMS checks.
    pub(crate) fn is_current_file_commonjs_for_vms(&self) -> bool {
        let current_file = &self.ctx.file_name;
        if current_file.ends_with(".cts") || current_file.ends_with(".cjs") {
            return true;
        }
        if current_file.ends_with(".mts") || current_file.ends_with(".mjs") {
            return false;
        }
        if let Some(is_esm) = self.ctx.file_is_esm {
            return !is_esm;
        }
        !self.ctx.compiler_options.module.is_es_module()
    }
}
