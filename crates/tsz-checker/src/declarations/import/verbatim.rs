//! verbatimModuleSyntax import/export checks (TS1282, TS1283, TS1286, TS1295, TS1484, TS1485, TS2748).

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
        if sym.has_any_flags(non_namespace_value_flags) {
            return true;
        }

        // `export * as Ns from "./mod"` creates an ALIAS-only namespace
        // symbol (no NAMESPACE_MODULE flag) whose own exports/members are
        // empty. Follow `import_module` to check the target module's
        // top-level exports for any runtime value before short-circuiting.
        let is_namespace_style_alias = sym.is_namespace_style_alias();
        if !sym.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
            && !is_namespace_style_alias
        {
            return false;
        }

        let member_has_runtime_value = |member_id: tsz_binder::SymbolId| {
            binder.get_symbol(member_id).is_some_and(|member_sym| {
                member_sym.has_any_flags(symbol_flags::VALUE)
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
        if let Some(module_specifier) = sym.import_module()
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
                        .is_some_and(|member_sym| member_sym.has_any_flags(symbol_flags::VALUE))
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

        let vms = self.ctx.compiler_options.verbatim_module_syntax;
        let preserve_isolated = self.preserve_isolated_modules_cjs_check_active();
        if !vms && !preserve_isolated {
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

        // TS1286/TS1295/TS1293: In a CJS file, ESM import syntax carrying a
        // binding (default/namespace/named import clause) is forbidden
        // entirely. `verbatimModuleSyntax` picks TS1286 (extension-locked) or
        // TS1295 (adjustable via `package.json`); `module: "preserve"` +
        // `isolatedModules` (without VMS) always reports TS1293 — under
        // `preserve`, per-file CJS-ness only ever comes from the `.cts`/
        // `.cjs` extension (never `package.json`, since `preserve` cannot
        // pair with the `node16`/`nodenext` `moduleResolution` that package.json-based
        // detection requires), so there is no adjustable variant to pick.
        // Emit on the import clause and skip ESM-specific checks below. TSC
        // skips this check for .d.ts files.
        //
        // An import clause that carries no runtime binding is exempt, exactly
        // like a bare side-effect `import "./m"` (which has no clause at all
        // and returns above): `import {} from "./m"` — an empty named-imports
        // list with no default and no namespace binding — is erased in emit
        // and tsc reports nothing. A default (`import x, {} from`) or
        // namespace (`import * as ns from`) binding still counts as ESM
        // syntax and reports. (Oracle-verified against `typescript@7.0.2`:
        // `import {} from "./m";` is clean; `import def, {} from "./m";`
        // fires TS1295 anchored at the default binding.)
        // `get_named_imports` covers both a namespace import (its `name` is the
        // `ns` in `* as ns`) and a named-imports list (its `elements` are the
        // specifiers). Resolve it once and reuse for both the binding check and
        // the error anchor below.
        let named_imports = self
            .ctx
            .arena
            .get(clause.named_bindings)
            .and_then(|bindings_node| self.ctx.arena.get_named_imports(bindings_node));
        // Either shape carrying content is a binding; an empty named-imports
        // list (`import {}`) is not.
        let clause_has_binding = clause.name.is_some()
            || named_imports.is_some_and(|b| b.name.is_some() || !b.elements.nodes.is_empty());
        if clause_has_binding
            && self.is_current_file_commonjs_for_vms()
            && !self.ctx.is_declaration_file()
        {
            // TSC positions the error at the binding NAME:
            // - Default import `import X from ...` → at X
            // - Namespace import `import * as X from ...` → at X
            // - Named imports `import { X } from ...` → at X (first specifier name)
            let error_node = if clause.named_bindings.is_some() {
                if let Some(ns_import) = named_imports {
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
            } else if clause.name.is_some() {
                clause.name
            } else {
                import.import_clause
            };
            let (message, code) = if preserve_isolated {
                (
                    diagnostic_messages::ECMASCRIPT_MODULE_SYNTAX_IS_NOT_ALLOWED_IN_A_COMMONJS_MODULE_WHEN_MODULE_IS_SET,
                    diagnostic_codes::ECMASCRIPT_MODULE_SYNTAX_IS_NOT_ALLOWED_IN_A_COMMONJS_MODULE_WHEN_MODULE_IS_SET,
                )
            } else if self.current_file_commonjs_is_extension_locked() {
                (
                    diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT,
                    diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT,
                )
            } else {
                (
                    diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                    diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                )
            };
            self.error_at_node(error_node, message, code);
            // Fall through to the per-specifier loop rather than returning:
            // under verbatimModuleSyntax tsc still reports the type-only-import
            // diagnostics (TS1484/TS1485) at their own anchors *alongside* this
            // ESM-in-CJS syntax error (oracle-verified: a type-only named import
            // in a CommonJS file reports both TS1295 and TS1484/TS1485 at the
            // same position). The `preserve_isolated` case (TS1293, where those
            // verbatimModuleSyntax-exclusive checks do not apply) is stopped by
            // the shared guard just below (tsz-org/tsz#17098).
        }

        // `module: "preserve"` + `isolatedModules` (without VMS) has no
        // ESM-file-mode checks of its own — TS1484/TS1485/TS2748 below are
        // verbatimModuleSyntax-exclusive (isolatedModules alone only requires
        // marking *re-exports* type-only, via TS1448, checked elsewhere).
        if preserve_isolated {
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

            // tsc's `checkAliasSymbol` decides *whether* a non-type-only import
            // needs a type-only import, and *which* message, in two steps:
            //   gate:  `isType || getTypeOnlyAliasDeclaration(symbol)` — the
            //          alias is a pure type, or its resolution chain crossed an
            //          explicit `import type`/`export type` boundary somewhere.
            //   split: `isType ? TS1484 : TS1485`, where
            //          `isType = !(getSymbolFlags(target) & Value)` on the
            //          FULLY resolved target (following the whole re-export
            //          chain), not on this module's immediate export symbol.
            //
            // The gate is preserved verbatim from before (`is_import_specifier_type_only`
            // handles pure types, uninstantiated namespaces and single-hop
            // type-only re-exports; `is_export_type_only_across_binders` handles
            // transitive type-only chains). Only the message split changed: the
            // old code keyed TS1485 off "the immediate export is a re-export
            // alias" (`is_import_specifier_alias_reexport`), which mislabels a
            // pure type reached through a plain re-export hop (`export { Foo }`)
            // or across an `export type` boundary — tsc reports TS1484 there
            // because the target carries no value, regardless of the boundary.
            // TS1485 is reserved for a target that STILL carries a runtime value
            // but was reached across an explicit type-only boundary. Following
            // the full chain also fixes arbitrary-depth plain re-export hops,
            // which the immediate-export check never saw (tsz-org/tsz#17098).
            let needs_type_only_import = self
                .is_import_specifier_type_only(module_name, &import_name)
                || self.is_export_type_only_across_binders(module_name, &import_name);
            if needs_type_only_import {
                let (_, target_has_value) =
                    self.lookup_imported_target_flags(module_name, &import_name);
                // `target_has_value` first so the pure-type case (the common
                // TS1484 path) short-circuits before the extra chain-walk in
                // `is_export_type_only_syntax_across_binders`.
                let is_value_across_type_only_boundary = target_has_value
                    && self.is_export_type_only_syntax_across_binders(module_name, &import_name);
                let (message_key, diag_code) = if is_value_across_type_only_boundary {
                    (
                        diagnostic_messages::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR,
                        diagnostic_codes::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPOR,
                    )
                } else {
                    (
                        diagnostic_messages::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                        diagnostic_codes::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                    )
                };
                let message = format_message(message_key, &[&local_name]);
                self.error_at_node(local_name_idx, &message, diag_code);
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
                            && default_sym.has_any_flags(symbol_flags::ALIAS)
                            && default_sym.import_module().is_none()
                            && let Some(target_decl_idx) = default_sym.primary_declaration()
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
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, &candidate)
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
                    if let Some(exports) = self.ctx.module_exports_for_module(binder, &candidate)
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

        // Try resolve_import_target first (multi-file mode). Ambient-ness is a
        // property of the *declaring* file, so evaluate the const enum's
        // declarations against the target module's arena rather than the
        // importing file. This recognizes both const enums declared in a `.d.ts`
        // and `declare const enum` declared in a regular `.ts`.
        if let Some(target_idx) = self.ctx.resolve_import_target(normalized) {
            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
            if let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
                && let Some(sym_id) = target_binder.file_locals.get(import_name)
                && let Some(sym) = target_binder.get_symbol(sym_id)
            {
                return sym.has_any_flags(tsz_binder::symbol_flags::CONST_ENUM)
                    && crate::types_domain::property_access_type::helpers::declarations_are_ambient(
                        target_arena,
                        sym,
                    );
            }
        }

        // Fallback: check module_exports (single-pass mode)
        for candidate in crate::module_resolution::module_specifier_candidates(module_name) {
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(self.ctx.binder, &candidate)
                && let Some(sym_id) = exports.get(import_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
            {
                // Single-pass mode keeps a single arena, so the const enum's
                // declarations are meaningful against `self.ctx.arena`. Share the
                // same ambient determination used on the multi-file path above.
                if sym.has_any_flags(tsz_binder::symbol_flags::CONST_ENUM)
                    && crate::types_domain::property_access_type::helpers::declarations_are_ambient(
                        self.ctx.arena,
                        sym,
                    )
                {
                    return true;
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
        if !sym.has_any_flags(tsz_binder::symbol_flags::CONST_ENUM) {
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

        // Check if this is a type-only import (TS1282/TS1283)
        // Only if the symbol doesn't also have VALUE flags — a local `const I = {}`
        // alongside `import type I = ...` makes `export = I` valid.
        let value_flags = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::VALUE_MODULE;
        if sym.is_type_only && !sym.has_any_flags(value_flags) {
            // tsc picks between these two on whether the symbol's FULL merged
            // meaning (across the alias chain, ignoring this file's own
            // `import type`) still carries Value: target has Value -> TS1283
            // ("resolves to a type-only declaration"); no Value anywhere ->
            // TS1282 ("only refers to a type"), same message the PURE_TYPE
            // branch below uses for a local type-only declaration. A plain
            // import symbol never carries VALUE flags itself (only ALIAS),
            // so `sym`'s own flags can't answer this — the resolved import
            // target's flags can. Mirrors the identical fix already applied
            // to `check_verbatim_module_syntax_export_default`'s TS1284/1285
            // pick.
            let target_has_value = sym
                .import_module()
                .map(|module_spec| {
                    let import_name = sym.import_name().unwrap_or(name.as_str());
                    self.lookup_imported_target_flags(module_spec, import_name)
                        .1
                })
                .unwrap_or(true);
            let (msg_key, code) = if target_has_value {
                (
                    diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
                    diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
                )
            } else {
                (
                    diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
                    diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
                )
            };
            let msg = format_message(msg_key, &[&name]);
            self.error_at_node(expression, &msg, code);
            return;
        }

        // Check if this is a pure type (TS1282)
        let pure_type = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        if sym.has_any_flags(pure_type) && !sym.has_any_flags(value_flags) {
            let msg = format_message(
                diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
                &[&name],
            );
            self.error_at_node(
                expression,
                &msg,
                diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLE,
            );
            return;
        }

        // TS1283: sibling of the TS1289 branch in
        // `check_isolated_modules_export_equals_type_only` — a plain
        // (non-type-only) import alias whose target resolves with real
        // value overall, but whose resolution chain crosses an explicit
        // `import type`/`export type` boundary in another file. The
        // `sym.is_type_only` branch above only sees a type-only marking on
        // the LOCAL import; it cannot see one that lives elsewhere in the
        // chain, which is exactly what `is_export_type_only_syntax_across_binders`
        // is for. Double-reported alongside TS1289 by tsc, oracle-verified.
        if sym.has_any_flags(symbol_flags::ALIAS)
            && !sym.is_type_only
            && !sym.has_any_flags(value_flags)
            && let Some(module_spec) = sym.import_module()
        {
            let import_name = sym.import_name().unwrap_or(name.as_str());
            let (_, target_has_value) = self.lookup_imported_target_flags(module_spec, import_name);
            if target_has_value
                && self.is_export_type_only_syntax_across_binders(module_spec, import_name)
            {
                let msg = format_message(
                    diagnostic_messages::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
                    &[&name],
                );
                self.error_at_node(
                    expression,
                    &msg,
                    diagnostic_codes::AN_EXPORT_DECLARATION_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_E,
                );
            }
        }
    }

    /// TS1293's gate: `module: "preserve"` with `isolatedModules` enabled and
    /// `verbatimModuleSyntax` OFF. When VMS is also on, TS1286/TS1295/TS1287
    /// take priority (oracle-confirmed: `typescript@7.0.2` reports TS1286 for
    /// a CJS+VMS+preserve file, never TS1293).
    pub(crate) fn preserve_isolated_modules_cjs_check_active(&self) -> bool {
        !self.ctx.compiler_options.verbatim_module_syntax
            && self.ctx.compiler_options.isolated_modules
            && self.ctx.compiler_options.module == tsz_common::common::ModuleKind::Preserve
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

    /// Whether the current file's CommonJS-ness is locked in by a fixed file
    /// extension (`.cts`/`.cjs`), as opposed to `module`/`moduleResolution`
    /// config or a `package.json` `"type"` field. tsc picks between two
    /// messages for the same "ESM import/export syntax in a CJS file" defect
    /// on exactly this distinction: TS1286 when the extension already fixes
    /// the file's module kind (adjusting `package.json` cannot help), TS1295
    /// when the CJS classification came from config and could be adjusted.
    pub(crate) fn current_file_commonjs_is_extension_locked(&self) -> bool {
        let current_file = &self.ctx.file_name;
        current_file.ends_with(".cts") || current_file.ends_with(".cjs")
    }
}
