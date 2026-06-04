impl<'a> CheckerState<'a> {
    // =========================================================================
    // Constructor Accessibility
    // =========================================================================

    /// Check if `child_sym` extends `ancestor_sym` by walking heritage clauses.
    ///
    /// This is a fallback for when `InheritanceGraph::is_derived_from` returns
    /// false because the graph hasn't been populated yet (e.g., during property
    /// initializer type-checking before the enclosing class's heritage is registered).
    fn is_heritage_derived_from(&self, child_sym: SymbolId, ancestor_sym: SymbolId) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let child = self.ctx.binder.get_symbol(child_sym);
        let child = match child {
            Some(s) => s,
            None => return false,
        };

        // Walk the class declarations for this symbol
        let Some(decl_idx) = child.primary_declaration() else {
            return false;
        };

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION
            && node.kind != syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return false;
        };

        let Some(heritage_clauses) = &class_data.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                let expr_idx = self
                    .ctx
                    .arena
                    .get_expr_type_args_at(type_idx)
                    .map_or(type_idx, |e| e.expression);

                // Resolve the heritage expression to a symbol
                let parent_sym = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, expr_idx)
                    .or_else(|| self.ctx.binder.get_node_symbol(expr_idx));

                if let Some(parent_sym) = parent_sym {
                    if parent_sym == ancestor_sym {
                        return true;
                    }
                    // Recurse for transitive inheritance
                    if self.is_heritage_derived_from(parent_sym, ancestor_sym) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Emit the appropriate constructor accessibility error.
    fn emit_constructor_access_error(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
        class_sym: SymbolId,
        is_private: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let class_name = self.get_symbol_display_name(class_sym);

        if is_private {
            // TS2673: Constructor of class 'X' is private
            let message = format!(
                "Constructor of class '{class_name}' is private and only accessible within the class declaration."
            );
            self.error_at_node(idx, &message, diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATION);
        } else {
            // TS2674: Constructor of class 'X' is protected
            let message = format!(
                "Constructor of class '{class_name}' is protected and only accessible within the class declaration."
            );
            self.error_at_node(idx, &message, diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATI);
        }
    }

    /// Return the class-expression node when `new <receiver>(...)` is targeting
    /// an anonymous class expression literal (after stripping parentheses).
    fn class_expression_from_new_expr(
        &self,
        new_expr_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<tsz_parser::parser::NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let call_expr = self.ctx.arena.get_call_expr_at(new_expr_idx)?;
        let receiver = self.ctx.arena.skip_parenthesized(call_expr.expression);
        let node = self.ctx.arena.get(receiver)?;
        if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            Some(receiver)
        } else {
            None
        }
    }

    /// True when `new_expr_idx` lives lexically inside the body of the given
    /// class-expression node. Mirrors the "same class allowed for private,
    /// subclass allowed for protected" lookup that named classes already get
    /// via `find_all_enclosing_classes`.
    fn new_expr_within_class_expression_body(
        &self,
        new_expr_idx: tsz_parser::parser::NodeIndex,
        class_expr_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let mut current = new_expr_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            if ext.parent.is_none() {
                return false;
            }
            if ext.parent == class_expr_idx {
                return true;
            }
            current = ext.parent;
        }
        false
    }

    /// Emit TS2673 / TS2674 for an anonymous class expression with
    /// inaccessible constructor. tsc uses the literal display
    /// `"(Anonymous class)"` in this message.
    fn emit_anonymous_constructor_access_error(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
        is_private: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let class_name = "(Anonymous class)";
        if is_private {
            let message = format!(
                "Constructor of class '{class_name}' is private and only accessible within the class declaration."
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATION,
            );
        } else {
            let message = format!(
                "Constructor of class '{class_name}' is protected and only accessible within the class declaration."
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATI,
            );
        }
    }

    /// Get the display name of a symbol for error messages.
    ///
    /// For generic classes, includes type parameters: `D<T>` instead of `D`.
    /// This matches tsc's behavior in TS2673/TS2674 diagnostics.
    fn get_symbol_display_name(&self, sym_id: SymbolId) -> String {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return "<unknown>".to_string();
        };
        let name = symbol.escaped_name.clone();

        // Check if the class declaration has type parameters.
        // Try value_declaration first, then fall back to declarations list.
        let decl_indices: Vec<tsz_parser::parser::NodeIndex> = if symbol.value_declaration.is_some()
        {
            vec![symbol.value_declaration]
        } else {
            symbol.declarations.clone()
        };

        for decl_idx in decl_indices {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_data) = self.ctx.arena.get_class(node) else {
                continue;
            };
            if let Some(ref type_params) = class_data.type_parameters {
                let param_names: Vec<&str> = type_params
                    .nodes
                    .iter()
                    .filter_map(|&idx| {
                        let tp = self.ctx.arena.get_type_parameter_at(idx)?;
                        let ident = self.ctx.arena.get_identifier_at(tp.name)?;
                        Some(ident.escaped_text.as_str())
                    })
                    .collect();
                if !param_names.is_empty() {
                    return format!("{}<{}>", name, param_names.join(", "));
                }
            }
        }
        name
    }
}
