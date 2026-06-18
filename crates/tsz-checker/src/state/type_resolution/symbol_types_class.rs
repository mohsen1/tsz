//! Class-specific helpers for symbol-level type resolution.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Type-position result for a class symbol whose resolution fell back to
    /// the value side.
    ///
    /// `get_type_of_symbol` returns the CONSTRUCTOR type for classes. A
    /// type-position reference (`x: Cls`, `field: Cls<T>`) must resolve to the
    /// INSTANCE type instead; serving the constructor flips instance/constructor
    /// identity depending on which side was computed first. That is
    /// deterministically wrong for classes declared in cross-file `.d.ts`
    /// modules once a co-included root has populated the value-side caches
    /// (#13185). Uses the `symbol_instance_types` entry the value-side
    /// computation registered; returns `None` (keeping the legacy result) when
    /// no instance type is available.
    pub(crate) fn class_type_position_result(
        &mut self,
        sym_id: SymbolId,
        value_type: TypeId,
        value_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let is_class = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
            .is_some_and(|symbol| {
                symbol.has_any_flags(symbol_flags::CLASS)
                    && !symbol.has_any_flags(symbol_flags::TYPE_ALIAS | symbol_flags::INTERFACE)
            });
        if !is_class {
            return None;
        }

        let instance_type = self
            .ctx
            .symbol_instance_types
            .get(&sym_id)
            .copied()
            .filter(|&t| !t.is_any_unknown_or_error())?;
        if instance_type == value_type {
            return None;
        }

        let params = if value_params.is_empty() {
            self.ctx
                .get_existing_def_id(sym_id)
                .and_then(|def_id| self.ctx.get_def_type_params(def_id))
                .unwrap_or_default()
        } else {
            value_params.to_vec()
        };
        Some((instance_type, params))
    }
}
