//! Lazy type-reference helpers for symbol resolution.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_solver::{DefId, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_actual_lib_name_to_def_id_for_cross_arena(
        &self,
        type_name: &str,
    ) -> Option<DefId> {
        Self::in_cross_arena_interface_delegation()
            .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
            .flatten()
    }

    /// Resolve a symbol to its structural type and return a `Lazy(DefId)` reference.
    ///
    /// This is the canonical stable-identity helper that consolidates the common
    /// two-step pattern:
    ///   1. `type_reference_symbol_type(sym_id)` ensures the body is materialized
    ///      in `type_env`.
    ///   2. `ctx.create_lazy_type_ref(sym_id)` creates `TypeData::Lazy(DefId)`.
    pub(crate) fn resolve_symbol_as_lazy_type(&mut self, sym_id: SymbolId) -> TypeId {
        let _ = self.type_reference_symbol_type(sym_id);
        self.ctx.create_lazy_type_ref(sym_id)
    }

    /// Resolve a named type reference to a lazy base while preserving canonical
    /// standard-lib identity across delegated checker contexts.
    pub(crate) fn resolve_symbol_as_lazy_type_named(
        &mut self,
        sym_id: SymbolId,
        name: &str,
    ) -> TypeId {
        if Self::in_cross_arena_interface_delegation()
            && self.ctx.has_lib_loaded()
            && self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
        {
            let _ = self.resolve_lib_type_by_name(name);
            let def_id = self.ctx.get_canonical_lib_def_id(name, sym_id);
            return self.ctx.types.lazy(def_id);
        }

        self.resolve_symbol_as_lazy_type(sym_id)
    }
}
