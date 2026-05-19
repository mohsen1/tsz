use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, TypeParamInfo};

use super::cross_file_direct::{
    is_builtin_lib_declaration_arena, is_dom_builtin_lib_declaration_arena,
};

pub(super) fn allow_generic_actual_lib_direct_fallback(name: &str) -> bool {
    matches!(
        name,
        "Array"
            | "ArrayIterator"
            | "Iterator"
            | "Map"
            | "MapIterator"
            | "Object"
            | "Promise"
            | "PromiseLike"
            | "RegExpStringIterator"
            | "Set"
            | "SetIterator"
            | "StringIterator"
            | "WeakMap"
            | "WeakSet"
    )
}

pub(super) fn allow_actual_lib_declaration_proof_bypass(name: &str) -> bool {
    matches!(name, "Iterator")
}

pub(super) fn is_direct_actual_lib_value_interface_name(name: &str) -> bool {
    matches!(
        name,
        "Array"
            | "Date"
            | "DateTimeFormatOptions"
            | "Error"
            | "Function"
            | "Iterator"
            | "IteratorObject"
            | "Locale"
            | "Map"
            | "NumberFormatOptions"
            | "NumberFormatOptionsCurrencyDisplayRegistry"
            | "NumberFormatOptionsSignDisplayRegistry"
            | "NumberFormatOptionsStyleRegistry"
            | "NumberFormatOptionsUseGroupingRegistry"
            | "NumberFormatPartTypeRegistry"
            | "NumberFormatRangePartTypeRegistry"
            | "Object"
            | "Promise"
            | "RegExp"
            | "Set"
            | "Symbol"
            | "WeakMap"
            | "WeakSet"
    )
}

pub(super) fn iterator_object_has_global_augmentations(
    ctx: &crate::context::CheckerContext<'_>,
) -> bool {
    if ctx
        .binder
        .global_augmentations
        .get("IteratorObject")
        .is_some_and(|augmentations| !augmentations.is_empty())
    {
        return true;
    }

    ctx.binder
        .file_locals
        .get("IteratorObject")
        .and_then(|sym_id| ctx.binder.get_symbol(sym_id))
        .is_some_and(|symbol| symbol.declarations.len() > 1)
}

impl<'a> CheckerState<'a> {
    pub(super) fn direct_builtin_lib_interface_symbol_type(
        &mut self,
        sym_id: SymbolId,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if needs_cross_file_delegation
            || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || !delegate_arena.is_some_and(is_builtin_lib_declaration_arena)
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
        {
            return None;
        }

        let symbol = self.get_cross_file_symbol(sym_id)?.clone();
        if symbol.flags & symbol_flags::INTERFACE == 0
            || symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                != 0
        {
            return None;
        }

        let name = symbol.escaped_name;
        if self.lib_name_locally_augmented(&name) {
            return None;
        }

        let (direct_type, params) = self.resolve_lib_type_with_params(&name);
        let direct_type = direct_type?;
        if matches!(direct_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        self.ctx.symbol_types.insert(sym_id, direct_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (direct_type, params.clone()));
        self.cache_shared_actual_lib_delegation(&name, direct_type);
        Some((direct_type, params))
    }

    pub(super) fn direct_value_merged_builtin_lib_interface_symbol_type(
        &mut self,
        sym_id: SymbolId,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if needs_cross_file_delegation
            || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || !delegate_arena.is_some_and(is_dom_builtin_lib_declaration_arena)
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
        {
            return None;
        }

        let symbol = self.get_cross_file_symbol(sym_id)?.clone();
        let has_value_interface =
            symbol.flags & symbol_flags::INTERFACE != 0 && symbol.flags & symbol_flags::VALUE != 0;
        if !has_value_interface
            || symbol.flags
                & (symbol_flags::CLASS
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                != 0
        {
            return None;
        }

        let name = symbol.escaped_name.clone();
        if self.lib_name_locally_augmented(&name) {
            return None;
        }
        let has_callable_member = symbol.declarations.iter().any(|&decl_idx| {
            let arena = self
                .ctx
                .binder
                .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
            arena
                .get(decl_idx)
                .and_then(|node| arena.get_interface(node))
                .is_some_and(|interface| {
                    interface.members.nodes.iter().any(|&member_idx| {
                        arena.get(member_idx).is_some_and(|member| {
                            member.kind == syntax_kind_ext::CALL_SIGNATURE
                                || member.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
                                || member.kind == syntax_kind_ext::METHOD_SIGNATURE
                        })
                    })
                })
        });
        if has_callable_member {
            let delegate_arena = delegate_arena?;
            let (mut direct_type, params) = self.direct_cross_file_interface_lowering(
                sym_id,
                self.ctx.binder,
                delegate_arena,
                true,
                false,
            )?;
            if matches!(direct_type, TypeId::UNKNOWN | TypeId::ERROR) {
                return None;
            }
            direct_type = self.merge_cross_file_heritage(&symbol.declarations, sym_id, direct_type);
            self.ctx.symbol_types.insert(sym_id, direct_type);
            self.ctx
                .lib_delegation_cache
                .insert_symbol_type(sym_id, (direct_type, params.clone()));
            return Some((direct_type, params));
        }

        let direct_type = self.resolve_lib_type_by_name(&name)?;
        if matches!(direct_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let params = self.get_type_params_for_symbol(sym_id);
        let def_id = self
            .resolve_actual_lib_name_to_def_id_for_lowering(&name)
            .unwrap_or_else(|| self.ctx.get_or_create_def_id(sym_id));
        if self.ctx.definition_store.get_body(def_id).is_none() {
            self.ctx
                .register_def_auto_params_in_envs(def_id, direct_type, params.clone());
        }
        let lazy_type = self.ctx.types.lazy(def_id);
        Some((lazy_type, params))
    }
}
