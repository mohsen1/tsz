//! verbatimModuleSyntax import/export checks (TS1282, TS1283, TS1295, TS1484, TS1485, TS2748).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    fn binder_symbol_is_type_only(
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

    pub(crate) fn symbol_has_runtime_value_in_binder(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let Some(sym) = binder.get_symbol(sym_id) else {
            return false;
        };

        let non_namespace_value_flags = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        if (sym.flags & non_namespace_value_flags) != 0 {
            return true;
        }

        // `export * as Ns from "./mod"` creates an ALIAS-only namespace
        // symbol (no NAMESPACE_MODULE flag) whose own exports/members are
        // empty. Follow `import_module` to check the target module's
        // top-level exports for any runtime value before short-circuiting.
        let is_namespace_style_alias = (sym.flags & symbol_flags::ALIAS) != 0
            && sym.import_module.is_some()
            && sym.import_name.as_deref() == Some("*");
        if (sym.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) == 0
            && !is_namespace_style_alias
        {
            return false;
        }

        let member_has_runtime_value = |member_id: tsz_binder::SymbolId| {
            binder.get_symbol(member_id).is_some_and(|member_sym| {
                (member_sym.flags & symbol_flags::VALUE) != 0
                    && !self.symbol_member_is_type_only(member_id, None)
            })
        };

        if sym.exports.as_ref().is_some_and(|exports| {
            exports
                .iter()
                .any(|(_, &member_id)| member_has_runtime_value(member_id))
        }) || sym.members.as_ref().is_some_and(|members| {
            members
                .iter()
                .any(|(_, &member_id)| member_has_runtime_value(member_id))
        }) {
            return true;
        }

        // `export * as Ns from "./mod"` creates a namespace symbol whose own
        // exports/members are empty — the runtime exports live in the target
        // module. Follow the import_module pointer to check that module's
        // top-level exports for any runtime value.
        if let Some(ref module_specifier) = sym.import_module
            && let Some(target_idx) = self.ctx.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            let Some(target_file_name) = target_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
            else {
                return false;
            };
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(target_binder, &target_file_name)
            {
                return exports.iter().any(|(_, &member_id)| {
                    target_binder
                        .get_symbol(member_id)
                        .is_some_and(|member_sym| (member_sym.flags & symbol_flags::VALUE) != 0)
                });
            }
        }

        false
    }

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

            // Determine which diagnostic to emit.  tsc distinguishes:
            //   TS1484: "is a type" — the imported symbol is DIRECTLY a type
            //           declaration in the source module (e.g. `export type A`).
            //   TS1485: "resolves to a type-only declaration" — the imported
            //           symbol is an alias that re-exports a type from
            //           elsewhere via `export type { X } from "./mod"` or a
            //           transitive chain.
            //
            // Both checks below hit `binder_symbol_is_type_only`, so we must
            // disambiguate by looking at the exported symbol's flags:
            // an ALIAS symbol routes to TS1485, otherwise TS1484.
            let is_direct_type = self.is_import_specifier_type_only(module_name, &import_name)
                && !self.is_import_specifier_alias_reexport(module_name, &import_name);
            if is_direct_type {
                let message = format_message(
                    diagnostic_messages::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                    &[&local_name],
                );
                self.error_at_node(
                    local_name_idx,
                    &message,
                    diagnostic_codes::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                );
                continue;
            }

            // TS1485: alias-reexport / type-only export chain.
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

            // TS2748: Cannot access ambient const enums when VMS is enabled
            if self.is_import_specifier_ambient_const_enum(module_name, &import_name) {
                let msg = format_message(
                    diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                    &["verbatimModuleSyntax"],
                );
                self.error_at_node(
                    local_name_idx,
                    &msg,
                    diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                );
            }
        }
    }

    /// Check whether the exported symbol for `import_name` in `module_name`
    /// is itself an ALIAS (e.g. `export type { X } from "./other"`) rather
    /// than a direct type declaration.  Used to pick TS1485 over TS1484.
    pub(crate) fn is_import_specifier_alias_reexport(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let target_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, normalized)
            .or_else(|| self.ctx.resolve_import_target(normalized));
        let Some(target_idx) = target_idx else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_idx) else {
            return false;
        };
        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|f| f.file_name.clone())
        else {
            return false;
        };
        let Some(exports) = self
            .ctx
            .module_exports_for_module(target_binder, &target_file_name)
        else {
            return false;
        };
        let Some(sym_id) = exports.get(import_name) else {
            return false;
        };
        let Some(sym) = target_binder.get_symbol(sym_id) else {
            return false;
        };
        (sym.flags & symbol_flags::ALIAS) != 0
    }

    /// Check if a named import refers to a purely type-only entity.
    pub(crate) fn is_import_specifier_type_only(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let normalized = module_name.trim_matches('"').trim_matches('\'');
        let import_names = if import_name == "default" {
            ["default", "export="]
        } else {
            [import_name, import_name]
        };

        let target_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, normalized)
            .or_else(|| self.ctx.resolve_import_target(normalized));

        if let Some(target_idx) = target_idx
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            if self.file_has_jsdoc_typedef_namespace_root(target_idx, import_name) {
                return true;
            }

            if let Some(sym_id) = target_binder.file_locals.get(import_name)
                && self.binder_symbol_is_type_only(target_binder, sym_id)
            {
                return true;
            }

            let target_file_name = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                .unwrap_or("");

            let mut target_lookup_keys = vec![
                module_name.to_string(),
                normalized.to_string(),
                target_file_name.to_string(),
            ];
            target_lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
                module_name,
            ));
            target_lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
                normalized,
            ));
            if !target_file_name.is_empty() {
                target_lookup_keys.extend(crate::module_resolution::module_specifier_candidates(
                    target_file_name,
                ));
            }

            for key in target_lookup_keys {
                if key.is_empty() {
                    continue;
                }
                if let Some(exports) = self.ctx.module_exports_for_module(target_binder, &key) {
                    for candidate_name in import_names {
                        if let Some(sym_id) = exports.get(candidate_name)
                            && self.binder_symbol_is_type_only(target_binder, sym_id)
                        {
                            return true;
                        }

                        // For ambient `export default X` surfaces, the `default` symbol is a
                        // synthetic alias-like export. If it is not directly marked type-only,
                        // follow `X` inside the same export table and classify based on that
                        // referenced symbol's runtime-ness.
                        if candidate_name == "default"
                            && let Some(default_sym_id) = exports.get(candidate_name)
                            && let Some(default_sym) = target_binder.get_symbol(default_sym_id)
                            && (default_sym.flags & symbol_flags::ALIAS) != 0
                            && default_sym.import_module.is_none()
                            && let Some(target_decl_idx) =
                                if default_sym.value_declaration.is_some() {
                                    Some(default_sym.value_declaration)
                                } else {
                                    default_sym.declarations.first().copied()
                                }
                            && let Some(target_decl_node) = target_arena.get(target_decl_idx)
                            && let Some(target_ident) =
                                target_arena.get_identifier(target_decl_node)
                        {
                            let target_name = target_ident.escaped_text.as_str();
                            let target_sym_id = exports
                                .get(target_name)
                                .or_else(|| target_binder.file_locals.get(target_name));
                            if let Some(target_sym_id) = target_sym_id
                                && target_sym_id != default_sym_id
                                && self.binder_symbol_is_type_only(target_binder, target_sym_id)
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        for candidate in crate::module_resolution::module_specifier_candidates(module_name) {
            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate)
                && import_names
                    .iter()
                    .filter_map(|name| exports.get(name))
                    .any(|sym_id| self.binder_symbol_is_type_only(self.ctx.binder, sym_id))
            {
                return true;
            }
        }

        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                for candidate in crate::module_resolution::module_specifier_candidates(module_name)
                {
                    if let Some(exports) = binder.module_exports.get(&candidate)
                        && import_names
                            .iter()
                            .filter_map(|name| exports.get(name))
                            .any(|sym_id| self.binder_symbol_is_type_only(binder, sym_id))
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if a named import refers to an ambient const enum in the target module.
    /// Returns true if the target symbol has `CONST_ENUM` flag and the source file is a .d.ts.
    pub(crate) fn is_import_specifier_ambient_const_enum(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        let normalized = module_name.trim_matches('"').trim_matches('\'');

        // Try resolve_import_target first (multi-file mode)
        if let Some(target_idx) = self.ctx.resolve_import_target(normalized) {
            // Check if the target file is a .d.ts
            let is_ambient_file = {
                let arena = self.ctx.get_arena_for_file(target_idx as u32);
                arena
                    .source_files
                    .first()
                    .is_some_and(|sf| sf.is_declaration_file)
            };
            if is_ambient_file
                && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
                && let Some(sym_id) = target_binder.file_locals.get(import_name)
                && let Some(sym) = target_binder.get_symbol(sym_id)
            {
                return (sym.flags & tsz_binder::symbol_flags::CONST_ENUM) != 0;
            }
        }

        // Fallback: check module_exports (single-pass mode)
        for candidate in crate::module_resolution::module_specifier_candidates(module_name) {
            if let Some(exports) = self.ctx.binder.module_exports.get(&candidate)
                && let Some(sym_id) = exports.get(import_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
            {
                // In module_exports mode, check if any declaration is in a .d.ts
                // We check symbol flags: if it's CONST_ENUM and declared in the current
                // binder's d.ts file context
                if (sym.flags & tsz_binder::symbol_flags::CONST_ENUM) != 0 {
                    // Check declarations to see if they come from ambient context
                    let all_ambient = sym.declarations.iter().all(|&decl_idx| {
                        self.ctx.arena.is_in_ambient_context(decl_idx)
                            || self.ctx.is_declaration_file()
                    });
                    if all_ambient {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if a resolved symbol is an ambient const enum.
    /// Returns true if the symbol has `CONST_ENUM` flag and its origin is a .d.ts file.
    pub(crate) fn is_ambient_const_enum_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let sym = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(sym) = sym else { return false };
        if (sym.flags & tsz_binder::symbol_flags::CONST_ENUM) == 0 {
            return false;
        }

        // Check via symbol_arenas: the binder tracks which arena each symbol came from.
        // If the symbol's arena is a .d.ts file, it's ambient.
        if let Some(origin_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            return origin_arena
                .source_files
                .first()
                .is_some_and(|sf| sf.is_declaration_file);
        }

        // Fallback: check if the symbol is from any lib context that is a .d.ts
        for lib_ctx in self.ctx.lib_contexts.iter() {
            if lib_ctx.binder.symbols.get(sym_id).is_some()
                && lib_ctx
                    .arena
                    .source_files
                    .first()
                    .is_some_and(|sf| sf.is_declaration_file)
            {
                return true;
            }
        }

        // Also check: if the symbol's declarations are all in ambient context
        for &decl_idx in &sym.declarations {
            if !self.ctx.arena.is_in_ambient_context(decl_idx) && !self.ctx.is_declaration_file() {
                return false;
            }
        }

        // All declarations are in ambient context
        !sym.declarations.is_empty()
    }

    /// TS1282/TS1283: VMS check for `export = X`.
    /// TS1282: X only refers to a type (interface/type alias, no value).
    /// TS1283: X resolves to a type-only declaration (import type).
    pub(crate) fn check_vms_export_equals(&mut self, expression: NodeIndex) {
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
        if self.ctx.compiler_options.module.is_node_module()
            && let Some(is_esm) = self.ctx.file_is_esm
        {
            return !is_esm;
        }
        !self.ctx.compiler_options.module.is_es_module()
    }
}
