//! verbatimModuleSyntax import/export checks (TS1282, TS1283, TS1295, TS1484, TS1485, TS2748).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
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
        for lib_ctx in &self.ctx.lib_contexts {
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
        if self.ctx.compiler_options.module.is_node_module() {
            if let Some(is_esm) = self.ctx.file_is_esm {
                return !is_esm;
            }
        }
        !self.ctx.compiler_options.module.is_es_module()
    }
}
