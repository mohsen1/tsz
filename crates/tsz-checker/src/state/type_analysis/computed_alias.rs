//! Helpers for computing type aliases in `compute_type_of_symbol`.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    pub(crate) fn lower_cross_arena_type_alias_declaration(
        &mut self,
        _sym_id: SymbolId,
        decl_idx: NodeIndex,
        decl_arena: &NodeArena,
        type_alias: &TypeAliasData,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        let binder = &self.ctx.binder;
        let lib_binders = self.get_lib_binders();
        let namespace_prefix = self.type_alias_namespace_prefix(decl_arena, decl_idx);
        let resolve_type_name = |name: &str| -> Option<SymbolId> {
            namespace_prefix
                .as_ref()
                .and_then(|prefix| {
                    let mut scoped = String::with_capacity(prefix.len() + 1 + name.len());
                    scoped.push_str(prefix);
                    scoped.push('.');
                    scoped.push_str(name);
                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                        .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                })
                .or_else(|| {
                    self.resolve_entity_name_text_to_def_id_for_lowering(name)
                        .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                })
        };
        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags & symbol_flags::TYPE) != 0).then_some(referenced_sym_id.0)
        };
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM))
                != 0)
                .then_some(referenced_sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let ident_name = decl_arena.get_identifier_text(node_idx)?;
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            let referenced_sym_id = resolve_type_name(ident_name)?;
            let symbol = binder.get_symbol_with_libs(referenced_sym_id, &lib_binders)?;
            ((symbol.flags & symbol_flags::TYPE) != 0)
                .then(|| self.ctx.get_or_create_def_id(referenced_sym_id))
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            namespace_prefix
                .as_ref()
                .and_then(|prefix| {
                    let mut scoped = String::with_capacity(prefix.len() + 1 + type_name.len());
                    scoped.push_str(prefix);
                    scoped.push('.');
                    scoped.push_str(type_name);
                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                })
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let lowering = TypeLowering::with_hybrid_resolver(
            decl_arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(self.get_type_param_bindings())
        .with_name_def_id_resolver(&name_resolver);

        lowering.lower_type_alias_declaration(type_alias)
    }

    fn type_alias_namespace_prefix(
        &self,
        decl_arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<String> {
        let mut parent = decl_arena
            .get_extended(decl_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);
        let mut prefixes = Vec::new();

        while !parent.is_none() {
            let parent_node = decl_arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = decl_arena.get_module(parent_node)
                && let Some(name_node) = decl_arena.get(module.name)
                && name_node.kind == SyntaxKind::Identifier as u16
                && let Some(name_ident) = decl_arena.get_identifier(name_node)
            {
                prefixes.push(name_ident.escaped_text.clone());
            }

            parent = decl_arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        (!prefixes.is_empty()).then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
    }
}
