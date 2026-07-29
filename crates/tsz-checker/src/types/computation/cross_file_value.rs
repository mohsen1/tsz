//! Cross-file value-declaration and runtime function-group resolution.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Resolve a value declaration in another file's binder/arena without
    /// consulting the symbol-type cache, whose merged-symbol entry can be the
    /// type-side rather than value-side type.
    pub(crate) fn type_of_value_declaration_for_cross_file_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
        target_file_idx: usize,
    ) -> TypeId {
        self.cross_file_value_type_with_mode(sym_id, decl_idx, target_file_idx, false)
    }

    /// Resolve the complete runtime function declaration group for a symbol in
    /// another file, excluding any same-name interface declaration space.
    pub(crate) fn type_of_function_group_for_cross_file_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
        target_file_idx: usize,
    ) -> TypeId {
        self.cross_file_value_type_with_mode(sym_id, decl_idx, target_file_idx, true)
    }

    fn cross_file_value_type_with_mode(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
        target_file_idx: usize,
        resolve_function_group: bool,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::ERROR;
        }

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        // A grouped overload value differs from the single declaration cache.
        let declaration_cache_mode = if resolve_function_group { 3 } else { 1 };
        if let Some(cached) = self.ctx.lib_delegation_cache.declaration_node_type(
            target_arena,
            decl_idx,
            declaration_cache_mode,
        ) {
            return cached;
        }

        let Some(_cross_arena_guard) = Self::enter_cross_arena_delegation() else {
            return TypeId::ANY;
        };
        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        let delegate_file_name = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());
        let delegate_binder = self
            .ctx
            .get_binder_for_arena(target_arena)
            .unwrap_or(self.ctx.binder);

        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();
        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            target_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
            tsz_common::perf_counters::CheckerCreationReason::CallHelpers,
        ));
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.current_file_idx = target_file_idx;
        checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
        checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
        checker
            .ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get());

        // Raw `SymbolId`s are binder-local; clear inherited colliding entries.
        checker.ctx.symbol_types.remove(&sym_id);
        checker.ctx.symbol_instance_types.remove(&sym_id);
        for &owned_sym_id in delegate_binder.node_symbols.values() {
            checker.ctx.symbol_types.remove(&owned_sym_id);
            checker.ctx.symbol_instance_types.remove(&owned_sym_id);
        }
        for (_, &owned_sym_id) in delegate_binder.file_locals.iter() {
            checker.ctx.symbol_types.remove(&owned_sym_id);
            checker.ctx.symbol_instance_types.remove(&owned_sym_id);
        }

        let mut result = if resolve_function_group {
            checker
                .function_declaration_only_symbol_type(sym_id)
                .unwrap_or_else(|| checker.type_of_value_declaration_with_mode(decl_idx, true))
        } else {
            checker.type_of_value_declaration_with_mode(decl_idx, true)
        };
        if !resolve_function_group
            && result.is_unknown_or_error()
            && let Some(node) = target_arena.get(decl_idx)
            && let Some(var_decl) = target_arena.get_variable_declaration(node)
            && var_decl.initializer.is_some()
        {
            result = checker.get_type_of_node(var_decl.initializer);
        }

        let result_is_bailout_artifact = Self::cross_arena_bailout_epoch() != bailout_epoch_before;
        if !result_is_bailout_artifact && !matches!(result, TypeId::ERROR | TypeId::UNKNOWN) {
            self.ctx.lib_delegation_cache.insert_declaration_node_type(
                target_arena,
                decl_idx,
                declaration_cache_mode,
                result,
            );
        }
        result
    }
}
