impl<'a> CheckerState<'a> {
    pub(super) fn redeclaration_initializer_request(
        &mut self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> TypingRequest {
        if !self.has_prior_value_declaration_for_symbol(decl_idx) {
            return TypingRequest::NONE;
        }

        let Some(init_node) = self.ctx.arena.get(
            self.ctx
                .arena
                .skip_parenthesized_and_assertions(initializer_idx),
        ) else {
            return TypingRequest::NONE;
        };
        // tsc does NOT propagate prior declaration types as contextual type for
        // call/new expressions. Generic inference in call expressions must rely on
        // argument types alone, not on the prior declaration's type. Providing
        // contextual type here would cause inference to succeed (e.g., T=Function
        // from contextual return type) when tsc would infer T=unknown, suppressing
        // TS2403 and potentially masking TS2345 argument errors.
        //
        // Contextual typing from prior declarations only applies to contextually
        // sensitive expressions (object/array literals, arrow/function expressions).
        let initializer_needs_context = matches!(
            init_node.kind,
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
        ) || is_contextually_sensitive(self, initializer_idx);
        if !initializer_needs_context {
            return TypingRequest::NONE;
        }

        let Some(cached_type) = self.cached_inferred_variable_type(decl_idx, name_idx) else {
            return TypingRequest::NONE;
        };
        if matches!(cached_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return TypingRequest::NONE;
        }

        TypingRequest::with_contextual_type(self.contextual_type_for_expression(cached_type))
    }

    pub(super) fn checked_js_remote_class_declared_type_for_variable(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || self.ctx.binder.is_external_module()
        {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let name = self
            .ctx
            .arena
            .get_identifier_at(var_decl.name)?
            .escaped_text
            .clone();

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx || binder.is_external_module() {
                continue;
            }
            let arena = all_arenas.get(file_idx)?;
            let source_file = arena.source_files.first()?;

            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    continue;
                }
                let Some(class_decl) = arena.get_class(stmt_node) else {
                    continue;
                };
                let Some(ident) = arena.get_identifier_at(class_decl.name) else {
                    continue;
                };
                if ident.escaped_text != name || !arena.is_in_ambient_context(stmt_idx) {
                    continue;
                }
                let Some(sym_id) = binder.get_node_symbol(stmt_idx) else {
                    continue;
                };
                self.ctx.register_symbol_file_index(sym_id, file_idx);
                let base_type = self.get_type_of_symbol(sym_id);
                // When a JS-file `const X = ...` merges with a TS-file
                // `declare class X`, fold JS-side expando assignments
                // (`X.prop = ...`) into the merged static type so the
                // initializer assignability check sees every property tsc
                // reports as missing (TS2739, not TS2741).
                return Some(self.augment_callable_type_with_expandos(&name, sym_id, base_type));
            }
        }

        None
    }

    pub(super) fn maybe_clear_checked_initializer_type_cache(
        &mut self,
        initializer_idx: NodeIndex,
    ) {
        // Some initializer forms are first visited during build_type_environment, where we only
        // want a stable type shape. The later checked pass must revisit them so body/member
        // diagnostics (for example TS2454 inside class-expression methods or TS2564 on class
        // fields) are emitted from the canonical checked path.
        if let Some(init_node) = self.ctx.arena.get(initializer_idx)
            && matches!(
                init_node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::NEW_EXPRESSION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            )
        {
            self.invalidate_initializer_for_context_change(initializer_idx);
        }
    }
}
