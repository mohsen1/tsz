use super::*;

impl<'a> CheckerState<'a> {
    pub(super) fn register_direct_source_file_type_alias_body_for_lowering(
        &self,
        def_id: DefId,
        delegate_binder: &BinderState,
        symbol_arena: &NodeArena,
        type_name: &str,
        decl_idx: NodeIndex,
    ) -> Option<()> {
        if self.ctx.definition_store.get_body(def_id).is_some() {
            return Some(());
        }
        let decl_node = symbol_arena.get(decl_idx)?;
        let type_alias = symbol_arena.get_type_alias(decl_node)?;
        if type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
            || Self::source_file_type_node_contains_kind(
                symbol_arena,
                type_alias.type_node,
                syntax_kind_ext::TYPE_QUERY,
            )
            || Self::source_file_type_node_contains_identifier_name(
                symbol_arena,
                type_alias.type_node,
                type_name,
            )
        {
            return None;
        }

        let mut seen_type_names = vec![type_name];
        if !Self::source_file_type_node_is_option_bag_lowerable(
            symbol_arena,
            delegate_binder,
            type_alias.type_node,
            &mut seen_type_names,
        ) {
            return None;
        }

        let name_resolver = |nested_name: &str| -> Option<DefId> {
            self.source_file_local_name_def_id_for_lowering(
                delegate_binder,
                symbol_arena,
                nested_name,
            )
        };
        let no_type_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let no_def_id = |_node_idx: NodeIndex| -> Option<DefId> { None };
        let no_value_symbol = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lazy_type_params_resolver =
            |nested_def_id: DefId| self.ctx.get_def_type_params(nested_def_id);
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
        .prefer_name_def_id_resolution();
        let (alias_type, params) = lowering.lower_type_alias_declaration(type_alias);
        if matches!(alias_type, TypeId::UNKNOWN | TypeId::ERROR) || !params.is_empty() {
            return None;
        }

        self.ctx
            .register_def_auto_params_in_envs(def_id, alias_type, params);
        self.ctx
            .definition_store
            .register_type_to_def(alias_type, def_id);
        Some(())
    }
}
