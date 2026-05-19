use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, TypeParamInfo};

use super::cross_file_direct::{
    is_builtin_lib_file_name, is_external_package_declaration_file_name,
};

fn is_direct_type_alias_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file
            && (is_builtin_lib_file_name(&source_file.file_name)
                || is_external_package_declaration_file_name(&source_file.file_name))
    })
}

impl<'a> CheckerState<'a> {
    pub(super) fn direct_declaration_file_type_alias_delegation_result(
        &mut self,
        sym_id: SymbolId,
        cross_file_idx: Option<usize>,
        symbol_type_cache_file_idx: Option<usize>,
        source_cache_scope: u64,
        symbol_type_cache_from_symbol_arena: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let declaration_alias_arena = cross_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(file_idx).cloned())
            })
            .or_else(|| self.ctx.binder.symbol_arenas.get(&sym_id).cloned());
        let declaration_alias_file_idx = declaration_alias_arena
            .as_deref()
            .and_then(|arena| self.ctx.get_file_idx_for_arena(arena));
        let symbol_arena = declaration_alias_arena?;
        let (direct_type, direct_params) =
            self.direct_declaration_file_type_alias_result(sym_id, symbol_arena.as_ref())?;

        self.ctx.symbol_types.insert(sym_id, direct_type);
        if let Some(file_idx) = symbol_type_cache_file_idx.or(declaration_alias_file_idx)
            && direct_params.is_empty()
        {
            self.cache_symbol_arena_or_cross_file_symbol_type(
                sym_id,
                file_idx,
                source_cache_scope,
                symbol_type_cache_from_symbol_arena,
                direct_type,
                direct_params.clone(),
            );
        }
        Some((direct_type, direct_params))
    }

    pub(super) fn direct_declaration_file_type_alias_result(
        &mut self,
        sym_id: SymbolId,
        symbol_arena: &NodeArena,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if !is_direct_type_alias_declaration_arena(symbol_arena) {
            return None;
        }

        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?
            .clone();
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return None;
        }
        if symbol.flags
            & (symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE)
            != 0
        {
            return None;
        }

        let name = symbol.escaped_name.clone();
        let builtin_lib_alias = symbol_arena
            .source_files
            .first()
            .is_some_and(|source_file| is_builtin_lib_file_name(&source_file.file_name));
        if builtin_lib_alias && self.ctx.file_local_type_shadow_for_lib_name(&name) {
            return None;
        }

        let mut alias_decl = None;
        for &decl_idx in &symbol.declarations {
            if !Self::lib_type_alias_declaration_name_matches(symbol_arena, decl_idx, &name) {
                continue;
            }
            let Some(decl_node) = symbol_arena.get(decl_idx) else {
                continue;
            };
            let Some(type_alias) = symbol_arena.get_type_alias(decl_node) else {
                continue;
            };
            if alias_decl.replace((decl_idx, type_alias)).is_some() {
                return None;
            }
        }

        let (decl_idx, type_alias) = alias_decl?;
        if Self::source_file_type_node_contains_kind(
            symbol_arena,
            type_alias.type_node,
            syntax_kind_ext::TYPE_QUERY,
        ) || Self::source_file_type_node_contains_identifier_name(
            symbol_arena,
            type_alias.type_node,
            &name,
        ) {
            return None;
        }

        let alias_has_type_params = type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty());

        if builtin_lib_alias {
            let alias_contains_literal = Self::source_file_type_node_contains_kind(
                symbol_arena,
                type_alias.type_node,
                syntax_kind_ext::LITERAL_TYPE,
            );
            if alias_contains_literal {
                if alias_has_type_params {
                    return None;
                }
                let alias_type = self.resolve_lib_type_by_name(&name)?;
                if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR)
                    || crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias_type)
                        .is_none()
                {
                    return None;
                }
                self.ctx
                    .lib_delegation_cache
                    .insert_symbol_type(sym_id, (alias_type, Vec::new()));
                return Some((alias_type, Vec::new()));
            }

            let (alias_type, mut params) = self.resolve_lib_type_with_params(&name);
            let alias_type = alias_type?;
            if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR) {
                return None;
            }
            if params.is_empty() && alias_has_type_params {
                params = self.get_type_params_for_symbol(sym_id);
                if params.is_empty() {
                    return None;
                }
            }
            self.ctx
                .lib_delegation_cache
                .insert_symbol_type(sym_id, (alias_type, params.clone()));
            return Some((alias_type, params));
        }

        let (alias_type, params) = self.lower_cross_arena_type_alias_declaration(
            sym_id,
            decl_idx,
            symbol_arena,
            type_alias,
        );
        if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(shape) = crate::query_boundaries::state::type_environment::object_shape(
            self.ctx.types,
            alias_type,
        ) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
        self.ctx
            .register_def_auto_params_in_envs(def_id, alias_type, params.clone());
        self.ctx
            .definition_store
            .register_type_to_def(alias_type, def_id);
        Some((alias_type, params))
    }
}

#[cfg(test)]
#[path = "cross_file_direct_declaration_alias_tests.rs"]
mod tests;
