//! Import member validation — checking that imported members exist in module exports.

use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
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

        let clause_node = match self.ctx.arena.get(import.import_clause) {
            Some(node) => node,
            None => return,
        };

        let clause = match self.ctx.arena.get_import_clause(clause_node) {
            Some(c) => c,
            None => return,
        };

        // Only whole-declaration type-only imports keep a `resolution-mode` override
        // when the current module kind would otherwise reject import attributes.
        let resolution_mode =
            self.requested_resolution_mode(import.attributes, clause.is_type_only);
        let uses_fallback_branch_resolution = resolution_mode.is_some()
            && !self.resolution_mode_override_is_effective(import.attributes, clause.is_type_only);
        let exports_table =
            self.resolve_effective_module_exports_with_mode(module_name, resolution_mode);

        let resolved_target = if resolution_mode.is_some() {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        };
        if let Some(target_idx) = resolved_target {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            if let Some(source_file) = arena.source_files.first()
                && !source_file.is_declaration_file
            {
                let file_name = source_file.file_name.as_str();
                let is_js_like = file_name.ends_with(".js")
                    || file_name.ends_with(".jsx")
                    || file_name.ends_with(".mjs")
                    || file_name.ends_with(".cjs");
                let has_export_surface = exports_table
                    .as_ref()
                    .is_some_and(|exports| !exports.is_empty());
                if is_js_like && !has_export_surface && resolution_mode.is_none() {
                    return;
                }
            }
        }

        let has_default_import = clause.name.is_some();
        let bindings_node = self.ctx.arena.get(clause.named_bindings);
        let has_named_imports = bindings_node
            .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::NAMED_IMPORTS);
        let mut named_default_binding_nodes = Vec::new();
        let mut has_non_default_named_imports = false;

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
                    named_default_binding_nodes.push(*element_idx);
                } else {
                    has_non_default_named_imports = true;
                }
            }
        }

        let has_named_default_binding = !named_default_binding_nodes.is_empty();

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
        let has_json_default_export =
            self.module_has_json_default_export(module_name, Some(self.ctx.current_file_idx));

        // TS2497: Module with `export =` targeting a non-module/non-variable symbol
        // can only be referenced via default import. Applies to namespace imports
        // (`import * as X`) and named imports (`import { X }`), regardless of
        // esModuleInterop / allowSyntheticDefaultImports.
        // Even with esModuleInterop enabled, namespace/named imports on `export =`
        // targeting a class/function/interface are invalid — the user must use a
        // default import (`import X from "mod"`) instead.
        if (has_namespace_import || has_non_default_named_imports)
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

            // For each named import specifier, emit an additional diagnostic
            // alongside TS2497 explaining how the symbol should be imported.
            // The exact code depends on the module kind and importing file type:
            //   - ES module targets: TS2595 "can only be imported by using a default import"
            //   - CommonJS + .ts file: TS2616 "can only be imported by using 'import X = require(...)' or a default import"
            //   - CommonJS + .js file: TS2597 "can only be imported by using a 'require' call or by using a default import"
            if has_non_default_named_imports {
                let is_es_module = (self.ctx.compiler_options.module as u32)
                    >= (tsz_common::ModuleKind::ES2015 as u32);
                let is_js_file = self.ctx.file_name.ends_with(".js")
                    || self.ctx.file_name.ends_with(".jsx")
                    || self.ctx.file_name.ends_with(".mjs")
                    || self.ctx.file_name.ends_with(".cjs");

                if let Some(bindings_node) = bindings_node
                    && let Some(named_imports) = self.ctx.arena.get_named_imports(bindings_node)
                {
                    for element_idx in &named_imports.elements.nodes {
                        let Some(element_node) = self.ctx.arena.get(*element_idx) else {
                            continue;
                        };
                        let Some(specifier) = self.ctx.arena.get_specifier(element_node) else {
                            continue;
                        };

                        let name_idx = specifier.name;
                        let Some(name_node) = self.ctx.arena.get(name_idx) else {
                            continue;
                        };
                        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
                            continue;
                        };

                        let name = name_ident.escaped_text.as_str();
                        if name == "default" {
                            continue;
                        }

                        let (msg, code) = if is_es_module {
                            // TS2595
                            (
                                format_message(
                                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_USING_A_DEFAULT_IMPORT,
                                    &[name],
                                ),
                                diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_USING_A_DEFAULT_IMPORT,
                            )
                        } else if is_js_file {
                            // TS2597
                            (
                                format_message(
                                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_USING_A_DEFAULT_IMPORT,
                                    &[name],
                                ),
                                diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_USING_A_REQUIRE_CALL_OR_BY_USING_A_DEFAULT_IMPORT,
                            )
                        } else {
                            // TS2616
                            let quoted_spec = format!("\"{module_name}\"");
                            (
                                format_message(
                                    diagnostic_messages::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT,
                                    &[name, name, &quoted_spec],
                                ),
                                diagnostic_codes::CAN_ONLY_BE_IMPORTED_BY_USING_IMPORT_REQUIRE_OR_A_DEFAULT_IMPORT,
                            )
                        };

                        self.error_at_node(name_idx, &msg, code);
                    }
                }
            }
        }

        // Check default import: import X from "module"
        // If the module has no "default" export, emit the canonical diagnostic
        // (TS1192 for no-default modules, TS1259 for export= modules).
        if has_default_import && !has_named_default_binding {
            let is_source_file = self.is_source_file_import(module_name);
            let uses_system_namespace_default =
                self.source_file_import_uses_system_default_namespace_fallback(module_name);
            let has_default_binding = has_json_default_export
                || exports_table
                    .as_ref()
                    .is_some_and(|table| table.has("default") || table.has("export="));
            if exports_table.is_some() {
                if !has_default_binding && !uses_system_namespace_default {
                    self.emit_no_default_export_error(module_name, clause.name, is_source_file);
                }
            } else if self
                .ctx
                .resolved_modules
                .as_ref()
                .is_some_and(|resolved| resolved.contains(module_name))
                && resolved_target.is_some()
                && !uses_system_namespace_default
                && !has_default_binding
            {
                // Module resolved but no exports table found - still emit TS1192
                self.emit_no_default_export_error(module_name, clause.name, is_source_file);
            }
        }

        if !has_default_import && has_named_default_binding {
            let is_source_file = self.is_source_file_import(module_name);
            let uses_system_namespace_default =
                self.source_file_import_uses_system_default_namespace_fallback(module_name);
            let has_default_binding = has_json_default_export
                || exports_table
                    .as_ref()
                    .is_some_and(|table| table.has("default") || table.has("export="));
            if exports_table.is_some() {
                if !has_default_binding && !uses_system_namespace_default {
                    for &specifier_node in &named_default_binding_nodes {
                        self.emit_no_default_export_error(
                            module_name,
                            specifier_node,
                            is_source_file,
                        );
                    }
                }
            } else if self
                .ctx
                .resolved_modules
                .as_ref()
                .is_some_and(|resolved| resolved.contains(module_name))
                && resolved_target.is_some()
                && !uses_system_namespace_default
                && !has_default_binding
            {
                for &specifier_node in &named_default_binding_nodes {
                    self.emit_no_default_export_error(module_name, specifier_node, is_source_file);
                }
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
                    && resolved_target.is_none()
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

                    if import_name == "default" {
                        continue;
                    }

                    if let Some(renamed_as) = self.local_named_export_alias_for_module(
                        module_name,
                        import_name,
                        resolution_mode,
                    ) {
                        let message = format_message(
                            diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                            &[&quoted_module, import_name, &renamed_as],
                        );
                        self.error_at_node(
                            name_idx,
                            &message,
                            diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                        );
                        continue;
                    }

                    // Check re-export chains before emitting TS2305
                    let found_via_reexport = self.named_import_found_via_reexport(
                        module_name,
                        normalized,
                        import_name,
                        resolution_mode,
                    );

                    if !found_via_reexport {
                        // Use the unified JS export surface to check for CommonJS
                        // property-assignment exports (exports.foo = ..., module.exports.foo = ...).
                        if resolution_mode.is_none()
                            && self.js_export_surface_has_export(
                                module_name,
                                import_name,
                                Some(self.ctx.current_file_idx),
                            )
                        {
                            continue;
                        }

                        // Check if the symbol exists locally in the target module
                        // to distinguish between TS2459, TS2460, and TS2305
                        let (mut exists_locally, exported_as) = self
                            .check_local_symbol_and_renamed_export(
                                module_name,
                                import_name,
                                resolution_mode,
                            );
                        if uses_fallback_branch_resolution {
                            exists_locally = false;
                        }

                        if exists_locally {
                            if let Some(ref renamed_as) = exported_as {
                                // TS2460: Symbol exists locally and is exported under a different name
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                    &[&quoted_module, import_name, renamed_as],
                                );
                                self.error_at_node(
                                    name_idx,
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
                                    name_idx,
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
                                name_idx,
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

                if import_name == "default" {
                    continue;
                }

                if let Some(renamed_as) = self.local_named_export_alias_for_module(
                    module_name,
                    import_name,
                    resolution_mode,
                ) {
                    let message = format_message(
                        diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                        &[&quoted_module, import_name, &renamed_as],
                    );
                    self.error_at_node(
                        name_idx,
                        &message,
                        diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                    );
                    continue;
                }

                if !exports_table.has(import_name)
                    && !self.has_named_export_via_export_equals(&exports_table, import_name)
                {
                    // Before emitting TS2305, check if this import can be resolved
                    // through re-export chains (wildcard or named re-exports).
                    let found_via_reexport = self.named_import_found_via_reexport(
                        module_name,
                        normalized,
                        import_name,
                        resolution_mode,
                    );

                    if !found_via_reexport {
                        // Use the unified JS export surface to check for CommonJS
                        // property-assignment exports (exports.foo = ..., module.exports.foo = ...).
                        if resolution_mode.is_none()
                            && self.js_export_surface_has_export(
                                module_name,
                                import_name,
                                Some(self.ctx.current_file_idx),
                            )
                        {
                            continue;
                        }

                        // Check if the symbol exists locally in the target module
                        // to distinguish between TS2459, TS2460, and TS2305
                        let (mut exists_locally, exported_as) = self
                            .check_local_symbol_and_renamed_export(
                                module_name,
                                import_name,
                                resolution_mode,
                            );
                        if uses_fallback_branch_resolution {
                            exists_locally = false;
                        }

                        if exists_locally {
                            if let Some(ref renamed_as) = exported_as {
                                // TS2460: Symbol exists locally and is exported under a different name
                                let message = format_message(
                                    diagnostic_messages::MODULE_DECLARES_LOCALLY_BUT_IT_IS_EXPORTED_AS,
                                    &[&quoted_module, import_name, renamed_as],
                                );
                                self.error_at_node(
                                    name_idx,
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
                                    name_idx,
                                    &message,
                                    diagnostic_codes::MODULE_DECLARES_LOCALLY_BUT_IT_IS_NOT_EXPORTED,
                                );
                            }
                        } else if has_json_default_export
                            || exports_table.has("default")
                            || exports_table.has("export=")
                        {
                            // Before emitting TS2614, try a type-level resolution for
                            // `export =` modules where the member may be a key of a
                            // mapped type stored as the type of the `export =` target.
                            let found_via_type = exports_table.has("export=")
                                && self.has_named_export_via_export_equals_type(
                                    &exports_table,
                                    import_name,
                                );

                            // When esModuleInterop or allowSyntheticDefaultImports is
                            // enabled and the module uses `export =`, tsc allows named
                            // imports without emitting TS2614.
                            let has_export_equals = exports_table.has("export=");
                            let has_interop = self.ctx.compiler_options.es_module_interop
                                || self.ctx.compiler_options.allow_synthetic_default_imports;
                            let suppress_for_interop = has_export_equals && has_interop;

                            if !found_via_type && !suppress_for_interop {
                                // TS2614: Symbol doesn't exist but a default export does
                                let message = format_message(
                                    diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                                    &[&quoted_module, import_name],
                                );
                                self.error_at_node(
                                    name_idx,
                                    &message,
                                    diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER_DID_YOU_MEAN_TO_USE_IMPORT_FROM_INSTEAD,
                                );
                            }
                        } else {
                            // Check for spelling suggestions (TS2724) before TS2305
                            let export_names: Vec<&str> = exports_table
                                .iter()
                                .map(|(name, _)| name.as_str())
                                .collect();
                            if let Some(suggestion) =
                                tsz_parser::parser::spelling::get_spelling_suggestion(
                                    import_name,
                                    &export_names,
                                )
                            {
                                // TS2724: did you mean?
                                let message = format_message(
                                    diagnostic_messages::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                                    &[&quoted_module, import_name, suggestion],
                                );
                                self.error_at_node(
                                    name_idx,
                                    &message,
                                    diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN,
                                );
                            } else {
                                // TS2305: Symbol doesn't exist in the module at all
                                let message = format_message(
                                    diagnostic_messages::MODULE_HAS_NO_EXPORTED_MEMBER,
                                    &[&quoted_module, import_name],
                                );
                                self.error_at_node(
                                    name_idx,
                                    &message,
                                    diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER,
                                );
                            }
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
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> (bool, Option<String>) {
        tracing::trace!("Checking if symbol exists locally and is renamed");

        // Try to get the target module's binder
        let resolved_target = self.ctx.resolve_import_target_from_file_with_mode(
            self.ctx.current_file_idx,
            module_name,
            resolution_mode,
        );
        let target_binder = if let Some(target_idx) = resolved_target {
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
                // Only fall back to all-binders scan when we couldn't resolve
                // the import target at all. If resolve_import_target succeeded
                // but get_binder_for_file returned None, we still know which
                // file the module points to — scanning all binders would find
                // symbols from unrelated files and cause false TS2459.
                if resolved_target.is_some() {
                    tracing::trace!(
                        "Import target resolved but binder not found, returning (false, None)"
                    );
                    return (false, None);
                }
                tracing::trace!("No direct target binder, checking all binders");
                // Use the global module binder index for O(1) lookup when available.
                if let Some(ref idx) = self.ctx.global_module_binder_index {
                    let normalized = module_name.trim_matches('"').trim_matches('\'');
                    let candidate_indices = idx
                        .get(module_name)
                        .into_iter()
                        .flatten()
                        .chain(idx.get(normalized).into_iter().flatten());
                    if let Some(all_binders) = &self.ctx.all_binders {
                        let mut seen = rustc_hash::FxHashSet::default();
                        for &binder_idx in candidate_indices {
                            if !seen.insert(binder_idx) {
                                continue;
                            }
                            if let Some(binder) = all_binders.get(binder_idx) {
                                tracing::trace!(binder_idx, "Found matching binder via index");
                                if let Some(exists) = self.check_symbol_in_binder(
                                    binder,
                                    import_name,
                                    module_name,
                                    resolution_mode,
                                ) {
                                    return exists;
                                }
                            }
                        }
                    }
                } else if let Some(all_binders) = &self.ctx.all_binders {
                    // Fallback: O(N) scan when index not built
                    let normalized = module_name.trim_matches('"').trim_matches('\'');
                    tracing::trace!(
                        num_binders = all_binders.len(),
                        "Checking all binders (fallback)"
                    );
                    for binder in all_binders.iter() {
                        if binder.module_exports.contains_key(module_name)
                            || binder.module_exports.contains_key(normalized)
                        {
                            tracing::trace!("Found matching binder via exports");
                            if let Some(exists) = self.check_symbol_in_binder(
                                binder,
                                import_name,
                                module_name,
                                resolution_mode,
                            ) {
                                return exists;
                            }
                        }
                    }
                }
                tracing::trace!("No binder found, returning (false, None)");
                return (false, None);
            }
        };

        if let Some(result) =
            self.check_symbol_in_binder(target_binder, import_name, module_name, resolution_mode)
        {
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
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
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
        let (file_name, target_arena) = if let Some(target_idx) =
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            ) {
            let arena = self.ctx.get_arena_for_file(target_idx as u32);
            (
                arena.source_files.first().map(|sf| sf.file_name.as_str()),
                Some(arena),
            )
        } else {
            (None, None)
        };

        if let Some(arena) = target_arena
            && let Some(renamed_as) = self.local_named_export_alias_for_import(arena, import_name)
        {
            return Some((true, Some(renamed_as)));
        }

        for &key in &module_keys {
            if let Some(exports) = binder.module_exports.get(key) {
                // Check if the symbol is exported under a different name
                // by looking through all export names
                for (export_name, sym_id) in exports.iter() {
                    if let Some(sym) = binder.symbols.get(*sym_id) {
                        let decl_arena = if sym.decl_file_idx == u32::MAX {
                            self.ctx.arena
                        } else {
                            self.ctx.get_arena_for_file(sym.decl_file_idx)
                        };
                        // Check if this symbol has a declaration with the import_name
                        let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                            self.declaration_name_matches_string(decl_arena, decl_idx, import_name)
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
                    let decl_arena = if sym.decl_file_idx == u32::MAX {
                        self.ctx.arena
                    } else {
                        self.ctx.get_arena_for_file(sym.decl_file_idx)
                    };
                    let has_matching_name = sym.declarations.iter().any(|&decl_idx| {
                        self.declaration_name_matches_string(decl_arena, decl_idx, import_name)
                    });

                    if has_matching_name && export_name.as_str() != import_name {
                        return Some((true, Some(export_name.clone())));
                    }
                }
            }
        }

        // If the module uses `export =`, the symbol may be the export target itself.
        // In that case it IS exported (just not as a named export), so don't report
        // it as "locally declared but not exported" (TS2459). Let the caller fall
        // through to the appropriate `export =` diagnostic (TS2616/TS2595/TS2597).
        let has_export_equals = module_keys.iter().any(|key| {
            binder
                .module_exports
                .get(*key)
                .is_some_and(|exports| exports.has("export="))
        }) || file_name.is_some_and(|fname| {
            binder
                .module_exports
                .get(fname)
                .is_some_and(|exports| exports.has("export="))
        });
        if has_export_equals {
            return None;
        }

        // Symbol exists locally but is not exported
        Some((true, None))
    }

    /// Check if a declaration's name matches the expected string.
    fn declaration_name_matches_string(
        &self,
        arena: &NodeArena,
        decl_idx: NodeIndex,
        expected_name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = arena.get(decl_idx) else {
            return false;
        };

        let name_node_idx = match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(var_decl) = arena.get_variable_declaration(node) {
                    var_decl.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node) {
                    func.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node) {
                    class.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(interface) = arena.get_interface(node) {
                    interface.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(type_alias) = arena.get_type_alias(node) {
                    type_alias.name
                } else {
                    return false;
                }
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node) {
                    enum_decl.name
                } else {
                    return false;
                }
            }
            _ => return false,
        };

        let Some(name_node) = arena.get(name_node_idx) else {
            return false;
        };

        let Some(ident) = arena.get_identifier(name_node) else {
            return false;
        };

        arena.resolve_identifier_text(ident) == expected_name
    }

    fn local_named_export_alias_for_import(
        &self,
        arena: &NodeArena,
        import_name: &str,
    ) -> Option<String> {
        let source_file = arena.source_files.first()?;
        let mut direct_export = false;
        let mut renamed_export = None;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if arena
                .get_declaration_modifiers(stmt_node)
                .is_some_and(|mods| arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword))
                && self.declaration_name_matches_string(arena, stmt_idx, import_name)
            {
                direct_export = true;
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() || export_decl.export_clause.is_none() {
                continue;
            }
            // Skip type-only export declarations (`export type { ... }`).
            // These don't create value exports, so they shouldn't trigger TS2460.
            let decl_is_type_only = export_decl.is_type_only;
            let Some(clause_node) = arena.get(export_decl.export_clause) else {
                continue;
            };
            if arena.get_named_imports(clause_node).is_none() {
                if !decl_is_type_only
                    && self.declaration_name_matches_string(
                        arena,
                        export_decl.export_clause,
                        import_name,
                    )
                {
                    direct_export = true;
                }
                continue;
            }
            let Some(named_exports) = arena.get_named_imports(clause_node) else {
                continue;
            };

            for &spec_idx in &named_exports.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(specifier) = arena.get_specifier(spec_node) else {
                    continue;
                };
                // Skip type-only specifiers (`export { type X as Y }`).
                if decl_is_type_only || specifier.is_type_only {
                    continue;
                }

                let original_name_idx = if specifier.property_name.is_none() {
                    specifier.name
                } else {
                    specifier.property_name
                };
                let exported_name_idx = if specifier.name.is_none() {
                    original_name_idx
                } else {
                    specifier.name
                };

                let Some(original_name_node) = arena.get(original_name_idx) else {
                    continue;
                };
                let Some(original_ident) = arena.get_identifier(original_name_node) else {
                    continue;
                };
                if arena.resolve_identifier_text(original_ident) != import_name {
                    continue;
                }

                let Some(exported_name_node) = arena.get(exported_name_idx) else {
                    continue;
                };
                let Some(exported_ident) = arena.get_identifier(exported_name_node) else {
                    continue;
                };
                let exported_name = arena.resolve_identifier_text(exported_ident);

                if exported_name == import_name {
                    direct_export = true;
                } else if renamed_export.is_none() {
                    renamed_export = Some(exported_name.to_string());
                }
            }
        }

        if direct_export { None } else { renamed_export }
    }

    fn local_named_export_alias_for_module(
        &self,
        module_name: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<String> {
        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_name)
        }?;
        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        self.local_named_export_alias_for_import(arena, import_name)
    }

    fn any_ambient_module_declared(&self, module_name: &str) -> bool {
        let normalized = module_name.trim_matches('"').trim_matches('\'');

        // Use the pre-built global index for O(1) exact lookup + small pattern scan
        if let Some(declared) = &self.ctx.global_declared_modules {
            // O(1) exact match
            if declared.exact.contains(normalized) {
                return true;
            }
            // Small linear scan over wildcard patterns only
            for pattern in &declared.patterns {
                if Self::module_name_matches_pattern_for_imports(pattern, normalized) {
                    return true;
                }
            }
            return false;
        }

        // Fallback: scan all binders (when global index not built)
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
}
