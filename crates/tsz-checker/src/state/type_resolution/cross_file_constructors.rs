//! Cross-file JS constructor-function base helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::perf_counters::CheckerCreationReason;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn cross_file_js_constructor_instance_type(
        &mut self,
        sym_id: SymbolId,
        ctor_type: TypeId,
    ) -> Option<TypeId> {
        let file_idx = self.ctx.resolve_symbol_file_index(sym_id)?;
        if file_idx == self.ctx.current_file_idx {
            return None;
        }
        let arena = self.ctx.all_arenas.as_ref()?.get(file_idx)?.clone();
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let symbol = binder.get_symbol(sym_id)?;
        let value_decl = self
            .checked_js_constructor_value_declaration(
                sym_id,
                symbol.value_declaration,
                &symbol.declarations,
            )
            .unwrap_or(symbol.value_declaration);
        if value_decl.is_none() {
            return None;
        }
        let cross_arena_guard = Self::enter_cross_arena_delegation()?;
        if !self.ctx.enter_recursion() {
            return None;
        }

        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());
        let mut checker = CheckerState::delegate_for_arena(
            arena.as_ref(),
            binder,
            file_name,
            self,
            CheckerCreationReason::DelegateCrossArenaOther,
        );
        checker.ctx.current_file_idx = file_idx;
        let instance_type =
            checker.synthesize_js_constructor_instance_type(value_decl, ctor_type, &[]);
        drop(checker);

        drop(cross_arena_guard);
        self.ctx.leave_recursion();
        instance_type
    }
}
