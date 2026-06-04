impl<'a> CheckerState<'a> {
    /// Check a variable statement by iterating through declaration lists.
    pub(crate) fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        self.check_variable_statement_with_request(stmt_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_variable_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(var) = self.ctx.arena.get_variable(node) {
            // VariableStatement.declarations contains VariableDeclarationList nodes
            for &list_idx in &var.declarations.nodes {
                self.check_variable_declaration_list_with_request(list_idx, request);
            }
        }
    }

    /// Check a variable declaration list (var/let/const x, y, z).
    ///
    /// Iterates through individual variable declarations in a list and
    /// validates each one.
    ///
    /// ## Parameters:
    /// - `list_idx`: The variable declaration list node index to check
    pub(crate) fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        self.check_variable_declaration_list_with_request(list_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_variable_declaration_list_with_request(
        &mut self,
        list_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };

        // Check if this is a using/await using declaration list.
        // Only check the USING bit (bit 2) — AWAIT_USING (6) = CONST (2) | USING (4),
        // so checking just the USING bit correctly matches both using and await using
        // but not const.
        use tsz_parser::parser::flags::node_flags;
        let flags_u32 = node.flags as u32;
        let is_using = (flags_u32 & node_flags::USING) != 0;
        let is_await_using = node_flags::is_await_using(flags_u32);

        if is_await_using {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

            if self.ctx.function_depth == 0 {
                // TS2853: Top-level 'await using' is only valid in modules.
                if !self.ctx.is_external_module_file() {
                    self.error_at_node(
                        list_idx,
                        diagnostic_messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL,
                        diagnostic_codes::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FIL,
                    );
                }

                // TS2854: Top-level 'await using' requires specific module + target options.
                // Routes through the environment capability boundary to determine whether
                // a diagnostic should be emitted.
                use crate::query_boundaries::capabilities::FeatureGate;
                if self
                    .ctx
                    .capabilities
                    .check_feature_gate(FeatureGate::TopLevelAwaitUsing)
                    .is_some()
                {
                    self.error_at_node(
                        list_idx,
                        diagnostic_messages::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                        diagnostic_codes::TOP_LEVEL_AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET,
                    );
                }
            } else if !self.enclosing_function_allows_await_using(list_idx) {
                // TS2852: Nested 'await using' is only valid inside async functions.
                self.error_at_node(
                    list_idx,
                    diagnostic_messages::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
                    diagnostic_codes::AWAIT_USING_STATEMENTS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LE,
                );
            }
        }

        // VariableDeclarationList uses the same VariableData structure
        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            // Now these are actual VariableDeclaration nodes
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration_with_request(decl_idx, request);

                // Check using/await using declarations have Symbol.dispose
                if is_using || is_await_using {
                    self.check_using_declaration_disposable(decl_idx, is_await_using);
                }
            }

            // TS2492: Check if let/const declarations inside a catch block shadow
            // the catch clause variable. `var` is allowed (different scoping), but
            // `let`/`const` are not.
            let is_let_or_const = node_flags::is_let_or_const(flags_u32) && !is_using;
            if is_let_or_const {
                self.check_catch_clause_variable_redeclaration(
                    list_idx,
                    &var_list.declarations.nodes,
                );
            }
        }
    }

    fn enclosing_function_allows_await_using(&self, idx: NodeIndex) -> bool {
        let Some(function_idx) = self.find_enclosing_function(idx) else {
            return false;
        };
        let Some(node) = self.ctx.arena.get(function_idx) else {
            return false;
        };

        self.ctx
            .arena
            .get_function(node)
            .is_some_and(|function| function.is_async)
            || self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|method| self.has_async_modifier(&method.modifiers))
            || self
                .ctx
                .arena
                .get_accessor(node)
                .is_some_and(|accessor| self.has_async_modifier(&accessor.modifiers))
    }

    /// TS2492: Check if any `let`/`const` declaration in a catch block shadows
    /// the catch clause variable name.
    ///
    /// In TypeScript, `try {} catch (x) { let x; }` is an error because the
    /// block-scoped `x` would shadow the catch clause binding `x`.
    fn check_catch_clause_variable_redeclaration(
        &mut self,
        list_idx: NodeIndex,
        declarations: &[NodeIndex],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Walk up: VarDeclList -> VarStatement -> Block -> CatchClause
        let var_stmt_idx = self
            .ctx
            .arena
            .get_extended(list_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        let block_idx = self
            .ctx
            .arena
            .get_extended(var_stmt_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        let catch_clause_idx = self
            .ctx
            .arena
            .get_extended(block_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);

        // Check if the ancestor is a CatchClause
        let Some(catch_node) = self.ctx.arena.get(catch_clause_idx) else {
            return;
        };
        if catch_node.kind != syntax_kind_ext::CATCH_CLAUSE {
            return;
        }
        let Some(catch_data) = self.ctx.arena.get_catch_clause(catch_node) else {
            return;
        };
        if catch_data.variable_declaration.is_none() {
            return;
        }

        // Get the catch clause variable name
        let catch_var_name = (|| {
            let var_node = self.ctx.arena.get(catch_data.variable_declaration)?;
            let var_decl = self.ctx.arena.get_variable_declaration(var_node)?;
            let name_node = self.ctx.arena.get(var_decl.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        })();
        let Some(catch_var_name) = catch_var_name else {
            return;
        };

        // Check each declaration in the list
        for &decl_idx in declarations {
            let decl_name = (|| {
                let decl_node = self.ctx.arena.get(decl_idx)?;
                let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                let name_node = self.ctx.arena.get(var_decl.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                Some((ident.escaped_text.clone(), var_decl.name))
            })();
            if let Some((name, name_idx)) = decl_name.filter(|(name, _)| name == &catch_var_name) {
                let message = format_message(
                    diagnostic_messages::CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE,
                    &[&name],
                );
                self.error_at_node(
                    name_idx,
                    &message,
                    diagnostic_codes::CANNOT_REDECLARE_IDENTIFIER_IN_CATCH_CLAUSE,
                );
            }
        }
    }

    // --- Using Declaration Validation (TS2804, TS2803) ---

    /// Check if a using/await using declaration's initializer type has the required dispose method.
    ///
    /// ## Parameters
    /// - `decl_idx`: The variable declaration node index
    /// - `is_await_using`: Whether this is an await using declaration
    ///
    /// Checks:
    /// - `using` requires type to have `[Symbol.dispose]()` method
    /// - `await using` requires type to have `[Symbol.asyncDispose]()` or `[Symbol.dispose]()` method
    fn check_using_declaration_disposable(&mut self, decl_idx: NodeIndex, is_await_using: bool) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        if self.ctx.arena.get(var_decl.name).is_some_and(|name| {
            name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        }) {
            return;
        }

        // Skip if no initializer
        if var_decl.initializer.is_none() {
            return;
        }

        // Get the type of the initializer
        let init_type = self.get_type_of_node(var_decl.initializer);

        // Skip error type and any (suppressed by convention)
        if init_type == TypeId::ERROR || init_type == TypeId::ANY {
            return;
        }

        // Check for the required dispose method
        if !self.type_has_disposable_method(init_type, is_await_using) {
            let (message, code) = if is_await_using {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                    diagnostic_codes::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SY,
                )
            } else {
                (
                    diagnostic_messages::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                    diagnostic_codes::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DI,
                )
            };
            self.error_at_node(var_decl.initializer, message, code);
        }
    }

    /// Check if a type has the appropriate dispose method.
    ///
    /// For `using`: checks for `[Symbol.dispose]()`
    /// For `await using`: checks for `[Symbol.asyncDispose]()` or `[Symbol.dispose]()`
    fn type_has_disposable_method(&mut self, type_id: TypeId, is_await_using: bool) -> bool {
        fn has_property(
            state: &mut CheckerState<'_>,
            type_id: TypeId,
            property_names: &[&str],
        ) -> bool {
            property_names.iter().any(|property_name| {
                matches!(
                    state.resolve_property_access_with_env(type_id, property_name),
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                        | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                            property_type: Some(_),
                            ..
                        }
                )
            })
        }

        // Check intrinsic types
        if type_id == TypeId::ANY
            || type_id == TypeId::UNKNOWN
            || type_id == TypeId::ERROR
            || type_id == TypeId::NEVER
        {
            return true; // Suppress errors on these types
        }

        // null and undefined can be disposed (no-op)
        if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
            return true;
        }

        // Only check for dispose methods if Symbol.dispose is available in the current environment
        // Check by looking for the dispose property on SymbolConstructor
        let symbol_type = self.type_of_value_symbol_by_name("Symbol");

        let symbol_has_dispose = has_property(self, symbol_type, &["dispose"]);

        let symbol_has_async_dispose = has_property(self, symbol_type, &["asyncDispose"]);

        // For await using, we need either Symbol.asyncDispose or Symbol.dispose
        if is_await_using && !symbol_has_async_dispose && !symbol_has_dispose {
            // Symbol.asyncDispose and Symbol.dispose are not available in this lib
            // Don't check for them (TypeScript will emit other errors about missing globals)
            return true;
        }

        // For regular using, we need Symbol.dispose
        if !is_await_using && !symbol_has_dispose {
            // Symbol.dispose is not available in this lib
            // Don't check for it
            return true;
        }

        // Check for the dispose method on the object type
        let has_dispose = has_property(self, type_id, &["[Symbol.dispose]", "Symbol.dispose"]);

        if is_await_using {
            // await using accepts either Symbol.asyncDispose or Symbol.dispose
            return has_dispose
                || has_property(
                    self,
                    type_id,
                    &["[Symbol.asyncDispose]", "Symbol.asyncDispose"],
                );
        }

        has_dispose
    }
}
