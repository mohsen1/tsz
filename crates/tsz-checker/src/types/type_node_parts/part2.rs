impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Resolve a type symbol from a node index.
    /// Looks up the identifier in `file_locals` and `lib_contexts` for symbols with
    /// TYPE, `REGULAR_ENUM`, or `CONST_ENUM` flags. Returns the raw symbol ID (u32).
    /// Skips unshadowed compiler-managed types handled specially by `TypeLowering`.
    pub(crate) fn resolve_type_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::is_compiler_managed_type;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if self.ctx.type_parameter_scope.contains_key(name) {
            return None;
        }

        if is_compiler_managed_type(name) && !self.ctx.file_local_type_shadow_for_lib_name(name) {
            return None;
        }

        let scoped_name = {
            let node = self.ctx.arena.get(node_idx)?;
            if node.kind != SyntaxKind::Identifier as u16 {
                None
            } else {
                let mut prefixes = Vec::new();
                let mut parent = self
                    .ctx
                    .arena
                    .get_extended(node_idx)
                    .map_or(NodeIndex::NONE, |info| info.parent);

                while parent.is_some() {
                    let parent_node = self.ctx.arena.get(parent)?;
                    if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module) = self.ctx.arena.get_module(parent_node)
                        && let Some(name_node) = self.ctx.arena.get(module.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        prefixes.push(name_ident.escaped_text.clone());
                    }

                    parent = self
                        .ctx
                        .arena
                        .get_extended(parent)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                }

                if prefixes.is_empty() {
                    None
                } else {
                    prefixes.reverse();
                    prefixes.push(name.to_string());
                    Some(prefixes.join("."))
                }
            }
        };

        let scoped_sym_id = scoped_name
            .as_deref()
            .and_then(|qualified| self.resolve_entity_name_text_symbol(qualified));

        // Prefer lexical scope resolution so local type parameters shadow outer
        // file-level aliases/types with the same name.
        if let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.escaped_name != name {
                // NodeIndex values are arena-local. During cross-file type-node
                // lowering, a raw node id can accidentally find an unrelated
                // symbol in the current binder; ignore that collision and fall
                // through to name-based file/lib lookup.
            } else {
                if let Some(target_sym_id) = self.resolve_import_alias_type_target_symbol(sym_id) {
                    return Some(target_sym_id.0);
                }
                if let Some(scoped_sym_id) = scoped_sym_id
                    && scoped_sym_id != sym_id
                    && let Some(scoped_symbol) = self.get_symbol_from_any_context(scoped_sym_id)
                    && scoped_symbol.has_any_flags(
                        symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM,
                    )
                    && scoped_symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                    && !symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                {
                    return Some(scoped_sym_id.0);
                }
                if symbol.has_any_flags(
                    symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM,
                ) {
                    return Some(sym_id.0);
                }
            }
        }

        if let Some(scoped_sym_id) = scoped_sym_id
            && let Some(scoped_symbol) = self.get_symbol_from_any_context(scoped_sym_id)
            && (scoped_symbol.flags
                & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                != 0
        {
            self.ctx
                .register_symbol_file_target(scoped_sym_id, scoped_symbol.decl_file_idx as usize);
            return Some(scoped_sym_id.0);
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if let Some(target_sym_id) = self.resolve_import_alias_type_target_symbol(sym_id) {
                return Some(target_sym_id.0);
            }
            if symbol.escaped_name == name
                && (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
            {
                return Some(sym_id.0);
            }
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE | symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM))
                    != 0
                {
                    self.ctx
                        .register_symbol_file_target(lib_sym_id, symbol.decl_file_idx as usize);
                    return Some(lib_sym_id.0);
                }
            }
        }

        None
    }

    /// Resolve a value symbol from a node index (`file_locals` only).
    ///
    /// Looks for symbols with VALUE or ALIAS flags. Used by `type_reference` and
    /// `function_type` resolvers.
    pub(super) fn resolve_value_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        self.resolve_value_symbol_in_scope(node_idx)
            .map(|sym_id| sym_id.0)
    }

    pub(super) fn resolve_value_symbol_in_scope(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, node_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name == name
            && (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
        {
            return Some(sym_id);
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                return Some(sym_id);
            }
        }

        None
    }

    pub(super) fn declared_type_annotation_for_value_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;
        if decl.is_none() {
            decl = symbol.primary_declaration()?;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            return var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation);
        }
        if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            return param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation);
        }
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                return (param.name == decl && param.type_annotation.is_some())
                    .then_some(param.type_annotation);
            }
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
                return (var_decl.name == decl && var_decl.type_annotation.is_some())
                    .then_some(var_decl.type_annotation);
            }
        }
        None
    }

    pub(super) fn is_direct_typeof_annotation_for_symbol(
        &self,
        annotation_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(annotation_node) = self.ctx.arena.get(annotation_idx) else {
            return false;
        };
        if annotation_node.kind != syntax_kind_ext::TYPE_QUERY {
            return false;
        }
        let Some(type_query) = self.ctx.arena.get_type_query(annotation_node) else {
            return false;
        };
        self.ctx
            .binder
            .get_node_symbol(type_query.expr_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, type_query.expr_name)
            })
            == Some(sym_id)
    }

    /// Resolve a value symbol from a node index (`file_locals` + libs, with enum flags).
    ///
    /// Extended variant used by `compute_type` fallback and `mapped_type` resolvers
    /// that also checks `lib_contexts` and includes `REGULAR_ENUM/CONST_ENUM` flags.
    pub(crate) fn resolve_value_symbol_with_libs(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags
                & (symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM))
                != 0
        {
            return Some(sym_id.0);
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                && let Some(symbol) = lib_ctx.binder.get_symbol(lib_sym_id)
                && (symbol.flags
                    & (symbol_flags::VALUE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM))
                    != 0
            {
                self.ctx
                    .register_symbol_file_target(lib_sym_id, symbol.decl_file_idx as usize);
                return Some(lib_sym_id.0);
            }
        }

        None
    }

    /// Extract parameter information from a signature.
    fn extract_params_from_signature(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        use tsz_solver::ParamInfo;

        let mut params: Vec<ParamInfo> = Vec::new();
        let mut this_type = None;

        if let Some(ref param_list) = sig.parameters {
            for &param_idx in &param_list.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Get parameter name
                let name = self.get_param_name(param_data.name);

                // Check for 'this' parameter
                if name == "this" {
                    this_type = (param_data.type_annotation.is_some())
                        .then(|| self.check(param_data.type_annotation));
                    continue;
                }

                // Later parameter annotations can reference earlier value
                // parameters via `typeof`.
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.insert(name, param.type_id);
                    }
                }
                let type_id = if param_data.type_annotation.is_some() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.remove(&name);
                    }
                }

                let optional = param_data.question_token || param_data.initializer.is_some();
                let rest = param_data.dot_dot_dot_token;

                let sig_type_id = if param_data.question_token
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && type_id != TypeId::ERROR
                    && !crate::query_boundaries::common::type_contains_undefined(
                        self.ctx.types,
                        type_id,
                    ) {
                    self.ctx.types.factory().union2(type_id, TypeId::UNDEFINED)
                } else {
                    type_id
                };
                params.push(ParamInfo {
                    name: Some(self.ctx.types.intern_string(&name)),
                    type_id: sig_type_id,
                    optional,
                    rest,
                });
            }
        }

        (params, this_type)
    }

    /// Resolve return type annotation with parameter names in scope for `typeof`.
    ///
    /// Pushes parameter names into `typeof_param_scope` so that `typeof paramName`
    /// in the return type annotation resolves to the parameter's declared type.
    fn resolve_return_type_with_params_in_scope(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
    ) -> TypeId {
        if type_annotation.is_none() {
            return TypeId::ANY;
        }

        // Push param names into typeof_param_scope
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.insert(name, param.type_id);
            }
        }

        let return_type = self.check(type_annotation);

        // Clear typeof_param_scope
        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.remove(&name);
            }
        }

        return_type
    }

    /// Get parameter name from a binding name node.
    fn get_param_name(&self, name_idx: NodeIndex) -> String {
        if self
            .ctx
            .arena
            .get(name_idx)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
        {
            return "this".to_string();
        }
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) {
            return ident.escaped_text.to_string();
        }
        "_".to_string()
    }
}
