use super::type_node::TypeNodeChecker;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Resolve a DefId from a node index via the type resolver.
    ///
    /// Uses the stable-identity helper `ensure_def_id_with_alias` to mint
    /// the DefId and ensure type alias body+params are registered.
    fn resolve_def_id(&self, node_idx: NodeIndex) -> Option<tsz_solver::def::DefId> {
        let sym_id_raw = self.resolve_type_symbol(node_idx)?;
        let sym_id = tsz_binder::SymbolId(sym_id_raw);
        let def_id = if let Some(ident) = self.ctx.arena.get_identifier_at(node_idx) {
            if self
                .ctx
                .type_parameter_scope
                .contains_key(ident.escaped_text.as_str())
            {
                return None;
            }
            if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || self.ctx.symbol_is_from_lib(sym_id)
            {
                self.ctx
                    .get_canonical_lib_def_id(ident.escaped_text.as_str(), sym_id)
            } else {
                self.ctx
                    .get_or_create_def_id_for_symbol_name(sym_id, ident.escaped_text.as_str())
            }
        } else {
            self.ensure_def_id_with_alias(sym_id)
        };
        if !self.ctx.symbol_resolution_set.contains(&sym_id) {
            self.ensure_type_alias_resolved(sym_id, def_id);
        }
        Some(def_id)
    }

    /// Collect type parameter bindings from the current scope.
    fn collect_type_param_bindings(&self) -> Vec<(tsz_common::interner::Atom, TypeId)> {
        self.ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect()
    }

    /// Run `TypeLowering` with the standard resolvers (type + value + `def_id`).
    ///
    /// This is the common path used by `compute_type` fallback, `type_reference`,
    /// `function_type`, and `mapped_type`. The `use_extended_value_resolver` flag
    /// controls whether enum flags and lib search are included in value resolution.
    /// The `use_qualified_names` flag enables qualified name support in `def_id` resolution.
    pub(crate) fn lower_with_resolvers(
        &self,
        idx: NodeIndex,
        use_extended_value_resolver: bool,
        use_qualified_names: bool,
    ) -> TypeId {
        self.lower_with_resolvers_impl(idx, use_extended_value_resolver, use_qualified_names, None)
    }

    /// Walk the AST subtree rooted at `idx`, resolve all `TYPE_REFERENCE` nodes
    /// whose `type_name` is or starts with an `import()` `CALL_EXPRESSION`, and
    /// return the results keyed by `type_name` `NodeIndex`.
    ///
    /// This pre-pass runs with `&mut self` (required for module resolution) before
    /// the immutable `lower_with_resolvers` closure context is created. The caller
    /// passes the resulting map so that `TypeLowering` can pick up the pre-resolved
    /// types via the `import_type_resolver` callback.
    pub(crate) fn collect_import_type_overrides(
        &mut self,
        idx: NodeIndex,
    ) -> FxHashMap<NodeIndex, TypeId> {
        // Skip the subtree walk for leaf-only nodes that structurally cannot contain
        // TYPE_REFERENCE nodes (and therefore cannot have import() type refs).
        if let Some(node) = self.ctx.arena.get(idx)
            && (node.kind == syntax_kind_ext::INFER_TYPE
                || node.kind == syntax_kind_ext::LITERAL_TYPE)
        {
            return FxHashMap::default();
        }
        let mut map = FxHashMap::default();
        self.collect_import_types_recursive(idx, &mut map, 0);
        map
    }

    fn collect_import_types_recursive(
        &mut self,
        idx: NodeIndex,
        map: &mut FxHashMap<NodeIndex, TypeId>,
        depth: u32,
    ) {
        // Guard against adversarially deep nesting (matches codebase consensus of 64
        // for AST recursive walkers).
        if depth > 64 {
            return;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        match node.kind {
            // These node kinds structurally cannot contain a TYPE_REFERENCE whose
            // type_name roots in an import() call.
            k if k == syntax_kind_ext::INFER_TYPE || k == syntax_kind_ext::LITERAL_TYPE => {}
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(resolved) = self.import_call_type_reference(type_ref.type_name)
                {
                    map.insert(type_ref.type_name, resolved);
                }
                let type_arguments = self
                    .ctx
                    .arena
                    .get_type_ref(node)
                    .and_then(|type_ref| type_ref.type_arguments.as_ref())
                    .map(|args| args.nodes.clone());
                if let Some(type_arguments) = type_arguments {
                    for child_idx in type_arguments {
                        self.collect_import_types_recursive(child_idx, map, depth + 1);
                    }
                }
            }
            _ => {
                for child_idx in self.ctx.arena.get_children(idx) {
                    self.collect_import_types_recursive(child_idx, map, depth + 1);
                }
            }
        }
    }

    pub(crate) fn lower_with_resolvers_impl(
        &self,
        idx: NodeIndex,
        use_extended_value_resolver: bool,
        use_qualified_names: bool,
        import_overrides: Option<&FxHashMap<NodeIndex, TypeId>>,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;

        let type_param_bindings = self.collect_type_param_bindings();

        let type_resolver =
            |node_idx: NodeIndex| -> Option<u32> { self.resolve_type_symbol(node_idx) };

        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            if use_extended_value_resolver {
                self.resolve_value_symbol_with_libs(node_idx)
            } else {
                self.resolve_value_symbol(node_idx)
            }
        };

        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            if use_qualified_names {
                self.resolve_def_id_with_qualified_names(node_idx)
            } else {
                self.resolve_def_id(node_idx)
            }
        };

        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
        let name_def_id_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
            if !type_name.contains('.') && self.ctx.type_parameter_scope.contains_key(type_name) {
                return None;
            }

            let expected_name = type_name.rsplit('.').next().unwrap_or(type_name);

            if !type_name.contains('.')
                && let Some(sym_id) = self.ctx.binder.file_locals.get(type_name)
                && let Some(sym_id) = self.resolve_import_alias_type_target_symbol(sym_id)
            {
                let def_id = self
                    .ctx
                    .get_or_create_def_id_for_symbol_name(sym_id, expected_name);
                if !self.ctx.symbol_resolution_set.contains(&sym_id) {
                    self.ensure_type_alias_resolved(sym_id, def_id);
                }
                return Some(def_id);
            }

            if !type_name.contains('.')
                && let Some(sym_id) = self.ctx.binder.file_locals.get(type_name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == type_name
                && symbol.decl_file_idx != u32::MAX
            {
                let sym_id = self
                    .resolve_import_alias_type_target_symbol(sym_id)
                    .unwrap_or(sym_id);
                let def_id = self
                    .ctx
                    .get_or_create_def_id_for_symbol_name(sym_id, expected_name);
                if !self.ctx.symbol_resolution_set.contains(&sym_id) {
                    self.ensure_type_alias_resolved(sym_id, def_id);
                }
                return Some(def_id);
            }

            if !type_name.contains('.') {
                for lib_ctx in self.ctx.lib_contexts.iter() {
                    if let Some(sym_id) = lib_ctx.binder.file_locals.get(type_name)
                        && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
                        && symbol.escaped_name == type_name
                    {
                        return Some(self.ctx.get_canonical_lib_def_id(type_name, sym_id));
                    }
                }
            }

            let sym_id = self.resolve_entity_name_text_symbol(type_name)?;
            let def_id = self
                .ctx
                .get_or_create_def_id_for_symbol_name(sym_id, expected_name);
            if !self.ctx.symbol_resolution_set.contains(&sym_id) {
                self.ensure_type_alias_resolved(sym_id, def_id);
            }
            Some(def_id)
        };
        let type_query_override = |expr_name_idx: NodeIndex| -> Option<TypeId> {
            if let Some(expr_node) = self.ctx.arena.get(expr_name_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                && let Some(&param_type) =
                    self.ctx.typeof_param_scope.get(ident.escaped_text.as_str())
            {
                return Some(param_type);
            }

            if let Some(tuple_type) = self.const_asserted_array_tuple_type_query(expr_name_idx) {
                return Some(tuple_type);
            }

            if let Some(property_type) = self.value_property_type_query(expr_name_idx) {
                return Some(property_type);
            }

            let type_query_idx = self.ctx.arena.get_extended(expr_name_idx)?.parent;
            let type_query_node = self.ctx.arena.get(type_query_idx)?;
            if type_query_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY
                && crate::types_domain::type_node_helpers::is_type_query_in_non_flow_sensitive_signature_parameter(
                    self.ctx.arena,
                    type_query_idx,
                )
                && let Some(sym_id) = self.resolve_value_symbol_in_scope(expr_name_idx)
                && let Some(annotation_idx) =
                    self.declared_type_annotation_for_value_symbol(sym_id)
                && !self.is_direct_typeof_annotation_for_symbol(annotation_idx, sym_id)
            {
                let annotation_lowering = TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_strict_null_checks(self.ctx.strict_null_checks())
                .with_name_def_id_resolver(&name_def_id_resolver)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver);
                let resolved = annotation_lowering.lower_type(annotation_idx);
                if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    return Some(resolved);
                }
            }
            None
        };

        let mut lowering = TypeLowering::with_hybrid_resolver(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_strict_null_checks(self.ctx.strict_null_checks())
        .with_name_def_id_resolver(&name_def_id_resolver)
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
        .with_type_query_override(&type_query_override);
        if use_qualified_names {
            lowering = lowering.prefer_name_def_id_resolution();
        }
        if !type_param_bindings.is_empty() {
            lowering = lowering.with_type_param_bindings(type_param_bindings);
        }
        // Wire in pre-resolved import type references. The closure is declared here
        // so its lifetime covers `lowering.lower_type(idx)`.
        let import_type_resolver = import_overrides.filter(|m| !m.is_empty()).map(|overrides| {
            move |type_name_idx: NodeIndex| -> Option<TypeId> {
                overrides.get(&type_name_idx).copied()
            }
        });
        if let Some(ref resolver) = import_type_resolver {
            lowering = lowering.with_import_type_resolver(resolver);
        }
        lowering.lower_type(idx)
    }
}
