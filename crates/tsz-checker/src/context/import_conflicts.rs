//! Namespace import conflict helpers.

use std::sync::Arc;

use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    pub(crate) fn is_conflicted_namespace_import_alias(
        &self,
        sym_id: SymbolId,
        name: &str,
        lib_binders: &[Arc<BinderState>],
    ) -> bool {
        if !self.import_conflict_names.contains(name) {
            return false;
        }

        let Some(symbol) = self.binder.get_symbol_with_libs(sym_id, lib_binders) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return false;
        }

        symbol.declarations.iter().copied().any(|decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::NAMESPACE_IMPORT)
        })
    }

    pub(crate) fn namespace_import_alias_has_local_namespace_conflict(
        &self,
        symbol: &tsz_binder::Symbol,
    ) -> bool {
        if !symbol.has_any_flags(symbol_flags::ALIAS)
            || !self
                .import_conflict_names
                .contains(symbol.escaped_name.as_str())
        {
            return false;
        }

        let mut has_namespace_import = false;
        let mut has_local_namespace = false;
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                has_namespace_import = true;
                continue;
            }
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module) = self.arena.get_module(node) else {
                continue;
            };
            let is_identifier_name = self
                .arena
                .get(module.name)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::Identifier as u16);
            if is_identifier_name {
                has_local_namespace = true;
            }
        }

        has_namespace_import && has_local_namespace
    }

    pub(crate) fn local_namespace_symbol_for_conflicted_namespace_import(
        &self,
        idx: NodeIndex,
        name: &str,
        alias_sym_id: SymbolId,
        lib_binders: &[Arc<BinderState>],
    ) -> Option<SymbolId> {
        if !self.is_conflicted_namespace_import_alias(alias_sym_id, name, lib_binders) {
            return None;
        }

        let usable_local_namespace = |sym_id: SymbolId| {
            let symbol = self.binder.get_symbol(sym_id)?;
            let has_local_namespace = symbol.declarations.iter().copied().any(|decl_idx| {
                let Some(node) = self.arena.get(decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    return false;
                }
                let Some(module) = self.arena.get_module(node) else {
                    return false;
                };
                self.arena
                    .get(module.name)
                    .is_some_and(|name_node| name_node.kind == SyntaxKind::Identifier as u16)
            });
            has_local_namespace.then_some(sym_id)
        };

        if let Some(sym_id) = usable_local_namespace(alias_sym_id) {
            return Some(sym_id);
        }

        let reference_scope = self.binder.find_enclosing_scope(self.arena, idx);
        for &sym_id in self.binder.symbols.find_all_by_name(name) {
            if sym_id == alias_sym_id {
                continue;
            }
            let Some(symbol) = self.binder.get_symbol(sym_id) else {
                continue;
            };
            let has_namespace_in_reference_scope = symbol.declarations.iter().any(|&decl_idx| {
                let Some(node) = self.arena.get(decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    return false;
                }
                let Some(module) = self.arena.get_module(node) else {
                    return false;
                };
                let is_identifier_name = self
                    .arena
                    .get(module.name)
                    .is_some_and(|name_node| name_node.kind == SyntaxKind::Identifier as u16);
                if !is_identifier_name {
                    return false;
                }
                let decl_scope = self.binder.find_enclosing_scope(self.arena, decl_idx);
                reference_scope.is_none() || decl_scope == reference_scope
            });
            if has_namespace_in_reference_scope {
                return Some(sym_id);
            }
        }

        None
    }
}
