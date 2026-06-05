impl<'a> CheckerState<'a> {
    pub(super) fn source_file_alias_body_node_is_direct_lowerable_for_attribution(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        current_file_idx: usize,
        direct_source_file_arena: bool,
        type_param_names: &[String],
        node_idx: NodeIndex,
    ) -> bool {
        let global_type_is_lowerable = |binder: &BinderState, type_name: &str| {
            self.source_file_global_type_is_direct_lowerable(binder, type_name)
        };
        let local_type_shadows_global = |binder: &BinderState, type_name: &str| {
            self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                .is_some()
                && binder
                    .file_locals
                    .get(type_name)
                    .and_then(|sym_id| binder.get_symbol(sym_id).map(|symbol| (sym_id, symbol)))
                    .is_some_and(|(sym_id, symbol)| {
                        symbol.has_any_flags(symbol_flags::TYPE)
                            && Self::source_file_symbol_has_local_type_declaration(
                                arena, binder, sym_id, symbol,
                            )
                    })
        };
        let global_value_is_lowerable = |binder: &BinderState, value_name: &str| {
            self.source_file_global_value_is_direct_lowerable(binder, value_name)
        };
        let import_alias_target =
            |source_file_idx: usize, binder: &BinderState, sym_id: SymbolId| {
                self.source_file_import_alias_target_for_lowering(source_file_idx, binder, sym_id)
            };
        let proof = SourceFileAliasProofContext {
            current_file_idx: Some(current_file_idx),
            global_type_is_lowerable: &global_type_is_lowerable,
            local_type_shadows_global: &local_type_shadows_global,
            global_value_is_lowerable: &global_value_is_lowerable,
            import_alias_target: Some(&import_alias_target),
        };
        let mut seen = Vec::new();
        if type_param_names.is_empty() {
            Self::source_file_type_node_is_scope_independent(arena, node_idx)
                || (direct_source_file_arena
                    && Self::source_file_type_node_is_local_alias_chain_lowerable(
                        arena, binder, node_idx, &mut seen, &proof,
                    ))
        } else if direct_source_file_arena {
            Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                arena,
                binder,
                node_idx,
                type_param_names,
                &mut seen,
                &proof,
            )
        } else {
            Self::source_file_type_node_is_generic_scope_independent(
                arena,
                node_idx,
                type_param_names,
            )
        }
    }

    pub(super) fn record_source_alias_rejection_kinds_for_direct_proof(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        type_alias: &TypeAliasData,
        current_file_idx: usize,
        direct_source_file_arena: bool,
        type_param_names: &[String],
    ) {
        let type_node_is_lowerable = |node_idx| {
            self.source_file_alias_body_node_is_direct_lowerable_for_attribution(
                arena,
                binder,
                current_file_idx,
                direct_source_file_arena,
                type_param_names,
                node_idx,
            )
        };
        record_source_alias_rejection_kinds(
            arena,
            binder,
            type_alias,
            type_param_names,
            &type_node_is_lowerable,
        );
    }
}
