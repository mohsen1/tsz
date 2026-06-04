impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Check a statement inside an ambient context (declare namespace/module).
    /// Emits TS1036 for non-declaration statements, plus specific errors for
    /// continue (TS1104), return (TS1108), and with (TS2410).
    pub(crate) fn check_statement_in_ambient_context(
        &mut self,
        stmt_idx: NodeIndex,
        reported_generic_ambient_statement_error: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        // Non-declaration statements are not allowed in ambient contexts
        let is_non_declaration = matches!(
            node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::WITH_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::DEBUGGER_STATEMENT
                || k == syntax_kind_ext::EMPTY_STATEMENT
        );

        if is_non_declaration
            && !*reported_generic_ambient_statement_error
            && !self.ctx.has_syntax_parse_errors
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS.to_string(),
                    diagnostic_codes::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                );
                *reported_generic_ambient_statement_error = true;
            }
        }

        // Additional specific checks for certain statements
        if node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT.to_string(),
                    diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT,
                );
            }
        }

        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY
                        .to_string(),
                    diagnostic_codes::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
                );
            }
        }

        if node.kind == syntax_kind_ext::WITH_STATEMENT {
            self.check_with_statement(stmt_idx);
        }

        // Ambient declarations still need index-signature parameter validation (TS1268).
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            self.check_ambient_variable_type_annotations_for_index_signatures(stmt_idx);
            self.check_ambient_variable_implicit_any(stmt_idx);
        }

        // Check labeled statements — the inner statement should also be checked
        if node.kind == syntax_kind_ext::LABELED_STATEMENT
            && let Some(labeled) = self.ctx.arena.get_labeled_statement(node)
        {
            self.check_label_on_declaration(labeled.label, labeled.statement);
            self.check_statement_in_ambient_context(
                labeled.statement,
                reported_generic_ambient_statement_error,
            );
        }
    }

    fn check_ambient_variable_type_annotations_for_index_signatures(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
            return;
        };

        for &list_idx in &var_stmt.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if var_decl.type_annotation.is_none() {
                    continue;
                }
                let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation) else {
                    continue;
                };
                if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
                    continue;
                }
                let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                    continue;
                };
                for &member_idx in &type_lit.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    let Some(&param_idx) = index_sig.parameters.nodes.first() else {
                        continue;
                    };
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };
                    if param.type_annotation.is_none() {
                        continue;
                    }
                    let is_valid =
                        crate::query_boundaries::index_signature::is_valid_index_sig_param_type_ast(
                            self.ctx.arena,
                            self.ctx.binder,
                            param.type_annotation,
                        );
                    if !is_valid && let Some((pos, end)) = self.ctx.get_node_span(param_idx) {
                        self.ctx.error(
                            pos,
                            end - pos,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT.to_string(),
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }
                }
            }
        }
    }

    /// TS7005: Emit "Variable 'x' implicitly has an 'any' type" for ambient variable
    /// declarations without a type annotation when `noImplicitAny` is enabled.
    ///
    /// Variables inside `declare namespace` blocks are only visited via
    /// `check_statement_in_ambient_context`, which doesn't run the full variable
    /// checking pipeline. This method fills that gap for the TS7005 diagnostic.
    fn check_ambient_variable_implicit_any(&mut self, stmt_idx: NodeIndex) {
        if !self.ctx.no_implicit_any() || self.ctx.is_declaration_file() {
            return;
        }

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
            return;
        };

        for &list_idx in &var_stmt.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                // Only flag declarations without a type annotation or initializer
                if var_decl.type_annotation.is_some() || var_decl.initializer.is_some() {
                    continue;
                }
                // Skip destructuring patterns
                if self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                    name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                }) {
                    continue;
                }
                use tsz_parser::parser::node::NodeAccess;
                let Some(name) = self.ctx.arena.get_identifier_text(var_decl.name) else {
                    continue;
                };
                use crate::diagnostics::diagnostic_codes;
                use tsz_common::diagnostics::get_message_template;
                let template =
                    get_message_template(diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE)
                        .unwrap_or("Variable '{0}' implicitly has an '{1}' type.");
                let message = format_message(template, &[name, "any"]);
                if let Some((pos, end)) = self.ctx.get_node_span(var_decl.name) {
                    self.ctx.error(
                        pos,
                        end - pos,
                        message,
                        diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
                    );
                }
            }
        }
    }

    fn is_strict_mode_for_node(&self, idx: NodeIndex) -> bool {
        self.ctx.is_strict_mode_for_node(idx)
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
            if !self.ctx.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled() {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A.to_string(),
                    diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
                );
            }

            if self.is_strict_mode_for_node(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE.to_string(),
                    diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
                );
            }
        }
    }

    fn check_label_on_declaration(&mut self, label_idx: NodeIndex, statement_idx: NodeIndex) {
        if !self.ctx.compiler_options.target.supports_es2015() {
            return;
        }
        if !self.is_strict_mode_for_node(label_idx) {
            return;
        }

        let Some(stmt_node) = self.ctx.arena.get(statement_idx) else {
            return;
        };

        let is_declaration_or_variable = matches!(
            stmt_node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::VARIABLE_STATEMENT
        );

        if is_declaration_or_variable && let Some((pos, end)) = self.ctx.get_node_span(label_idx) {
            self.ctx.error(
                pos,
                end - pos,
                "'A label is not allowed here.".to_string(),
                1344, // TS1344
            );
        }
    }

    /// Check parameter properties (only valid in constructors).
    pub fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };

            if let Some(param) = self.ctx.arena.get_parameter(node) {
                // If parameter has parameter property modifiers (public/private/protected/readonly)
                // and we're not in a constructor, report error at the modifier keyword (matching tsc).
                // Decorators on parameters are NOT parameter properties.
                let modifier_idx = if let Some(ref mods) = param.modifiers {
                    mods.nodes.iter().copied().find(|&mod_idx| {
                        self.ctx.arena.get(mod_idx).is_some_and(|m| {
                            use tsz_scanner::SyntaxKind;
                            m.kind == SyntaxKind::PublicKeyword as u16
                                || m.kind == SyntaxKind::PrivateKeyword as u16
                                || m.kind == SyntaxKind::ProtectedKeyword as u16
                                || m.kind == SyntaxKind::ReadonlyKeyword as u16
                        })
                    })
                } else {
                    None
                };
                if let Some(mod_idx) = modifier_idx
                    && let Some((pos, end)) = self.ctx.get_node_span(mod_idx)
                {
                    self.ctx.error(
                        pos,
                        end - pos,
                        "A parameter property is only allowed in a constructor implementation."
                            .to_string(),
                        diagnostic_codes::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
                    );
                }
            }
        }
    }

    /// Check function implementations for overload sequences.
    pub const fn check_function_implementations(&mut self, _nodes: &[NodeIndex]) {
        // Implementation of overload checking
        // Will be migrated from CheckerState
    }
}
