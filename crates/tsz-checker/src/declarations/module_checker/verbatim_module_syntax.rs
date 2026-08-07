//! verbatimModuleSyntax and isolatedModules export checks.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // verbatimModuleSyntax / isolatedModules Export Checks (TS1205, TS1284, TS1285, TS1286, TS1448)
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

            // Decide whether this value re-export is disallowed under
            // verbatimModuleSyntax / isolatedModules, and which code applies.
            //
            // tsc's rule (oracle-verified against typescript@7.0.2 for both
            // modes): a value re-export is an error when the re-exported
            // binding is type-only — either an inherent type, or a value that
            // is reached through a type-only alias (`export type { X }` /
            // `import type`). The *code* is decided purely by whether the
            // resolved target carries a runtime value:
            //   * no runtime value (a pure type)  -> TS1205 (re-exporting a type)
            //   * runtime value reached type-only -> TS1448 (resolves to a
            //                                        type-only declaration)
            //   * runtime value reached normally  -> no error
            // Both modes select the same code; only the substituted option
            // name differs.
            let (is_type_only, target_has_value) =
                if let Some(ref module_spec) = module_specifier_text {
                    let type_only = self.is_import_specifier_type_only(module_spec, &source_name)
                        || self.is_export_type_only_across_binders(module_spec, &source_name);
                    let has_value = self
                        .lookup_imported_target_flags(module_spec, &source_name)
                        .1;
                    (type_only, has_value)
                } else {
                    let type_only = self.is_local_symbol_type_only(&source_name)
                        || self.is_local_symbol_from_type_only_chain(&source_name);
                    let has_value = self.local_symbol_target_has_runtime_value(&source_name);
                    (type_only, has_value)
                };

            if is_type_only {
                if target_has_value {
                    let message = format_message(
                        diagnostic_messages::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                        &[&source_name, option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RESOLVES_TO_A_TYPE_ONLY_DECLARATION_AND_MUST_BE_RE_EXPORTED_USING_A_TYPE_ONLY_RE,
                    );
                } else {
                    let message = format_message(
                        diagnostic_messages::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
                        &[option_name],
                    );
                    self.error_at_node(
                        source_name_idx,
                        &message,
                        diagnostic_codes::RE_EXPORTING_A_TYPE_WHEN_IS_ENABLED_REQUIRES_USING_EXPORT_TYPE,
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
                && let Some(module_spec) = sym.import_module()
            {
                let import_name = sym.import_name().unwrap_or(name);
                return self.is_export_type_only_across_binders(module_spec, import_name);
            }
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
            if sym.has_any_flags(PURE_TYPE) && !sym.has_any_flags(VALUE) {
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
                && let Some(module_spec) = sym.import_module()
            {
                let import_name = sym.import_name().unwrap_or(name);
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

    /// TS1286/TS1295: ESM exports cannot be written in a CommonJS file under verbatimModuleSyntax.
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
            let (message, code) = if self.current_file_commonjs_is_extension_locked() {
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
            self.error_at_node(export_idx, message, code);
        }
        true
    }

    /// TS1284/TS1285: export default checks under verbatimModuleSyntax.
    /// TS1292: export default of a type-only alias under isolatedModules (and,
    /// alongside TS1284, under verbatimModuleSyntax — tsc double-reports when
    /// the exported name is an import alias resolving to a pure type).
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
                    // tsc picks between these two on whether the symbol's
                    // FULL merged meaning (across the alias chain, ignoring
                    // this file's own `import type`) still carries Value:
                    // `getSymbolFlags(sym) & Value` true -> TS1285 ("resolves
                    // to a type-only declaration"); false -> TS1284 ("only
                    // refers to a type"), same message the PURE_TYPE branch
                    // below uses for a local type-only declaration. A plain
                    // import symbol never carries VALUE flags itself (only
                    // ALIAS), so `sym`'s own flags can't answer this — the
                    // resolved import target's flags can, mirroring the
                    // lookup TS1292 already does further down.
                    let target_has_value = sym
                        .import_module()
                        .map(|module_spec| {
                            let import_name = sym.import_name().unwrap_or(name.as_str());
                            self.lookup_imported_target_flags(module_spec, import_name)
                                .1
                        })
                        .unwrap_or(true);
                    let (message_key, diag_code) = if target_has_value {
                        (
                            diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                            diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_REAL_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABL,
                        )
                    } else {
                        (
                            diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                            diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                        )
                    };
                    let message = format_message(message_key, &[&name]);
                    self.error_at_node(clause_idx, &message, diag_code);
                    return;
                }

                if sym.has_any_flags(PURE_TYPE) && !sym.has_any_flags(VALUE) {
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
            if sym.has_any_flags(VALUE) {
                return;
            }

            let sym_is_direct_alias = sym.has_any_flags(symbol_flags::ALIAS);
            let alias_sym_id = if sym_is_direct_alias {
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
            let Some(module_spec) = alias_sym.import_module() else {
                return;
            };
            let import_name = alias_sym.import_name().unwrap_or(name.as_str()).to_string();

            // Resolve the imported target's flags. If the target is type-only
            // (Type but not Value), TS1292 applies.
            let (target_has_type, target_has_value) =
                self.lookup_imported_target_flags(module_spec, &import_name);
            if target_has_type && !target_has_value {
                // tsc double-reports here for verbatimModuleSyntax: TS1284 is
                // evaluated directly against `export default <name>` (the
                // local binding "only refers to a type", same shape as the
                // PURE_TYPE branch above) *in addition to* TS1292's deeper
                // resolve-through-the-import check. The PURE_TYPE branch
                // above cannot see this because a plain import alias symbol
                // never carries INTERFACE/TYPE_ALIAS flags itself — only its
                // resolved target does, which is exactly what
                // `lookup_imported_target_flags` just computed. Oracle-
                // verified against typescript@7.0.2: both codes fire at the
                // same position for `import { Foo } from "./m"; export
                // default Foo;` under verbatimModuleSyntax. isolatedModules
                // alone does not get TS1284 (verbatimModuleSyntax-only, same
                // gate as the PURE_TYPE branch).
                if option_name == "verbatimModuleSyntax" && sym_is_direct_alias {
                    let message = format_message(
                        diagnostic_messages::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                        &[&name],
                    );
                    self.error_at_node(
                        clause_idx,
                        &message,
                        diagnostic_codes::AN_EXPORT_DEFAULT_MUST_REFERENCE_A_VALUE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED_BU,
                    );
                }

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

    /// Whether the target a *local* symbol ultimately resolves to carries a
    /// runtime value. Used by the verbatimModuleSyntax / isolatedModules
    /// re-export check to pick TS1448 (resolves to a type-only declaration —
    /// the target is a value reached type-only) over TS1205 (re-exporting a
    /// pure type). Follows an import alias's `import type`/`export type {}`
    /// chain via `lookup_imported_target_flags`; for a plain local declaration
    /// it consults the binder directly.
    pub(super) fn local_symbol_target_has_runtime_value(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
        {
            if sym.has_any_flags(symbol_flags::ALIAS)
                && let Some(module_spec) = sym.import_module()
            {
                let import_name = sym.import_name().unwrap_or(name);
                return self
                    .lookup_imported_target_flags(module_spec, import_name)
                    .1;
            }
            return self.symbol_has_runtime_value_in_binder(self.ctx.binder, sym_id);
        }
        false
    }

    /// Best-effort resolution of an imported symbol's non-local meanings.
    /// Returns `(has_type, has_value)` for the target of `import { name } from module_spec`.
    /// Used by TS1292 / TS2865 isolatedModules checks.
    pub(crate) fn lookup_imported_target_flags(
        &self,
        module_spec: &str,
        import_name: &str,
    ) -> (bool, bool) {
        use tsz_binder::symbol_flags;
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

        // Try regular file exports — follow re-export chains, including a
        // local `import { X } from "./m"; export { X }` hop where the resolved
        // export is itself an import alias into a further module.
        if !has_value && let Some(target_idx) = self.ctx.resolve_import_target(module_spec) {
            let (t, v) = self.resolved_export_meanings(target_idx, import_name, 0);
            has_type |= t;
            has_value |= v;
        }

        (has_type, has_value)
    }

    /// `(has_type, has_value)` for the export `name` of `file_idx`, following a
    /// resolved import alias one further hop into its own source module. This
    /// closes the `import type` intermediate-chain case: `x` declares `class X`,
    /// `a` does `import type { X } from "./x"; export { X }`, and `b`'s
    /// `export { X } from "./a"` must still see `X`'s runtime value (TS1448, not
    /// TS1205). `depth` bounds the walk defensively; `resolve_export_in_file`'s
    /// own `visited` set already breaks re-export cycles per hop.
    fn resolved_export_meanings(&self, file_idx: usize, name: &str, depth: usize) -> (bool, bool) {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if depth > 8 {
            return (false, false);
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let Some((resolved_sym_id, resolved_file_idx)) =
            self.resolve_export_in_file(file_idx, name, &mut visited)
        else {
            return (false, false);
        };
        let Some(resolved_binder) = self.ctx.get_binder_for_file(resolved_file_idx) else {
            return (false, false);
        };
        let Some(resolved_sym) = resolved_binder.symbols.get(resolved_sym_id) else {
            return (false, false);
        };

        // Skip namespace pseudo-symbols (`namespace foo { ... }` with only type
        // members) — they appear in exports but don't introduce a runtime value.
        let mut has_value =
            resolved_sym.has_any_flags(symbol_flags::VALUE | symbol_flags::EXPORT_VALUE);
        if has_value
            && resolved_sym.has_any_flags(symbol_flags::VALUE_MODULE)
            && !resolved_sym.has_any_flags(symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)
        {
            // declarations carry file-local NodeIndex into the resolved file's
            // arena, not the current file's arena.
            let resolved_arena = self.ctx.get_arena_for_file(resolved_file_idx as u32);
            has_value = resolved_sym.declarations.iter().any(|&decl_idx| {
                resolved_arena
                    .get(decl_idx)
                    // Only namespace declarations contribute runtime value;
                    // type-only declarations (interface/type alias) do not.
                    .is_some_and(|decl_node| decl_node.kind == syntax_kind_ext::MODULE_DECLARATION)
            });
        }
        let mut has_type = resolved_sym.has_any_flags(symbol_flags::TYPE);

        // A local import alias (`import { X } from "./m"`, incl. `import type`)
        // carries no VALUE flag itself — follow it into `./m` to find whether
        // the ultimate target is a runtime value.
        if !has_value
            && resolved_sym.has_any_flags(symbol_flags::ALIAS)
            && let Some(inner_module) = resolved_sym.import_module()
        {
            let inner_name = resolved_sym.import_name().unwrap_or(name);
            if let Some(inner_idx) = self
                .ctx
                .resolve_import_target_from_file(resolved_file_idx, inner_module)
            {
                let (t, v) = self.resolved_export_meanings(inner_idx, inner_name, depth + 1);
                has_type |= t;
                has_value |= v;
            }
        }

        (has_type, has_value)
    }
}
