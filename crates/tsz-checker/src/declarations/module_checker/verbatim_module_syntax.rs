//! verbatimModuleSyntax and isolatedModules export checks.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // verbatimModuleSyntax / isolatedModules Export Checks (TS1205, TS1284, TS1285, TS1448)
    // =========================================================================

    /// TS1205: Re-exporting a type when 'verbatimModuleSyntax' or 'isolatedModules' is enabled
    /// requires using `export type`.
    /// TS1448: Re-exporting a type-only declaration requires type-only re-export under isolatedModules.
    pub(crate) fn check_verbatim_module_syntax_named_exports(
        &mut self,
        named_exports_idx: NodeIndex,
        module_specifier_idx: NodeIndex,
    ) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;

        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
            "verbatimModuleSyntax"
        } else if self.ctx.compiler_options.isolated_modules {
            "isolatedModules"
        } else {
            return;
        };

        if self.ctx.is_declaration_file() {
            return;
        }

        let Some(clause_node) = self.ctx.arena.get(named_exports_idx) else {
            return;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return;
        }
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return;
        };

        let module_specifier_text = if module_specifier_idx.is_some() {
            self.ctx
                .arena
                .get(module_specifier_idx)
                .and_then(|n| self.ctx.arena.get_literal(n))
                .map(|l| l.text.clone())
        } else {
            None
        };

        for &specifier_idx in &named_exports.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(specifier_idx) else {
                continue;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };

            if specifier.is_type_only {
                continue;
            }

            let source_name_idx = if specifier.property_name.is_some() {
                specifier.property_name
            } else {
                specifier.name
            };
            let Some(source_name) = self.get_identifier_text_from_idx(source_name_idx) else {
                continue;
            };

            let is_inherent_type = if let Some(ref module_spec) = module_specifier_text {
                self.is_import_specifier_type_only(module_spec, &source_name)
            } else {
                let type_only = self.is_local_symbol_type_only(&source_name);
                if type_only
                    && option_name == "isolatedModules"
                    && (self.is_local_symbol_imported_as_type_only(&source_name)
                        || self.is_local_symbol_from_type_only_reexport_chain(&source_name))
                {
                    false
                } else {
                    type_only
                }
            };

            if is_inherent_type {
                let message = format_message(
                    diagnostic_messages::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                    &[option_name],
                );
                self.error_at_node(
                    source_name_idx,
                    &message,
                    diagnostic_codes::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                );
                continue;
            }

            let is_type_only_chain = if let Some(ref module_spec) = module_specifier_text {
                self.is_export_type_only_across_binders(module_spec, &source_name)
            } else {
                self.is_local_symbol_from_type_only_chain(&source_name)
            };

            if is_type_only_chain {
                if option_name == "verbatimModuleSyntax" {
                    let message = format_message(
                        diagnostic_messages::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                        &[option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                    );
                } else {
                    let export_name = self
                        .get_identifier_text_from_idx(specifier.name)
                        .unwrap_or_else(|| source_name.clone());
                    let message = format_message(
                        diagnostic_messages::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                        &[&export_name, option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                    );
                }
                continue;
            }

            if option_name == "verbatimModuleSyntax"
                && let Some(ref module_spec) = module_specifier_text
                && self.is_import_specifier_ambient_const_enum(module_spec, &source_name)
            {
                let msg = format_message(
                    diagnostic_messages::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                    &["verbatimModuleSyntax"],
                );
                self.error_at_node(
                    source_name_idx,
                    &msg,
                    diagnostic_codes::CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED,
                );
            }
        }
    }

    /// TS1269: Check `export import X = require("...")` when the target is type-only.
    /// Called when the export clause of an export declaration is an `ImportEqualsDeclaration`.
    pub(crate) fn check_export_import_equals_type_only(
        &mut self,
        export_idx: NodeIndex,
        import_clause_idx: NodeIndex,
    ) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
            "verbatimModuleSyntax"
        } else if self.ctx.compiler_options.isolated_modules {
            "isolatedModules"
        } else {
            return;
        };

        if self.ctx.is_declaration_file() {
            return;
        }

        let Some(import_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(import_node) else {
            return;
        };

        if import.is_type_only {
            return;
        }

        let import_name = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text.clone());
        let require_module_specifier = self.get_require_module_specifier(import.module_specifier);
        let target_is_type_only = if let Some(module_spec) = require_module_specifier.as_deref() {
            self.is_import_specifier_type_only(module_spec, import_name.as_deref().unwrap_or(""))
                || self.is_module_export_equals_type_only(module_spec)
        } else {
            self.entity_name_text(import.module_specifier)
                .is_some_and(|entity_name| self.is_local_symbol_type_only(&entity_name))
        };

        if target_is_type_only {
            let msg = format_message(
                diagnostic_messages::CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED,
                &[option_name],
            );
            self.error_at_node(
                export_idx,
                &msg,
                diagnostic_codes::CANNOT_USE_EXPORT_IMPORT_ON_A_TYPE_OR_TYPE_ONLY_NAMESPACE_WHEN_IS_ENABLED,
            );
        }
    }

    /// Check if a local symbol was imported from a module where the export is type-only
    /// (e.g., the source module uses `export type { X }`), but the symbol itself is not
    /// inherently a type. This is the TS1448 case for isolatedModules.
    pub(super) fn is_local_symbol_from_type_only_chain(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            if sym.is_type_only {
                return false;
            }
            if sym.has_any_flags(symbol_flags::ALIAS)
                && let Some(ref module_spec) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(name);
                return self.is_export_type_only_across_binders(module_spec, import_name);
            }
        }
        false
    }

    /// Like `is_local_symbol_from_type_only_chain`, but only returns true when
    /// the chain includes explicit `export type { ... }` syntax (where `is_type_only`
    /// is set on the export symbol). Does NOT return true for plain type declarations
    /// like `export type T = number`. This distinction is important for choosing
    /// between TS1205 (re-exporting a type) and TS1448 (type-only re-export chain).
    pub(super) fn is_local_symbol_from_type_only_reexport_chain(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            if sym.is_type_only {
                return false;
            }
            if sym.has_any_flags(symbol_flags::ALIAS)
                && let Some(ref module_spec) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(name);
                return self.is_export_type_only_syntax_across_binders(module_spec, import_name);
            }
        }
        false
    }

    /// Check if a local symbol was imported via `import type` (directly type-only import).
    pub(super) fn is_local_symbol_imported_as_type_only(&self, name: &str) -> bool {
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            return sym.is_type_only;
        }
        false
    }

    /// Check if a local symbol is purely a type entity.
    /// Resolves through import chains: if `name` is an imported symbol,
    /// checks whether the source module's export is type-only.
    pub(super) fn is_local_symbol_type_only(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        if self.is_js_file()
            && self.ctx.should_resolve_jsdoc()
            && self.file_has_jsdoc_typedef_named(self.ctx.current_file_idx, name)
        {
            return true;
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            if sym.is_type_only {
                return true;
            }
            if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
                let has_syntactic_type_decl_in_js = self.is_js_file()
                    && sym.declarations.iter().any(|&decl_idx| {
                        self.ctx.arena.get(decl_idx).is_some_and(|n| {
                            n.kind == syntax_kind_ext::INTERFACE_DECLARATION
                                || n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                        })
                    });
                return !has_syntactic_type_decl_in_js;
            }
            if (sym.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && !self.symbol_has_runtime_value_in_binder(self.ctx.binder, sym_id)
            {
                return true;
            }
            if sym.has_any_flags(symbol_flags::ALIAS)
                && let Some(ref module_spec) = sym.import_module
            {
                let import_name = sym.import_name.as_deref().unwrap_or(name);
                return self.is_import_specifier_type_only(module_spec, import_name);
            }
        }
        false
    }

    fn is_current_file_commonjs(&self) -> bool {
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

    /// TS1295: ESM exports cannot be written in a CommonJS file under verbatimModuleSyntax.
    /// TS1287: top-level export on value declarations in CJS.
    /// Returns true if a CJS-specific diagnostic was emitted.
    pub(crate) fn check_verbatim_module_syntax_cjs_export(
        &mut self,
        export_idx: NodeIndex,
        is_type_only: bool,
        is_value_export: bool,
    ) -> bool {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.ctx.compiler_options.verbatim_module_syntax {
            return false;
        }
        if !self.is_current_file_commonjs() {
            return false;
        }
        if is_type_only {
            return false;
        }
        if is_value_export {
            self.error_at_node(
                export_idx,
                diagnostic_messages::A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M,
                diagnostic_codes::A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_M,
            );
        } else {
            self.error_at_node(
                export_idx,
                diagnostic_messages::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
                diagnostic_codes::ECMASCRIPT_IMPORTS_AND_EXPORTS_CANNOT_BE_WRITTEN_IN_A_COMMONJS_FILE_UNDER_VERBAT_2,
            );
        }
        true
    }

    /// TS1284/TS1285: export default checks under verbatimModuleSyntax.
    /// TS1292: export default of a type-only alias under isolatedModules.
    pub(crate) fn check_verbatim_module_syntax_export_default(&mut self, clause_idx: NodeIndex) {
        use tsz_binder::symbol_flags;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let option_name = if self.ctx.compiler_options.verbatim_module_syntax {
            "verbatimModuleSyntax"
        } else if self.ctx.compiler_options.isolated_modules {
            "isolatedModules"
        } else {
            return;
        };

        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(clause_node) else {
            return;
        };
        let name = ident.escaped_text.clone();

        const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
        const VALUE: u32 = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ENUM_MEMBER
            | symbol_flags::VALUE_MODULE;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(&name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            // verbatimModuleSyntax-only: TS1284/TS1285.
            if option_name == "verbatimModuleSyntax" {
                if sym.is_type_only {
                    let message = format_message(
                        diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                        &[&name],
                    );
                    self.error_at_node(
                        clause_idx,
                        &message,
                        diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                    );
                    return;
                }

                if (sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0 {
                    let message = format_message(
                        diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                        &[&name],
                    );
                    self.error_at_node(
                        clause_idx,
                        &message,
                        diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                    );
                    return;
                }
            }

            // TS1292: under isolatedModules (or verbatimModuleSyntax),
            // emit when `export default <Identifier>` references an alias
            // whose non-local meanings include Type but not Value, and the
            // alias was NOT declared as `import type` in this file.
            //
            // Mirrors tsc's checker logic at checkExportAssignment:
            //   if (sym.flags & Alias && nonLocalMeanings & Type
            //       && !(nonLocalMeanings & Value)) { TS1292 }
            //
            // Local meanings are the merged TYPE_ALIAS/INTERFACE that the
            // binder tracks via alias_partners. The "non-local" alias is the
            // partner: it provides the original import.
            //
            // The `sym` we have here is whichever is in file_locals: either
            // the import alias itself, or the local TYPE_ALIAS that shadowed
            // it. We resolve the import alias via alias_partner_for if the
            // local sym is a TYPE_ALIAS.
            if (sym.flags & VALUE) != 0 {
                return;
            }

            let alias_sym_id = if sym.has_any_flags(symbol_flags::ALIAS) {
                Some(sym_id)
            } else {
                self.ctx.alias_partner_for(self.ctx.binder, sym_id)
            };

            let Some(alias_sym_id) = alias_sym_id else {
                return;
            };
            let Some(alias_sym) = self.ctx.binder.get_symbol(alias_sym_id) else {
                return;
            };
            if !alias_sym.has_any_flags(symbol_flags::ALIAS) {
                return;
            }
            // Ambient declarations are exempt.
            if alias_sym.is_type_only {
                // `import type` in this file: typeOnlyDeclaration is in the
                // current file, suppressing TS1292.
                return;
            }
            let Some(ref module_spec) = alias_sym.import_module else {
                return;
            };
            let import_name = alias_sym
                .import_name
                .as_deref()
                .unwrap_or(name.as_str())
                .to_string();

            // Resolve the imported target's flags. If the target is type-only
            // (Type but not Value), TS1292 applies.
            let (target_has_type, target_has_value) =
                self.lookup_imported_target_flags(module_spec, &import_name);
            if target_has_type && !target_has_value {
                let message = format_message(
                    diagnostic_messages::RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_2,
                    &[&name, option_name],
                );
                self.error_at_node(
                    clause_idx,
                    &message,
                    diagnostic_codes::RESOLVES_TO_A_TYPE_AND_MUST_BE_MARKED_TYPE_ONLY_IN_THIS_FILE_BEFORE_RE_EXPORTING_2,
                );
            }
        }
    }

    /// Best-effort resolution of an imported symbol's non-local meanings.
    /// Returns `(has_type, has_value)` for the target of `import { name } from module_spec`.
    /// Used by TS1292 / TS2865 isolatedModules checks.
    fn lookup_imported_target_flags(&self, module_spec: &str, import_name: &str) -> (bool, bool) {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let mut has_type = false;
        let mut has_value = false;

        // Try declared modules first (`declare module "x"`) via the
        // global module binder index.
        if let Some(binders) = &self.ctx.all_binders {
            let candidate_indices = self
                .ctx
                .global_module_binder_index
                .as_ref()
                .and_then(|idx| idx.get(module_spec));
            let scan_indices: Vec<usize> = match candidate_indices {
                Some(indices) => indices.to_vec(),
                None => (0..binders.len()).collect(),
            };
            for binder_idx in scan_indices {
                if let Some(binder) = binders.get(binder_idx)
                    && let Some(exports) = self.ctx.module_exports_for_module(binder, module_spec)
                    && let Some(target_sym_id) = exports.get(import_name)
                    && let Some(target_sym) = binder.symbols.get(target_sym_id)
                {
                    if target_sym.has_any_flags(symbol_flags::TYPE) {
                        has_type = true;
                    }
                    if target_sym.has_any_flags(symbol_flags::VALUE | symbol_flags::EXPORT_VALUE) {
                        has_value = true;
                    }
                    if has_value {
                        break;
                    }
                }
            }
        }

        // Try regular file exports — follow re-export chains.
        if !has_value && let Some(target_idx) = self.ctx.resolve_import_target(module_spec) {
            let mut visited = rustc_hash::FxHashSet::default();
            if let Some((resolved_sym_id, resolved_file_idx)) =
                self.resolve_export_in_file(target_idx, import_name, &mut visited)
                && let Some(resolved_binder) = self.ctx.get_binder_for_file(resolved_file_idx)
                && let Some(resolved_sym) = resolved_binder.symbols.get(resolved_sym_id)
            {
                // Skip namespace pseudo-symbols (`namespace foo { ... }` with
                // only type members) — they appear in exports but don't
                // introduce a runtime value.
                let mut sym_has_value =
                    resolved_sym.has_any_flags(symbol_flags::VALUE | symbol_flags::EXPORT_VALUE);
                if sym_has_value
                    && resolved_sym.has_any_flags(symbol_flags::VALUE_MODULE)
                    && !resolved_sym
                        .has_any_flags(symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)
                {
                    // declarations carry file-local NodeIndex into the resolved
                    // file's arena, not the current file's arena.
                    let resolved_arena = self.ctx.get_arena_for_file(resolved_file_idx as u32);
                    let any_instantiated = resolved_sym.declarations.iter().any(|&decl_idx| {
                        let Some(decl_node) = resolved_arena.get(decl_idx) else {
                            return false;
                        };
                        // Only namespace declarations contribute runtime value;
                        // type-only declarations (interface/type alias) do not.
                        decl_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    });
                    sym_has_value = any_instantiated;
                }
                if resolved_sym.has_any_flags(symbol_flags::TYPE) {
                    has_type = true;
                }
                if sym_has_value {
                    has_value = true;
                }
            }
        }

        (has_type, has_value)
    }
}
