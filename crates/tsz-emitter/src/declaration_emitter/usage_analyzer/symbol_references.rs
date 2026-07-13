use rustc_hash::FxHashSet;
use std::sync::Arc;
use tracing::debug;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::{UsageAnalyzer, UsageKind};

impl UsageAnalyzer<'_> {
    pub(super) fn analyze_type_query_entity_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(sym_id) = self.resolve_type_query_value_symbol(name_idx) {
                    self.mark_symbol_used(sym_id, UsageKind::VALUE);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(name_node) {
                    self.analyze_type_query_entity_name(name.left);
                    self.analyze_type_query_entity_name(name.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(name_node) {
                    self.analyze_type_query_entity_name(access.expression);
                    self.analyze_type_query_entity_name(access.name_or_argument);
                }
            }
            _ => {}
        }
    }

    fn resolve_type_query_value_symbol(&self, name_idx: NodeIndex) -> Option<SymbolId> {
        let name_node = self.arena.get(name_idx)?;
        let ident = self.arena.get_identifier(name_node)?;
        if ident.escaped_text == "default" {
            return None;
        }

        if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0)
            && let Some(sym_id) = self.type_query_value_dependency_symbol(sym_id)
        {
            return Some(sym_id);
        }

        if let Some(&sym_id) = self.import_name_map.get(ident.escaped_text.as_str())
            && let Some(sym_id) = self.type_query_value_dependency_symbol(sym_id)
        {
            return Some(sym_id);
        }

        if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text)
            && let Some(sym_id) = self.type_query_value_dependency_symbol(sym_id)
        {
            return Some(sym_id);
        }

        for scope in self.binder.scopes.iter() {
            if let Some(sym_id) = scope.table.get(&ident.escaped_text)
                && let Some(sym_id) = self.type_query_value_dependency_symbol(sym_id)
            {
                return Some(sym_id);
            }
        }

        None
    }

    pub(super) fn type_query_value_dependency_symbol(&self, sym_id: SymbolId) -> Option<SymbolId> {
        let symbol = self.binder.symbols.get(sym_id)?;

        if symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) && symbol.import_module().is_some()
        {
            let Some(resolved_sym_id) = self.resolve_import_alias_target_symbol(sym_id) else {
                // If the module graph is unavailable, preserve the source import.
                return Some(sym_id);
            };
            return self
                .symbol_has_type_query_value_meaning(resolved_sym_id)
                .then_some(sym_id);
        }

        self.symbol_has_type_query_value_meaning(sym_id)
            .then_some(sym_id)
    }

    pub(super) fn resolve_import_alias_target_symbol(&self, sym_id: SymbolId) -> Option<SymbolId> {
        if let Some(resolved) = self.binder.resolve_import_symbol(sym_id) {
            return Some(resolved);
        }

        let symbol = self.binder.symbols.get(sym_id)?;
        let module_specifier = symbol.import_module()?;
        let export_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());

        for module_key in self.relative_module_export_keys(module_specifier) {
            if let Some(exports) = self.binder.module_exports.get(&module_key)
                && let Some(target) = exports.get(export_name)
            {
                return Some(target);
            }
        }

        None
    }

    fn relative_module_export_keys(&self, module_specifier: &str) -> Vec<String> {
        if !module_specifier.starts_with('.') {
            return vec![module_specifier.to_string()];
        }

        let Some(current_file_name) = self.current_file_path.as_deref().or_else(|| {
            self.current_arena
                .source_files
                .first()
                .map(|source_file| source_file.file_name.as_str())
        }) else {
            return vec![module_specifier.to_string()];
        };

        let current_dir = std::path::Path::new(current_file_name)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let joined = Self::normalize_path(current_dir.join(module_specifier));
        let mut keys = Vec::new();
        keys.push(joined.display().to_string());

        if joined.extension().is_none() {
            for ext in [".ts", ".tsx", ".mts", ".cts", ".d.ts", ".js", ".jsx"] {
                keys.push(format!("{}{}", joined.display(), ext));
            }
        }

        keys
    }

    fn normalize_path(path: std::path::PathBuf) -> std::path::PathBuf {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        parts.iter().collect()
    }

    fn symbol_has_type_query_value_meaning(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };
        if symbol.is_type_only {
            return false;
        }
        symbol
            .has_any_flags(tsz_binder::symbol_flags::VALUE | tsz_binder::symbol_flags::EXPORT_VALUE)
    }

    /// Analyze an entity name to extract the leftmost symbol.
    ///
    /// For `A.B.C`, we need to mark `A` as used (otherwise `import * as A` gets elided).
    pub(super) fn analyze_entity_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let kind = if self.in_value_pos {
                    UsageKind::VALUE
                } else {
                    UsageKind::TYPE
                };
                if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0) {
                    let should_mark = self.arena.get_identifier(name_node).is_none_or(|ident| {
                        !self.is_current_ambient_module_self_import(sym_id, &ident.escaped_text)
                    });
                    if should_mark {
                        self.mark_symbol_used(sym_id, kind);
                    }
                    if kind == UsageKind::TYPE
                        && self.binder.symbols.get(sym_id).is_some_and(|symbol| {
                            symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_PARAMETER)
                        })
                    {
                        return;
                    }
                }
                if let Some(ident) = self.arena.get_identifier(name_node) {
                    if kind == UsageKind::TYPE {
                        let mut matched_type_parameter = false;
                        for scope in self.binder.scopes.iter() {
                            if let Some(sym_id) = scope.table.get(&ident.escaped_text)
                                && self.binder.symbols.get(sym_id).is_some_and(|symbol| {
                                    symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_PARAMETER)
                                })
                            {
                                self.mark_symbol_used(sym_id, kind);
                                matched_type_parameter = true;
                            }
                        }
                        if matched_type_parameter {
                            return;
                        }
                    }
                    if let Some(&sym_id) = self.import_name_map.get(ident.escaped_text.as_str()) {
                        if !self.is_current_ambient_module_self_import(sym_id, &ident.escaped_text)
                        {
                            self.mark_symbol_used(sym_id, kind);
                        }
                    }
                    if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text) {
                        if !self.is_current_ambient_module_self_import(sym_id, &ident.escaped_text)
                        {
                            self.mark_symbol_used(sym_id, kind);
                        }
                    }
                    for scope in self.binder.scopes.iter() {
                        if let Some(sym_id) = scope.table.get(&ident.escaped_text) {
                            if !self
                                .is_current_ambient_module_self_import(sym_id, &ident.escaped_text)
                            {
                                self.mark_symbol_used(sym_id, kind);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(name_node) {
                    // A qualified-name prefix is a namespace/module qualifier,
                    // so a same-named type parameter must not hide the import
                    // needed to make the emitted `A.B` reference valid.
                    self.analyze_entity_name_qualifier(name.left);
                    self.analyze_entity_name(name.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(name_node) {
                    self.analyze_entity_name(access.expression);
                    self.analyze_entity_name(access.name_or_argument);
                }
            }
            _ => {}
        }
    }

    fn is_current_ambient_module_self_import(&self, sym_id: SymbolId, ident: &str) -> bool {
        let Some(module_specifier) = self.current_ambient_module_specifier.as_deref() else {
            return false;
        };
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return false;
        };
        if symbol.import_module() != Some(module_specifier) {
            return false;
        }

        // An unaliased `import { Observable } from "observable"` is redundant
        // inside `declare module "observable"` because the declaration body is
        // already scoped to that ambient module. Aliased imports still need to
        // count as usages because the alias is not introduced by the module body.
        symbol.import_name().unwrap_or(&symbol.escaped_name) == ident
            && symbol.escaped_name == ident
    }

    fn analyze_entity_name_qualifier(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let kind = if self.in_value_pos {
                    UsageKind::VALUE
                } else {
                    UsageKind::TYPE
                };
                if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0) {
                    self.mark_symbol_used(sym_id, kind);
                }
                if let Some(ident) = self.arena.get_identifier(name_node) {
                    if let Some(&sym_id) = self.import_name_map.get(ident.escaped_text.as_str()) {
                        self.mark_symbol_used(sym_id, kind);
                    }
                    if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text) {
                        self.mark_symbol_used(sym_id, kind);
                    }
                    for scope in self.binder.scopes.iter() {
                        if let Some(sym_id) = scope.table.get(&ident.escaped_text) {
                            self.mark_symbol_used(sym_id, kind);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(name_node) {
                    self.analyze_entity_name_qualifier(name.left);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(name_node) {
                    self.analyze_entity_name_qualifier(access.expression);
                }
            }
            _ => {}
        }
    }

    /// Mark a symbol as used in the public API.
    ///
    /// Categorizes symbols as:
    /// - Global/lib symbols: Ignored (don't need imports)
    /// - Local symbols: Added to `used_symbols` (for elision logic)
    /// - Foreign symbols: Added to both `used_symbols` AND `foreign_symbols` (for import generation)
    pub(super) fn mark_symbol_used(&mut self, sym_id: SymbolId, usage_kind: UsageKind) {
        debug!(
            "[DEBUG] mark_symbol_used: sym_id={:?}, usage_kind={:?}",
            sym_id, usage_kind
        );
        if self.binder.lib_symbol_ids.contains(&sym_id) {
            debug!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is lib symbol - skipping",
                sym_id
            );
            return;
        }

        let is_local = self.binder.symbols.get(sym_id).is_some_and(|symbol| {
            symbol.declarations.iter().any(|&decl_idx| {
                self.binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .and_then(|v| v.first())
                    .is_some_and(|arena| Arc::ptr_eq(arena, &self.current_arena))
            })
        });

        debug!(
            "[DEBUG] mark_symbol_used: sym_id={:?} is_local={}",
            sym_id, is_local
        );

        let previous_usage = self
            .used_symbols
            .get(&sym_id)
            .copied()
            .unwrap_or(UsageKind::NONE);
        let is_new = previous_usage == UsageKind::NONE;
        let usage_expanded = (usage_kind.is_type() && !previous_usage.is_type())
            || (usage_kind.is_value() && !previous_usage.is_value());
        self.used_symbols
            .entry(sym_id)
            .and_modify(|kind| *kind |= usage_kind)
            .or_insert(usage_kind);

        if !is_local {
            debug!(
                "[DEBUG] mark_symbol_used: sym_id={:?} is FOREIGN - adding to foreign_symbols",
                sym_id
            );
            self.foreign_symbols.insert(sym_id);
        }

        if is_new || usage_expanded {
            self.analyze_referenced_declaration_body(sym_id);
        }
    }

    fn analyze_referenced_declaration_body(&mut self, sym_id: SymbolId) {
        let decls = self
            .binder
            .symbols
            .get(sym_id)
            .map(|s| s.all_declarations())
            .unwrap_or_default();
        for decl_idx in decls {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            match decl_node.kind {
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    self.analyze_type_alias_declaration(decl_idx);
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    self.analyze_interface_declaration(decl_idx);
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.analyze_function_declaration(decl_idx);
                }
                // A non-exported class can enter the public-API surface by being
                // referenced from an exported declaration (e.g. an exported
                // function returns it, or it is the base of an exported class).
                // tsc then emits the class and, transitively, every local type
                // its members name. Without walking the class body here, those
                // member-type dependencies are dropped and the emitted
                // `declare class` references types whose declarations are gone.
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.analyze_class_declaration(decl_idx);
                }
                // Symmetric gap for a non-exported variable pulled in via
                // `typeof x`: its annotated/inferred type can name local types
                // that must survive elision.
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    self.analyze_variable_declaration(decl_idx);
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.analyze_module_declaration(decl_idx);
                }
                _ => {}
            }
        }
    }

    /// Get the set of foreign symbols that need imports.
    pub const fn get_foreign_symbols(&self) -> &FxHashSet<SymbolId> {
        &self.foreign_symbols
    }
}
