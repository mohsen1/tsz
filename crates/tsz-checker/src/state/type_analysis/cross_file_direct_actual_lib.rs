use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::parser::NodeIndex;
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
    fn value_merged_dom_interface_can_stay_type_position_lazy(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
    ) -> bool {
        symbol.declarations.iter().all(|&decl_idx| {
            let arena = self
                .ctx
                .binder
                .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
            let Some(interface) = arena
                .get(decl_idx)
                .and_then(|node| arena.get_interface(node))
            else {
                return true;
            };

            if interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|clauses| !clauses.nodes.is_empty())
            {
                return false;
            }

            interface.members.nodes.iter().all(|&member_idx| {
                let Some(member) = arena.get(member_idx) else {
                    return false;
                };
                match member.kind {
                    kind if kind == syntax_kind_ext::CALL_SIGNATURE
                        || kind == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                    {
                        false
                    }
                    kind if kind == syntax_kind_ext::METHOD_SIGNATURE => {
                        let Some(signature) = arena.get_signature(member) else {
                            return false;
                        };
                        signature.type_annotation != NodeIndex::NONE
                    }
                    _ => true,
                }
            })
        })
    }

    pub(crate) fn symbol_has_builtin_lib_declaration_provenance(
        &self,
        sym_id: SymbolId,
        symbol: &tsz_binder::Symbol,
        delegate_arena: &NodeArena,
    ) -> bool {
        !symbol.declarations.is_empty()
            && symbol.declarations.iter().all(|&decl_idx| {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    return !arenas.is_empty()
                        && arenas
                            .iter()
                            .all(|arena| is_builtin_lib_declaration_arena(arena.as_ref()));
                }

                is_builtin_lib_declaration_arena(delegate_arena)
            })
    }

    pub(super) fn direct_builtin_lib_interface_symbol_type(
        &mut self,
        sym_id: SymbolId,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let delegate_arena = delegate_arena?;
        if needs_cross_file_delegation
            || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || !is_builtin_lib_declaration_arena(delegate_arena)
        {
            return None;
        }

        let symbol = self.get_cross_file_symbol(sym_id)?.clone();
        if !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            && !self.symbol_has_builtin_lib_declaration_provenance(sym_id, &symbol, delegate_arena)
        {
            return None;
        }
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
        let delegate_arena = delegate_arena?;
        if needs_cross_file_delegation
            || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || !is_dom_builtin_lib_declaration_arena(delegate_arena)
        {
            return None;
        }

        let symbol = self.get_cross_file_symbol(sym_id)?.clone();
        if !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            && !self.symbol_has_builtin_lib_declaration_provenance(sym_id, &symbol, delegate_arena)
        {
            return None;
        }
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
        // DOM value/interface pairs used in type position can stay as lazy lib
        // identities only when their own member surface is resolvable by the
        // lazy member gateway and the interface has no heritage. Method
        // signatures are safe for member reads because
        // `resolve_simple_lib_interface_own_property` lowers unambiguous method
        // groups through `TypeLowering`, but relation checks for a type-position
        // `Lazy(DefId)` still need the full inherited DOM base closure. Until
        // that relation/materialization story is heritage-aware, inherited DOM
        // interfaces fall back to the existing full child/interface path.
        if !self.value_merged_dom_interface_can_stay_type_position_lazy(sym_id, &symbol) {
            return None;
        }

        let direct_type = self.resolve_lib_type_by_name(&name)?;
        if matches!(direct_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let params = self.get_type_params_for_symbol(sym_id);
        let def_id = self
            .resolve_actual_lib_name_to_def_id_for_lowering(&name)
            .unwrap_or_else(|| self.ctx.get_or_create_def_id(sym_id));
        self.ctx
            .register_def_auto_params_in_envs(def_id, direct_type, params.clone());
        let lazy_type = self.ctx.types.lazy(def_id);
        self.ctx.symbol_types.insert(sym_id, lazy_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (lazy_type, params.clone()));
        self.cache_shared_actual_lib_delegation(&name, lazy_type);
        Some((lazy_type, params))
    }
}
