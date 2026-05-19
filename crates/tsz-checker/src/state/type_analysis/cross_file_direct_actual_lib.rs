use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::{TypeId, TypeParamInfo};

use super::cross_file_direct::is_builtin_lib_declaration_arena;

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
}
