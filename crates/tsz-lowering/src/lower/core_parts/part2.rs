impl<'a> TypeLowering<'a> {
    /// Internal implementation that optionally stamps the interface type with a `SymbolId`.
    fn lower_interface_declarations_with_params_impl(
        &self,
        declarations: &[NodeIndex],
        symbol_id: Option<tsz_binder::SymbolId>,
    ) -> (TypeId, Vec<TypeParamInfo>) {
        if declarations.is_empty() {
            return (TypeId::ERROR, Vec::new());
        }

        let mut parts = InterfaceParts::new();
        let mut type_params: Option<&NodeList> = None;
        let mut found = false;

        for &decl_idx in declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            found = true;
            if type_params.is_none() {
                type_params = interface.type_parameters.as_ref();
            }
        }

        if !found {
            return (TypeId::ERROR, Vec::new());
        }

        let collected_params = if let Some(params) = type_params {
            self.push_type_param_scope();
            let collected = self.collect_type_parameters(params);
            self.pop_type_param_scope();
            collected
        } else {
            Vec::new()
        };

        let saved_type_param_scopes = self.type_param_scopes.borrow().clone();
        *self.type_param_scopes.borrow_mut() = Vec::new();

        // Process declarations in reverse order: TypeScript's interface merging
        // rule puts later declarations' members first for overload resolution.
        // E.g., PromiseConstructor from es2015.iterable (earlier) and es2015.promise
        // (later) — the tuple overload from es2015.promise should be tried first.
        let num_declarations = declarations.len();
        for (rev_i, &decl_idx) in declarations.iter().rev().enumerate() {
            let forward_decl_index = num_declarations - 1 - rev_i;
            parts.set_declaration_pass(forward_decl_index);

            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(node) else {
                continue;
            };
            if let Some(params) = &interface.type_parameters
                && !params.nodes.is_empty()
            {
                self.push_type_param_scope();
                let _ = self.collect_type_parameters(params);
                self.collect_interface_members(&interface.members, &mut parts);
                self.pop_type_param_scope();
            } else {
                self.collect_interface_members(&interface.members, &mut parts);
            }
        }

        *self.type_param_scopes.borrow_mut() = saved_type_param_scopes;

        // Assign declaration_order in FORWARD declaration order for diagnostics.
        // The reverse iteration above is needed for overload resolution priority,
        // but TS2740 "missing properties" messages should list properties in the
        // order they first appear across declarations (earliest declaration first).
        self.assign_forward_declaration_order(&mut parts, declarations.iter().copied());

        (
            self.finish_interface_parts(parts, symbol_id),
            collected_params,
        )
    }

    pub fn lower_type_alias_declaration(
        &self,
        alias: &TypeAliasData,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        if let Some(params) = alias.type_parameters.as_ref()
            && !params.nodes.is_empty()
        {
            self.push_type_param_scope();
            let collected_params = self.collect_type_parameters(params);
            let result = self.lower_type(alias.type_node);
            self.pop_type_param_scope();
            return (result, collected_params);
        }

        (self.lower_type(alias.type_node), Vec::new())
    }
}
