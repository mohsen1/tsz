impl<'a> UsageAnalyzer<'a> {
    /// Analyze a type node (AST walk for explicit types).
    ///
    /// Sets `in_value_pos = false` since we're in a type position.
    fn analyze_type_node(&mut self, type_idx: NodeIndex) {
        if !self.visited_nodes.insert(type_idx) {
            return;
        }

        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        // We're in a type position, so set in_value_pos to false
        // Save the previous value to restore it later
        let old_in_value_pos = self.in_value_pos;
        self.in_value_pos = false;

        match type_node.kind {
            // Some explicit type positions, especially heritage clauses in error
            // recovery, surface a bare entity name instead of a wrapped TypeReference.
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::QUALIFIED_NAME
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                self.analyze_entity_name(type_idx);
            }

            // Type references - extract the symbol
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    // First try AST walk via analyze_entity_name
                    self.analyze_entity_name(type_ref.type_name);

                    // Fallback: If AST walk didn't find the symbol, try semantic walk via TypeId
                    // This handles imported types where node_symbols doesn't have entries
                    debug!(
                        "[DEBUG] TYPE_REFERENCE: looking up type_cache.node_types for type_idx={:?}",
                        type_idx
                    );
                    if let Some(&type_id) = self.type_cache.node_types.get(&type_idx.0) {
                        debug!(
                            "[DEBUG] TYPE_REFERENCE: found type_id={:?}, walking it",
                            type_id
                        );
                        self.walk_type_id(type_id);
                    } else {
                        debug!(
                            "[DEBUG] TYPE_REFERENCE: no type_id found for type_idx={:?}",
                            type_idx
                        );
                    }

                    // CRITICAL: Walk type arguments to catch generic types like Promise<User>
                    if let Some(ref type_args) = type_ref.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
                }
            }

            // Expression with type arguments (heritage clauses)
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(type_node) {
                    self.analyze_entity_name(expr.expression);
                    if let Some(ref type_args) = expr.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
                }
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.arena.get_array_type(type_node) {
                    self.analyze_type_node(arr.element_type);
                }
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                if let Some(union) = self.arena.get_composite_type(type_node) {
                    for &type_idx in &union.types.nodes {
                        self.analyze_type_node(type_idx);
                    }
                }
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(inter) = self.arena.get_composite_type(type_node) {
                    for &type_idx in &inter.types.nodes {
                        self.analyze_type_node(type_idx);
                    }
                }
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(type_node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.analyze_type_node(elem_idx);
                    }
                }
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if let Some(ref type_params) = func.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    for &param_idx in &func.parameters.nodes {
                        self.analyze_parameter(param_idx);
                    }
                    self.analyze_type_node(func.type_annotation);
                }
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.arena.get_type_literal(type_node) {
                    for &member_idx in &lit.members.nodes {
                        self.analyze_interface_member(member_idx);
                    }
                }
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(paren) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(paren.type_node);
                }
            }

            // Type query (typeof X) - CRITICAL: marks X as VALUE usage
            // Even though typeof appears in a type position, it requires the value to exist
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(type_query) = self.arena.get_type_query(type_node) {
                    self.analyze_type_query_entity_name(type_query.expr_name);
                    // Also track import alias dependencies so that non-exported
                    // `import =` aliases referenced via `typeof` are preserved.
                    self.analyze_local_import_equals_dependency(type_query.expr_name);

                    // Walk type arguments (e.g., typeof X<A, B>)
                    if let Some(ref type_args) = type_query.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            self.analyze_type_node(arg_idx);
                        }
                    }
                }
            }

            // Type operator (keyof, readonly, etc.)
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(type_op) = self.arena.get_type_operator(type_node) {
                    self.analyze_type_node(type_op.type_node);
                }
            }

            // Indexed access type (T[K])
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed_access) = self.arena.get_indexed_access_type(type_node) {
                    self.analyze_type_node(indexed_access.object_type);
                    self.analyze_type_node(indexed_access.index_type);
                }
            }

            // Mapped type
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(type_node) {
                    self.analyze_type_parameter(mapped_type.type_parameter);
                    self.analyze_type_node(mapped_type.type_node);
                    if mapped_type.name_type.is_some() {
                        self.analyze_type_node(mapped_type.name_type);
                    }
                }
            }

            // Conditional type
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(conditional) = self.arena.get_conditional_type(type_node) {
                    self.analyze_type_node(conditional.check_type);
                    self.analyze_type_node(conditional.extends_type);
                    self.analyze_type_node(conditional.true_type);
                    self.analyze_type_node(conditional.false_type);
                }
            }

            // Type predicate (x is T)
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(predicate) = self.arena.get_type_predicate(type_node) {
                    self.analyze_type_node(predicate.type_node);
                }
            }

            // Infer type
            k if k == syntax_kind_ext::INFER_TYPE => {
                // Infer type doesn't reference external symbols
            }

            // Keyword types (no external references)
            k if k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 => {}

            // Literal types
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16 => {}

            // Constructor type: new (...) => T
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if let Some(ref type_params) = func.type_parameters {
                        for &param_idx in &type_params.nodes {
                            self.analyze_type_parameter(param_idx);
                        }
                    }
                    for &param_idx in &func.parameters.nodes {
                        self.analyze_parameter(param_idx);
                    }
                    self.analyze_type_node(func.type_annotation);
                }
            }

            // Optional type: T? (in tuples)
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(wrapped.type_node);
                }
            }

            // Rest type: ...T (in tuples)
            k if k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    self.analyze_type_node(wrapped.type_node);
                }
            }

            // Named tuple member: name: T
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(named) = self.arena.get_named_tuple_member(type_node) {
                    self.analyze_type_node(named.type_node);
                }
            }

            // Template literal type: `hello${T}world`
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(tlt) = self.arena.get_template_literal_type(type_node) {
                    for &span_idx in &tlt.template_spans.nodes {
                        // Spans reuse TemplateSpanData — expression field holds the type
                        if let Some(span_node) = self.arena.get(span_idx)
                            && let Some(span) = self.arena.get_template_span(span_node)
                        {
                            self.analyze_type_node(span.expression);
                        }
                    }
                }
            }

            // Import type: import("mod").T — handled by walk_inferred_type
            k if k == syntax_kind_ext::IMPORT_TYPE => {}

            _ => {}
        }

        // Restore the previous in_value_pos
        self.in_value_pos = old_in_value_pos;
    }

    /// Check if a member name is a private identifier (`#foo`).
    fn member_has_private_identifier_name(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
    }

    /// Analyze the expression inside a computed property name (e.g., `[symb]`).
    /// This ensures that symbols referenced in computed names are tracked as used,
    /// so their declarations (e.g., `const symb: unique symbol`) are emitted in .d.ts.
    fn analyze_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        if self.source_is_js_file && self.computed_property_name_resolves_to_literal_key(expr_idx) {
            return;
        }
        // The expression inside [] may be an identifier, property access, etc.
        let old_in_value_pos = self.in_value_pos;
        self.in_value_pos = true;
        self.analyze_entity_name(expr_idx);
        self.in_value_pos = old_in_value_pos;
    }

    fn computed_property_name_resolves_to_literal_key(&self, expr_idx: NodeIndex) -> bool {
        self.type_cache
            .node_types
            .get(&expr_idx.0)
            .copied()
            .and_then(|type_id| visitor::literal_value(self.type_interner, type_id))
            .is_some()
    }
}
