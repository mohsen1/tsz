impl<'a> UsageAnalyzer<'a> {
    /// Create a new usage analyzer.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        type_cache: &'a TypeCacheView,
        type_interner: &'a TypeInterner,
        current_arena: Arc<NodeArena>,
        current_file_path: Option<String>,
        import_name_map: &'a FxHashMap<String, SymbolId>,
        source_flags: UsageAnalyzerSourceFlags,
    ) -> Self {
        Self {
            arena,
            binder,
            type_cache,
            type_interner,
            import_name_map,
            used_symbols: FxHashMap::default(),
            visited_nodes: FxHashSet::default(),
            visited_types: FxHashSet::default(),
            type_symbol_cache: FxHashMap::default(),
            memoizing_types: FxHashSet::default(),
            current_arena,
            current_file_path,
            source_is_js_file: source_flags.source_is_js_file,
            source_is_declaration_file: source_flags.source_is_declaration_file,
            foreign_symbols: FxHashSet::default(),
            in_value_pos: false,
            current_ambient_module_specifier: None,
        }
    }

    /// Analyze all exported declarations in a source file.
    ///
    /// Returns the map of `SymbolIds` to their usage kinds that are referenced in the public API.
    pub fn analyze(&mut self, root_idx: NodeIndex) -> &FxHashMap<SymbolId, UsageKind> {
        let Some(root_node) = self.arena.get(root_idx) else {
            return &self.used_symbols;
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return &self.used_symbols;
        };

        // Walk all statements to find exported declarations
        for &stmt_idx in &source_file.statements.nodes {
            self.analyze_statement(stmt_idx);
        }

        &self.used_symbols
    }

    /// Analyze a single statement to find exported declarations.
    fn analyze_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            // Exported declarations - only analyze if they have the Export modifier
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(stmt_node)
                    && self
                        .arena
                        .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_function_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && self
                        .arena
                        .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_class_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if self.arena.get_interface(stmt_node).is_some_and(|iface| {
                    !self.binder.is_external_module()
                        || self
                            .arena
                            .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword)
                }) {
                    self.analyze_interface_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(stmt_node)
                    && self
                        .arena
                        .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_type_alias_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_data) = self.arena.get_enum(stmt_node)
                    && self
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_enum_declaration(stmt_idx);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                    && self
                        .arena
                        .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                {
                    self.analyze_variable_statement(stmt_idx);
                }
            }
            // Export declarations - check if clause contains a declaration to analyze
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_node) = self.arena.get(stmt_idx)
                    && let Some(export) = self.arena.get_export_decl(export_node)
                    && export.export_clause.is_some()
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                {
                    match clause_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.analyze_function_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::CLASS_DECLARATION => {
                            self.analyze_class_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                            self.analyze_interface_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                            self.analyze_type_alias_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::ENUM_DECLARATION => {
                            self.analyze_enum_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                            self.analyze_variable_statement(export.export_clause);
                        }
                        k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                            self.analyze_import_equals_declaration(export.export_clause);
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            self.analyze_module_declaration(export.export_clause);
                        }
                        // Named exports: export { x, y as z }
                        // Mark each specifier's local name as used
                        k if k == syntax_kind_ext::NAMED_EXPORTS => {
                            self.analyze_named_exports(export.export_clause);
                        }
                        // Identifier reference: export default <Identifier>
                        // Mark the referenced declaration as used so it's included in .d.ts.
                        // We look up via file_locals rather than node_symbols because
                        // node_symbols maps this reference to the export symbol, not the
                        // underlying declaration symbol.
                        k if k == SyntaxKind::Identifier as u16 => {
                            if let Some(ident) = self.arena.get_identifier(clause_node)
                                && let Some(sym_id) =
                                    self.binder.file_locals.get(&ident.escaped_text)
                            {
                                self.mark_symbol_used(sym_id, UsageKind::VALUE | UsageKind::TYPE);
                            }
                        }
                        // Default export with expression: export default new A()
                        // Prefer the construct return type as the public surface.
                        // Fall back to the constructor reference only when type
                        // info cannot expose an instance type.
                        k if k == syntax_kind_ext::NEW_EXPRESSION => {
                            self.analyze_export_default_new_expression(export.export_clause);
                        }
                        k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                            self.analyze_export_default_object_literal_value_references(
                                export.export_clause,
                            );
                        }
                        k if k == syntax_kind_ext::CALL_EXPRESSION => {}
                        _ => {}
                    }
                }
            }
            // Export assignment: export default expr
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.analyze_export_assignment(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.analyze_commonjs_assignment_public_surface(stmt_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if self.module_declaration_contributes_public_surface(stmt_idx) {
                    self.analyze_module_declaration(stmt_idx);
                }
            }
            _ => {}
        }
    }

    fn module_declaration_contributes_public_surface(&self, module_idx: NodeIndex) -> bool {
        let Some(module_node) = self.arena.get(module_idx) else {
            return false;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return false;
        };

        if self.source_is_declaration_file || !self.binder.is_external_module() {
            return true;
        }
        if self.is_ambient_module_body_name(module.name) {
            return true;
        }

        self.statement_has_export_modifier(module_node)
    }

    fn analyze_module_declaration(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let previous_ambient_module_specifier = self.current_ambient_module_specifier.clone();
        if let Some(module_specifier) = string_literal_text(self.arena, module.name) {
            self.current_ambient_module_specifier = Some(module_specifier);
        }

        if let Some(body_node) = self.arena.get(module.body) {
            if let Some(module_block) = self.arena.get_module_block(body_node) {
                if let Some(ref stmts) = module_block.statements {
                    for &stmt_idx in &stmts.nodes {
                        if self.is_ambient_module_body_name(module.name) {
                            self.analyze_ambient_module_member_statement(stmt_idx);
                        } else if self.source_is_declaration_file
                            || self.namespace_statement_contributes_public_surface(stmt_idx)
                        {
                            self.analyze_statement(stmt_idx);
                        }
                    }
                }
            } else if let Some(_nested_module) = self.arena.get_module(body_node) {
                self.analyze_module_declaration(module.body);
            }
        }

        self.current_ambient_module_specifier = previous_ambient_module_specifier;
    }

    fn namespace_statement_contributes_public_surface(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            || stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            || stmt_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
        {
            return true;
        }
        self.statement_has_export_modifier(stmt_node)
    }

    fn statement_has_export_modifier(&self, stmt_node: &tsz_parser::parser::node::Node) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(stmt_node).is_some_and(|func| {
                    self.arena
                        .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(stmt_node).is_some_and(|class| {
                    self.arena
                        .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(stmt_node).is_some_and(|iface| {
                    self.arena
                        .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(stmt_node).is_some_and(|alias| {
                    self.arena
                        .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(stmt_node).is_some_and(|enum_data| {
                    self.arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(stmt_node).is_some_and(|var_stmt| {
                    self.arena
                        .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(stmt_node).is_some_and(|module| {
                    self.arena
                        .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl(stmt_node)
                .is_some_and(|import_decl| {
                    self.arena
                        .has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword)
                }),
            _ => false,
        }
    }

    fn analyze_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Mark the RHS namespace/type/value as used by this declaration.
        if import.module_specifier.is_some() {
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(import.module_specifier);
            self.in_value_pos = old;
        }
    }

    /// Analyze named exports: `export { x, y as z }`.
    /// Marks each specifier's local binding as used so non-exported declarations
    /// referenced by the export clause survive into .d.ts output.
    fn analyze_named_exports(&mut self, clause_idx: NodeIndex) {
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return;
        };
        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return;
        };
        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                continue;
            };
            // The local name is `property_name` if it exists, otherwise `name`
            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            // Mark the local symbol as used (both type and value, since
            // we don't know which side of the export is being consumed)
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(local_name_idx);
            self.in_value_pos = old;
            // Also mark as type usage
            self.analyze_entity_name(local_name_idx);
        }
    }

    /// Analyze export assignment: `export default expr`.
    fn analyze_export_assignment(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(export_assign) = self.arena.get_export_assignment(stmt_node) else {
            return;
        };
        // Mark the expression as used (could be a type or value reference)
        if export_assign.expression.is_some() {
            let expr_idx = export_assign.expression;
            if self
                .arena
                .get(expr_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION)
            {
                self.analyze_export_default_new_expression(expr_idx);
                return;
            }
            let expr_idx = self.unwrap_export_default_expression(expr_idx);
            self.analyze_reference_as_value_and_type(expr_idx);
            self.analyze_export_default_object_literal_value_references(expr_idx);
        }
    }

    fn analyze_export_default_new_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return;
        };
        if self.walk_construct_return_type_for_new_expression(new_expr.expression) {
            return;
        }
        self.analyze_entity_name(new_expr.expression);
    }

    fn walk_construct_return_type_for_new_expression(&mut self, callee_idx: NodeIndex) -> bool {
        let Some(constructor_type) = self
            .type_cache
            .node_types
            .get(&callee_idx.0)
            .copied()
            .or_else(|| self.symbol_cached_type(callee_idx))
        else {
            return false;
        };
        let Some(return_type) = tsz_solver::type_queries::construct_return_type_for_type(
            self.type_interner,
            constructor_type,
        ) else {
            return false;
        };
        if matches!(
            return_type,
            tsz_solver::TypeId::ANY | tsz_solver::TypeId::UNKNOWN | tsz_solver::TypeId::ERROR
        ) {
            return false;
        }
        self.walk_type_id(return_type);
        true
    }

    fn symbol_cached_type(&self, node_idx: NodeIndex) -> Option<tsz_solver::TypeId> {
        let sym_id = self.binder.node_symbols.get(&node_idx.0)?;
        self.type_cache.symbol_types.get(sym_id).copied()
    }

    fn analyze_export_default_object_literal_value_references(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(object) = self.arena.get_literal_expr(expr_node) else {
            return;
        };

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let value_idx = if let Some(data) = self.arena.get_shorthand_property(member_node) {
                data.name
            } else if let Some(data) = self.arena.get_property_assignment(member_node) {
                data.initializer
            } else {
                continue;
            };

            if self.initializer_preserves_value_reference(value_idx) {
                self.analyze_reference_as_value_and_type(value_idx);
            }
        }
    }

    fn analyze_reference_as_value_and_type(&mut self, expr_idx: NodeIndex) {
        let old = self.in_value_pos;
        self.in_value_pos = true;
        self.analyze_entity_name(expr_idx);
        self.analyze_local_import_equals_dependency(expr_idx);
        self.in_value_pos = false;
        self.analyze_entity_name(expr_idx);
        self.analyze_local_import_equals_dependency(expr_idx);
        self.in_value_pos = old;
    }

    /// Unwrap `new X()` and `X()` expressions to find the constructor/callee
    /// reference for dependency tracking in default exports.
    fn unwrap_export_default_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return expr_idx;
        };
        // `export default new A()` → track `A`
        if (expr_node.kind == syntax_kind_ext::NEW_EXPRESSION
            || expr_node.kind == syntax_kind_ext::CALL_EXPRESSION)
            && let Some(call) = self.arena.get_call_expr(expr_node)
        {
            return call.expression;
        }
        expr_idx
    }

    fn analyze_local_import_equals_dependency(&mut self, name_idx: NodeIndex) {
        let mut seen_symbols = FxHashSet::default();
        self.analyze_local_import_equals_dependency_inner(name_idx, &mut seen_symbols);
    }

    fn analyze_local_import_equals_dependency_inner(
        &mut self,
        name_idx: NodeIndex,
        seen_symbols: &mut FxHashSet<SymbolId>,
    ) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.arena.get_identifier(name_node) else {
            return;
        };
        // Prefer lexical, scope-aware resolution of this exact reference node:
        // it resolves to the in-scope binding (e.g. the namespace-local
        // `import c = x.c` alias) rather than the ambiguous result of a global
        // name-only scan that could match a same-named declaration in an
        // unrelated scope (such as the underlying class `x.c`). Fall back to
        // the bound node symbol and then name lookup when scope resolution is
        // unavailable.
        let sym_id = if let Some(sym_id) = self.binder.resolve_identifier(self.arena, name_idx) {
            sym_id
        } else if let Some(sym_id) = self.binder.get_node_symbol(name_idx) {
            sym_id
        } else if let Some(sym_id) = self.binder.file_locals.get(&ident.escaped_text) {
            sym_id
        } else {
            let mut found = None;
            for scope in self.binder.scopes.iter() {
                if let Some(sym_id) = scope.table.get(&ident.escaped_text) {
                    found = Some(sym_id);
                    break;
                }
            }
            let Some(sym_id) = found else {
                return;
            };
            sym_id
        };
        if !seen_symbols.insert(sym_id) {
            return;
        }
        let declarations = {
            let Some(symbol) = self.binder.symbols.get(sym_id) else {
                return;
            };
            symbol.all_declarations()
        };

        for decl_idx in declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                // Mark the import alias symbol itself as used so it survives
                // elision in the .d.ts output.
                self.mark_symbol_used(sym_id, UsageKind::TYPE | UsageKind::VALUE);
                self.analyze_import_equals_declaration(decl_idx);
                if let Some(import) = self.arena.get_import_decl(decl_node)
                    && import.module_specifier.is_some()
                {
                    self.analyze_local_import_equals_dependency_inner(
                        import.module_specifier,
                        seen_symbols,
                    );
                }
            }
        }
    }

    /// Analyze a function declaration.
    fn analyze_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = func.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk parameters
        for &param_idx in &func.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // CRITICAL: Also walk the inferred type of the function itself
        // This catches imported types via the type system even when
        // there's an explicit type annotation
        self.walk_inferred_type_or_related(&[func_idx, func.name]);

        // Walk return type (explicit or inferred)
        if func.type_annotation.is_some() {
            self.analyze_type_node(func.type_annotation);
        } else {
            // No explicit annotation - use inferred type from node_types
            self.walk_inferred_type_or_related(&[func_idx, func.name]);
            // Also walk return-statement type assertions in the body so
            // imports referenced only via `return {} as X;` survive elision.
            // The inferred TypeId may resolve to an ambient symbol that
            // doesn't transitively mark the local import alias as used.
            // Matches typeReferenceRelatedFiles.
            if func.body.is_some() {
                self.analyze_return_statement_assertions(func.body);
                self.analyze_return_expression_public_surface(func.body);
                self.analyze_return_statement_inferred_dependencies(func.body);
            }
        }
    }

    /// Walk return statements in a function/method body and analyze any
    /// `as Type` / `<Type>expr` assertion's type-position node so imports
    /// referenced only there are marked used.
    fn analyze_return_statement_assertions(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };
        for &stmt_idx in &block.statements.nodes {
            self.analyze_return_assertion_in_statement(stmt_idx);
        }
    }

    fn analyze_return_statement_inferred_dependencies(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };
        for &stmt_idx in &block.statements.nodes {
            self.analyze_return_inferred_dependency_in_statement(stmt_idx);
        }
    }

    fn analyze_return_inferred_dependency_in_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return;
        }
        let Some(ret) = self.arena.get_return_statement(stmt_node) else {
            return;
        };
        if !ret.expression.is_some() {
            return;
        }

        self.walk_inferred_type_or_related(&[ret.expression]);
        self.analyze_returned_identifier_declared_type(ret.expression);
    }

    fn analyze_returned_identifier_declared_type(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return;
        }
        let Some(sym_id) = self.value_reference_symbol(expr_idx) else {
            return;
        };
        let declarations = self
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.all_declarations())
            .unwrap_or_default();
        for decl_idx in declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                && decl.type_annotation.is_some()
            {
                self.analyze_type_node(decl.type_annotation);
            } else if let Some(param) = self.arena.get_parameter(decl_node)
                && param.type_annotation.is_some()
            {
                self.analyze_type_node(param.type_annotation);
            } else if let Some(prop) = self.arena.get_property_decl(decl_node)
                && prop.type_annotation.is_some()
            {
                self.analyze_type_node(prop.type_annotation);
            }
        }
    }

    fn analyze_return_assertion_in_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return;
        }
        let Some(ret) = self.arena.get_return_statement(stmt_node) else {
            return;
        };
        if !ret.expression.is_some() {
            return;
        }
        self.analyze_type_assertion_chain(ret.expression);
    }

    fn analyze_type_assertion_chain(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if let Some(assertion) = self.arena.get_type_assertion(expr_node) {
            self.analyze_type_node(assertion.type_node);
        }
    }

    fn analyze_return_expression_public_surface(&mut self, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };
        for &stmt_idx in &block.statements.nodes {
            self.analyze_return_expression_public_surface_in_statement(stmt_idx);
        }
    }

    fn analyze_return_expression_public_surface_in_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return;
                };
                if ret.expression.is_some() {
                    self.analyze_expression_public_surface(ret.expression);
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    for &nested_stmt_idx in &block.statements.nodes {
                        self.analyze_return_expression_public_surface_in_statement(nested_stmt_idx);
                    }
                }
            }
            _ => {}
        }
    }

    /// Analyze a class declaration.
    fn analyze_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = class.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk heritage clauses (extends, implements)
        if let Some(ref heritage) = class.heritage_clauses {
            self.analyze_heritage_clauses(heritage);
        }

        // Walk ALL members (including private - they can have type annotations referencing external types)
        for &member_idx in &class.members.nodes {
            self.analyze_class_member(member_idx);
        }
    }

    /// Analyze a class member.
    fn analyze_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.analyze_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.analyze_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.analyze_constructor(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.analyze_accessor(member_idx);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.analyze_index_signature(member_idx);
            }
            _ => {}
        }
    }

    fn analyze_property_declaration(&mut self, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };

        // Private properties emit as `private x;` without type — skip type dependencies.
        // Computed property names are still tracked since the name IS emitted.
        let is_private = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(prop.name);

        if !is_private {
            if prop.type_annotation.is_some() {
                self.analyze_type_node(prop.type_annotation);
            } else {
                self.walk_inferred_type(prop_idx);
                self.analyze_export_default_initializer_reference(prop.initializer);
            }
        }

        // For computed properties, analyze the name expression to mark referenced symbols
        // (e.g., `const symb = Symbol(); class C { [symb]: boolean }` — symb needs to be tracked)
        self.analyze_computed_property_name(prop.name);

        // Also walk the inferred type for computed properties (non-private only)
        if !is_private
            && let Some(name_node) = self.arena.get(prop.name)
            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
        {
            self.walk_inferred_type(prop_idx);
        }
    }

    /// Analyze a method declaration.
    fn analyze_method_declaration(&mut self, method_idx: NodeIndex) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        // Track symbols referenced in computed property names
        self.analyze_computed_property_name(method.name);

        if self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(method.name)
        {
            return;
        }

        // Walk type parameters
        if let Some(ref type_params) = method.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk parameters
        for &param_idx in &method.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type
        if method.type_annotation.is_some() {
            self.analyze_type_node(method.type_annotation);
        } else {
            self.walk_inferred_type(method_idx);
        }
    }

    fn analyze_constructor(&mut self, ctor_idx: NodeIndex) {
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        // Private constructors don't emit parameters in .d.ts — skip dependency tracking
        if self
            .arena
            .has_modifier(&ctor.modifiers, SyntaxKind::PrivateKeyword)
        {
            return;
        }

        for &param_idx in &ctor.parameters.nodes {
            self.analyze_parameter(param_idx);
        }
        self.analyze_constructor_public_surface_assignments(ctor.body);
    }

    fn analyze_accessor(&mut self, accessor_idx: NodeIndex) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        // Track symbols referenced in computed property names
        self.analyze_computed_property_name(accessor.name);

        // Private accessors emit without types — skip type deps
        if self
            .arena
            .has_modifier(&accessor.modifiers, SyntaxKind::PrivateKeyword)
            || self.member_has_private_identifier_name(accessor.name)
        {
            return;
        }

        // Walk parameters (setter parameter types)
        for &param_idx in &accessor.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type (for getters)
        if accessor.type_annotation.is_some() {
            self.analyze_type_node(accessor.type_annotation);
        }
    }

    /// Analyze an index signature.
    fn analyze_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        // Walk parameter type
        for &param_idx in &sig.parameters.nodes {
            self.analyze_parameter(param_idx);
        }

        // Walk return type
        if sig.type_annotation.is_some() {
            self.analyze_type_node(sig.type_annotation);
        }
    }

    /// Analyze an interface declaration.
    fn analyze_interface_declaration(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = iface.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk heritage clauses
        if let Some(ref heritage) = iface.heritage_clauses {
            self.analyze_heritage_clauses(heritage);
        }

        // Walk members
        for &member_idx in &iface.members.nodes {
            self.analyze_interface_member(member_idx);
        }
    }

    /// Analyze an interface member.
    fn analyze_interface_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    if sig.type_annotation.is_some() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                    // Track symbols referenced in computed property names
                    self.analyze_computed_property_name(sig.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Track symbols referenced in computed property names
                    self.analyze_computed_property_name(sig.name);
                    // Walk type parameters
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    // Walk parameters
                    if let Some(ref params) = sig.parameters {
                        for &param_idx in &params.nodes {
                            self.analyze_parameter(param_idx);
                        }
                    }
                    // Walk return type
                    if sig.type_annotation.is_some() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
            {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Walk type parameters
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    // Walk parameters
                    if let Some(ref params) = sig.parameters {
                        for &param_idx in &params.nodes {
                            self.analyze_parameter(param_idx);
                        }
                    }
                    // Walk return type
                    if sig.type_annotation.is_some() {
                        self.analyze_type_node(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.analyze_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Analyze a type alias declaration.
    fn analyze_type_alias_declaration(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        // Walk type parameters
        if let Some(ref type_params) = alias.type_parameters {
            for &param_idx in &type_params.nodes {
                self.analyze_type_parameter(param_idx);
            }
        }

        // Walk the aliased type
        self.analyze_type_node(alias.type_node);
    }

    /// Analyze an enum declaration.
    const fn analyze_enum_declaration(&mut self, _enum_idx: NodeIndex) {
        // Enum declarations don't reference other types in their signature
    }

    /// Analyze a variable statement.
    fn analyze_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                for &decl_idx in &decl_list.declarations.nodes {
                    self.analyze_variable_declaration(decl_idx);
                }
            }
        }
    }

    /// Analyze a variable declaration.
    fn analyze_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        // Walk type annotation
        if decl.type_annotation.is_some() {
            self.analyze_type_node(decl.type_annotation);
        } else {
            self.walk_inferred_type_or_related(&[decl_idx, decl.name]);
        }

        // For arrow/function-expression initializers, walk the return-type
        // and parameter-type annotations so their references survive elision.
        // Without this, `export const f = (...): X<...> => ...` with `X` from
        // an external import elides the import (the inferred TypeId may not
        // expose the source-level reference). Matches
        // declarationEmitRecursiveConditionalAliasPreserved.
        if decl.initializer.is_some()
            && let Some(init_node) = self.arena.get(decl.initializer)
            && (init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
            && let Some(func) = self.arena.get_function(init_node)
        {
            if func.type_annotation.is_some() {
                self.analyze_type_node(func.type_annotation);
            }
            for &param_idx in &func.parameters.nodes {
                if let Some(param_node) = self.arena.get(param_idx)
                    && let Some(param) = self.arena.get_parameter(param_node)
                    && param.type_annotation.is_some()
                {
                    self.analyze_type_node(param.type_annotation);
                }
            }
        }

        // Preserve imported type arguments used only at the call site of an
        // inferred initializer, e.g. `export const f = create<T>()`.
        if decl.type_annotation.is_none()
            && decl.initializer.is_some()
            && let Some(init_node) = self.arena.get(decl.initializer)
            && let Some(call) = self.arena.get_call_expr(init_node)
            && let Some(type_args) = call.type_arguments.as_ref()
        {
            for &arg_idx in &type_args.nodes {
                self.analyze_type_node(arg_idx);
            }
        }

        if decl.initializer.is_some()
            && self.initializer_preserves_value_reference(decl.initializer)
        {
            let old = self.in_value_pos;
            self.in_value_pos = true;
            self.analyze_entity_name(decl.initializer);
            self.analyze_local_import_equals_dependency(decl.initializer);
            self.in_value_pos = false;
            self.analyze_entity_name(decl.initializer);
            self.analyze_local_import_equals_dependency(decl.initializer);
            self.in_value_pos = old;
        }

        // When there is no explicit type annotation AND no inferred type from
        // the type cache, the declaration emitter may use the initializer's
        // referenced name as the emitted type (e.g. `var d: X` for
        // `var d = new X()`, or `typeof b` for `var b2 = b`).
        // We must mark import alias dependencies from the initializer so that
        // non-exported `import =` aliases are preserved in the .d.ts.
        //
        // When an inferred type IS available, `walk_inferred_type_or_related`
        // already captured all type-level dependencies. Adding the callee
        // here would incorrectly mark value-only references (e.g. a
        // `declare function` used in a call expression) as needed in the
        // .d.ts, even though tsc expands/inlines the result type instead.
        let has_inferred_type = self.type_cache.node_types.contains_key(&decl_idx.0)
            || (decl.name.is_some() && self.type_cache.node_types.contains_key(&decl.name.0))
            || self
                .get_node_type_related_nodes(decl_node)
                .iter()
                .any(|related_idx| {
                    related_idx.is_some() && self.type_cache.node_types.contains_key(&related_idx.0)
                });
        let initializer_is_call_expression = decl.initializer.is_some()
            && self
                .arena
                .get(decl.initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION);
        if decl.type_annotation.is_none()
            && decl.initializer.is_some()
            && !has_inferred_type
            && !initializer_is_call_expression
        {
            // Unwrap `new X()` / `X()` to get the callee, or use the
            // initializer directly if it's a plain identifier/expression.
            let callee = self.unwrap_export_default_expression(decl.initializer);
            self.analyze_entity_name(callee);
            self.analyze_local_import_equals_dependency(callee);
            // If the initializer itself was different (i.e. it IS a plain
            // identifier, not a new/call), also track it directly.
            if callee != decl.initializer {
                self.analyze_entity_name(decl.initializer);
                self.analyze_local_import_equals_dependency(decl.initializer);
            }
        }

        // Even when an inferred type IS available, import-equals aliases
        // referenced by the initializer must still be marked as used.
        // The emitter may produce `typeof b` references that require the
        // alias to survive elision, but `walk_inferred_type_or_related`
        // only walks TypeIds and doesn't discover import-equals AST
        // dependencies.  `analyze_local_import_equals_dependency` is
        // safe to call unconditionally — it only marks symbols whose
        // declarations are ImportEqualsDeclaration nodes.
        if decl.type_annotation.is_none()
            && decl.initializer.is_some()
            && has_inferred_type
            && !initializer_is_call_expression
        {
            let callee = self.unwrap_export_default_expression(decl.initializer);
            self.analyze_local_import_equals_dependency(callee);
            if callee != decl.initializer {
                self.analyze_local_import_equals_dependency(decl.initializer);
            }
        }
    }

    /// Analyze heritage clauses (extends/implements).
    fn analyze_heritage_clauses(&mut self, clauses: &tsz_parser::parser::NodeList) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            for &type_idx in &heritage.types.nodes {
                self.analyze_type_node(type_idx);
            }
        }
    }

    /// Analyze a type parameter.
    fn analyze_type_parameter(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_type_parameter(param_node) else {
            return;
        };

        // Walk constraint
        if param.constraint.is_some() {
            self.analyze_type_node(param.constraint);
        }

        // Walk default type
        if param.default.is_some() {
            self.analyze_type_node(param.default);
        }
    }

    /// Analyze a parameter.
    fn analyze_parameter(&mut self, param_idx: NodeIndex) {
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return;
        };

        // Walk type annotation
        if param.type_annotation.is_some() {
            self.analyze_type_node(param.type_annotation);
        } else {
            self.walk_inferred_type_or_related(&[param_idx, param.name, param.initializer]);
        }
    }
}
