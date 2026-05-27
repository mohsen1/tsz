impl<'a> CheckerState<'a> {
    pub(super) fn direct_cross_file_interface_lowering(
        &self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_complex_declarations: bool,
        allow_source_file_arena: bool,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let record = |outcome: DirectCrossFileInterfaceLoweringOutcome| {
            tsz_common::perf_counters::record_direct_cross_file_interface_lowering_outcome(outcome);
        };

        // Source and local test-fixture interfaces need exact binder-local symbol
        // resolution for diagnostics. Built-in libs may use this path only when
        // the declaration-shape guard below proves they do not need the mature
        // merged/heritage checker path.
        let direct_declaration_arena = is_direct_lowering_declaration_arena(symbol_arena)
            || is_builtin_lib_declaration_arena(symbol_arena);
        let direct_source_file_arena =
            allow_source_file_arena && is_direct_lowering_source_file_arena(symbol_arena);
        if !direct_declaration_arena && !direct_source_file_arena {
            record(DirectCrossFileInterfaceLoweringOutcome::RejectedNonDirectArena);
            return None;
        }

        let Some(symbol) = delegate_binder.get_symbol(sym_id) else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingSymbol);
            return None;
        };
        let disallowed_merge_flags = symbol_flags::CLASS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        if symbol.flags & symbol_flags::INTERFACE == 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::NotInterface);
            return None;
        }
        if symbol.flags & disallowed_merge_flags != 0 {
            record(DirectCrossFileInterfaceLoweringOutcome::DisallowedMergeFlags);
            return None;
        }

        let Some(declarations) =
            self.cross_file_interface_declarations(sym_id, delegate_binder, symbol_arena)
        else {
            record(DirectCrossFileInterfaceLoweringOutcome::MissingDeclarations);
            return None;
        };
        let has_heritage = Self::interface_declarations_have_heritage(&declarations);
        let has_computed_names = Self::interface_declarations_have_computed_names(&declarations);
        if direct_source_file_arena {
            if has_heritage
                || has_computed_names
                || !Self::source_file_interface_declarations_are_direct_lowerable(
                    &declarations,
                    delegate_binder,
                )
            {
                record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
                return None;
            }
        } else if !allow_complex_declarations && (has_heritage || has_computed_names) {
            record(DirectCrossFileInterfaceLoweringOutcome::ComplexDeclaration);
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            if direct_source_file_arena {
                return self.source_file_local_name_def_id_for_lowering(
                    delegate_binder,
                    symbol_arena,
                    type_name,
                );
            }
            (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                .flatten()
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            symbol_arena,
            self.ctx.types,
            &no_type_symbol,
            &no_def_id,
            &no_value_symbol,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_preferred_self_reference(symbol.escaped_name.clone(), def_id)
        .prefer_name_def_id_resolution();

        let (interface_type, params) =
            lowering.lower_merged_interface_declarations_with_symbol(&declarations, Some(sym_id));
        if interface_type == TypeId::UNKNOWN || interface_type == TypeId::ERROR {
            record(DirectCrossFileInterfaceLoweringOutcome::UnknownOrError);
            return None;
        }
        record(DirectCrossFileInterfaceLoweringOutcome::Success);

        if !params.is_empty() {
            self.ctx.insert_def_type_params(def_id, params.clone());
        }
        self.ctx.definition_store.set_body(def_id, interface_type);
        self.ctx
            .definition_store
            .register_type_to_def(interface_type, def_id);
        Some((interface_type, params))
    }

    pub(super) fn direct_cross_file_interface_member_simple_types(
        &mut self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
        interface_arena: &NodeArena,
        delegate_binder: &BinderState,
        type_args: Option<&[TypeId]>,
        allow_source_file_arena: bool,
    ) -> Option<rustc_hash::FxHashMap<NodeIndex, TypeId>> {
        let direct_member_arena = is_direct_actual_lib_declaration_arena(interface_arena)
            || is_direct_lowering_declaration_arena(interface_arena)
            || (allow_source_file_arena && is_direct_lowering_source_file_arena(interface_arena));
        if direct_member_arena {
            let direct_source_file_arena =
                allow_source_file_arena && is_direct_lowering_source_file_arena(interface_arena);
            let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                if direct_source_file_arena {
                    return self.source_file_local_name_def_id_for_lowering(
                        delegate_binder,
                        interface_arena,
                        type_name,
                    );
                }
                self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
            };
            let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
            let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let lazy_type_params_resolver =
                |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
            let lowering = TypeLowering::with_hybrid_resolver(
                interface_arena,
                self.ctx.types,
                &no_type_symbol,
                &no_def_id,
                &no_value_symbol,
            )
            .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
            .with_name_def_id_resolver(&name_resolver)
            .with_lazy_type_params_resolver(&lazy_type_params_resolver)
            .prefer_name_def_id_resolution();
            let (params, lowered_members) =
                lowering.lower_interface_members_simple_types(interface_idx, member_indices)?;
            let substitution = type_args
                .filter(|type_args| !params.is_empty() && type_args.len() <= params.len())
                .and_then(|type_args| {
                    crate::query_boundaries::type_defaults::fill_application_defaults(
                        self.ctx.types,
                        type_args,
                        &params,
                    )
                })
                .map(|type_args| {
                    crate::query_boundaries::type_rewrite::TypeSubstitution::from_args(
                        self.ctx.types,
                        &params,
                        &type_args,
                    )
                });

            let mut results = rustc_hash::FxHashMap::default();
            for (member_idx, mut member_type) in lowered_members {
                if matches!(member_type, TypeId::UNKNOWN | TypeId::ERROR) {
                    return None;
                }
                if let Some(substitution) = substitution.as_ref() {
                    member_type = crate::query_boundaries::type_rewrite::instantiate_type(
                        self.ctx.types,
                        member_type,
                        substitution,
                    );
                }
                if matches!(member_type, TypeId::UNKNOWN | TypeId::ERROR) {
                    return None;
                }
                results.insert(member_idx, member_type);
            }

            return (!results.is_empty()).then_some(results);
        }

        let sym_id = delegate_binder.get_node_symbol(interface_idx).or_else(|| {
            let arena_ptr = interface_arena as *const NodeArena as usize;
            self.ctx
                .cross_file_node_symbols_for_arena(delegate_binder, arena_ptr)
                .and_then(|symbols| symbols.get(&interface_idx.0).copied())
        })?;

        let (interface_type, params) = self.direct_cross_file_interface_lowering(
            sym_id,
            delegate_binder,
            interface_arena,
            true,
            allow_source_file_arena,
        )?;

        let substitution = type_args
            .filter(|type_args| !params.is_empty() && type_args.len() <= params.len())
            .and_then(|type_args| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    self.ctx.types,
                    type_args,
                    &params,
                )
            })
            .map(|type_args| {
                crate::query_boundaries::type_rewrite::TypeSubstitution::from_args(
                    self.ctx.types,
                    &params,
                    &type_args,
                )
            });

        let mut results = rustc_hash::FxHashMap::default();
        for &member_idx in member_indices {
            let Some(member_node) = interface_arena.get(member_idx) else {
                continue;
            };
            let name_idx = interface_arena
                .get_signature(member_node)
                .map(|signature| signature.name)
                .or_else(|| {
                    interface_arena
                        .get_accessor(member_node)
                        .map(|accessor| accessor.name)
                });
            let Some(name) = name_idx.and_then(|idx| {
                crate::types_domain::queries::core::get_literal_property_name(interface_arena, idx)
            }) else {
                continue;
            };
            let atom = self.ctx.types.intern_string(&name);
            let Some(mut member_type) = crate::query_boundaries::property_access::raw_property_type(
                self.ctx.types,
                interface_type,
                atom,
            ) else {
                continue;
            };
            if let Some(substitution) = substitution.as_ref() {
                member_type = crate::query_boundaries::type_rewrite::instantiate_type(
                    self.ctx.types,
                    member_type,
                    substitution,
                );
            }
            if member_type != TypeId::UNKNOWN && member_type != TypeId::ERROR {
                results.insert(member_idx, member_type);
            }
        }

        (!results.is_empty()).then_some(results)
    }
}
