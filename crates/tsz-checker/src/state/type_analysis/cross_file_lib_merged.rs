//! Lib-merged symbol delegation (issue #15687).
//!
//! Symbols cloned into the program binder by `merge_lib_contexts_into_binder`
//! exist in no per-file binder: their remapped `SymbolId`s and lib-arena
//! declaration `NodeIndex`es are meaningless to the raw-id cross-file
//! machinery. These helpers translate a merged id back to its owning lib
//! context (via `lib_symbol_reverse_remap`) and run a child checker entirely
//! in that context's coordinates, sharing definition identity with the
//! parent's merged defs.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Prepare a delegated child checker that runs in a LIB binder's id
    /// space on behalf of a lib-MERGED parent symbol (issue #15687).
    ///
    /// The parent's copied cross-file state is keyed in the merged id space:
    /// the dynamic overlay and declaring-file index would route lib-local ids
    /// to unrelated files, the symbol-keyed caches would surface unrelated
    /// merged symbols' types, and fresh def minting would split definition
    /// identity from the parent's merged defs. Reset, clear, and seed
    /// accordingly (in that order — the collision clear would discard the
    /// seeded mappings).
    pub(super) fn prepare_lib_merged_delegation_child(
        &self,
        checker: &mut CheckerState<'_>,
        lib_binder: &std::sync::Arc<tsz_binder::BinderState>,
        preserve_sym: SymbolId,
    ) {
        *checker.ctx.cross_file_symbol_targets.borrow_mut() = Default::default();
        checker.ctx.global_symbol_file_index = None;
        self.clear_delegated_symbol_cache_collisions(checker, lib_binder.as_ref(), preserve_sym);
        let lib_binder_ptr = std::sync::Arc::as_ptr(lib_binder) as usize;
        let mut child_symbol_to_def = checker.ctx.symbol_to_def.borrow_mut();
        let mut child_def_to_symbol = checker.ctx.def_to_symbol.borrow_mut();
        for (&merged_id, &(ptr, local_id)) in self.ctx.binder.lib_symbol_reverse_remap.iter() {
            if ptr != lib_binder_ptr {
                continue;
            }
            let def_id = self.ctx.get_or_create_def_id(merged_id);
            if def_id != tsz_solver::def::DefId::INVALID {
                child_symbol_to_def.insert(local_id, def_id);
                child_def_to_symbol.insert(def_id, local_id);
            }
        }
    }

    /// Resolve a lib-MERGED symbol's type by delegating into its originating
    /// lib context under the lib-LOCAL id (issue #15687).
    ///
    /// A symbol merged by `merge_lib_contexts_into_binder` exists in no
    /// per-file binder, so the raw-id direct-lowering shortcuts and the
    /// generic child-checker path of `delegate_cross_arena_symbol_resolution`
    /// would interpret it in the wrong id space. Runs the child entirely in
    /// the owning lib context's coordinates; results are cached under the
    /// caller-visible merged id.
    pub(super) fn delegate_lib_merged_symbol_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let (lib_ctx, local_id) = self.lib_merged_symbol_origin(sym_id)?;
        // Only TYPE_ALIAS symbols take this path: classes delegate through
        // `delegate_cross_arena_class_instance_type` and interfaces through
        // the declaration-merge path in `compute_type_of_symbol`, both of
        // which already own lib-merged provenance. Intercepting them here
        // would bypass declaration merging (e.g. `JSX.ElementAttributesProperty`
        // under a `declare global` inside a module — false TS2607).
        if !lib_ctx
            .binder
            .get_symbol(local_id)
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS))
        {
            return None;
        }
        let lib_arena = std::sync::Arc::clone(&lib_ctx.arena);
        let lib_binder = std::sync::Arc::clone(&lib_ctx.binder);

        if let Some((cached_type, cached_params)) =
            self.ctx.lib_delegation_cache.symbol_type(sym_id)
        {
            return Some((cached_type, cached_params));
        }

        let Some(cross_arena_guard) = Self::enter_cross_arena_delegation() else {
            return Some((TypeId::ANY, Vec::new()));
        };
        if !self.ctx.enter_recursion() {
            Self::mark_cross_arena_bailout();
            drop(cross_arena_guard);
            return Some((TypeId::ANY, Vec::new()));
        }

        let delegate_file_name = lib_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());
        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            lib_arena.as_ref(),
            lib_binder.as_ref(),
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaSymbol,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        if let Some(file_idx) = self.ctx.get_file_idx_for_arena(lib_arena.as_ref()) {
            checker.ctx.current_file_idx = file_idx;
        }
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.symbol_types.remove(&local_id);
        checker.ctx.symbol_instance_types.remove(&local_id);
        self.prepare_lib_merged_delegation_child(&mut checker, &lib_binder, local_id);

        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        let result_type = checker.get_type_of_symbol(local_id);
        let mut result_params = if Self::cross_arena_bailout_epoch() == bailout_epoch_before {
            checker.get_type_params_for_symbol(local_id)
        } else {
            Vec::new()
        };
        let resolved_under_bailout = Self::cross_arena_bailout_epoch() != bailout_epoch_before;

        drop(checker);
        self.ctx.leave_recursion();
        drop(cross_arena_guard);

        if resolved_under_bailout {
            for param in &mut result_params {
                param.constraint = param.constraint.map(|_| TypeId::ANY);
                param.default = param.default.map(|_| TypeId::ANY);
            }
            return Some((TypeId::ANY, result_params));
        }
        if matches!(result_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }
        self.ctx.symbol_types.insert(sym_id, result_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (result_type, result_params.clone()));
        Some((result_type, result_params))
    }
}
