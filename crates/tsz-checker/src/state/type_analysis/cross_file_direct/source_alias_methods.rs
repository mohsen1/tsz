impl<'a> CheckerState<'a> {
    fn direct_lower_source_file_annotation_type(
        &self,
        annotation: NodeIndex,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
    ) -> Option<TypeId> {
        if Self::source_file_type_node_is_scope_independent(symbol_arena, annotation) {
            let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let no_def_id = |_node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> { None };
            let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
            let lowering = TypeLowering::with_hybrid_resolver(
                symbol_arena,
                self.ctx.types,
                &no_type_symbol,
                &no_def_id,
                &no_value_symbol,
            )
            .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type());
            let lowered = lowering.lower_type(annotation);
            return (lowered != TypeId::UNKNOWN && lowered != TypeId::ERROR).then_some(lowered);
        }

        let type_ref = symbol_arena
            .get(annotation)
            .and_then(|node| symbol_arena.get_type_ref(node))?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let name = symbol_arena
            .get(type_ref.type_name)
            .and_then(|name_node| symbol_arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())?;
        let target_sym_id = delegate_binder.file_locals.get(name)?;
        let target_symbol = delegate_binder.get_symbol(target_sym_id)?;
        if target_symbol.flags & symbol_flags::INTERFACE == 0 {
            return None;
        }

        let (_interface_type, _params) = self.direct_cross_file_interface_lowering(
            target_sym_id,
            delegate_binder,
            symbol_arena,
            false,
            true,
        )?;
        let def_id = self.ctx.get_or_create_def_id(target_sym_id);
        Some(self.ctx.types.lazy(def_id))
    }

    pub(super) fn direct_source_file_variable_annotation_type(
        &self,
        sym_id: SymbolId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        if !allow_source_file_arena || !is_direct_lowering_source_file_arena(symbol_arena) {
            return None;
        }
        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::VARIABLE == 0 {
            return None;
        }
        if symbol.flags & (symbol_flags::MODULE | symbol_flags::ALIAS) != 0 {
            return None;
        }
        if symbol.declarations.len() != 1 {
            return None;
        }

        let decl_idx = symbol.declarations[0];
        let decl_node = symbol_arena.get(decl_idx)?;
        let variable = symbol_arena.get_variable_declaration(decl_node)?;
        let annotation = variable.type_annotation.into_option()?;
        self.direct_lower_source_file_annotation_type(annotation, delegate_binder, symbol_arena)
    }

    pub(super) fn direct_source_file_variable_annotation_result(
        &self,
        sym_id: SymbolId,
        direct_target: Option<(&NodeArena, &BinderState, Option<usize>)>,
        allow_source_file_arena: bool,
    ) -> Option<TypeId> {
        let (symbol_arena, delegate_binder, _) = direct_target?;
        self.direct_source_file_variable_annotation_type(
            sym_id,
            delegate_binder,
            symbol_arena,
            allow_source_file_arena,
        )
    }

    pub(crate) fn direct_source_file_type_alias_result(
        &mut self,
        sym_id: SymbolId,
        target_file_idx: Option<usize>,
        allow_source_file_arena: bool,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let record = |outcome: DirectSourceFileTypeAliasLoweringOutcome| {
            record_direct_source_file_type_alias_lowering_outcome(outcome);
        };

        let target_file_idx = target_file_idx?;
        let (symbol_arena_arc, delegate_binder_arc) = {
            let symbol_arena_arc = self.ctx.all_arenas.as_ref()?.get(target_file_idx)?.clone();
            let delegate_binder_arc = self.ctx.all_binders.as_ref()?.get(target_file_idx)?.clone();
            (symbol_arena_arc, delegate_binder_arc)
        };
        let symbol_arena = symbol_arena_arc.as_ref();
        let delegate_binder = delegate_binder_arc.as_ref();
        let direct_source_file_arena =
            allow_source_file_arena && is_direct_lowering_source_file_arena(symbol_arena);
        let direct_external_declaration_arena = is_direct_lowering_declaration_arena(symbol_arena);
        if !direct_source_file_arena && !direct_external_declaration_arena {
            return None;
        }

        let symbol = delegate_binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return None;
        }
        if symbol.flags
            & (symbol_flags::VALUE
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE)
            != 0
        {
            return None;
        }
        if symbol.declarations.len() != 1 {
            return None;
        }

        let name = symbol.escaped_name.clone();
        let decl_idx = symbol.declarations[0];
        if !Self::lib_type_alias_declaration_name_matches(symbol_arena, decl_idx, &name) {
            return None;
        }
        let decl_node = symbol_arena.get(decl_idx)?;
        let type_alias = symbol_arena.get_type_alias(decl_node)?;
        let type_param_names = Self::type_alias_type_param_names(symbol_arena, type_alias);
        if direct_external_declaration_arena
            && Self::external_declaration_body_uses_local_array_shadow(
                symbol_arena,
                delegate_binder,
                type_alias.type_node,
            )
        {
            return None;
        }
        let body_is_direct_lowerable = {
            let global_type_is_lowerable = |binder: &BinderState, type_name: &str| {
                self.source_file_global_type_is_direct_lowerable(binder, type_name)
            };
            let import_alias_target = |source_file_idx: usize,
                                       binder: &BinderState,
                                       sym_id: SymbolId| {
                self.source_file_import_alias_target_for_lowering(source_file_idx, binder, sym_id)
            };
            let proof = super::cross_file_direct_alias_chain::SourceFileAliasProofContext {
                current_file_idx: Some(target_file_idx),
                global_type_is_lowerable: &global_type_is_lowerable,
                import_alias_target: Some(&import_alias_target),
            };
            let mut seen = Vec::new();
            if type_param_names.is_empty() {
                Self::source_file_type_node_is_scope_independent(symbol_arena, type_alias.type_node)
                    || (direct_source_file_arena
                        && Self::source_file_type_node_is_local_alias_chain_lowerable(
                            symbol_arena,
                            delegate_binder,
                            type_alias.type_node,
                            &mut seen,
                            &proof,
                        ))
            } else if direct_source_file_arena {
                Self::source_file_type_node_is_generic_local_alias_application_lowerable_with_seen(
                    symbol_arena,
                    delegate_binder,
                    type_alias.type_node,
                    &type_param_names,
                    &mut seen,
                    &proof,
                )
            } else {
                Self::source_file_type_node_is_generic_scope_independent(
                    symbol_arena,
                    type_alias.type_node,
                    &type_param_names,
                )
            }
        };
        if !body_is_direct_lowerable {
            self.record_source_alias_rejection_kinds_for_direct_proof(
                symbol_arena,
                delegate_binder,
                type_alias,
                target_file_idx,
                direct_source_file_arena,
                &type_param_names,
            );
            record(DirectSourceFileTypeAliasLoweringOutcome::BodyNotDirectLowerable);
            return None;
        }

        // Keep flow-sensitive `typeof` aliases and direct self/cycle cases on
        // the child-checker path, where diagnostics and resolution are handled.
        if Self::source_file_type_node_contains_kind(
            symbol_arena,
            type_alias.type_node,
            syntax_kind_ext::TYPE_QUERY,
        ) || Self::source_file_type_node_contains_identifier_name(
            symbol_arena,
            type_alias.type_node,
            &name,
        ) {
            record(DirectSourceFileTypeAliasLoweringOutcome::TypeQueryOrSelfReference);
            return None;
        }

        self.prime_source_file_alias_application_targets(
            symbol_arena,
            delegate_binder,
            type_alias.type_node,
            &mut Vec::new(),
        );

        let (alias_type, params) = self.lower_cross_arena_type_alias_declaration(
            sym_id,
            decl_idx,
            symbol_arena,
            type_alias,
        );
        let explicit_external_unknown_alias = direct_external_declaration_arena
            && type_param_names.is_empty()
            && Self::source_file_type_node_is_explicit_unknown(symbol_arena, type_alias.type_node);
        if alias_type == TypeId::ERROR
            || (alias_type == TypeId::UNKNOWN && !explicit_external_unknown_alias)
        {
            record(DirectSourceFileTypeAliasLoweringOutcome::UnknownOrError);
            return None;
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(shape) = crate::query_boundaries::state::type_environment::object_shape(
            self.ctx.types,
            alias_type,
        ) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
        self.ctx
            .register_def_auto_params_in_envs(def_id, alias_type, params.clone());
        self.ctx
            .definition_store
            .register_type_to_def(alias_type, def_id);

        record(DirectSourceFileTypeAliasLoweringOutcome::Success);
        Some((alias_type, params))
    }

    fn prime_source_file_alias_application_targets(
        &mut self,
        symbol_arena: &NodeArena,
        delegate_binder: &BinderState,
        root: NodeIndex,
        seen: &mut Vec<SymbolId>,
    ) {
        let Some(node) = symbol_arena.get(root) else {
            return;
        };
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = symbol_arena.get_type_ref(node)
            && let Some(args) = type_ref.type_arguments.as_ref()
            && !args.nodes.is_empty()
            && let Some(name) = symbol_arena
                .get(type_ref.type_name)
                .and_then(|name_node| symbol_arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.as_str())
            && let Some(sym_id) = delegate_binder.file_locals.get(name)
            && !seen.contains(&sym_id)
            && let Some(symbol) = delegate_binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::TYPE_ALIAS != 0
            && symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::CLASS
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE)
                == 0
            && symbol.declarations.len() == 1
            && let Some(decl_idx) = symbol.declarations.first().copied()
            && Self::lib_type_alias_declaration_name_matches(symbol_arena, decl_idx, name)
            && let Some(decl_node) = symbol_arena.get(decl_idx)
            && let Some(type_alias) = symbol_arena.get_type_alias(decl_node)
        {
            seen.push(sym_id);
            self.prime_source_file_alias_application_targets(
                symbol_arena,
                delegate_binder,
                type_alias.type_node,
                seen,
            );
            let (alias_type, params) = self.lower_cross_arena_type_alias_declaration(
                sym_id,
                decl_idx,
                symbol_arena,
                type_alias,
            );
            if alias_type != TypeId::UNKNOWN && alias_type != TypeId::ERROR {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                self.ctx
                    .register_def_auto_params_in_envs(def_id, alias_type, params);
                self.ctx
                    .definition_store
                    .register_type_to_def(alias_type, def_id);
            }
            seen.pop();
        }

        for child in symbol_arena.get_children(root) {
            self.prime_source_file_alias_application_targets(
                symbol_arena,
                delegate_binder,
                child,
                seen,
            );
        }
    }
}
