//! Core import/export checking implementation.

use crate::diagnostics::format_message;
use crate::query_boundaries::capabilities::FeatureGate;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::symbol_flags;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;

// =============================================================================
// Import/Export Checking Methods
// =============================================================================

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

    fn check_export_assignment_target_member_duplicates(
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

    fn named_import_found_via_reexport(
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
    fn has_named_export_via_export_equals_type(
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

    /// In `module: system`, TypeScript permits default imports from source `.ts`
    /// modules by treating the default binding as the module namespace object.
    pub(crate) fn source_file_import_uses_system_default_namespace_fallback(
        &self,
        module_name: &str,
    ) -> bool {
        self.ctx.compiler_options.module == ModuleKind::System
            && self.is_source_file_import(module_name)
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
                            // Also suppress TS2614 when the module has a default export
                            // and the user tried to import a named member that doesn't
                            // exist - in these cases, tsc emits other errors like
                            // TS2497 or TS2595 instead.
                            let has_export_equals = exports_table.has("export=");
                            let has_interop = self.ctx.compiler_options.es_module_interop
                                || self.ctx.compiler_options.allow_synthetic_default_imports;
                            let suppress_for_interop = has_export_equals && has_interop;
                            let suppress_for_default =
                                exports_table.has("default") && !exports_table.has(import_name);

                            if !found_via_type && !suppress_for_interop && !suppress_for_default {
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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

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
                        // TS1294: erasableSyntaxOnly — export = is not erasable.
                        // Exception: `export = x` is allowed in .cts/.cjs files because
                        // it's the standard CJS export syntax and compiles to `module.exports = x`.
                        let is_cts_file = self.ctx.file_name.ends_with(".cts")
                            || self.ctx.file_name.ends_with(".cjs");
                        if export_data.is_export_equals
                            && self.ctx.compiler_options.erasable_syntax_only
                            && !self.ctx.is_ambient_declaration(stmt_idx)
                            && !is_cts_file
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                diagnostic_messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED
                                    .to_string(),
                                diagnostic_codes::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED,
                            );
                        }

                        // TS1282/TS1283: VMS checks for export = <type>
                        if export_data.is_export_equals
                            && self.ctx.compiler_options.verbatim_module_syntax
                            && !is_declaration_file
                        {
                            self.check_vms_export_equals(export_data.expression);
                        }

                        // TS2714: In ambient context, export assignment expression must be
                        // an identifier or qualified name. This check applies to both
                        // `export = <expr>` and `export default <expr>` in ambient contexts.
                        let is_ambient =
                            is_declaration_file || self.is_ambient_declaration(stmt_idx);
                        if is_ambient
                            && !self.is_identifier_or_qualified_name(export_data.expression)
                        {
                            // Only emit TS2714 when the expression is NOT an identifier
                            // or qualified name. Valid forms like `export = X` or
                            // `export default Y` (where X/Y are identifiers) should not
                            // trigger this error.
                            self.error_at_node(
                                export_data.expression,
                                "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                            );
                        } else if export_data.is_export_equals
                            && let Some(ident) = self
                                .ctx
                                .arena
                                .get(export_data.expression)
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                            && let Some(report_node) = self
                                .global_augmentation_namespace_export_cycle_report_node(
                                    statements,
                                    ident.escaped_text.as_str(),
                                )
                        {
                            self.error_at_node(
                                report_node,
                                &format_message(
                                    diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                                    &[ident.escaped_text.as_str()],
                                ),
                                diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                            );
                        } else if let Some(expected_type) =
                            self.jsdoc_type_annotation_for_node(stmt_idx)
                        {
                            let request =
                                crate::context::TypingRequest::with_contextual_type(expected_type);
                            let actual_type = self
                                .get_type_of_node_with_request(export_data.expression, &request);
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
                        let has_named_default_export =
                            self.export_decl_has_named_default_export(stmt_idx);
                        if export_data.is_default_export || has_named_default_export {
                            export_default_indices.push(stmt_idx);

                            // TS2714: In ambient context, export default expression must be
                            // an identifier or qualified name. Skip for declarations
                            // (class, function, interface, enum) which are always valid.
                            // Only applies to `export default <expr>`, NOT to
                            // `export { x as default }` re-exports.
                            if export_data.is_default_export {
                                let is_ambient =
                                    is_declaration_file || self.is_ambient_declaration(stmt_idx);
                                let is_declaration =
                                    self.ctx.arena.get(export_data.export_clause).is_some_and(
                                        |n| {
                                            matches!(
                                                n.kind,
                                                k if k == syntax_kind_ext::CLASS_DECLARATION
                                                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                                                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                                                    || k == syntax_kind_ext::ENUM_DECLARATION
                                                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                            )
                                        },
                                    );
                                if is_ambient
                                    && !is_declaration
                                    && export_data.export_clause.is_some()
                                    && !self
                                        .is_identifier_or_qualified_name(export_data.export_clause)
                                {
                                    self.error_at_node(
                                        export_data.export_clause,
                                        "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                        diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                                    );
                                }
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

        let mut emitted_ts1203 = false;
        if (is_es_module || is_system_module || is_node_esm_file)
            && !is_preserve
            && !is_declaration_file
            && !self.is_js_file()
            && !is_cjs_extension
            && !self.ctx.has_syntax_parse_errors
        {
            for &export_idx in &export_assignment_indices {
                if !self.is_ambient_declaration(export_idx) {
                    emitted_ts1203 = true;
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
        // Skip in ambient declarations - they describe external module shapes, not
        // actual conflicting runtime exports.
        if export_assignment_indices.len() > 1 {
            for &export_idx in &export_assignment_indices {
                // Skip ambient declarations
                if self.is_ambient_declaration(export_idx) {
                    continue;
                }
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
        // Skip in `preserve` mode — it allows mixing CJS (`export =`) and ESM syntax.
        // When TS1203 already flags `export =` as invalid, tsc suppresses TS2309 for
        // ESNext/Node module modes. For ES2015 targets, tsc emits both TS1203 and TS2309.
        let suppress_ts2309 = emitted_ts1203
            && !matches!(
                self.ctx.compiler_options.module,
                tsz_common::common::ModuleKind::ES2015
            );
        if let Some(&export_idx) = export_assignment_indices.first()
            && has_other_exports
            && export_assignment_indices.len() == 1
            && !is_preserve
            && !suppress_ts2309
        {
            self.check_export_assignment_target_member_duplicates(statements, export_idx);
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
        // Special case: function + class default exports emit TS2323 + TS2813 + TS2814 instead.
        let bridged_default_exports =
            self.default_export_interface_merge_bridge_indices(&export_default_indices);
        let effective_default_indices: Vec<NodeIndex> = export_default_indices
            .iter()
            .copied()
            .filter(|idx| !bridged_default_exports.contains(idx))
            .collect();

        if effective_default_indices.len() > 1 {
            // Classify each default export
            let mut has_interface = false;
            let mut has_class = false;
            let mut has_function = false;
            let mut has_named_default_export = false;
            let mut value_count = 0;
            let mut function_value_count = 0;
            let mut function_name: Option<String> = None;

            for &export_idx in &effective_default_indices {
                if self.export_decl_has_named_default_export(export_idx) {
                    has_named_default_export = true;
                }
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
                        has_function = true;
                        // Check if all function defaults share the same name (overloads)
                        let name = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .and_then(|n| self.ctx.arena.get_function(n))
                            .map(|f| self.node_text(f.name).unwrap_or_default());
                        match (&function_name, name) {
                            (None, Some(n)) if !n.is_empty() => {
                                function_name = Some(n);
                                value_count += 1;
                                function_value_count += 1;
                            }
                            (Some(existing), Some(n)) if !n.is_empty() && *existing == n => {
                                // Same non-empty function name: overload, don't count again
                            }
                            _ => {
                                value_count += 1;
                                function_value_count += 1;
                            }
                        }
                    }
                    Some(k) if k == syntax_kind_ext::CLASS_DECLARATION => {
                        has_class = true;
                        value_count += 1;
                    }
                    _ => {
                        value_count += 1;
                    }
                }
            }

            // Emit TS2528 for any multiple default exports that are not
            // function overloads. tsc allows interface + value (function/class)
            // to coexist as a declaration merge, so interface+function is NOT a
            // conflict. However, interface + type re-export IS a conflict.
            // The merge is only valid when: (1) one default is an interface
            // declaration, AND (2) the other is a function or class (value).
            let interface_can_merge =
                has_interface && (has_function || has_class) && value_count == 1;
            let is_conflict =
                value_count > 1 || (effective_default_indices.len() > 1 && !interface_can_merge);
            if is_conflict {
                if has_function && has_class {
                    // When function + class both export as default, tsc emits
                    // TS2323 + TS2813 + TS2814 (merge conflict diagnostics).
                    self.emit_function_class_default_merge_errors(&effective_default_indices);
                } else if has_class && value_count > 1 && {
                    // tsc emits TS2323 only when a named variable reference
                    // (export default foo) accompanies a class, not for anonymous
                    // expressions (export default {...}). multipleExportDefault3/4
                    // have class + object literal and expect TS2528.
                    // Also, if the identifier refers to a type-only binding (e.g.,
                    // a type alias), tsc uses TS2528 instead of TS2323.
                    effective_default_indices.iter().any(|&idx| {
                        self.ctx
                            .arena
                            .get_export_decl_at(idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .is_some_and(|c| {
                                if c.kind != SyntaxKind::Identifier as u16 {
                                    return false;
                                }
                                // Check if identifier refers to a value (not type-only).
                                // tsc uses TS2528 for type-only default exports (e.g.,
                                // `type Bar = {}; export default Bar`).
                                let ed = self.ctx.arena.get_export_decl_at(idx);
                                let clause_idx = ed.map(|ed| ed.export_clause).unwrap_or(idx);
                                if let Some(name) = self.node_text(clause_idx)
                                    && let Some(sym_id) =
                                        self.resolve_name_at_node(&name, clause_idx)
                                    && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                                {
                                    // Only treat as TS2323 if the symbol has value flags
                                    return sym.has_any_flags(symbol_flags::VALUE);
                                }
                                // If we can't resolve, treat as value (conservative)
                                true
                            })
                    })
                } {
                    // Classify each default export as value or type-only.
                    // tsc emits TS2323 for value exports (identifiers resolving to
                    // values, or class/interface declarations). When there are also
                    // type-only exports (e.g., type aliases), tsc additionally emits
                    // TS2528 for ALL default exports. When ALL exports are
                    // values/classes/interfaces, only TS2323 is emitted (no TS2528).
                    let mut has_type_only_export = false;
                    let mut per_export: Vec<(NodeIndex, NodeIndex, bool, bool, bool)> = Vec::new();

                    for &export_idx in &effective_default_indices {
                        let default_anchor = self.get_default_export_anchor(export_idx);
                        let clause_idx = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .map(|ed| ed.export_clause)
                            .unwrap_or(NodeIndex::NONE);
                        let clause_kind = self.ctx.arena.get(clause_idx).map(|c| c.kind);
                        let is_ident = clause_kind == Some(SyntaxKind::Identifier as u16);
                        let is_class_decl = clause_kind == Some(syntax_kind_ext::CLASS_DECLARATION);
                        let is_interface_decl =
                            clause_kind == Some(syntax_kind_ext::INTERFACE_DECLARATION);

                        let is_type_only_ident = is_ident
                            && self
                                .resolve_identifier_symbol(clause_idx)
                                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                .is_some_and(|sym| {
                                    use tsz_binder::symbols::symbol_flags;
                                    let value_flags = symbol_flags::FUNCTION
                                        | symbol_flags::VARIABLE
                                        | symbol_flags::CLASS
                                        | symbol_flags::ENUM
                                        | symbol_flags::ENUM_MEMBER;
                                    (sym.flags & value_flags) == 0
                                        && (sym.flags & symbol_flags::TYPE) != 0
                                });

                        if is_type_only_ident {
                            has_type_only_export = true;
                        }

                        let is_value =
                            (is_ident && !is_type_only_ident) || is_class_decl || is_interface_decl;
                        per_export.push((
                            export_idx,
                            default_anchor,
                            is_ident,
                            is_value,
                            is_class_decl,
                        ));
                    }

                    for &(export_idx, default_anchor, is_ident, is_value, _is_class_decl) in
                        &per_export
                    {
                        // TS2323 for value exports (identifiers resolving to values,
                        // class declarations, interface declarations).
                        if is_value {
                            if is_ident {
                                self.error_at_node(
                                    export_idx,
                                    "Cannot redeclare exported variable 'default'.",
                                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                                );
                            } else {
                                self.error_at_default_export_anchor(
                                    export_idx,
                                    "Cannot redeclare exported variable 'default'.",
                                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                                );
                            }
                        }
                        // TS2528 only when type-only exports are present in the mix.
                        if has_type_only_export {
                            self.error_at_node(
                                default_anchor,
                                diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                                diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            );
                        }
                    }
                } else if has_interface
                    && has_function
                    && !has_class
                    && value_count == function_value_count
                {
                    // Interface + function default exports (all values are functions):
                    // TS2323 for all declarations. Note: a single function + interface
                    // is allowed (declaration merging), but that case is excluded by
                    // is_conflict requiring value_count > 1.
                    // When additional non-function value exports exist (e.g., identifier
                    // references to classes), tsc uses TS2528 instead, so we fall through
                    // to the else.
                    for &export_idx in &effective_default_indices {
                        self.error_at_default_export_anchor(
                            export_idx,
                            "Cannot redeclare exported variable 'default'.",
                            diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                        );
                    }
                } else if has_named_default_export
                    && effective_default_indices.iter().any(|&idx| {
                        let Some(clause_idx) = self
                            .ctx
                            .arena
                            .get_export_decl_at(idx)
                            .map(|ed| ed.export_clause)
                        else {
                            return false;
                        };
                        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                            return false;
                        };
                        match clause_node.kind {
                            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                || k == syntax_kind_ext::CLASS_DECLARATION
                                || k == syntax_kind_ext::INTERFACE_DECLARATION =>
                            {
                                true
                            }
                            k if k == SyntaxKind::Identifier as u16 => self
                                .resolve_identifier_symbol(clause_idx)
                                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                .is_some_and(|sym| sym.has_any_flags(symbol_flags::VALUE)),
                            _ => false,
                        }
                    })
                {
                    for &export_idx in &effective_default_indices {
                        let anchor = self.get_default_export_anchor(export_idx);
                        let clause_idx = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .map(|ed| ed.export_clause)
                            .unwrap_or(NodeIndex::NONE);
                        let is_value_default =
                            self.ctx.arena.get(clause_idx).is_some_and(|clause_node| {
                                match clause_node.kind {
                                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                        || k == syntax_kind_ext::CLASS_DECLARATION
                                        || k == syntax_kind_ext::INTERFACE_DECLARATION =>
                                    {
                                        true
                                    }
                                    k if k == SyntaxKind::Identifier as u16 => self
                                        .resolve_identifier_symbol(clause_idx)
                                        .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                        .is_some_and(|sym| sym.has_any_flags(symbol_flags::VALUE)),
                                    _ if self.export_decl_has_direct_named_default_export(
                                        export_idx,
                                    ) =>
                                    {
                                        true
                                    }
                                    _ => false,
                                }
                            });
                        if is_value_default {
                            self.error_at_default_export_anchor(
                                export_idx,
                                "Cannot redeclare exported variable 'default'.",
                                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                            );
                        }
                        self.error_at_node(
                            anchor,
                            diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        );
                    }
                } else {
                    // Fallback: TS2528 "A module cannot have multiple default exports"
                    // tsc skips interface declarations when emitting TS2528 (interfaces
                    // merge with values and don't count as conflicting defaults).
                    for &export_idx in &effective_default_indices {
                        let is_interface = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .is_some_and(|n| n.kind == syntax_kind_ext::INTERFACE_DECLARATION);
                        if is_interface {
                            continue;
                        }
                        let anchor = self.get_default_export_anchor(export_idx);
                        self.error_at_node(
                            anchor,
                            diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        );
                    }
                }

                // TS2393: Duplicate function implementation.
                // When multiple `export default function` declarations have bodies,
                // tsc emits TS2393 on each, regardless of whether they are named or anonymous.
                if has_function {
                    let func_impls: Vec<NodeIndex> = effective_default_indices
                        .iter()
                        .filter_map(|&idx| {
                            let ed = self.ctx.arena.get_export_decl_at(idx)?;
                            let clause_node = self.ctx.arena.get(ed.export_clause)?;
                            if clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                                return None;
                            }
                            let func = self.ctx.arena.get_function(clause_node)?;
                            if func.body.is_some() { Some(idx) } else { None }
                        })
                        .collect();

                    if func_impls.len() > 1 {
                        for &impl_idx in &func_impls {
                            self.error_at_node(
                                impl_idx,
                                diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                                diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            );
                        }
                    }
                }
            } else if has_interface && !(has_function && value_count == 1) {
                // Multiple default exports with at least one interface but not a valid
                // interface + function merge. E.g.:
                //   export default interface A {}
                //   export default B;  // B is an interface
                // TSC reports TS2528 because these can't merge.
                for &export_idx in &effective_default_indices {
                    let anchor = self.get_default_export_anchor(export_idx);
                    self.error_at_node(
                        anchor,
                        diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                    );
                }
            }
        }
    }

    /// Get the best anchor node for a default export diagnostic (declaration name or the
    /// export statement itself).
    fn get_default_export_anchor(&self, export_idx: NodeIndex) -> NodeIndex {
        self.ctx
            .arena
            .get_export_decl_at(export_idx)
            .and_then(|ed| {
                let clause = self.ctx.arena.get(ed.export_clause)?;
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
                } else if clause.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    self.ctx.arena.get_interface(clause).and_then(|i| {
                        let n = self.ctx.arena.get(i.name)?;
                        if n.kind == SyntaxKind::Identifier as u16 {
                            Some(i.name)
                        } else {
                            None
                        }
                    })
                } else if let Some(named_exports) = self.ctx.arena.get_named_imports(clause) {
                    named_exports
                        .elements
                        .nodes
                        .iter()
                        .find_map(|&specifier_idx| {
                            let specifier_node = self.ctx.arena.get(specifier_idx)?;
                            let specifier = self.ctx.arena.get_specifier(specifier_node)?;
                            if specifier.is_type_only {
                                return None;
                            }
                            let exported_name =
                                self.get_identifier_text_from_idx(specifier.name)?;
                            (exported_name == "default").then_some(specifier.name)
                        })
                } else if clause.kind == SyntaxKind::Identifier as u16 {
                    Some(ed.export_clause)
                } else {
                    None
                }
            })
            .unwrap_or(export_idx)
    }

    fn error_at_default_export_anchor(&mut self, export_idx: NodeIndex, message: &str, code: u32) {
        let anchor = self.get_default_export_anchor(export_idx);
        self.error_at_node(anchor, message, code);
    }

    fn export_decl_has_named_default_export(&self, export_idx: NodeIndex) -> bool {
        let Some(clause_idx) = self
            .ctx
            .arena
            .get_export_decl_at(export_idx)
            .map(|ed| ed.export_clause)
        else {
            return false;
        };
        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return false;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return false;
        };

        named_exports.elements.nodes.iter().any(|&specifier_idx| {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                return false;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                return false;
            };
            !specifier.is_type_only
                && self
                    .get_identifier_text_from_idx(specifier.name)
                    .is_some_and(|name| name == "default")
        })
    }

    fn export_decl_has_direct_named_default_export(&self, export_idx: NodeIndex) -> bool {
        let Some(clause_idx) = self
            .ctx
            .arena
            .get_export_decl_at(export_idx)
            .map(|ed| ed.export_clause)
        else {
            return false;
        };
        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return false;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return false;
        };

        named_exports.elements.nodes.iter().any(|&specifier_idx| {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                return false;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                return false;
            };
            if specifier.is_type_only
                || self
                    .get_identifier_text_from_idx(specifier.name)
                    .is_none_or(|name| name != "default")
            {
                return false;
            }

            specifier.property_name.is_none()
                || self
                    .get_identifier_text_from_idx(specifier.property_name)
                    .is_some_and(|name| name == "default")
        })
    }

    fn default_export_interface_merge_bridge_indices(
        &mut self,
        export_default_indices: &[NodeIndex],
    ) -> FxHashSet<NodeIndex> {
        let interface_default_names: FxHashSet<String> = export_default_indices
            .iter()
            .filter_map(|&export_idx| {
                let clause_idx = self.ctx.arena.get_export_decl_at(export_idx)?.export_clause;
                let clause = self.ctx.arena.get(clause_idx)?;
                if clause.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                    return None;
                }
                let interface_decl = self.ctx.arena.get_interface(clause)?;
                self.get_identifier_text_from_idx(interface_decl.name)
            })
            .collect();

        if interface_default_names.is_empty() {
            return FxHashSet::default();
        }

        export_default_indices
            .iter()
            .filter_map(|&export_idx| {
                let clause_idx = self.ctx.arena.get_export_decl_at(export_idx)?.export_clause;
                let clause_node = self.ctx.arena.get(clause_idx)?;
                let named_exports = self.ctx.arena.get_named_imports(clause_node)?;

                let bridges_interface_merge =
                    named_exports.elements.nodes.iter().any(|&specifier_idx| {
                        let specifier_node = match self.ctx.arena.get(specifier_idx) {
                            Some(node) => node,
                            None => return false,
                        };
                        let specifier = match self.ctx.arena.get_specifier(specifier_node) {
                            Some(specifier) if !specifier.is_type_only => specifier,
                            _ => return false,
                        };
                        let exported_name = match self.get_identifier_text_from_idx(specifier.name)
                        {
                            Some(name) if name == "default" => name,
                            _ => return false,
                        };
                        let _ = exported_name;
                        let mut candidate_name_indices = Vec::with_capacity(2);
                        if specifier.property_name.is_some() {
                            candidate_name_indices.push(specifier.property_name);
                        }
                        if specifier.name.is_some() {
                            candidate_name_indices.push(specifier.name);
                        }

                        candidate_name_indices.into_iter().any(|candidate_idx| {
                            self.get_identifier_text_from_idx(candidate_idx)
                                .is_some_and(|local_name| {
                                    local_name != "default"
                                        && interface_default_names.contains(&local_name)
                                })
                        })
                    });

                bridges_interface_merge.then_some(export_idx)
            })
            .collect()
    }

    /// Emit TS2323 + TS2813 + TS2814 for function + class default export merge conflicts.
    /// tsc treats `export default function` + `export default class` as a declaration merge
    /// conflict rather than a "multiple default exports" (TS2528) scenario.
    fn emit_function_class_default_merge_errors(&mut self, export_default_indices: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // TS2323: "Cannot redeclare exported variable 'default'." on every declaration
        for &export_idx in export_default_indices {
            let message = format_message(
                diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                &["default"],
            );
            self.error_at_default_export_anchor(
                export_idx,
                &message,
                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
            );
        }

        // TS2813: "Class declaration cannot implement overload list for 'default'." on class
        // TS2814: "Function with bodies can only merge with classes that are ambient." on function
        for &export_idx in export_default_indices {
            let wrapped_kind = self
                .ctx
                .arena
                .get_export_decl_at(export_idx)
                .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                .map(|n| n.kind);

            let anchor = self.get_default_export_anchor(export_idx);

            match wrapped_kind {
                Some(k) if k == syntax_kind_ext::CLASS_DECLARATION => {
                    let message = format_message(
                        diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        &["default"],
                    );
                    self.error_at_node(
                        anchor,
                        &message,
                        diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                    );
                }
                Some(k) if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.error_at_node(
                        anchor,
                        diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                        diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                    );
                }
                _ => {}
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
}
