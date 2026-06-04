impl<'a> Printer<'a> {
    pub(in crate::emitter) fn configure_nested_es5_class_aliases(
        &self,
        es5_emitter: &mut ClassES5Emitter<'a>,
    ) {
        if let Some((_, alias)) = &self.scoped_class_expression_self_alias {
            es5_emitter.set_outer_reserved_for_generator_state(vec![alias.as_ref().to_string()]);
        }
        if let Some(alias) = &self.scoped_static_this_alias {
            es5_emitter.set_inherited_computed_name_this(alias.as_ref().to_string());
        }

        let mut outer_rename_map = self.ctx.block_scope_state.visible_outer_rename_map();
        for (class_name, class_alias) in &self.scoped_class_expression_self_alias_ancestors {
            outer_rename_map.insert(
                class_name.as_ref().to_string(),
                class_alias.as_ref().to_string(),
            );
        }
        if let Some((class_name, class_alias)) = &self.scoped_class_expression_self_alias {
            outer_rename_map.insert(
                class_name.as_ref().to_string(),
                class_alias.as_ref().to_string(),
            );
        }
        if !outer_rename_map.is_empty() {
            es5_emitter.set_outer_rename_map(outer_rename_map);
        }
    }

    /// Emit a node.
    pub(in crate::emitter) fn emit_node(&mut self, node: &Node, idx: NodeIndex) {
        // Recursion depth check to prevent infinite loops
        self.emit_recursion_depth += 1;
        if self.emit_recursion_depth > MAX_EMIT_RECURSION_DEPTH {
            // Log a warning about the recursion limit being exceeded.
            // This helps developers identify problematic deeply nested ASTs.
            warn!(
                depth = MAX_EMIT_RECURSION_DEPTH,
                node_kind = node.kind,
                node_pos = node.pos,
                "Emit recursion limit exceeded"
            );
            self.write("/* emit recursion limit exceeded */");
            self.emit_recursion_depth -= 1;
            return;
        }

        // Check transform directives first
        let has_transform = !self.transforms.is_empty()
            && Self::kind_may_have_transform(node.kind)
            && self.transforms.has_transform(idx);
        let previous_pending = self.pending_source_pos;

        self.queue_source_mapping(node);
        if has_transform {
            self.apply_transform(node, idx);
        } else {
            let kind = node.kind;
            self.emit_node_by_kind(node, idx, kind);
        }

        self.pending_source_pos = previous_pending;
        self.emit_recursion_depth -= 1;
    }

    const fn kind_may_have_transform(kind: u16) -> bool {
        matches!(
            kind,
            k if k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::MODULE_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        )
    }

    /// Emit a node by kind using default logic (no transforms).
    /// This is the main dispatch method for emission.
    pub(crate) fn emit_node_by_kind(&mut self, node: &Node, idx: NodeIndex, kind: u16) {
        match kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => {
                // Check for substitution directives on identifier nodes.
                if self.transforms.has_transform(idx) {
                    if let Some(directive) = self.transforms.get(idx) {
                        match directive {
                            TransformDirective::SubstituteArguments => self.write("arguments"),
                            TransformDirective::SubstituteThis { capture_name } => {
                                let name = std::sync::Arc::clone(capture_name);
                                self.write(&name);
                            }
                            _ => self.emit_identifier(node),
                        }
                    } else {
                        self.emit_identifier(node);
                    }
                } else {
                    self.emit_identifier(node);
                }
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                let preserve_array_recovery = self
                    .arena
                    .parent_of(idx)
                    .and_then(|parent| self.arena.get(parent))
                    .is_some_and(|parent| parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
                if (!self.ctx.needs_es2022_lowering || preserve_array_recovery)
                    && let Some(ident) = self.arena.get_identifier(node)
                {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                self.emit_type_parameter(node);
            }

            // Qualified name: A.B.C (used in type references, import types)
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(qn) = self.arena.get_qualified_name(node) {
                    self.emit(qn.left);
                    self.write(".");
                    self.emit(qn.right);
                }
            }

            // Literals
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.emit_numeric_literal(node);
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                self.emit_bigint_literal(node);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.emit_string_literal(node);
            }
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => {
                self.emit_regex_literal(node);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.write("false");
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.write("null");
            }

            // Binary expression
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.emit_binary_expression(node);
            }

            // Unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.emit_prefix_unary(node);
            }
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                self.emit_postfix_unary(node);
            }

            // Call expression
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.emit_call_expression(idx, node);
            }

            // New expression
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.emit_new_expression(node);
            }

            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.emit_property_access(node);
            }

            // Meta property (new.target, import.meta)
            k if k == syntax_kind_ext::META_PROPERTY => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    // The expression is the keyword token (new/import)
                    if let Some(kw_node) = self.arena.get(access.expression) {
                        if kw_node.kind == SyntaxKind::NewKeyword as u16 {
                            if self.ctx.target_es5 {
                                let substitution = self
                                    .current_new_target_substitution
                                    .as_deref()
                                    .unwrap_or("_newTarget")
                                    .to_string();
                                self.write(&substitution);
                                return;
                            }
                            self.write("new");
                        } else if kw_node.kind == SyntaxKind::ImportKeyword as u16 {
                            self.write("import");
                        }
                    }
                    self.write(".");
                    let name = self.get_identifier_text_idx(access.name_or_argument);
                    self.write(&name);
                }
            }

            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.emit_element_access(node);
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.emit_parenthesized(node);
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.emit_type_assertion_expression(node);
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.emit_non_null_expression(node);
            }

            // Conditional expression
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.emit_conditional(node);
            }

            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_array_literal(node);
            }

            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_object_literal(node);
            }

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                self.emit_arrow_function(node, idx);
            }

            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_function_expression(node, idx);
                });
            }

            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_function_declaration(node, idx);
                });
            }

            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.emit_variable_declaration(node);
            }

            // Variable declaration list
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                self.emit_variable_declaration_list(node);
            }

            // Variable statement
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_statement(node);
            }

            // Expression statement
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.emit_expression_statement(node);
            }

            // Block
            k if k == syntax_kind_ext::BLOCK => {
                self.emit_block(node, idx);
            }

            // Class static block: `static { ... }`
            // Treated like a function body for single-line formatting purposes.
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                self.write("static ");
                let prev = self.emitting_function_body_block;
                let prev_in_static_block = self.ctx.flags.in_class_static_block;
                self.emitting_function_body_block = true;
                self.ctx.flags.in_class_static_block = true;
                // A native `static { ... }` block is a function-body block, but the
                // outer-scope hoisted temps (e.g. legacy-decorator class-self
                // aliases destined for the file-top `var C_1;`) do not belong
                // inside it. Save/restore them so the block's function-body flush
                // only emits temps generated within the block, mirroring the
                // lowered static-block IIFE path.
                let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
                self.emit_block(node, idx);
                self.hoisted_assignment_temps = saved_temps;
                self.emitting_function_body_block = prev;
                self.ctx.flags.in_class_static_block = prev_in_static_block;
            }

            // If statement
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.emit_if_statement(node);
            }

            // While statement
            k if k == syntax_kind_ext::WHILE_STATEMENT => {
                self.emit_while_statement(node);
            }

            // For statement
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.emit_for_statement(node);
            }

            // For-in statement
            k if k == syntax_kind_ext::FOR_IN_STATEMENT => {
                self.emit_for_in_statement(node);
            }

            // For-of statement
            k if k == syntax_kind_ext::FOR_OF_STATEMENT => {
                self.emit_for_of_statement(node);
            }

            // Return statement
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                self.emit_return_statement(node);
            }

            // Class declaration
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_class_declaration(node, idx);
                });
            }

            // Class expression (e.g., `return class extends Base { ... }`)
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                self.emit_class_expression_with_captured_computed_names(node, idx);
            }

            // Property assignment
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.emit_property_assignment(node);
            }

            // Shorthand property assignment
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                self.emit_shorthand_property(node);
            }

            // Spread assignment in object literal: `{ ...expr }` (ES2018+ native spread)
            // For pre-ES2018 targets this is handled by emit_object_literal_with_object_assign.
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("...");
                    if let Some(expr_node) = self.arena.get(spread.expression) {
                        self.emit_comments_after_dot_dot_dot(node.pos, expr_node.pos, true);
                    }
                    self.emit_expression(spread.expression);
                }
            }

            // Parameter declaration
            k if k == syntax_kind_ext::PARAMETER => {
                self.emit_parameter(node);
            }

            // Type keywords (for type annotations)
            k if k == SyntaxKind::NumberKeyword as u16 => self.write("number"),
            k if k == SyntaxKind::StringKeyword as u16 => self.write("string"),
            k if k == SyntaxKind::BooleanKeyword as u16 => self.write("boolean"),
            k if k == SyntaxKind::VoidKeyword as u16 => self.write("void"),
            k if k == SyntaxKind::AnyKeyword as u16 => self.write("any"),
            k if k == SyntaxKind::NeverKeyword as u16 => self.write("never"),
            k if k == SyntaxKind::UnknownKeyword as u16 => self.write("unknown"),
            k if k == SyntaxKind::UndefinedKeyword as u16 => self.write("undefined"),
            k if k == SyntaxKind::ObjectKeyword as u16 => self.write("object"),
            k if k == SyntaxKind::SymbolKeyword as u16 => self.write("symbol"),
            k if k == SyntaxKind::BigIntKeyword as u16 => self.write("bigint"),

            // Type reference
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                self.emit_type_reference(node);
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                self.emit_array_type(node);
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                self.emit_union_type(node);
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                self.emit_intersection_type(node);
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                self.emit_tuple_type(node);
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                self.emit_function_type(node);
            }

            // Constructor type: `new (...) => T`
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.emit_constructor_type(node);
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                self.emit_type_literal(node);
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                self.emit_parenthesized_type(node);
            }

            // Conditional type: T extends U ? X : Y
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                self.emit_conditional_type(node);
            }

            // Indexed access type: T[K]
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.emit_indexed_access_type(node);
            }

            // Infer type: infer U
            k if k == syntax_kind_ext::INFER_TYPE => {
                self.emit_infer_type(node);
            }

            // Literal type wrapper (string/number/boolean/bigint literals in type position)
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                self.emit_literal_type(node);
            }

            // Mapped type: { [P in keyof T]: T[P] }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                self.emit_mapped_type(node);
            }

            // Named tuple member: [name: Type]
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                self.emit_named_tuple_member(node);
            }

            // Optional type: T? (in tuple elements)
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                self.emit_optional_type(node);
            }

            // Rest type: ...T (in tuple elements)
            k if k == syntax_kind_ext::REST_TYPE => {
                self.emit_rest_type(node);
            }

            // Template literal type: `prefix${T}suffix`
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                self.emit_template_literal_type(node);
            }

            // this type in type position
            k if k == syntax_kind_ext::THIS_TYPE => {
                self.write("this");
            }

            // Type operator: keyof T, readonly T, unique symbol
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                self.emit_type_operator(node);
            }

            // Type predicate: x is T, asserts x is T
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                self.emit_type_predicate(node);
            }

            // Type query: typeof x
            k if k == syntax_kind_ext::TYPE_QUERY => {
                self.emit_type_query(node);
            }

            // Empty statement
            k if k == syntax_kind_ext::EMPTY_STATEMENT => {
                if self.emit_recovered_invalid_import_expression(node) {
                    return;
                }
                if self.emit_recovered_let_array_assignment(node) {
                    return;
                }
                self.write_semicolon();
                self.skip_recovered_empty_statement_skipped_token_comments(node);
            }

            // JSX
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                self.emit_jsx_element(node);
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.emit_jsx_self_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_ELEMENT => {
                self.emit_jsx_opening_element(node);
            }
            k if k == syntax_kind_ext::JSX_CLOSING_ELEMENT => {
                self.emit_jsx_closing_element(node);
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                self.emit_jsx_fragment(node);
            }
            k if k == syntax_kind_ext::JSX_OPENING_FRAGMENT => {
                self.write("<>");
            }
            k if k == syntax_kind_ext::JSX_CLOSING_FRAGMENT => {
                self.write("</>");
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                self.emit_jsx_attributes(node);
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                self.emit_jsx_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                self.emit_jsx_spread_attribute(node);
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                self.emit_jsx_expression(node);
            }
            k if k == SyntaxKind::JsxText as u16 => {
                self.emit_jsx_text(node);
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                self.emit_jsx_namespaced_name(node);
            }

            // Imports/Exports
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.emit_import_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.emit_import_equals_declaration(node);
            }
            k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                self.emit_import_clause(node);
            }
            k if k == syntax_kind_ext::NAMED_IMPORTS || k == syntax_kind_ext::NAMESPACE_IMPORT => {
                self.emit_named_imports(node);
            }
            k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                self.emit_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(node);
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT => {
                // `* as name` in `export * as name from "..."`
                if let Some(data) = self.arena.get_named_imports(node) {
                    self.write("* as ");
                    self.emit(data.name);
                }
            }
            k if k == syntax_kind_ext::NAMED_EXPORTS => {
                self.emit_named_exports(node);
            }
            k if k == syntax_kind_ext::EXPORT_SPECIFIER => {
                self.emit_specifier(node);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(node);
            }

            // Additional statements
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.emit_throw_statement(node);
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.emit_try_statement(node);
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                self.emit_catch_clause(node);
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.emit_switch_statement(node);
            }
            k if k == syntax_kind_ext::CASE_CLAUSE => {
                self.emit_case_clause(node);
            }
            k if k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.emit_default_clause(node);
            }
            k if k == syntax_kind_ext::CASE_BLOCK => {
                self.emit_case_block(node);
            }
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                self.emit_break_statement(node);
            }
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                self.emit_continue_statement(node);
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                self.emit_labeled_statement(node);
            }
            k if k == syntax_kind_ext::DO_STATEMENT => {
                self.emit_do_statement(node);
            }
            k if k == syntax_kind_ext::DEBUGGER_STATEMENT => {
                self.emit_debugger_statement(node);
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                self.emit_with_statement(node);
            }

            // Declarations
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(node, idx);
            }
            k if k == syntax_kind_ext::ENUM_MEMBER => {
                self.emit_enum_member(node);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                // Interface declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_interface_declaration(node);
                } else {
                    self.emit_recovered_interface_body_statements(node);
                    // Skip comments belonging to erased declarations so they don't
                    // get emitted later by gap/before-pos comment handling.
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // Type alias declarations are TypeScript-only - emit only in declaration mode (.d.ts)
                if self.ctx.flags.in_declaration_emit {
                    self.emit_type_alias_declaration(node);
                } else {
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                // `export as namespace X` is TypeScript-only (UMD global declaration) -
                // erased in JS output, preserved only in .d.ts declaration emit.
                if self.ctx.flags.in_declaration_emit {
                    self.emit_namespace_export_declaration(node);
                } else {
                    self.skip_comments_for_erased_node(node);
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(node, idx);
            }

            // Computed property name: [expr]
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.write("[");
                    // If this expression has been hoisted to a temp variable, emit the
                    // temp name instead of the original expression.
                    if let Some(temp_name) = self.computed_prop_temp_map.get(&computed.expression) {
                        self.write(&temp_name.clone());
                    } else {
                        self.emit(computed.expression);
                        if self.is_static_block_await_identifier(computed.expression) {
                            self.write(" ");
                        }
                    }
                    // Map closing `]` to its source position.
                    // The expression's end points past the expression, so `]`
                    // is at the expression's end position (where the expression
                    // text ends and `]` begins).
                    if self.source_text_for_map().is_some() {
                        let expr_end = self
                            .arena
                            .get(computed.expression)
                            .map_or(node.pos + 1, |e| e.end);
                        self.pending_source_pos = self.fast_source_position(expr_end);
                    }
                    self.write("]");
                }
            }

            // Class members
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_method_declaration(node);
                });
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(node);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_constructor_declaration(node);
                });
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_get_accessor(node, idx);
                });
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_set_accessor(node, idx);
                });
            }
            k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => {
                self.write(";");
            }
            k if k == syntax_kind_ext::DECORATOR => {
                self.emit_decorator(node);
            }

            // Interface/type members (signatures)
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                self.emit_property_signature(node);
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                self.emit_method_signature(node);
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE && self.ctx.flags.in_declaration_emit => {
                // Call signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                self.emit_call_signature(node);
            }
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE
                && self.ctx.flags.in_declaration_emit =>
            {
                // Construct signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                self.emit_construct_signature(node);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE && self.ctx.flags.in_declaration_emit => {
                // Index signatures are TypeScript-only - emit only in declaration mode (.d.ts)
                self.emit_index_signature(node);
            }

            // Template literals
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.emit_tagged_template_expression(node, idx);
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.emit_template_expression(node);
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                self.emit_no_substitution_template(node);
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                self.emit_template_span(node);
            }
            k if k == SyntaxKind::TemplateHead as u16 => {
                self.emit_template_head(node);
            }
            k if k == SyntaxKind::TemplateMiddle as u16 => {
                self.emit_template_middle(node);
            }
            k if k == SyntaxKind::TemplateTail as u16 => {
                self.emit_template_tail(node);
            }

            // Yield/Await/Spread
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                self.emit_yield_expression(node);
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                self.emit_await_expression(node);
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                self.emit_spread_element(node);
            }

            // Source file
            k if k == syntax_kind_ext::SOURCE_FILE => {
                self.emit_source_file(node, idx);
            }

            // Other tokens and keywords - emit their text
            k if k == SyntaxKind::ThisKeyword as u16 => {
                // Check for SubstituteThis directive from lowering pass (Phase C)
                // Directive approach is now the only path (fallback removed)
                if let Some(TransformDirective::SubstituteThis { capture_name }) =
                    self.transforms.get(idx)
                {
                    let name = std::sync::Arc::clone(capture_name);
                    self.write(&name);
                } else if let Some(loop_this) = self.ctx.loop_this_capture_name.clone() {
                    // Inside an ES5 converted-loop (`_loop_N`) IIFE body, a
                    // lexical `this` is rewritten to the captured `this_N`
                    // binding declared at the real function scope.
                    self.write(&loop_this);
                } else if let Some(alias) = self.scoped_static_this_alias.as_ref().cloned() {
                    self.write(&alias);
                } else {
                    self.write("this");
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == SyntaxKind::ImportKeyword as u16 => self.write("import"),

            // Binding patterns (for destructuring)
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                // When emitting as-is (non-ES5 or for parameters), just emit the pattern
                self.emit_object_binding_pattern(node);
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                self.emit_array_binding_pattern(node);
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                self.emit_binding_element(node);
            }

            // ExpressionWithTypeArguments / instantiation expression:
            // Strip type arguments and wrap the expression in parentheses.
            // tsc wraps the result in parens when erasing type arguments,
            // e.g. `f<string>` becomes `(f)`. An *empty* type argument list
            // (`f<>` — a parser-recovery shape) doesn't need wrapping; tsc
            // emits it as the bare expression `f`.
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.arena.get_expr_type_args(node) {
                    let expression = data.expression;
                    let type_arg_nodes: Vec<NodeIndex> = data
                        .type_arguments
                        .as_ref()
                        .map_or_else(Vec::new, |ta| ta.nodes.clone());
                    if let Some(recovered_type_args) =
                        self.recovered_jsdoc_type_arguments_text(&type_arg_nodes)
                    {
                        self.emit(expression);
                        self.write(&recovered_type_args);
                        return;
                    }

                    let needs_parens = !type_arg_nodes.is_empty();
                    if needs_parens {
                        self.open_paren();
                    }
                    self.emit(expression);
                    if needs_parens {
                        self.close_paren();
                    }
                    // Skip comments inside the erased type arguments so they
                    // don't leak into subsequent output.
                    if !self.ctx.options.remove_comments {
                        for ta_idx in &type_arg_nodes {
                            if let Some(ta_node) = self.arena.get(*ta_idx) {
                                self.skip_comments_in_range(ta_node.pos, ta_node.end);
                            }
                        }
                    }
                }
            }

            // Default: do nothing (or handle other cases as needed)
            _ => {}
        }
    }
}

pub(crate) use crate::transforms::emit_utils::is_valid_identifier_name;

pub(crate) const fn get_operator_text(op: u16) -> &'static str {
    crate::transforms::emit_utils::operator_to_str(op)
}
