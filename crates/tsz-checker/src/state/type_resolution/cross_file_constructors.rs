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
        if !Self::enter_cross_arena_delegation() {
            return None;
        }
        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());
        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            arena.as_ref(),
            binder,
            self.ctx.types,
            file_name,
            self.ctx.compiler_options.clone(),
            self,
            CheckerCreationReason::DelegateCrossArenaOther,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to_attributed(
            &mut checker.ctx,
            CheckerCreationReason::DelegateCrossArenaOther,
        );
        checker.ctx.current_file_idx = file_idx;
        let instance_type =
            checker.synthesize_js_constructor_instance_type(value_decl, ctor_type, &[]);
        drop(checker);

        Self::leave_cross_arena_delegation();
        self.ctx.leave_recursion();
        instance_type
    }
}
