//! Resolution mode helpers, module-not-found diagnostics, and export helpers.

use crate::diagnostics::format_message;
use crate::query_boundaries::capabilities::FeatureGate;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::symbol_flags;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleNotFoundSite {
    Import,
    ImportType,
    RequireLike,
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Helpers
    // =========================================================================

    /// Extract the `resolution-mode` override from an import/export declaration's
    /// attributes (e.g., `with { "resolution-mode": "require" }`).
    pub(crate) fn get_resolution_mode_override(
        &self,
        attributes_idx: NodeIndex,
    ) -> Option<crate::context::ResolutionModeOverride> {
        use crate::context::ResolutionModeOverride;

        let attr_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_import_attributes_data(attr_node)?;

        for &elem_idx in &attrs.elements.nodes {
            let elem_node = match self.ctx.arena.get(elem_idx) {
                Some(node) if node.kind == syntax_kind_ext::IMPORT_ATTRIBUTE => node,
                _ => continue,
            };
            let attr = self.ctx.arena.get_import_attribute_data(elem_node)?;

            let name = if let Some(ident) = self
                .ctx
                .arena
                .get(attr.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
            {
                ident.escaped_text.as_str()
            } else if let Some(lit) = self.ctx.arena.get_literal_text(attr.name) {
                lit.trim_matches('"').trim_matches('\'')
            } else {
                continue;
            };

            if name != "resolution-mode" {
                continue;
            }

            let value_text = self.ctx.arena.get_literal_text(attr.value)?;
            return match value_text.trim_matches('"').trim_matches('\'') {
                "import" => Some(ResolutionModeOverride::Import),
                "require" => Some(ResolutionModeOverride::Require),
                _ => None,
            };
        }
        None
    }

    fn has_only_valid_resolution_mode_attribute(&self, attributes_idx: NodeIndex) -> bool {
        let Some(attr_node) = self.ctx.arena.get(attributes_idx) else {
            return false;
        };
        let Some(attrs) = self.ctx.arena.get_import_attributes_data(attr_node) else {
            return false;
        };
        if attrs.elements.nodes.len() != 1 {
            return false;
        }
        self.get_resolution_mode_override(attributes_idx).is_some()
    }

    pub(crate) fn resolution_mode_override_is_effective(
        &self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) -> bool {
        if self.get_resolution_mode_override(attributes_idx).is_none() {
            return false;
        }

        if self
            .ctx
            .capabilities
            .feature_available(FeatureGate::ImportAttributes)
        {
            return true;
        }

        self.ctx.capabilities.module == ModuleKind::Node16
            && declaration_is_type_only
            && self.has_only_valid_resolution_mode_attribute(attributes_idx)
    }

    pub(crate) fn effective_resolution_mode_override(
        &self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) -> Option<crate::context::ResolutionModeOverride> {
        if self.resolution_mode_override_is_effective(attributes_idx, declaration_is_type_only) {
            self.get_resolution_mode_override(attributes_idx)
        } else {
            None
        }
    }

    fn current_file_emit_resolution_mode(&self) -> crate::context::ResolutionModeOverride {
        let file_name = self.ctx.file_name.as_str();
        if file_name.ends_with(".mts") || file_name.ends_with(".mjs") {
            return crate::context::ResolutionModeOverride::Import;
        }
        if file_name.ends_with(".cts") || file_name.ends_with(".cjs") {
            return crate::context::ResolutionModeOverride::Require;
        }
        if self.ctx.compiler_options.module == ModuleKind::Preserve
            || self.ctx.compiler_options.module.is_es_module()
        {
            return crate::context::ResolutionModeOverride::Import;
        }
        if let Some(map) = self.ctx.file_is_esm_map.as_ref() {
            let normalized = file_name.replace('\\', "/");
            let trimmed = normalized.trim_start_matches('/');
            let slash_trimmed = format!("/{trimmed}");
            for candidate in [
                file_name,
                normalized.as_str(),
                trimmed,
                slash_trimmed.as_str(),
            ] {
                if let Some(&is_esm) = map.get(candidate) {
                    return if is_esm {
                        crate::context::ResolutionModeOverride::Import
                    } else {
                        crate::context::ResolutionModeOverride::Require
                    };
                }
            }
            if let Some(&is_esm) = map.iter().find_map(|(path, is_esm)| {
                let path = path.replace('\\', "/");
                (path == normalized
                    || path == trimmed
                    || path.ends_with(&normalized)
                    || path.ends_with(trimmed))
                .then_some(is_esm)
            }) {
                return if is_esm {
                    crate::context::ResolutionModeOverride::Import
                } else {
                    crate::context::ResolutionModeOverride::Require
                };
            }
        }
        if self.ctx.file_is_esm == Some(true) {
            crate::context::ResolutionModeOverride::Import
        } else {
            crate::context::ResolutionModeOverride::Require
        }
    }

    pub(crate) fn requested_resolution_mode(
        &self,
        attributes_idx: NodeIndex,
        declaration_is_type_only: bool,
    ) -> Option<crate::context::ResolutionModeOverride> {
        if let Some(raw_mode) = self.get_resolution_mode_override(attributes_idx) {
            if self.resolution_mode_override_is_effective(attributes_idx, declaration_is_type_only)
            {
                return Some(raw_mode);
            }
            if self.ctx.capabilities.module == ModuleKind::Node16 && !declaration_is_type_only {
                return Some(self.current_file_emit_resolution_mode());
            }
            return None;
        }

        if !declaration_is_type_only
            && (self.ctx.compiler_options.module.is_node_module()
                || self.ctx.compiler_options.module.is_es_module())
        {
            return Some(self.current_file_emit_resolution_mode());
        }

        None
    }

    /// Returns the appropriate "module not found" diagnostic code and message.
    /// Uses TS2792 when the effective module resolution is "Classic", otherwise TS2307.
    ///
    /// tsc uses `getEmitModuleResolutionKind(compilerOptions) === Classic` to decide.
    /// The `implied_classic_resolution` flag is computed at config resolution time from
    /// the effective module resolution (considering both `module` and `moduleResolution`
    /// options), matching tsc's `getEmitModuleResolutionKind()`.
    pub(crate) fn module_not_found_diagnostic(&self, module_name: &str) -> (String, u32) {
        self.module_not_found_diagnostic_for_site(module_name, ModuleNotFoundSite::Import)
    }

    /// Like `module_not_found_diagnostic`, but preserves the syntax site for
    /// Node built-in module specifiers. `import("fs")` in a type position uses
    /// the same TS2591 family as other require-like queries in tsc.
    pub(crate) fn module_not_found_diagnostic_for_site(
        &self,
        module_name: &str,
        site: ModuleNotFoundSite,
    ) -> (String, u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::query_boundaries::capabilities::is_known_node_module;

        // Known Node.js built-ins use the "cannot find name" family instead of TS2307,
        // but the exact code depends on the import context:
        // - TS2580 for regular TypeScript import/export sites
        // - TS2591 for require-like sites, import-type expressions, JavaScript files,
        //   and noTypesAndSymbols runs
        //
        // This takes priority over any resolution error from the driver.
        if is_known_node_module(module_name) {
            let use_types_field_hint = matches!(
                site,
                ModuleNotFoundSite::RequireLike | ModuleNotFoundSite::ImportType
            ) || self.ctx.compiler_options.no_types_and_symbols
                || self.ctx.is_js_file();
            let (message_template, code) = if use_types_field_hint {
                (
                    diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                    diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                )
            } else {
                (
                    diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                    diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                )
            };
            return (format_message(message_template, &[module_name]), code);
        }

        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            let use_2792 = self.ctx.compiler_options.implied_classic_resolution
                || matches!(
                    self.ctx.compiler_options.module,
                    ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System | ModuleKind::None
                );
            if use_2792
                && error.code
                    == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            {
                return (
                    format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                        &[module_name],
                    ),
                    diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O,
                );
            }
            return (error.message.clone(), error.code);
        }

        let use_2792 = self.ctx.compiler_options.implied_classic_resolution
            || matches!(
                self.ctx.compiler_options.module,
                ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System | ModuleKind::None
            );

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
    pub(crate) fn export_equals_target_is_not_module_or_variable(
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

        // Resolve aliases to find the actual target symbol.
        // If alias resolution fails (e.g., the target doesn't exist),
        // we cannot determine the export= target's kind, so bail out
        // rather than falsely triggering TS2497 on the unresolved alias.
        let resolved = if let Some(sym) = self
            .ctx
            .binder
            .get_symbol_with_libs(export_equals_sym, &lib_binders)
            && (sym.flags & symbol_flags::ALIAS) != 0
        {
            let mut visited = Vec::new();
            let Some(resolved_sym) = self.resolve_alias_symbol(export_equals_sym, &mut visited)
            else {
                return false;
            };
            resolved_sym
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

        // Namespace imports (`import * as X from "mod"`) resolve back to their
        // alias symbol rather than the module symbol.  They represent the entire
        // module namespace, so treat them as module-like for TS2497 purposes.
        if !is_module_or_variable
            && (target.flags & symbol_flags::ALIAS) != 0
            && target.import_module.is_some()
            && target.import_name.as_deref() == Some("*")
        {
            return false;
        }

        !is_module_or_variable
    }

    pub(crate) fn global_augmentation_namespace_export_cycle_report_node(
        &self,
        statements: &[NodeIndex],
        ident_name: &str,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::node_flags;

        let mut matching_namespace_export = None;
        let mut matching_global_augmentation = None;

        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
                if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                    && let Some(export_name_node) = self.ctx.arena.get(export_decl.export_clause)
                    && let Some(export_name) = self.ctx.arena.get_identifier(export_name_node)
                    && export_name.escaped_text == ident_name
                {
                    matching_namespace_export = Some(stmt_idx);
                }
                continue;
            }

            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }

            let Some(module_decl) = self.ctx.arena.get_module(stmt_node) else {
                continue;
            };
            let is_global_augmentation =
                (u32::from(stmt_node.flags) & node_flags::GLOBAL_AUGMENTATION) != 0
                    || self
                        .ctx
                        .arena
                        .get(module_decl.name)
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .is_some_and(|ident| ident.escaped_text == "global");
            if !is_global_augmentation || !module_decl.body.is_some() {
                continue;
            }

            let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
                continue;
            };
            let has_matching_namespace_decl = if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
                self.ctx
                    .arena
                    .get_module_block(body_node)
                    .and_then(|block| block.statements.as_ref())
                    .is_some_and(|stmts| {
                        stmts.nodes.iter().any(|&inner_idx| {
                            self.ctx
                                .arena
                                .get(inner_idx)
                                .filter(|node| node.kind == syntax_kind_ext::MODULE_DECLARATION)
                                .and_then(|node| self.ctx.arena.get_module(node))
                                .and_then(|module| self.ctx.arena.get(module.name))
                                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                .is_some_and(|ident| ident.escaped_text == ident_name)
                        })
                    })
            } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                self.ctx
                    .arena
                    .get_module(body_node)
                    .and_then(|module| self.ctx.arena.get(module.name))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == ident_name)
            } else {
                false
            };

            if has_matching_namespace_decl {
                matching_global_augmentation = Some(stmt_idx);
            }
        }

        if matching_namespace_export.is_some() && matching_global_augmentation.is_some() {
            matching_namespace_export
        } else {
            return None;
        }
    }

    /// Check whether a named import can be satisfied via `export =` target members.
    pub(crate) fn has_named_export_via_export_equals(
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
        if import_name == "default" {
            return export_equals_sym.is_some();
        }

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
            for &candidate_id in self
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

    fn resolve_export_assignment_target_symbol(
        &self,
        export_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let node = self.ctx.arena.get(export_idx)?;
        let export_data = self.ctx.arena.get_export_assignment(node)?;
        let expr_node = self.ctx.arena.get(export_data.expression)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            self.resolve_identifier_symbol(export_data.expression)
        } else if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol(export_data.expression)
        } else {
            None
        }
    }

    fn top_level_exported_name_nodes(
        &self,
        statements: &[NodeIndex],
    ) -> FxHashMap<String, Vec<NodeIndex>> {
        let mut exported = FxHashMap::default();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export_decl) = self.ctx.arena.get_export_decl(node) else {
                        continue;
                    };
                    if export_decl.is_default_export || export_decl.module_specifier.is_some() {
                        continue;
                    }
                    let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                        continue;
                    };
                    if let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) {
                        for &specifier_idx in &named_exports.elements.nodes {
                            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                                continue;
                            };
                            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node)
                            else {
                                continue;
                            };
                            if specifier.is_type_only {
                                continue;
                            }
                            let Some(name) = self.get_identifier_text_from_idx(specifier.name)
                            else {
                                continue;
                            };
                            exported
                                .entry(name)
                                .or_insert_with(Vec::new)
                                .push(specifier.name);
                        }
                    } else if let Some(name_idx) =
                        self.get_declaration_name_node(export_decl.export_clause)
                        && let Some(name) = self.get_identifier_text_from_idx(name_idx)
                    {
                        exported.entry(name).or_insert_with(Vec::new).push(name_idx);
                    }
                }
                _ => {
                    if !self.has_export_modifier(stmt_idx) {
                        continue;
                    }
                    let Some(name_idx) = self.get_declaration_name_node(stmt_idx) else {
                        continue;
                    };
                    let Some(name) = self.get_identifier_text_from_idx(name_idx) else {
                        continue;
                    };
                    exported.entry(name).or_insert_with(Vec::new).push(name_idx);
                }
            }
        }

        exported
    }

    fn namespace_exported_name_nodes_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> FxHashMap<String, Vec<NodeIndex>> {
        let mut exported = FxHashMap::default();
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return exported;
        };

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = self.ctx.arena.get_module(node) else {
                continue;
            };
            let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
                continue;
            };
            let Some(block) = self.ctx.arena.get_module_block(body_node) else {
                continue;
            };
            let Some(statements) = &block.statements else {
                continue;
            };

            for &stmt_idx in &statements.nodes {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                match stmt_node.kind {
                    syntax_kind_ext::EXPORT_DECLARATION => {
                        let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                            continue;
                        };
                        if export_decl.is_default_export || export_decl.module_specifier.is_some() {
                            continue;
                        }
                        let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
                        else {
                            continue;
                        };
                        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node)
                        else {
                            continue;
                        };
                        for &specifier_idx in &named_exports.elements.nodes {
                            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                                continue;
                            };
                            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node)
                            else {
                                continue;
                            };
                            if specifier.is_type_only {
                                continue;
                            }
                            let Some(name) = self.get_identifier_text_from_idx(specifier.name)
                            else {
                                continue;
                            };
                            exported
                                .entry(name)
                                .or_insert_with(Vec::new)
                                .push(specifier.name);
                        }
                    }
                    _ => {
                        if !self.has_export_modifier(stmt_idx) {
                            continue;
                        }
                        let Some(name_idx) = self.get_declaration_name_node(stmt_idx) else {
                            continue;
                        };
                        let Some(name) = self.get_identifier_text_from_idx(name_idx) else {
                            continue;
                        };
                        exported.entry(name).or_insert_with(Vec::new).push(name_idx);
                    }
                }
            }
        }

        exported
    }

    pub(crate) fn check_export_assignment_target_member_duplicates(
        &mut self,
        statements: &[NodeIndex],
        export_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let Some(target_sym_id) = self.resolve_export_assignment_target_symbol(export_idx) else {
            return;
        };

        let namespace_exports = self.namespace_exported_name_nodes_for_symbol(target_sym_id);
        if namespace_exports.is_empty() {
            return;
        }

        let file_exports = self.top_level_exported_name_nodes(statements);
        let mut reported_nodes = FxHashSet::default();

        for (name, namespace_nodes) in namespace_exports {
            let Some(file_nodes) = file_exports.get(&name) else {
                continue;
            };
            let message = format_message(diagnostic_messages::DUPLICATE_IDENTIFIER, &[&name]);

            for &node_idx in file_nodes.iter().chain(namespace_nodes.iter()) {
                if reported_nodes.insert(node_idx) {
                    self.error_at_node(node_idx, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
                }
            }
        }
    }

    pub(crate) fn named_import_found_via_reexport(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        if resolution_mode.is_some() {
            return self.resolve_import_via_target_binder(
                module_name,
                import_name,
                resolution_mode,
            ) || self.import_found_via_module_augmentation(
                module_name,
                normalized,
                import_name,
            );
        }

        self.ctx
            .binder
            .resolve_import_if_needed_public(module_name, import_name)
            .is_some()
            || self
                .ctx
                .binder
                .resolve_import_if_needed_public(normalized, import_name)
                .is_some()
            || self.resolve_import_via_target_binder(module_name, import_name, None)
            || self.resolve_import_via_all_binders(module_name, normalized, import_name)
            || self.cross_file_export_is_actual_export(module_name, import_name, None)
            || self.cross_file_export_is_actual_export(normalized, import_name, None)
            || self.import_found_via_module_augmentation(module_name, normalized, import_name)
    }

    /// Check if a symbol is declared in a module augmentation targeting the given module.
    /// `declare module "x" { type C = ... }` makes `C` importable from module `"x"`.
    fn import_found_via_module_augmentation(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        for key in [module_name, normalized] {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(key)
                && augmentations.iter().any(|aug| aug.name == import_name)
            {
                return true;
            }
        }
        false
    }

    /// Like `resolve_cross_file_export_from_file`, but filters out symbols that
    /// only exist in `file_locals` of an external module without being exported.
    /// This prevents non-exported local symbols from being treated as re-exports,
    /// which would suppress TS2459 ("declares locally but not exported").
    fn cross_file_export_is_actual_export(
        &self,
        module_specifier: &str,
        export_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> bool {
        let sym_id = match self.resolve_cross_file_export_from_file(
            module_specifier,
            export_name,
            Some(self.ctx.current_file_idx),
        ) {
            Some(id) => id,
            None => return false,
        };

        // If the target module is an external module (has import/export statements),
        // verify the symbol is actually exported (in module_exports), not just local.
        let target_idx = if let Some(mode) = resolution_mode {
            self.ctx.resolve_import_target_from_file_with_mode(
                self.ctx.current_file_idx,
                module_specifier,
                Some(mode),
            )
        } else {
            self.ctx.resolve_import_target(module_specifier)
        };
        if let Some(target_idx) = target_idx
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
            && target_binder.is_external_module
        {
            // Check if the symbol is in file_locals but NOT in any module_exports.
            // If so, it's a local-only symbol and should not count as a re-export.
            if target_binder.file_locals.get(export_name) == Some(sym_id) {
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                let target_file_name = target_arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.as_str());

                let exports_table = target_file_name
                    .and_then(|fname| target_binder.module_exports.get(fname))
                    .or_else(|| target_binder.module_exports.get(module_specifier));

                let in_module_exports =
                    exports_table.is_some_and(|exports| exports.has(export_name));

                // For `export =` modules, the export target is in file_locals but
                // accessible via the `export =` binding. Don't filter these out.
                let has_export_equals = exports_table.is_some_and(|exports| exports.has("export="));

                if !in_module_exports && !has_export_equals {
                    return false;
                }
            }
        }

        true
    }

    /// Type-level fallback for `has_named_export_via_export_equals`.
    ///
    /// When the `export =` target is a typed value (e.g., `const x: T` where
    /// `T = { [P in 'NotFound']: unknown }`), `NotFound` is invisible to symbol-table
    /// lookups but accessible via type-level property access.
    pub(crate) fn has_named_export_via_export_equals_type(
        &mut self,
        exports_table: &tsz_binder::SymbolTable,
        import_name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let Some(export_equals_sym) = exports_table.get("export=") else {
            return false;
        };
        if import_name == "default" {
            return true;
        }

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
    ///
    /// Most source-file imports still require an explicit `export default`, but some
    /// module transforms synthesize the default binding from the namespace object.
    /// Those cases are handled by
    /// `source_file_import_uses_system_default_namespace_fallback`.
    pub(crate) fn is_source_file_import(&self, module_name: &str) -> bool {
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

    /// In `module: system`, TypeScript permits default imports from source `.ts`
    /// modules by treating the default binding as the module namespace object.
    pub(crate) fn source_file_import_uses_system_default_namespace_fallback(
        &self,
        module_name: &str,
    ) -> bool {
        self.ctx.compiler_options.module == ModuleKind::System
            && self.is_source_file_import(module_name)
    }
}
