//! Assignment expression checking (simple, compound, logical, readonly).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::flags::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// =============================================================================
// Assignment Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn report_abstract_properties_in_destructuring_assignment(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) {
        let right_idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
        if !self.is_this_expression(right_idx) || self.ctx.function_depth != 0 {
            return;
        }

        let Some(class_idx) = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx) else {
            return;
        };
        if !self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|info| info.in_constructor)
        {
            return;
        }

        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }

        let Some(obj) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let (prop_name, error_node) =
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    (self.get_property_name_resolved(prop.name), prop.name)
                } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node) {
                        (
                            self.ctx
                                .arena
                                .get(shorthand.name)
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                                .map(|ident| ident.escaped_text.clone()),
                            shorthand.name,
                        )
                    } else {
                        (None, NodeIndex::NONE)
                    }
                } else {
                    (None, NodeIndex::NONE)
                };

            if let Some(prop_name) = prop_name
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_idx, &prop_name)
            {
                self.error_abstract_property_in_constructor(
                    &prop_name,
                    &declaring_class_name,
                    error_node,
                );
            }
        }
    }

    /// TS2341/TS2445: Check private/protected accessibility for properties
    /// accessed in destructuring assignment patterns.
    ///
    /// In `{ o: target } = source`, property `o` is accessed on the source type
    /// and must respect visibility modifiers. This walks the destructuring
    /// pattern recursively and checks each property name against the source type.
    fn check_destructuring_property_accessibility(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) {
        if source_type == TypeId::ANY || source_type == TypeId::ERROR {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };

        if pattern_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(obj) = self.ctx.arena.get_literal_expr(pattern_node) else {
                return;
            };

            for &elem_idx in &obj.elements.nodes {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };

                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    // Property assignment: { name: target }
                    if let Some(name) = self.get_property_name_resolved(prop.name) {
                        self.check_property_accessibility(
                            NodeIndex::NONE,
                            &name,
                            prop.name,
                            source_type,
                        );

                        // Recurse into nested patterns: resolve property type from source
                        if let Some(value_node) = self.ctx.arena.get(prop.initializer) {
                            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || value_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            {
                                let prop_type = self
                                    .resolve_property_type_for_destructuring(source_type, &name);
                                if let Some(prop_type) = prop_type {
                                    self.check_destructuring_property_accessibility(
                                        prop.initializer,
                                        prop_type,
                                    );
                                }
                            } else if value_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                // { name: pattern = default } — check the LHS of the assignment
                                if let Some(bin) = self.ctx.arena.get_binary_expr(value_node)
                                    && bin.operator_token == SyntaxKind::EqualsToken as u16
                                    && let Some(lhs_node) = self.ctx.arena.get(bin.left)
                                    && (lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        || lhs_node.kind
                                            == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                                {
                                    let prop_type = self.resolve_property_type_for_destructuring(
                                        source_type,
                                        &name,
                                    );
                                    if let Some(prop_type) = prop_type {
                                        self.check_destructuring_property_accessibility(
                                            bin.left, prop_type,
                                        );
                                    }
                                }
                            }
                        }
                    }
                } else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    // Shorthand: { x } — property name is the identifier
                    if let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                        && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        self.check_property_accessibility(
                            NodeIndex::NONE,
                            &ident.escaped_text,
                            shorthand.name,
                            source_type,
                        );
                    }
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            // Array destructuring: recurse into elements with element types
            let Some(array_lit) = self.ctx.arena.get_literal_expr(pattern_node) else {
                return;
            };

            for (index, &elem_idx) in array_lit.elements.nodes.iter().enumerate() {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };
                if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }

                // Get element type from the source array/tuple
                let elem_type = self.resolve_element_type_for_destructuring(source_type, index);
                let Some(elem_type) = elem_type else {
                    continue;
                };

                // Handle spread elements
                let target_idx = if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    if let Some(spread) = self.ctx.arena.get_spread(elem_node) {
                        spread.expression
                    } else {
                        continue;
                    }
                } else {
                    elem_idx
                };

                if let Some(target_node) = self.ctx.arena.get(target_idx) {
                    if target_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || target_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    {
                        self.check_destructuring_property_accessibility(target_idx, elem_type);
                    } else if target_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        // element = default — check LHS
                        if let Some(bin) = self.ctx.arena.get_binary_expr(target_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                            && let Some(lhs_node) = self.ctx.arena.get(bin.left)
                            && (lhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || lhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
                        {
                            self.check_destructuring_property_accessibility(bin.left, elem_type);
                        }
                    }
                }
            }
        }
    }

    /// Resolve the type of a property on an object type for destructuring checks.
    fn resolve_property_type_for_destructuring(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use tsz_solver::operations::property::PropertyAccessResult;
        match self.resolve_property_access_with_env(object_type, property_name) {
            PropertyAccessResult::Success { type_id, .. } => Some(type_id),
            _ => None,
        }
    }

    /// Resolve the element type at a given index from an array/tuple type.
    fn resolve_element_type_for_destructuring(
        &mut self,
        source_type: TypeId,
        index: usize,
    ) -> Option<TypeId> {
        // Try tuple element type first
        if let Some(elems) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, source_type)
        {
            if index < elems.len() {
                return Some(elems[index].type_id);
            }
            return None;
        }
        // Fall back to array element type
        tsz_solver::type_queries::get_array_element_type(self.ctx.types, source_type)
    }

    // =========================================================================
    // Assignment Operator Utilities
    // =========================================================================

    /// Check if a token is an assignment operator (=, +=, -=, etc.)
    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    // =========================================================================
    // Assignment Expression Checking
    // =========================================================================

    /// Check if a node is a valid assignment target (variable, property access, element access,
    /// or destructuring pattern).
    ///
    /// Returns false for literals, call expressions, and other non-assignable expressions.
    /// Used to emit TS2364: "The left-hand side of an assignment expression must be a variable
    /// or a property access."
    pub(crate) fn is_valid_assignment_target(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => true,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                // Check the inner expression
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.is_valid_assignment_target(paren.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION =>
            {
                // Satisfies and as expressions are valid assignment targets if their inner expression is valid
                // Example: (x satisfies number) = 10
                if let Some(assertion) = self.ctx.arena.get_type_assertion(node) {
                    self.is_valid_assignment_target(assertion.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if a node is part of an optional chain (has `?.` somewhere in its left spine).
    ///
    /// Walks through property access, element access, and call expression chains looking
    /// for any node with `question_dot_token: true` (for accesses) or the `OPTIONAL_CHAIN`
    /// flag (for calls). For example, in `obj?.a.b`, both `obj?.a` and `obj?.a.b` are
    /// considered part of the optional chain.
    ///
    /// Skips through transparent wrappers (parenthesized, non-null, type assertions, satisfies).
    pub(crate) fn is_optional_chain_access(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    // This node itself is an optional chain root (has `?.`)
                    if access.question_dot_token {
                        return true;
                    }
                    // Check if the base expression is part of an optional chain
                    self.is_optional_chain_access(access.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // Call expressions get the OPTIONAL_CHAIN flag from the parser
                if (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0 {
                    return true;
                }
                // Check if the callee is part of an optional chain
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.is_optional_chain_access(call.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if an identifier node refers to a const variable.
    ///
    /// Returns `Some(name)` if the identifier refers to a const, `None` otherwise.
    fn get_const_variable_name(&self, ident_idx: NodeIndex) -> Option<String> {
        let ident_idx = self.unwrap_assignment_target_for_symbol(ident_idx);
        let node = self.ctx.arena.get(ident_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.clone();

        // Use binder-level resolution (no tracking side-effect) to avoid marking
        // the assignment target as "read" in `referenced_symbols`. The const check
        // is a read-only query — assignment targets should only be tracked via
        // `resolve_identifier_symbol_for_write` in `get_type_of_assignment_target`.
        // Using the tracking `resolve_identifier_symbol` here would suppress TS6133
        // for write-only parameters (e.g., `person2 = "dummy value"` should still
        // flag `person2` as unused).
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, ident_idx)?;

        // Find the correct binder and arena for this symbol
        let mut target_binder = self.ctx.binder;
        let mut target_arena = self.ctx.arena;

        if let Some(&file_idx) = self.ctx.cross_file_symbol_targets.borrow().get(&sym_id) {
            if let Some(all_binders) = &self.ctx.all_binders
                && let Some(b) = all_binders.get(file_idx)
            {
                target_binder = b;
            }
            if let Some(all_arenas) = &self.ctx.all_arenas
                && let Some(a) = all_arenas.get(file_idx)
            {
                target_arena = a;
            }
        } else if let Some(arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            // It could be a lib symbol where target_binder is still self.ctx.binder (due to merging)
            // or one of the lib_contexts.
            target_arena = arena.as_ref();
        }

        // Also check if it's from a lib context
        for lib in &self.ctx.lib_contexts {
            if let Some(sym) = lib.binder.get_symbol(sym_id)
                && sym.escaped_name == name
            {
                target_binder = &lib.binder;
                target_arena = lib.arena.as_ref();
                break;
            }
        }

        let symbol = target_binder
            .get_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }

        // Sometimes the declaration is specifically registered in declaration_arenas
        if let Some(arenas) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, value_decl))
            && let Some(first) = arenas.first()
        {
            target_arena = first.as_ref();
        }

        target_arena.get(value_decl)?;
        target_arena
            .is_const_variable_declaration(value_decl)
            .then_some(name)
    }

    /// Strip wrappers that preserve assignment target identity for symbol checks.
    ///
    /// Examples:
    /// - `(x)` -> `x`
    /// - `x!` -> `x`
    /// - `(x as T)` -> `x`
    /// - `(x satisfies T)` -> `x`
    fn unwrap_assignment_target_for_symbol(&self, idx: NodeIndex) -> NodeIndex {
        self.ctx.arena.skip_parenthesized_and_assertions(idx)
    }

    /// Check if the operand of an increment/decrement operator is a valid l-value (TS2357).
    ///
    /// The operand must be a variable (Identifier), property access, or element access.
    /// Expressions like `(1 + 2)++` or `1++` are not valid.
    /// Transparent wrappers are skipped: parenthesized, non-null assertion, type assertion,
    /// and satisfies expressions (e.g., `foo[x]!++` and `(a satisfies number)++` are valid).
    /// Returns `true` if an error was emitted.
    pub(crate) fn check_increment_decrement_operand(&mut self, operand_idx: NodeIndex) -> bool {
        let inner = self.skip_assignment_transparent_wrappers(operand_idx);
        let Some(node) = self.ctx.arena.get(inner) else {
            return false;
        };

        let is_valid = node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;

        if !is_valid {
            self.error_at_node(
                operand_idx,
                diagnostic_messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER,
                diagnostic_codes::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPER,
            );
            return true;
        }

        // TS2777: The operand of an increment or decrement operator may not be an optional property access.
        if self.is_optional_chain_access(inner) {
            self.error_at_node(
                operand_idx,
                diagnostic_messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT,
                diagnostic_codes::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERT,
            );
            return true;
        }

        false
    }

    /// Skip through transparent wrapper expressions that don't affect l-value validity.
    ///
    /// Skips: parenthesized, non-null assertion (`!`), type assertion (`as`/angle-bracket),
    /// and `satisfies` expressions.
    fn skip_assignment_transparent_wrappers(&self, idx: NodeIndex) -> NodeIndex {
        self.ctx.arena.skip_parenthesized_and_assertions(idx)
    }

    /// Check if the assignment target (LHS) is a const variable and emit TS2588 if so.
    ///
    /// Resolves through parenthesized expressions to find the underlying identifier.
    /// Returns `true` if a TS2588 error was emitted (caller should skip further type checks).
    pub(crate) fn check_const_assignment(&mut self, target_idx: NodeIndex) -> bool {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);
        if let Some(name) = self.get_const_variable_name(inner) {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CONSTANT,
                &[&name],
            );
            return true;
        }
        false
    }

    /// TS1100: Cannot assign to `eval` or `arguments` in strict mode.
    pub(crate) fn check_strict_mode_eval_or_arguments_assignment(&mut self, target_idx: NodeIndex) {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(node) = self.ctx.arena.get(inner) else {
            return;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return;
        }
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return;
        };
        let name = &ident.escaped_text;
        if crate::state_checking::is_eval_or_arguments(name) && self.is_strict_mode_for_node(inner)
        {
            self.emit_eval_or_arguments_strict_mode_error(inner, name);
        }
    }

    /// Check if assignment target is a function and emit TS2630 error.
    ///
    /// TypeScript does not allow direct assignment to functions:
    /// ```typescript
    /// function foo() {}
    /// foo = bar;  // Error TS2630: Cannot assign to 'foo' because it is a function.
    /// ```
    ///
    /// Also checks for built-in global functions (eval, arguments) which always
    /// emit TS2630 when assigned to, even without explicit function declarations.
    ///
    /// This check helps catch common mistakes where users try to reassign function names.
    pub(crate) fn check_function_assignment(&mut self, target_idx: NodeIndex) -> bool {
        let inner = self.ctx.arena.skip_parenthesized(target_idx);

        // Only check identifiers - property access like obj.fn = x is allowed
        let Some(node) = self.ctx.arena.get(inner) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        // Get the identifier name
        let Some(id_data) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        let name = &id_data.escaped_text;

        // `undefined` is not a variable — it's a global constant that cannot be assigned to.
        // TypeScript emits TS2539 for `undefined = ...` or `undefined++` etc.
        if name == "undefined" {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_NOT_A_VARIABLE,
                &[name],
            );
            return true;
        }

        // Check for built-in global functions that always error with TS2630
        // Note: `arguments` is NOT included here because inside function bodies,
        // `arguments` is an IArguments object (handled by type_computation_complex.rs).
        // Only at module scope would `arguments` resolve to a function-like global.
        if name == "eval" {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION,
                &[name],
            );
            return true;
        }

        // TS2632: Check if this identifier is an import binding BEFORE resolving
        // through imports. resolve_identifier follows aliases, so the resolved symbol
        // would be the export target (e.g., `var x`) rather than the import binding.
        // Import bindings are readonly in ESM — you cannot reassign them.
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(local_sym) = self.ctx.binder.get_symbol(local_sym_id)
            && local_sym.flags & symbol_flags::ALIAS != 0
        {
            self.error_at_node_msg(
                inner,
                diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_IMPORT,
                &[name],
            );
            return true;
        }

        // Look up the symbol for this identifier by resolving it through the scope chain
        // Note: We use resolve_identifier instead of node_symbols because node_symbols
        // only contains declaration nodes, not identifier references.
        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, inner);
        let Some(sym_id) = sym_id else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check for uninstantiated namespaces first (TS2708)
        let is_namespace = (symbol.flags & symbol_flags::NAMESPACE_MODULE) != 0;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = (symbol.flags & value_flags_except_module) != 0;

        if is_namespace && !has_other_value {
            let mut is_instantiated = false;
            for decl_idx in &symbol.declarations {
                if self.is_namespace_declaration_instantiated(*decl_idx) {
                    is_instantiated = true;
                    break;
                }
            }
            if !is_instantiated {
                self.error_namespace_used_as_value_at(name, inner);
                return true;
            }
        }

        // Check for type-only symbols used as values in assignment position (TS2693)
        if symbol.flags & symbol_flags::TYPE != 0 && symbol.flags & symbol_flags::VALUE == 0 {
            self.error_type_only_value_at(name, inner);
            return true;
        }

        // Check if this symbol is a class, enum, function, or namespace (TS2629, TS2628, TS2630, TS2631)
        let code = if symbol.flags & symbol_flags::MODULE != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_NAMESPACE
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CLASS
        } else if symbol.flags & symbol_flags::ENUM != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_ENUM
        } else if symbol.flags & symbol_flags::FUNCTION != 0 {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION
        } else {
            return false;
        };

        self.error_at_node_msg(inner, code, &[name]);
        true
    }

    fn is_js_namespace_enum_rebind_assignment_target(&self, target_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if !self.is_js_file() {
            return false;
        }

        let target_idx = self.ctx.arena.skip_parenthesized(target_idx);
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        if let Some(member_sym_id) = self.resolve_qualified_symbol(target_idx)
            && let Some(member_symbol) = self
                .get_cross_file_symbol(member_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
            && (member_symbol.flags & symbol_flags::ENUM) != 0
        {
            let parent_sym_id = member_symbol.parent;
            if let Some(parent_symbol) = self
                .get_cross_file_symbol(parent_sym_id)
                .or_else(|| self.ctx.binder.get_symbol(parent_sym_id))
                && (parent_symbol.flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE
                        | symbol_flags::NAMESPACE_MODULE))
                    != 0
                && (parent_symbol.flags & symbol_flags::ENUM) == 0
            {
                return true;
            }
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        let Some(prop_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument) else {
            return false;
        };

        let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(base_symbol) = self
            .get_cross_file_symbol(base_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
        else {
            return false;
        };
        if (base_symbol.flags
            & (symbol_flags::MODULE | symbol_flags::NAMESPACE | symbol_flags::NAMESPACE_MODULE))
            == 0
        {
            return false;
        }

        let Some(exports) = base_symbol.exports.as_ref() else {
            return false;
        };
        let Some(member_sym_id) = exports.get(prop_ident.escaped_text.as_str()) else {
            return false;
        };
        let Some(member_symbol) = self
            .get_cross_file_symbol(member_sym_id)
            .or_else(|| self.ctx.binder.get_symbol(member_sym_id))
        else {
            return false;
        };

        (member_symbol.flags & symbol_flags::ENUM) != 0
    }

    /// Check an assignment expression (=).
    ///
    /// ## Contextual Typing:
    /// - The LHS type is used as contextual type for the RHS expression
    /// - This enables better type inference for object literals, etc.
    ///
    /// ## Validation:
    /// - Checks constructor accessibility (if applicable)
    /// - Validates that RHS is assignable to LHS
    /// - Checks for excess properties in object literals
    /// - Validates readonly assignments
    pub(crate) fn check_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        // TS2364: The left-hand side of an assignment expression must be a variable or a property access.
        // Suppress when the LHS is near a parse error (e.g. `1 >>/**/= 2;` where `>>=` is split
        // by a comment — the parser already emits TS1109 and the assignment is a recovery artifact).
        if !self.is_valid_assignment_target(left_idx) && !self.node_has_nearby_parse_error(left_idx)
        {
            self.error_at_node(
                left_idx,
                "The left-hand side of an assignment expression must be a variable or a property access.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY,
            );
            self.get_type_of_node(left_idx);
            self.get_type_of_node(right_idx);
            return TypeId::ANY;
        }

        // TS2779: The left-hand side of an assignment expression may not be an optional property access.
        {
            let inner = self.skip_assignment_transparent_wrappers(left_idx);
            if self.is_optional_chain_access(inner) {
                self.error_at_node(
                    left_idx,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                );
            }
        }

        // TS2588: Cannot assign to 'x' because it is a constant.
        // Check early - if this fires, skip type assignability checks (tsc behavior).
        let is_const = self.check_const_assignment(left_idx);

        // TS2630: Cannot assign to 'x' because it is a function.
        // This check must come after valid assignment target check but before type checking.
        let is_function_assignment = self.check_function_assignment(left_idx);

        // TS1100: Cannot assign to `eval` or `arguments` in strict mode.
        self.check_strict_mode_eval_or_arguments_assignment(left_idx);

        // Set destructuring flag when LHS is an object/array pattern to suppress
        // TS1117 (duplicate property) checks in destructuring targets.
        let (is_destructuring, is_array_destructuring) =
            if let Some(left_node) = self.ctx.arena.get(left_idx) {
                let is_obj = left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
                let is_arr = left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
                (is_obj || is_arr, is_arr)
            } else {
                (false, false)
            };
        let prev_destructuring = self.ctx.in_destructuring_target;
        if is_destructuring {
            self.ctx.in_destructuring_target = true;
        }
        let left_target = self.get_type_of_assignment_target(left_idx);
        self.ctx.in_destructuring_target = prev_destructuring;
        let mut left_type = self.resolve_type_query_type(left_target);

        // In JS/checkJs mode, allow JSDoc `@type` on assignment statements to
        // provide the contextual target type for the LHS.
        //
        // Example:
        //   /** @type {string} */
        //   C.prototype = 12;
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && let Some(jsdoc_left_type) = self
                .jsdoc_type_annotation_for_node_direct(expr_idx)
                .or_else(|| self.jsdoc_type_annotation_for_node_direct(left_idx))
        {
            left_type = jsdoc_left_type;
        }

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY
            && left_type != TypeId::NEVER
            && left_type != TypeId::UNKNOWN
            && !self.type_contains_error(left_type)
        {
            let contextual_target = if let Some(right_node) = self.ctx.arena.get(right_idx) {
                if right_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || right_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                {
                    self.evaluate_contextual_type(left_type)
                } else {
                    left_type
                }
            } else {
                left_type
            };
            self.ctx.contextual_type = Some(contextual_target);
            if let Some(right_node) = self.ctx.arena.get(right_idx) {
                let needs_fresh_contextual_check = right_node.kind
                    == syntax_kind_ext::ARROW_FUNCTION
                    || right_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                    || (right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && self.ctx.arena.get_binary_expr(right_node).is_some_and(|bin| {
                            matches!(
                                bin.operator_token,
                                k if k == SyntaxKind::BarBarToken as u16
                                    || k == SyntaxKind::AmpersandAmpersandToken as u16
                                    || k == SyntaxKind::QuestionQuestionToken as u16
                                    || k == SyntaxKind::CommaToken as u16
                            )
                        }));
                if needs_fresh_contextual_check {
                    self.clear_type_cache_recursive(right_idx);
                }
            }
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ctx.contextual_type = prev_context;

        if is_function_assignment {
            // TS2630 is terminal in TypeScript for simple assignment targets.
            // Avoid cascading TS2322/other assignability diagnostics.
            return right_type;
        }

        self.ensure_relation_input_ready(right_type);
        self.ensure_relation_input_ready(left_type);

        if is_array_destructuring {
            // TS2488: Array destructuring assignments require an iterable RHS.
            // Keep parity with `[] = value` behavior by skipping empty patterns.
            let should_check_iterability = self
                .ctx
                .arena
                .get(left_idx)
                .and_then(|node| self.ctx.arena.get_literal_expr(node))
                .is_none_or(|array_lit| !array_lit.elements.nodes.is_empty());
            if should_check_iterability {
                self.check_destructuring_iterability(left_idx, right_type, NodeIndex::NONE);
            }
            self.check_array_destructuring_rest_position(left_idx);
            self.check_tuple_destructuring_bounds(left_idx, right_type);
        }

        let is_readonly = if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx)
        } else {
            false
        };

        if !is_const && !is_readonly && self.is_js_namespace_enum_rebind_assignment_target(left_idx)
        {
            return right_type;
        }

        if !is_const && !is_readonly && left_type != TypeId::ANY {
            // For destructuring assignments (both object and array patterns),
            // skip the whole-object assignability check. tsc processes each
            // property/element individually, which correctly handles private
            // members and other access-controlled properties.
            let mut check_assignability = !is_destructuring;

            if is_destructuring {
                self.report_abstract_properties_in_destructuring_assignment(left_idx, right_idx);
                self.check_destructuring_property_accessibility(left_idx, right_type);
            }

            if check_assignability {
                let widened_left = tsz_solver::widening::widen_type(self.ctx.types, left_type);
                if widened_left != left_type
                    && let Some(right_node) = self.ctx.arena.get(right_idx)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    use tsz_scanner::SyntaxKind;
                    if right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(bin) = self.ctx.arena.get_binary_expr(right_node)
                    {
                        let op = bin.operator_token;
                        let is_compound_like = op == SyntaxKind::PlusToken as u16
                            || op == SyntaxKind::MinusToken as u16
                            || op == SyntaxKind::AsteriskToken as u16
                            || op == SyntaxKind::SlashToken as u16
                            || op == SyntaxKind::PercentToken as u16
                            || op == SyntaxKind::AsteriskAsteriskToken as u16
                            || op == SyntaxKind::LessThanLessThanToken as u16
                            || op == SyntaxKind::GreaterThanGreaterThanToken as u16
                            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16;

                        if is_compound_like && self.is_assignable_to(right_type, widened_left) {
                            check_assignability = false;
                        }
                    }
                }
            }

            self.check_assignment_compatibility(
                left_idx,
                right_idx,
                right_type,
                left_type,
                check_assignability, // check_assignability
                true,
            );

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        right_type
    }

    fn check_tuple_destructuring_bounds(&mut self, left_idx: NodeIndex, right_type: TypeId) {
        let rhs = tsz_solver::type_queries::unwrap_readonly(self.ctx.types, right_type);

        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        // Single tuple case
        if let Some(tuple_elements) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, rhs)
        {
            let has_rest_tail = tuple_elements.last().is_some_and(|element| element.rest);
            if has_rest_tail {
                return;
            }

            for (index, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
                if index < tuple_elements.len() || element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    return;
                }

                let tuple_type_str = self.format_type(rhs);
                self.error_at_node(
                    element_idx,
                    &format!(
                        "Tuple type '{}' of length '{}' has no element at index '{}'.",
                        tuple_type_str,
                        tuple_elements.len(),
                        index
                    ),
                    diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                );
                return;
            }
            return;
        }

        // Union of tuples case: check if ALL members are out of bounds
        if let Some(members) =
            tsz_solver::type_queries::data::get_union_members(self.ctx.types, rhs)
        {
            for (index, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION
                    || element_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                {
                    continue;
                }

                let all_out_of_bounds = !members.is_empty()
                    && members.iter().all(|&m| {
                        let m = tsz_solver::type_queries::unwrap_readonly(self.ctx.types, m);
                        if let Some(elems) =
                            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, m)
                        {
                            let has_rest = elems.iter().any(|e| e.rest);
                            !has_rest && index >= elems.len()
                        } else {
                            false
                        }
                    });

                if all_out_of_bounds {
                    let type_str = self.format_type(right_type);
                    self.error_at_node(
                        element_idx,
                        &format!("Property '{index}' does not exist on type '{type_str}'.",),
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                    return;
                }
            }
        }
    }

    /// TS2462: A rest element in array destructuring must be the last element.
    ///
    /// Enforce syntax for array destructuring assignment targets.
    fn check_array_destructuring_rest_position(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        let elements_len = array_lit.elements.nodes.len();
        if elements_len == 0 {
            return;
        }
        for (i, &element_idx) in array_lit.elements.nodes.iter().enumerate() {
            if i + 1 >= elements_len {
                break;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                self.error_at_node_msg(
                    element_idx,
                    diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    &[],
                );
            }
        }
    }

    // =========================================================================
    // Arithmetic Operand Validation
    // =========================================================================

    /// Check if an operand type is valid for arithmetic operations.
    ///
    /// Returns true if the type is number, bigint, any, or an enum type.
    /// This is used to validate operands for TS2362/TS2363 errors.
    fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
        use tsz_solver::BinaryOpEvaluator;

        // Check if this is an enum type (Lazy/DefId to an enum symbol)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // Check if the symbol is an enum (ENUM flags)
            use tsz_binder::symbol_flags;
            if (symbol.flags & symbol_flags::ENUM) != 0 {
                return true;
            }
        }

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        evaluator.is_arithmetic_operand(type_id)
    }

    /// Check and emit TS2362/TS2363 errors for arithmetic operations.
    ///
    /// For operators like -, *, /, %, **, -=, *=, /=, %=, **=,
    /// validates that operands are of type number, bigint, any, or enum.
    /// Emits appropriate errors when operands are invalid.
    /// Returns true if any error was emitted.
    fn check_arithmetic_operands(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) -> bool {
        // Evaluate types to resolve unevaluated conditional/mapped types before checking.
        // e.g. DeepPartial<number> (conditional: number extends object ? ... : number) → number
        let left_eval = self.evaluate_type_for_binary_ops(left_type);
        let right_eval = self.evaluate_type_for_binary_ops(right_type);
        let left_is_valid = self.is_arithmetic_operand(left_eval);
        let right_is_valid = self.is_arithmetic_operand(right_eval);

        // When strictNullChecks is on, null/undefined operands get TS18050 ("The value
        // 'null'/'undefined' cannot be used here") which takes priority over TS2362/TS2363.
        // When strictNullChecks is off, null/undefined are in number's domain and
        // should not trigger arithmetic errors either.
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;

        let mut emitted = false;

        if !left_is_valid && !(left_is_nullish) {
            self.error_at_node(
                left_idx,
                "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        if !right_is_valid && !(right_is_nullish) {
            self.error_at_node(
                right_idx,
                "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        emitted || !left_is_valid || !right_is_valid
    }

    /// Emit TS2447 error for boolean bitwise operators (&, |, ^, &=, |=, ^=).
    fn emit_boolean_operator_error(&mut self, node_idx: NodeIndex, op_str: &str, suggestion: &str) {
        let message = format!(
            "The '{op_str}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
        );
        self.error_at_node(
            node_idx,
            &message,
            diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
        );
    }

    // =========================================================================
    // Compound Assignment Checking
    // =========================================================================

    /// Check a compound assignment expression (+=, &&=, ??=, etc.).
    ///
    /// Compound assignments have special type computation rules:
    /// - Logical assignments (&&=, ||=, ??=) assign the RHS type
    /// - Other compound assignments assign the computed result type
    ///
    /// ## Type Computation:
    /// - Numeric operators (+, -, *, /, %) compute number type
    /// - Bitwise operators compute number type
    /// - Logical operators return RHS type
    pub(crate) fn check_compound_assignment_expression(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        operator: u16,
        expr_idx: NodeIndex,
    ) -> TypeId {
        // TS2364: The left-hand side of an assignment expression must be a variable or a property access.
        // Suppress when near a parse error (same rationale as in check_assignment_expression).
        if !self.is_valid_assignment_target(left_idx) && !self.node_has_nearby_parse_error(left_idx)
        {
            self.error_at_node(
                left_idx,
                "The left-hand side of an assignment expression must be a variable or a property access.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY,
            );
            self.get_type_of_node(left_idx);
            self.get_type_of_node(right_idx);
            return TypeId::ANY;
        }

        // TS2779: The left-hand side of an assignment expression may not be an optional property access.
        {
            let inner = self.skip_assignment_transparent_wrappers(left_idx);
            if self.is_optional_chain_access(inner) {
                self.error_at_node(
                    left_idx,
                    diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                    diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
                );
            }
        }

        // TS2588: Cannot assign to 'x' because it is a constant.
        let is_const = self.check_const_assignment(left_idx);

        // TS2629/TS2628/TS2630: Cannot assign to class/enum/function.
        let is_function_assignment = self.check_function_assignment(left_idx);

        // TS1100: Cannot assign to `eval` or `arguments` in strict mode.
        self.check_strict_mode_eval_or_arguments_assignment(left_idx);

        // Compound assignments read the LHS before writing, so the LHS identifier
        // must go through definite assignment analysis (TS2454). Without this,
        // `var x: number; x += 1;` would not trigger "used before assigned".
        if let Some(left_node) = self.ctx.arena.get(left_idx)
            && left_node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol(left_idx)
        {
            let declared_type = self.get_type_of_symbol(sym_id);
            self.check_flow_usage(left_idx, declared_type, sym_id);
        }

        // Compound assignments also read the LHS value. For private setter-only
        // accessors, this triggers TS2806 ("Private accessor was defined without
        // a getter"). Evaluate in read context first.
        let _ = self.get_type_of_node(left_idx);

        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY
            && left_type != TypeId::NEVER
            && left_type != TypeId::UNKNOWN
            && !self.type_contains_error(left_type)
        {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ctx.contextual_type = prev_context;

        self.ensure_relation_input_ready(right_type);
        self.ensure_relation_input_ready(left_type);

        let is_readonly = if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx)
        } else {
            false
        };

        // Track whether an operator error was emitted so we can suppress cascading TS2322.
        // TSC doesn't emit TS2322 when there's already an operator error (TS2447/TS2362/TS2363).
        let mut emitted_operator_error = is_const || is_readonly || is_function_assignment;

        let op_str = match operator {
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+",
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-",
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*",
            k if k == SyntaxKind::SlashEqualsToken as u16 => "/",
            k if k == SyntaxKind::PercentEqualsToken as u16 => "%",
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**",
            k if k == SyntaxKind::AmpersandEqualsToken as u16 => "&",
            k if k == SyntaxKind::BarEqualsToken as u16 => "|",
            k if k == SyntaxKind::CaretEqualsToken as u16 => "^",
            k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<",
            k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>",
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => ">>>",
            _ => "",
        };

        if !op_str.is_empty() {
            emitted_operator_error |= self.check_and_emit_nullish_binary_operands(
                left_idx, right_idx, left_type, right_type, op_str,
            );
        }

        // TS2469: For += with symbol operands, emit when one side is symbol and the
        // other is string or any. Uses "+=" in the message (not "+").
        if operator == SyntaxKind::PlusEqualsToken as u16
            && left_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            let left_is_symbol = evaluator.is_symbol_like(left_type);
            let right_is_symbol = evaluator.is_symbol_like(right_type);
            if left_is_symbol || right_is_symbol {
                let left_is_string_or_any = left_type == TypeId::ANY
                    || left_type == TypeId::STRING
                    || tsz_solver::type_queries::is_string_literal(self.ctx.types, left_type);
                let right_is_string_or_any = right_type == TypeId::ANY
                    || right_type == TypeId::STRING
                    || tsz_solver::type_queries::is_string_literal(self.ctx.types, right_type);
                let should_emit_2469 = (left_is_symbol && right_is_string_or_any)
                    || (right_is_symbol && left_is_string_or_any);
                if should_emit_2469 {
                    use crate::diagnostics::diagnostic_codes;
                    if left_is_symbol {
                        self.error_at_node_msg(
                            left_idx,
                            diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                            &["+="],
                        );
                        emitted_operator_error = true;
                    }
                    if right_is_symbol {
                        self.error_at_node_msg(
                            right_idx,
                            diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                            &["+="],
                        );
                        emitted_operator_error = true;
                    }
                }
            }
        }

        // TS2365: For +=, check if the + operation is valid using the solver.
        // Emit "Operator '+=' cannot be applied to types X and Y" when the operands
        // aren't compatible for addition (neither both numeric, both string, nor one any).
        // Skip if a more specific error (TS18050 for null/undefined, TS2469 for symbol)
        // was already emitted.
        if operator == SyntaxKind::PlusEqualsToken as u16
            && !emitted_operator_error
            && left_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            // Evaluate types to resolve IndexAccess/Application types before checking.
            // e.g. `T[K]` where `T extends Record<K, number>` should resolve to `number`
            // so the += operator is correctly accepted.
            let eval_left = self.evaluate_type_for_binary_ops(left_type);
            let eval_right = self.evaluate_type_for_binary_ops(right_type);
            let result = evaluator.evaluate(eval_left, eval_right, "+");
            if let tsz_solver::BinaryOpResult::TypeError { .. } = result {
                // For the diagnostic message, tsc uses widened types for most
                // operands (e.g., `0` → `number`, `true` → `boolean`).
                // Widen literal types to base types and enum members to
                // parent enums, matching tsc behavior for messages like
                // "Operator '+=' cannot be applied to types 'boolean' and 'number'."
                let left_diag = self.widen_enum_member_type(tsz_solver::widen_literal_type(
                    self.ctx.types,
                    left_type,
                ));
                let right_diag = self.widen_enum_member_type(tsz_solver::widen_literal_type(
                    self.ctx.types,
                    right_type,
                ));
                let left_str = self.format_type(left_diag);
                let right_str = self.format_type(right_diag);
                let message = format!(
                    "Operator '+=' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.error_at_node(
                    expr_idx,
                    &message,
                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                );
                emitted_operator_error = true;
            }
        }

        // Check arithmetic operands for compound arithmetic assignments
        // Emit TS2362/TS2363 for -=, *=, /=, %=, **=
        let is_arithmetic_compound = matches!(
            operator,
            k if k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
        );
        if is_arithmetic_compound && !is_function_assignment {
            // Don't emit arithmetic errors if either operand is ERROR - prevents cascading errors
            if left_type != TypeId::ERROR && right_type != TypeId::ERROR {
                emitted_operator_error |=
                    self.check_arithmetic_operands(left_idx, right_idx, left_type, right_type);
            }
        }

        // TS2791: bigint exponentiation assignment requires target >= ES2016.
        // Skip when either type is any/unknown (TSC skips the bigint branch for those).
        if operator == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            && (self.ctx.compiler_options.target as u32)
                < (tsz_common::common::ScriptTarget::ES2016 as u32)
            && left_type != TypeId::ANY
            && right_type != TypeId::ANY
            && left_type != TypeId::UNKNOWN
            && right_type != TypeId::UNKNOWN
            && self.is_subtype_of(left_type, TypeId::BIGINT)
            && self.is_subtype_of(right_type, TypeId::BIGINT)
        {
            self.error_at_node_msg(
                expr_idx,
                crate::diagnostics::diagnostic_codes::EXPONENTIATION_CANNOT_BE_PERFORMED_ON_BIGINT_VALUES_UNLESS_THE_TARGET_OPTION_IS,
                &[],
            );
            emitted_operator_error = true;
        }

        // Check bitwise compound assignments: &=, |=, ^=, <<=, >>=, >>>=
        let is_boolean_bitwise_compound = matches!(
            operator,
            k if k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        );
        let is_shift_compound = matches!(
            operator,
            k if k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
        );
        if is_boolean_bitwise_compound && !is_function_assignment {
            // TS2447: For &=, |=, ^= with both boolean operands, emit special error
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            let left_is_boolean = evaluator.is_boolean_like(left_type);
            let right_is_boolean = evaluator.is_boolean_like(right_type);
            if left_is_boolean && right_is_boolean {
                let (op_str, suggestion) = match operator {
                    k if k == SyntaxKind::AmpersandEqualsToken as u16 => ("&=", "&&"),
                    k if k == SyntaxKind::BarEqualsToken as u16 => ("|=", "||"),
                    _ => ("^=", "!=="),
                };
                self.emit_boolean_operator_error(left_idx, op_str, suggestion);
                emitted_operator_error = true;
            } else if left_type != TypeId::ERROR && right_type != TypeId::ERROR {
                emitted_operator_error |=
                    self.check_arithmetic_operands(left_idx, right_idx, left_type, right_type);
            }
        } else if is_shift_compound
            && !is_function_assignment
            && left_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            emitted_operator_error |=
                self.check_arithmetic_operands(left_idx, right_idx, left_type, right_type);
        }

        let result_type = self.compound_assignment_result_type(left_type, right_type, operator);
        let is_logical_assignment = matches!(
            operator,
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
        );
        let assigned_type = if is_logical_assignment {
            right_type
        } else {
            result_type
        };

        if left_type != TypeId::ANY && !emitted_operator_error {
            self.check_assignment_compatibility(
                left_idx,
                right_idx,
                assigned_type,
                left_type,
                true,
                false,
            );

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        result_type
    }

    /// Compute the result type of a compound assignment operator.
    ///
    /// This function determines what type a compound assignment expression
    /// produces based on the operator and operand types.
    fn compound_assignment_result_type(
        &self,
        left_type: TypeId,
        right_type: TypeId,
        operator: u16,
    ) -> TypeId {
        use tsz_solver::{BinaryOpEvaluator, BinaryOpResult};

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let op_str = match operator {
            k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
            k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::SlashEqualsToken as u16 => Some("/"),
            k if k == SyntaxKind::PercentEqualsToken as u16 => Some("%"),
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => Some("&&"),
            k if k == SyntaxKind::BarBarEqualsToken as u16 => Some("||"),
            k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => Some("??"),
            _ => None,
        };

        if let Some(op) = op_str {
            return match evaluator.evaluate(left_type, right_type, op) {
                BinaryOpResult::Success(result) => result,
                // Return ANY instead of UNKNOWN for type errors to prevent cascading errors
                BinaryOpResult::TypeError { .. } => TypeId::ANY,
            };
        }

        if matches!(
            operator,
            k if k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
        ) {
            return TypeId::NUMBER;
        }

        // Return ANY for unknown binary operand types to prevent cascading errors
        TypeId::ANY
    }

    fn check_assignment_compatibility(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
        check_assignability: bool,
        suppress_error_for_error_types: bool,
    ) {
        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
        {
            self.error_constructor_accessibility_not_assignable(
                source_type,
                target_type,
                source_level,
                target_level,
                left_idx,
            );
            return;
        }

        if !check_assignability {
            return;
        }

        if suppress_error_for_error_types
            && (source_type == TypeId::ERROR || target_type == TypeId::ERROR)
        {
            return;
        }

        if let Some(generic_target) =
            self.deferred_generic_element_write_target(left_idx, source_type)
        {
            let _ = self.check_assignable_or_report_at(
                source_type,
                generic_target,
                right_idx,
                left_idx,
            );
            return;
        }

        // TS2322 anchoring should point at the assignment target (LHS), not the RHS expression.
        // This aligns diagnostic fingerprints with tsc for assignment-compatibility suites.
        let _ = self.check_assignable_or_report_at(source_type, target_type, right_idx, left_idx);
    }

    fn deferred_generic_element_write_target(
        &mut self,
        left_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<TypeId> {
        if source_type == TypeId::ANY
            || source_type == TypeId::NEVER
            || crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                source_type,
            )
        {
            return None;
        }

        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let object_type = self.get_type_of_node(access.expression);
        if !tsz_solver::visitor::is_type_parameter(self.ctx.types, object_type) {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let index_type = self.get_type_of_node(access.name_or_argument);
        self.ctx.preserve_literal_types = prev_preserve;

        if !self.is_valid_index_for_type_param(index_type, object_type) {
            return None;
        }

        Some(
            self.ctx
                .types
                .factory()
                .index_access(object_type, index_type),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;

    fn diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        check_source(source, "test.ts", CheckerOptions::default())
    }

    #[test]
    fn conditional_type_intersection_assignment_ts2322() {
        // tsc emits TS2322 for both assignments because Something<A> contains
        // a deferred conditional type in its intersection.
        let source = r#"
            type Something<T> = { test: string } & (T extends object ? {
                arg: T
            } : {
                arg?: undefined
            });

            function testFunc2<A extends object>(a: A, sa: Something<A>) {
                sa = { test: 'hi', arg: a };
                sa = { test: 'bye', arg: a, arr: a };
            }
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
        assert!(
            ts2322_count >= 2,
            "expected at least 2 TS2322 for assigning to intersection with deferred conditional, got {ts2322_count}. Diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn constructor_accessibility_assignment_error_targets_lhs() {
        let source = r#"
            class Foo {
                constructor(public x: number) {}
            }
            class Bar {
                protected constructor(public x: number) {}
            }
            let a = Foo;
            a = Bar;
        "#;

        let diagnostics = diagnostics_for(source);
        let diag = diagnostics
            .iter()
            .find(|d| d.code == 2322)
            .expect("expected TS2322");

        let expected_start = source.find("a = Bar").expect("expected assignment span") as u32;

        assert_eq!(
            diag.start, expected_start,
            "TS2322 should be anchored to LHS"
        );
        assert_eq!(
            diag.length, 1,
            "TS2322 should target only the assignment target"
        );
    }

    #[test]
    fn non_distributive_conditional_with_any_evaluates_to_true_branch() {
        // `[any] extends [number] ? 1 : 0` should evaluate to `1` (non-distributive).
        // `any extends number ? 1 : 0` should evaluate to `0 | 1` (distributive, picks both).
        // Assigning `0` to `U` (= 1) should emit TS2322, with message "...to type '1'",
        // NOT "...to type '[any] extends [number] ? 1 : 0'".
        let source = r#"
            type T = any extends number ? 1 : 0;
            let x: T;
            x = 1;
            x = 0;

            type U = [any] extends [number] ? 1 : 0;
            let y: U;
            y = 1;
            y = 0;
        "#;

        let diagnostics = diagnostics_for(source);
        // `x = 0` should NOT error: T = 0 | 1, and 0 is assignable to 0 | 1
        let x_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code == 2322
                    && d.message_text.contains("'0'")
                    && d.message_text.contains("'0 | 1'")
            })
            .collect();
        assert!(
            x_errors.is_empty(),
            "x = 0 should not error since T = 0 | 1"
        );

        // `y = 0` should error: U = 1, and 0 is not assignable to 1
        let y_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code == 2322 && d.message_text.contains("'0'") && d.message_text.contains("'1'")
            })
            .collect();
        assert_eq!(
            y_errors.len(),
            1,
            "y = 0 should emit TS2322 with type '1', got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );

        // The error message should reference the evaluated type '1', not the deferred conditional
        assert!(
            !y_errors[0].message_text.contains("extends"),
            "Error message should use evaluated type '1', not deferred conditional. Got: {}",
            y_errors[0].message_text
        );
    }

    #[test]
    fn union_keyed_index_write_type_is_intersection() {
        // When writing to obj[k] where k is a union key, the write type is the
        // intersection of all property types. For `{ a: string, b: number }` with
        // key `'a' | 'b'`, write type = string & number = never.
        // tsc emits TS2322: Type 'any' is not assignable to type 'never'.
        let source = r#"
            const x1 = { a: 'foo', b: 42 };
            declare let k: 'a' | 'b';
            x1[k] = 'bar' as any;
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
        assert_eq!(
            ts2322_count,
            1,
            "expected 1 TS2322 for assigning any to never (intersection of string & number), got {ts2322_count}. Diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn any_not_assignable_to_never() {
        // tsc: Type 'any' is not assignable to type 'never'. (TS2322)
        // `any` bypasses most type checks but cannot be assigned to `never`.
        let source = r#"
            declare let x: never;
            x = 'bar' as any;
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
        assert_eq!(
            ts2322_count,
            1,
            "expected 1 TS2322 for assigning any to never, got {ts2322_count}. Diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn generic_conditional_type_alias_stays_deferred() {
        // Generic type aliases should NOT be eagerly evaluated — they stay deferred
        // until instantiated. This ensures we don't break generic conditional types.
        let source = r#"
            type IsString<T> = T extends string ? true : false;
            let a: IsString<string> = true;
            let b: IsString<number> = false;
            let c: IsString<string> = false;
        "#;

        let diagnostics = diagnostics_for(source);
        // `c = false` should error: IsString<string> = true, and false is not assignable to true
        let errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
        assert_eq!(
            errors.len(),
            1,
            "expected 1 TS2322 for `c = false`, got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn private_setter_only_no_false_ts2322() {
        // A class with a set-only private accessor should not emit TS2322
        // when assigning to it. The write type (setter param) is `number`,
        // not the read type (`undefined`).
        let source = r#"
            class C {
                set #foo(a: number) {}
                bar() {
                    let x = (this.#foo = 42 * 2);
                }
            }
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
        assert_eq!(
            ts2322,
            0,
            "setter-only private accessor should not produce TS2322, got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn private_setter_only_read_emits_ts2806() {
        // Reading from a private setter-only accessor should emit TS2806
        // ("Private accessor was defined without a getter"), not cascade
        // into TS2532/TS2488 from the `undefined` read type.
        let source = r#"
            class C {
                set #foo(a: number) {}
                bar() {
                    console.log(this.#foo);
                }
            }
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2806 = diagnostics.iter().filter(|d| d.code == 2806).count();
        assert_eq!(
            ts2806,
            1,
            "expected 1 TS2806 for reading setter-only private accessor, got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
        // Should NOT produce cascading TS2532 (possibly undefined)
        let ts2532 = diagnostics.iter().filter(|d| d.code == 2532).count();
        assert_eq!(ts2532, 0, "should not cascade into TS2532");
    }

    #[test]
    fn private_setter_only_compound_assignment_emits_ts2806() {
        // Compound assignments (`+=`) read the LHS, so setter-only accessors
        // should trigger TS2806 for the read part.
        let source = r#"
            class C {
                set #val(a: number) {}
                bar() {
                    this.#val += 3;
                }
            }
        "#;

        let diagnostics = diagnostics_for(source);
        let ts2806 = diagnostics.iter().filter(|d| d.code == 2806).count();
        assert_eq!(
            ts2806,
            1,
            "expected 1 TS2806 for compound assignment to setter-only private accessor, got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn inner_assignment_in_variable_decl_anchors_at_assignment_target() {
        let source = r#"interface A { x: number; }
interface B { y: string; }
declare let b: B;
declare let a: A;
const x = a = b;"#;

        let diagnostics = diagnostics_for(source);

        let ts2741: Vec<_> = diagnostics.iter().filter(|d| d.code == 2741).collect();
        assert!(
            !ts2741.is_empty(),
            "expected TS2741 for inner assignment in variable decl, got: {:?}",
            diagnostics
                .iter()
                .map(|d| (d.code, d.start, &d.message_text))
                .collect::<Vec<_>>()
        );

        // The diagnostic should anchor at `a` (the inner assignment target),
        // NOT at `const` (the variable statement start).
        let diag = ts2741[0];
        let a_offset = source.find("const x = a = b;").unwrap() + "const x = ".len();
        assert_eq!(
            diag.start as usize, a_offset,
            "TS2741 should point to inner assignment target 'a' (offset {}), not offset {}",
            a_offset, diag.start
        );
    }
}
