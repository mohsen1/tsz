//! Import member validation — checking that imported members exist in module exports.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
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
            self.check_js_type_only_imports_for_ambient_module(import, module_name);
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

        // Only whole-declaration type-only imports keep a `resolution-mode` override
        // when the current module kind would otherwise reject import attributes.
        let resolution_mode =
            self.requested_resolution_mode(import.attributes, clause.is_type_only);
        let uses_fallback_branch_resolution = resolution_mode.is_some()
            && !self.resolution_mode_override_is_effective(import.attributes, clause.is_type_only);

        let resolved_target = if resolution_mode.is_some() {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            )
        } else {
            self.ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
                .or_else(|| self.ctx.resolve_import_target(module_name))
        };

        // Default-only imports only need to know whether a default-like binding exists.
        // Building the full export surface for large declaration bundles such as React
        // is much more expensive than the diagnostic question we need to answer here.
        let needs_full_exports = has_namespace_import
            || has_non_default_named_imports
            || (!has_default_import && has_named_default_binding);
        let exports_table = if needs_full_exports {
            if resolution_mode.is_some() {
                self.resolve_effective_module_exports_with_mode(module_name, resolution_mode)
            } else {
                self.resolve_effective_module_exports_from_file(
                    module_name,
                    Some(self.ctx.current_file_idx),
                )
            }
        } else {
            None
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
                let has_export_surface = if let Some(exports) = exports_table.as_ref() {
                    !exports.is_empty()
                } else {
                    self.module_has_default_binding_fast_path(module_name, resolution_mode)
                };
                if is_js_like && !has_export_surface && resolution_mode.is_none() {
                    return;
                }
            }
        }

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
        let has_module_exports_binding =
            self.module_uses_module_exports_interop(module_name, resolution_mode);
        let has_default_binding = has_json_default_export
            || has_module_exports_binding
            || self.module_has_default_binding_fast_path(module_name, resolution_mode)
            || exports_table.as_ref().is_some_and(|table| {
                table.has("default")
                    || table.has("export=")
                    || (has_module_exports_binding && table.has("module.exports"))
            });

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
            if exports_table.is_some() {
                if !has_default_binding && !uses_system_namespace_default {
                    self.emit_no_default_export_error(module_name, clause.name, is_source_file);
                }
            } else if self.ctx.resolved_modules.as_ref().is_some_and(|resolved| {
                crate::module_resolution::module_specifier_candidates(module_name)
                    .iter()
                    .any(|candidate| resolved.contains(candidate))
            }) && resolved_target.is_some()
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
            } else if self.ctx.resolved_modules.as_ref().is_some_and(|resolved| {
                crate::module_resolution::module_specifier_candidates(module_name)
                    .iter()
                    .any(|candidate| resolved.contains(candidate))
            }) && resolved_target.is_some()
                && !uses_system_namespace_default
                && !has_default_binding
            {
                for &specifier_node in &named_default_binding_nodes {
                    self.emit_no_default_export_error(module_name, specifier_node, is_source_file);
                }
            }
        }

        if has_default_import
            && clause.name.is_some()
            && let default_name_idx = clause.name
            && let Some(default_name) = self.get_identifier_text_from_idx(default_name_idx)
            && (self.local_import_binding_is_type_only(&default_name)
                || self.import_local_binding_is_type_only(default_name_idx)
                || self.import_binding_is_type_only(module_name, "default"))
            && self.should_report_js_type_only_import_diagnostic(clause.is_type_only, false)
        {
            self.emit_js_type_only_import_diagnostic(default_name_idx, &default_name, module_name);
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
                if self.ctx.resolved_modules.as_ref().is_some_and(|resolved| {
                    crate::module_resolution::module_specifier_candidates(module_name)
                        .iter()
                        .any(|candidate| resolved.contains(candidate))
                }) && resolved_target.is_none()
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

                    // Some ambient/module-resolution paths don't materialize a direct
                    // exports table, but named imports can still be satisfied via
                    // members of an `export =` target (e.g. `export = a.b`).
                    if self
                        .resolve_named_export_via_export_equals(module_name, import_name)
                        .is_some()
                    {
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

                        // When the target module uses `export = X` and `X`
                        // exists locally with the imported name, the TS2497 +
                        // TS2616/TS2595/TS2597 path earlier in this function
                        // already reports the import-style mismatch. Skip the
                        // duplicate "declares 'X' locally" TS2459/TS2460.
                        let module_uses_export_equals = exports_table.has("export=");
                        let suppress_for_export_equals =
                            exists_locally && module_uses_export_equals;
                        if exists_locally && !suppress_for_export_equals {
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
                        } else if suppress_for_export_equals {
                            // TS2497 + TS2616/TS2595/TS2597 already emitted
                            // earlier in this function for the export-equals
                            // import mismatch.
                        } else if has_json_default_export
                            || has_module_exports_binding
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
                    // Import exists - check if it should be elided from JavaScript output.
                    // This must account for plain type aliases/interfaces, namespace-like
                    // imports with no runtime value, and cross-binder type-only chains.
                    let local_is_type_only = self
                        .get_identifier_text_from_idx(specifier.name)
                        .as_deref()
                        .is_some_and(|local_name| {
                            self.local_import_binding_is_type_only(local_name)
                        });
                    if local_is_type_only
                        || self.import_binding_is_type_only(module_name, import_name)
                    {
                        // Mark this specifier node as type-only for elision during emit.
                        self.ctx.type_only_nodes.insert(*element_idx);

                        // TS18042: type-only import in JavaScript file.
                        let specifier_is_type_only = self
                            .ctx
                            .arena
                            .get(*element_idx)
                            .and_then(|n| self.ctx.arena.get_specifier(n))
                            .is_some_and(|s| s.is_type_only);
                        if self.should_report_js_type_only_import_diagnostic(
                            clause.is_type_only,
                            specifier_is_type_only,
                        ) {
                            self.emit_js_type_only_import_diagnostic(
                                *element_idx,
                                import_name,
                                module_name,
                            );
                        }
                    }
                }
            }
        }
    }

    fn check_js_type_only_imports_for_ambient_module(
        &mut self,
        import: &tsz_parser::parser::node::ImportDeclData,
        module_name: &str,
    ) {
        self.check_js_type_only_imports_after_import_validation(import, module_name);
    }

    pub(crate) fn check_js_type_only_imports_after_import_validation(
        &mut self,
        import: &tsz_parser::parser::node::ImportDeclData,
        module_name: &str,
    ) {
        if !self.is_js_file() || !self.ctx.should_resolve_jsdoc() {
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

        if clause.name.is_some()
            && let default_name_idx = clause.name
            && let Some(default_name) = self.get_identifier_text_from_idx(default_name_idx)
            && (self.import_binding_is_type_only(module_name, "default")
                || self.import_local_binding_is_type_only(default_name_idx))
            && self.should_report_js_type_only_import_diagnostic(clause.is_type_only, false)
        {
            self.emit_js_type_only_import_diagnostic(default_name_idx, &default_name, module_name);
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

            let imported_name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            let Some(import_name) = self.get_identifier_text_from_idx(imported_name_idx) else {
                continue;
            };

            let specifier_is_type_only = specifier.is_type_only;
            if (self.import_binding_is_type_only(module_name, &import_name)
                || self.import_local_binding_is_type_only(specifier.name))
                && self.should_report_js_type_only_import_diagnostic(
                    clause.is_type_only,
                    specifier_is_type_only,
                )
            {
                self.emit_js_type_only_import_diagnostic(*element_idx, &import_name, module_name);
            }
        }
    }
    fn import_binding_is_type_only(&self, module_name: &str, import_name: &str) -> bool {
        if self.is_import_specifier_type_only(module_name, import_name)
            || self.is_export_type_only_across_binders(module_name, import_name)
            || (import_name == "default" && self.module_default_export_is_type_only(module_name))
            || (import_name == "default" && self.is_module_export_equals_type_only(module_name))
            || self.resolved_import_symbol_is_type_only(module_name, import_name)
        {
            return true;
        }

        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let Some(target_idx) = self.ctx.resolve_import_target(normalized) else {
            return false;
        };

        if self.file_has_jsdoc_typedef_namespace_root(target_idx, import_name)
            || self.declaration_file_direct_export_is_type_only(target_idx, import_name)
        {
            return true;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let Some((sym_id, owner_idx)) =
            self.resolve_export_in_file(target_idx, import_name, &mut visited)
        else {
            return false;
        };
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_idx) else {
            return false;
        };

        self.import_member_binder_symbol_is_type_only(owner_binder, sym_id)
    }

    fn import_local_binding_is_type_only(&self, local_name_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        // Use the non-tracking resolver here: this is a pure pre-condition
        // probe (asking whether the import binding is type-only). Tracking it
        // as "referenced" would cause noUnusedLocals to silently treat every
        // import binding as used — even one that no source code ever refers to.
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(local_name_idx) else {
            return false;
        };
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };

        if symbol.is_type_only {
            return true;
        }

        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited = AliasCycleTracker::new();
            if let Some(resolved) = self.resolve_alias_symbol(sym_id, &mut visited) {
                return self.symbol_member_is_type_only(resolved, None);
            }
        }

        let has_type = symbol.has_any_flags(symbol_flags::TYPE);
        let has_value = symbol.has_any_flags(symbol_flags::VALUE);
        has_type && !has_value
    }

    fn resolved_import_symbol_is_type_only(&self, module_name: &str, import_name: &str) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let Some(target_idx) = self.ctx.resolve_import_target(normalized) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return false;
        };

        if self.file_has_jsdoc_typedef_namespace_root(target_idx, import_name) {
            return true;
        }
        if self.declaration_file_direct_export_is_type_only(target_idx, import_name) {
            return true;
        }

        let target_file_name = self
            .ctx
            .get_arena_for_file(target_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_default();

        let mut resolved = None;
        let mut lookup_keys = vec![
            module_name.to_string(),
            normalized.to_string(),
            target_file_name.clone(),
        ];
        lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
            module_name,
        ));
        lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
            normalized,
        ));
        if !target_file_name.is_empty() {
            lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
                &target_file_name,
            ));
        }

        for key in lookup_keys {
            if key.is_empty() {
                continue;
            }
            if let Some(result) =
                target_binder.resolve_import_with_reexports_type_only(&key, import_name)
            {
                resolved = Some(result);
                break;
            }
        }
        let (sym_id, path_is_type_only) = if let Some(result) = resolved {
            result
        } else {
            let mut visited = rustc_hash::FxHashSet::default();
            let Some((resolved_sym_id, owner_idx)) =
                self.resolve_export_in_file(target_idx, import_name, &mut visited)
            else {
                return false;
            };
            let Some(owner_binder) = self.ctx.get_binder_for_file(owner_idx) else {
                return false;
            };
            return self.import_member_binder_symbol_is_type_only(owner_binder, resolved_sym_id);
        };
        if path_is_type_only {
            return true;
        }

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = target_binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };
        let flags = symbol.flags;

        symbol.is_type_only
            || ((flags & PURE_TYPE) != 0 && (flags & VALUE) == 0)
            || ((flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && !self.symbol_has_runtime_value_in_binder(target_binder, sym_id))
            || self.declaration_file_direct_export_is_type_only(target_idx, import_name)
    }

    fn declaration_file_direct_export_is_type_only(
        &self,
        target_idx: usize,
        import_name: &str,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        if !source_file.is_declaration_file {
            return false;
        }
        let target_binder = self.ctx.get_binder_for_file(target_idx);

        let mut has_named_type = false;
        let mut has_named_value = false;
        let mut has_default_type = false;
        let mut has_default_value = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(node) = arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export_decl) = arena.get_export_decl(node) else {
                    continue;
                };
                let Some(clause_node) = arena.get(export_decl.export_clause) else {
                    continue;
                };

                if export_decl.is_default_export {
                    match clause_node.kind {
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
                        {
                            has_default_type = true;
                        }
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION
                            || k == syntax_kind_ext::VARIABLE_STATEMENT =>
                        {
                            has_default_value = true;
                        }
                        k if k == SyntaxKind::Identifier as u16 => {
                            if let Some(ident) = arena.get_identifier(clause_node)
                                && let Some(binder) = target_binder
                                && let Some(sym_id) = binder.file_locals.get(&ident.escaped_text)
                                && let Some(sym) = binder.get_symbol(sym_id)
                            {
                                let has_type = sym.has_any_flags(symbol_flags::TYPE);
                                let has_value = sym.has_any_flags(symbol_flags::VALUE);
                                has_default_type |= has_type && !has_value;
                                has_default_value |= has_value;
                            }
                        }
                        _ => {}
                    }
                }

                continue;
            }

            let modifiers = self.get_declaration_modifiers(node);
            let is_export = arena.has_modifier_ref(modifiers, SyntaxKind::ExportKeyword);
            let is_default = arena.has_modifier_ref(modifiers, SyntaxKind::DefaultKeyword);
            if !is_export && !is_default {
                continue;
            }

            match node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = arena.get_interface(node)
                        && let Some(name) = self.get_identifier_text_from_idx(iface.name)
                    {
                        if name == import_name {
                            has_named_type = true;
                        }
                        if is_default {
                            has_default_type = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(ty_alias) = arena.get_type_alias(node)
                        && let Some(name) = self.get_identifier_text_from_idx(ty_alias.name)
                    {
                        if name == import_name {
                            has_named_type = true;
                        }
                        if is_default {
                            has_default_type = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION =>
                {
                    if let Some(name) = match node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => arena
                            .get_function(node)
                            .and_then(|func| self.get_identifier_text_from_idx(func.name)),
                        k if k == syntax_kind_ext::CLASS_DECLARATION => arena
                            .get_class(node)
                            .and_then(|class| self.get_identifier_text_from_idx(class.name)),
                        _ => arena.get_enum(node).and_then(|enum_decl| {
                            self.get_identifier_text_from_idx(enum_decl.name)
                        }),
                    } && name == import_name
                    {
                        has_named_value = true;
                    }
                    if is_default {
                        has_default_value = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = arena.get_variable(node) {
                        for &decl_list_idx in &var_stmt.declarations.nodes {
                            let Some(decl_list_node) = arena.get(decl_list_idx) else {
                                continue;
                            };
                            let Some(decl_list) = arena.get_variable(decl_list_node) else {
                                continue;
                            };
                            for &decl_idx in &decl_list.declarations.nodes {
                                let Some(decl_node) = arena.get(decl_idx) else {
                                    continue;
                                };
                                let Some(var_decl) = arena.get_variable_declaration(decl_node)
                                else {
                                    continue;
                                };
                                if let Some(name) = self.get_identifier_text_from_idx(var_decl.name)
                                    && name == import_name
                                {
                                    has_named_value = true;
                                }
                            }
                        }
                    }
                    if is_default {
                        has_default_value = true;
                    }
                }
                _ => {}
            }
        }

        if import_name == "default" {
            return has_default_type && !has_default_value;
        }

        has_named_type && !has_named_value
    }

    fn import_member_binder_symbol_is_type_only(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let Some(sym) = binder.get_symbol(sym_id) else {
            return false;
        };
        let flags = sym.flags;

        sym.is_type_only
            || ((flags & PURE_TYPE) != 0 && (flags & VALUE) == 0)
            || ((flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && !self.symbol_has_runtime_value_in_binder(binder, sym_id))
    }

    fn module_default_export_is_type_only(&self, module_name: &str) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 =
            symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS | symbol_flags::TYPE_PARAMETER;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let Some(target_idx) = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))
        else {
            return false;
        };

        let mut visited = rustc_hash::FxHashSet::default();
        let Some((sym_id, owner_file_idx)) =
            self.resolve_export_in_file(target_idx, "default", &mut visited)
        else {
            return false;
        };
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_file_idx) else {
            return false;
        };
        let Some(sym) = owner_binder
            .get_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };

        if sym.is_type_only {
            return true;
        }
        if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
            return true;
        }
        if sym.has_any_flags(symbol_flags::ALIAS) && sym.import_module.is_none() {
            let arena = self.ctx.get_arena_for_file(owner_file_idx as u32);
            if sym.all_declarations().into_iter().any(|decl_idx| {
                arena.get(decl_idx).is_some_and(|node| {
                    node.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                        || node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                })
            }) {
                return true;
            }
        }
        if sym.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(resolved_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                for alias_id in &visited_aliases {
                    if owner_binder
                        .get_symbol(alias_id)
                        .or_else(|| self.ctx.binder.get_symbol(alias_id))
                        .is_some_and(|alias_sym| alias_sym.is_type_only)
                    {
                        return true;
                    }
                }

                if let Some(resolved_sym) = owner_binder
                    .get_symbol(resolved_sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(resolved_sym_id))
                {
                    return resolved_sym.is_type_only
                        || ((resolved_sym.flags & PURE_TYPE) != 0
                            && (resolved_sym.flags & VALUE) == 0);
                }
            }
        }

        false
    }

    fn local_import_binding_is_type_only(&self, local_name: &str) -> bool {
        use tsz_binder::symbol_flags;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        let Some(sym_id) = self.ctx.binder.file_locals.get(local_name) else {
            return false;
        };
        let Some(sym) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if sym.is_type_only {
            return true;
        }
        if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
            return true;
        }
        if sym.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
            && !self.symbol_has_runtime_value_in_binder(self.ctx.binder, sym_id)
        {
            return true;
        }

        sym.has_any_flags(symbol_flags::ALIAS) && self.alias_resolves_to_type_only(sym_id)
    }

    fn should_report_js_type_only_import_diagnostic(
        &self,
        clause_is_type_only: bool,
        specifier_is_type_only: bool,
    ) -> bool {
        self.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && !clause_is_type_only
            && !specifier_is_type_only
    }

    fn emit_js_type_only_import_diagnostic(
        &mut self,
        report_at: NodeIndex,
        import_name: &str,
        module_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let clean_module = module_name.trim_matches('\'').trim_matches('"');
        let quoted_import = format!("import(\"{clean_module}\").{import_name}");
        let message = format_message(
            diagnostic_messages::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT,
            &[import_name, &quoted_import],
        );
        let start = self.ctx.arena.get(report_at).map_or(0, |n| n.pos);
        if self.ctx.diagnostics.iter().any(|diag| {
            diag.code
                == diagnostic_codes::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT
                && diag.start == start
        }) {
            return;
        }
        self.error_at_node(
            report_at,
            &message,
            diagnostic_codes::IS_A_TYPE_AND_CANNOT_BE_IMPORTED_IN_JAVASCRIPT_FILES_USE_IN_A_JSDOC_TYPE_ANNOTAT,
        );
    }

    fn module_has_default_binding_fast_path(
        &self,
        module_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        if self.module_uses_module_exports_interop(module_name, resolution_mode) {
            return true;
        }

        let resolved_target = if resolution_mode.is_some() {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            )
        } else {
            self.ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
                .or_else(|| self.ctx.resolve_import_target(module_name))
        };
        let Some(target_idx) = resolved_target else {
            return false;
        };

        let mut visited = rustc_hash::FxHashSet::default();
        self.resolve_export_in_file(target_idx, "default", &mut visited)
            .is_some()
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
                        if self.ctx.module_exports_contains_module(binder, module_name)
                            || self.ctx.module_exports_contains_module(binder, normalized)
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
        // (not just the arena, which doesn't include all declarations).
        // Per-file binders share user symbols across files (lib_symbols_merged
        // contamination), so file_locals.has(name) alone is too permissive — it
        // can return true for a name declared in an unrelated file.
        // Verify the symbol's declaration is from THIS file (decl_file_idx).
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_name,
                resolution_mode,
            )
            .map(|i| i as u32);
        let mut symbol_exists = binder.file_locals.has(import_name)
            && match (
                binder
                    .file_locals
                    .get(import_name)
                    .and_then(|sym_id| binder.get_symbol(sym_id)),
                target_file_idx,
            ) {
                (Some(sym), Some(target_idx)) => {
                    sym.decl_file_idx == u32::MAX || sym.decl_file_idx == target_idx
                }
                _ => true,
            };
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
            if let Some(exports) = self.ctx.module_exports_for_module(binder, key) {
                // Check if the symbol is exported under a different name
                // by looking through all export names. Skip the synthetic
                // `"export="` key — `export = Foo` is not a "renamed export"
                // for TS2460 purposes; tsc falls through to TS2497/TS2616
                // ("module can only be referenced via default-export") in
                // that case, so we let the caller emit the export-equals
                // diagnostic.
                for (export_name, sym_id) in exports.iter() {
                    if export_name.as_str() == "export=" {
                        continue;
                    }
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
            && let Some(exports) = self.ctx.module_exports_for_module(binder, fname)
        {
            for (export_name, sym_id) in exports.iter() {
                if export_name.as_str() == "export=" {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::module_resolution::build_module_resolution_maps;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_common::common::ModuleKind;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn check_imported_members_emits_ts18042_for_default_interface_import_in_js() {
        let files = [
            (
                "dep.d.ts",
                r#"
export default interface TruffleContract {
  foo: number;
}
                "#,
            ),
            (
                "caller.js",
                r#"
import TruffleContract from "./dep";
                "#,
            ),
        ];

        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

        for (name, source) in files {
            let mut parser = ParserState::new(name.to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let entry_idx = file_names
            .iter()
            .position(|name| name == "caller.js")
            .expect("entry file should exist");
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
                allow_js: true,
                check_js: true,
                target: ScriptTarget::ES2015,
                module: ModuleKind::ES2020,
                ..CheckerOptions::default()
            },
        );

        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(entry_idx);
        checker.ctx.set_lib_contexts(Vec::new());
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);
        checker.check_source_file(roots[entry_idx]);

        let source_file = checker
            .ctx
            .arena
            .get(roots[entry_idx])
            .and_then(|node| checker.ctx.arena.get_source_file(node))
            .expect("source file data should exist");
        let import_idx = *source_file
            .statements
            .nodes
            .first()
            .expect("entry file should start with an import");
        let import = checker
            .ctx
            .arena
            .get(import_idx)
            .and_then(|node| checker.ctx.arena.get_import_decl(node))
            .cloned()
            .expect("import declaration should exist");
        let clause = checker
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|node| checker.ctx.arena.get_import_clause(node))
            .expect("import clause should exist");

        assert!(clause.name.is_some(), "expected default import binding");
        assert!(
            checker.import_binding_is_type_only("./dep", "default"),
            "default import should be recognized as type-only"
        );

        let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&18042),
            "expected TS18042 from the checked-JS import walk, got codes: {codes:?}"
        );
    }
}
