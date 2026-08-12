//! Assignment operator utilities, validation, JS/CommonJS helpers,
//! polymorphic this checking, tuple/array destructuring bounds, and arithmetic checks.

use crate::context::TypingRequest;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use crate::state_checking::readonly::ReadonlyAssignmentDiagnostic;
use tsz_binder::symbol_flags;
use tsz_common::diagnostics::get_message_template;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a token is an assignment operator (=, +=, -=, etc.)
    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
        crate::query_boundaries::common::is_assignment_operator(operator)
    }

    fn is_js_prototype_private_name_assignment_target(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let outer_access = self.ctx.arena.get_access_expr(node)?;
        let name_node = self.ctx.arena.get(outer_access.name_or_argument)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }

        let proto_node = self.ctx.arena.get(outer_access.expression)?;
        if proto_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let proto_access = self.ctx.arena.get_access_expr(proto_node)?;
        let is_prototype = self
            .ctx
            .arena
            .get_identifier_at(proto_access.name_or_argument)
            .is_some_and(|ident| ident.escaped_text == "prototype");
        is_prototype.then_some(outer_access.name_or_argument)
    }

    fn is_checked_js_implicit_any_member_assignment_target(&self, idx: NodeIndex) -> bool {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        if self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .is_none()
        {
            return false;
        };
        self.ctx.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE
                && diag.start == name_node.pos
        })
    }

    fn is_control_flow_typed_any_assignment_target(&mut self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        self.resolve_identifier_symbol_for_write(idx)
            .or_else(|| self.resolve_identifier_symbol(idx))
            .is_some_and(|sym_id| self.assignment_target_is_control_flow_typed_any_symbol(sym_id))
    }

    /// Whether the assignment target `idx` is an evolving/auto array reference
    /// (`let x = []`, or the control-flow-any `let x; x = []`). Such a target's
    /// finalized `any[]` is provisional, so the caller withholds it as the RHS
    /// contextual type (a contextual `any` return would fix a generic call's
    /// type parameter and swallow argument errors); tsc does the same.
    ///
    /// The underlying evolving-array query resolves the target identifier with
    /// the read-tracking resolver, which would mark a write-only variable as
    /// "read" and suppress its TS6133. This is a pure structural query on the
    /// assignment *target* (not a read), so it must not perturb unused-variable
    /// tracking: resolve the symbol without tracking and, if the query inserted
    /// the target into the read-set, restore it. The query only ever resolves
    /// this one target reference, so restoring that single symbol is sufficient.
    fn assignment_target_is_evolving_array(&mut self, idx: NodeIndex) -> bool {
        if self
            .ctx
            .arena
            .get(idx)
            .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
        {
            return false;
        }
        let target_sym = self.resolve_identifier_symbol_without_tracking(idx);
        let already_referenced =
            target_sym.is_some_and(|sym| self.ctx.referenced_symbols.borrow().contains(&sym));
        let is_evolving = self.reference_is_reachable_evolving_array_mutation_target(idx);
        if let Some(sym) = target_sym
            && !already_referenced
        {
            self.ctx.referenced_symbols.borrow_mut().remove(&sym);
        }
        is_evolving
    }

    fn is_evolving_array_element_assignment_target(&mut self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        let receiver_ref = self.receiver_reference_for_evolving_array_mutation(access.expression);
        self.reference_is_reachable_evolving_array_mutation_target(receiver_ref)
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
                // `import.meta` is parsed as PROPERTY_ACCESS_EXPRESSION with
                // ImportKeyword as the expression. It is NOT a valid assignment
                // target — assigning to `import.meta` itself must emit TS2364.
                // (Assigning to `import.meta.foo` is fine — that's a real
                // property access on the meta object, not `import.meta` itself.)
                if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(node)
                    && let Some(expr_node) = self.ctx.arena.get(access.expression)
                    && expr_node.kind == SyntaxKind::ImportKeyword as u16
                {
                    return false;
                }
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

    /// Check if a node is a valid target for object rest assignment.
    /// Valid targets are identifiers, property accesses, and element accesses.
    /// Binary expressions like `a + b` are NOT valid rest targets (TS2701).
    pub(crate) fn is_valid_rest_assignment_target(&self, idx: NodeIndex) -> bool {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
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

        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) {
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
        for lib in self.ctx.lib_contexts.iter() {
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
            .then_some(name.to_string())
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
    pub(crate) fn skip_assignment_transparent_wrappers(&self, idx: NodeIndex) -> NodeIndex {
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

        // TS2632: Check if this identifier's *innermost* binding is an import
        // alias BEFORE resolving through imports. `resolve_identifier` follows
        // aliases, so the fully-resolved symbol would be the export target
        // (e.g. `var x`) rather than the import binding, losing the ALIAS flag.
        // We therefore walk the scope chain to the innermost binding and inspect
        // its raw flags: import bindings are readonly in ESM and cannot be
        // reassigned. Crucially, a `let`/`const`/parameter that SHADOWS the
        // import wins the scope walk, so assigning to the shadowing local must
        // NOT report TS2632 (tsc parity).
        let lib_binders = self.get_lib_binders();
        let innermost_binding = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            inner,
            &lib_binders,
            |_sym_id| true,
        );
        if let Some(local_sym_id) = innermost_binding
            && let Some(local_sym) = self.ctx.binder.get_symbol(local_sym_id)
            && local_sym.has_any_flags(symbol_flags::ALIAS)
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
        let is_namespace = symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE);
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = symbol.has_any_flags(value_flags_except_module);

        if is_namespace && !has_other_value {
            let mut is_instantiated = false;
            for decl_idx in &symbol.declarations {
                if self.is_namespace_declaration_instantiated(*decl_idx) {
                    is_instantiated = true;
                    break;
                }
            }
            if !is_instantiated {
                self.report_wrong_meaning_diagnostic(
                    name,
                    inner,
                    crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                );
                return true;
            }
        }

        // Check for type-only symbols used as values in assignment position (TS2693)
        if symbol.has_any_flags(symbol_flags::TYPE) && !symbol.has_any_flags(symbol_flags::VALUE) {
            self.report_wrong_meaning_diagnostic(
                name,
                inner,
                crate::query_boundaries::name_resolution::NameLookupKind::Type,
            );
            return true;
        }

        // Check if this symbol is a class, enum, function, or namespace (TS2629, TS2628, TS2630, TS2631)
        let code = if symbol.has_any_flags(symbol_flags::MODULE) {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_NAMESPACE
        } else if symbol.has_any_flags(symbol_flags::CLASS) {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_CLASS
        } else if symbol.has_any_flags(symbol_flags::ENUM) {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_AN_ENUM
        } else if symbol.has_any_flags(symbol_flags::FUNCTION) {
            diagnostic_codes::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_FUNCTION
        } else {
            return false;
        };

        self.error_at_node_msg(inner, code, &[name]);
        true
    }

    // CommonJS/JS assignment helpers are in commonjs_assignment.rs
    // Arithmetic operand validation is in arithmetic_ops.rs

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

        if self.is_js_file()
            && let Some(private_name_idx) =
                self.is_js_prototype_private_name_assignment_target(left_idx)
        {
            self.error_at_node(
                private_name_idx,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
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
        let mut has_explicit_jsdoc_left_type = false;

        // In JS/checkJs mode, allow JSDoc `@type` on assignment statements to
        // provide the contextual target type for the LHS.
        //
        // Example:
        //   /** @type {string} */
        //   C.prototype = 12;
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && let Some(jsdoc_left_type) = self
                .enclosing_expression_statement(expr_idx)
                .and_then(|stmt_idx| self.js_statement_declared_type(stmt_idx))
                .or_else(|| {
                    // Nested assignments inside JS accessors/functions should not inherit
                    // an enclosing declaration's JSDoc @type as the assignment target.
                    // Only direct JSDoc attached to the assignment expression/LHS should
                    // act as a declared target type here.
                    self.jsdoc_type_annotation_for_node_direct(expr_idx)
                        .or_else(|| self.jsdoc_type_annotation_for_node_direct(left_idx))
                })
        {
            left_type = jsdoc_left_type;
            has_explicit_jsdoc_left_type = true;
        }
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && matches!(left_type, TypeId::ANY | TypeId::UNKNOWN)
            && let Some(name) = self.expression_text(left_idx)
            && let Some(jsdoc_left_type) = self.resolve_jsdoc_assigned_value_type_for_write(&name)
        {
            left_type = jsdoc_left_type;
        }

        self.maybe_report_commonjs_export_implicit_any_assignment(left_idx, right_idx);
        self.maybe_report_js_expando_implicit_any_assignment(left_idx, right_idx, expr_idx);

        if is_function_assignment {
            // TS2629/TS2628/TS2630 are terminal for simple assignment targets in tsc.
            // Do not contextually type the RHS against the class/function/enum object
            // type, or we can produce spurious follow-on errors like missing
            // `prototype` on a function expression assigned to a class symbol.
            return self.get_type_of_node(right_idx);
        }

        if !is_const && self.is_commonjs_module_exports_assignment(left_idx) {
            // In JS files, `module.exports = X` and `exports = X` are declarations.
            // The export surface is inferred from the RHS, so using the current
            // `module.exports` shape as a contextual type for `X` can introduce
            // false excess-property errors before assignability is even skipped.
            // However, when an explicit JSDoc `@type` provides the assignment target,
            // tsc does contextually type the RHS from that declared type.
            if self.ctx.is_checking_statements
                && self.ctx.emit_declarations()
                && !self.ctx.is_declaration_file()
                && self.ctx.should_resolve_jsdoc()
            {
                let right_type = self.get_type_of_node(right_idx);
                let mut private_ref =
                    self.first_private_name_from_external_module_reference(right_type);

                // Fallback for `module.exports = id` when `id` currently resolves to
                // `any`/`unknown` at this site: inspect the identifier's initializer
                // type directly, which is often richer in checked JS.
                if private_ref.is_none()
                    && matches!(right_type, TypeId::ANY | TypeId::UNKNOWN)
                    && let Some(right_node) = self.ctx.arena.get(right_idx)
                    && right_node.kind == SyntaxKind::Identifier as u16
                    && let Some(sym_id) = self.resolve_identifier_symbol(right_idx)
                    && let Some(symbol) = self.get_symbol_from_any_binder(sym_id)
                {
                    let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
                    if decl_idx.is_some()
                        && let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                        && var_decl.initializer.is_some()
                    {
                        let init_type = self.get_type_of_node(var_decl.initializer);
                        private_ref =
                            self.first_private_name_from_external_module_reference(init_type);
                    }
                }

                if let Some((private_name, module_specifier)) = private_ref {
                    let quoted_module = format!("\"{module_specifier}\"");
                    let report_node = self
                        .ctx
                        .arena
                        .get(right_idx)
                        .and_then(|right_node| {
                            (right_node.kind == SyntaxKind::Identifier as u16).then_some(right_idx)
                        })
                        .and_then(|identifier_idx| self.resolve_identifier_symbol(identifier_idx))
                        .and_then(|sym_id| self.get_symbol_from_any_binder(sym_id))
                        .and_then(|symbol| symbol.primary_declaration())
                        .and_then(|decl_idx| self.enclosing_statement_node(decl_idx))
                        .unwrap_or(left_idx);
                    self.error_at_node_msg(
                        report_node,
                        crate::diagnostics::diagnostic_codes::DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_USING_PRIVATE_NAME_FROM_MODULE_AN_EXPLIC,
                        &[&private_name, &quoted_module],
                    );
                }
            }
            if !has_explicit_jsdoc_left_type {
                return self.get_type_of_node(right_idx);
            }
        }

        if !is_const && self.error_invalid_commonjs_export_property_assignment(left_idx) {
            return self.get_type_of_node(right_idx);
        }

        if !is_const && self.is_commonjs_exports_property_declaration(left_idx) {
            // In JS files, `exports.X = value` is a declaration, not an assignment.
            // The type is inferred from the union of all assigned values, so individual
            // assignments should not be checked against the inferred type.
            //
            // A chain declares just as much as a single write does. `exports.y =
            // exports.x = void 0` followed by `exports.x = 1; exports.y = 2` is
            // silent in tsc 7.0.2 (`salsa/assignmentToVoidZero1`, upstream #38552):
            // no assignment is checked against a type derived from a *different*
            // assignment, so the inner write is not compared to `1` nor the outer
            // to `2`. An earlier comment here claimed that test expects those two
            // TS2322s; measured against the pinned tsc, it expects none, so the
            // nested-assignment and assignment-RHS carve-outs are gone.
            //
            // Reads are unaffected: the declared type is still the union of all
            // assignments, and both compilers type `exports.x` as `number` here.
            // An explicit JSDoc `@type` still makes the declared type
            // authoritative and keeps reporting, via `has_explicit_jsdoc_left_type`.
            //
            // TS7: when a bare `module.exports = X` is mixed with sibling property
            // exports (TS2309), the siblings are not declarations — resolve them as
            // ordinary assignments so a missing member on `X` surfaces TS2339.
            let merge_suppressed = self
                .resolve_js_export_surface(self.ctx.current_file_idx)
                .suppresses_expando_merge();
            if !merge_suppressed && !has_explicit_jsdoc_left_type {
                return self.get_type_of_node(right_idx);
            }
        }

        let is_namespace_enum_rebind =
            !is_const && self.is_js_namespace_enum_rebind_assignment_target(left_idx);
        if is_namespace_enum_rebind {
            return self.get_type_of_node(right_idx);
        }

        if !is_const && self.is_js_namespace_enum_expando_member_assignment(left_idx) {
            return self.get_type_of_node(right_idx);
        }

        if !is_const && self.is_js_container_export_declaration(left_idx) {
            // In JS files, assignments like `exports.n = {}` or `module.exports.b = function() {}`
            // where the target is later augmented with property assignments (e.g., `exports.n.K = ...`)
            // are JS container declarations. The type flows from the RHS, not into the RHS.
            // Without this suppression, tsz would emit false TS2741 errors checking `{}` against
            // the augmented type `{ K: () => void }`.
            if !has_explicit_jsdoc_left_type {
                return self.get_type_of_node(right_idx);
            }
        }

        if !is_const
            && !has_explicit_jsdoc_left_type
            && self.is_checked_js_constructor_property_declaration(left_idx, right_idx)
        {
            // In checked-JS, assignments to `Ctor.prop` where `Ctor` is a checked
            // JS constructor declaration are declaration-like container writes.
            // tsc uses these to build the merged container type and reports property
            // lookup failures (TS2339), not assignment compatibility errors.
            return self.get_type_of_node(right_idx);
        }

        let js_global_fallback =
            self.is_checked_js_global_element_access_fallback_assignment(left_idx, right_idx);
        // A cross-file generic-interface member whose declared type is a type-alias
        // reference (e.g. `interface I<V> { read: Read<V> }`) is left as an
        // unresolved `Application(Lazy(alias DefId), args)` after the solver
        // instantiates the interface: the solver-internal evaluator cannot resolve
        // the alias `DefId`, so the application traverses to `ERROR`. That spurious
        // error then trips the `!type_contains_error` guard below, which would skip
        // contextual typing for a function/arrow RHS and emit false TS7006/TS7019.
        // Resolve the LHS through the `TypeEnvironment` (which owns DefId -> TypeId)
        // so the member alias collapses to its real signature before the guard.
        if self.type_contains_error(left_type)
            && self.ctx.arena.get(right_idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            })
        {
            let resolved_left = self.evaluate_type_with_env(left_type);
            if resolved_left != left_type && !self.type_contains_error(resolved_left) {
                left_type = resolved_left;
            }
        }

        let contextual_request = if is_destructuring {
            self.destructuring_assignment_initializer_request(left_idx, right_idx)
        } else if left_type != TypeId::ANY
            && left_type != TypeId::NEVER
            && left_type != TypeId::UNKNOWN
            && !self.type_contains_error(left_type)
            // An evolving/auto array target's provisional `any[]` must not be
            // sourced as the RHS contextual type (see
            // `assignment_target_is_evolving_array`). Checked last so the query
            // only runs once the target carries a real non-`any` array type.
            && !self.assignment_target_is_evolving_array(left_idx)
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
            if let Some(right_node) = self.ctx.arena.get(right_idx) {
                let needs_fresh_contextual_check = right_node.kind
                    == syntax_kind_ext::ARROW_FUNCTION
                    || right_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || right_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                    || (right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && self
                            .ctx
                            .arena
                            .get_binary_expr(right_node)
                            .is_some_and(|bin| {
                                matches!(
                                    bin.operator_token,
                                    k if k == SyntaxKind::BarBarToken as u16
                                        || k == SyntaxKind::AmpersandAmpersandToken as u16
                                        || k == SyntaxKind::QuestionQuestionToken as u16
                                        || k == SyntaxKind::CommaToken as u16
                                )
                            }));
                if needs_fresh_contextual_check {
                    self.invalidate_expression_for_contextual_retry(right_idx);
                }
            }
            if self
                .ctx
                .arena
                .get(left_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && self
                    .ctx
                    .arena
                    .get(right_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            {
                TypingRequest::NONE
            } else {
                TypingRequest::with_contextual_type(contextual_target)
            }
        } else {
            TypingRequest::NONE
        };

        let diag_count_before_rhs = self.ctx.diagnostics.len();
        let right_raw = self.get_type_of_node_with_request(right_idx, &contextual_request);
        let right_type = self.resolve_type_query_type(right_raw);
        if js_global_fallback {
            self.relocate_js_global_element_access_fallback_diagnostics(
                left_idx,
                right_idx,
                diag_count_before_rhs,
            );
        }

        // Ensure the RHS type is also available in node_types for flow analysis.
        // When clear_type_cache_recursive removes the RHS entry for contextual
        // re-checking, the result ends up only in request_node_types. Flow analysis
        // needs node_types to compute assignment-based narrowing (e.g., `d ?? (d = x ?? "x")`).
        //
        if right_raw != TypeId::ERROR {
            tracing::trace!(
                ?right_idx,
                ?right_raw,
                "check_assignment_expression: publishing RHS type for flow analysis"
            );
            self.ctx.node_types.or_insert(right_idx.0, right_raw);
        }

        let declared_recursive_tuple_assignment =
            self.recursive_tuple_declared_assignment_types(left_idx, right_idx);
        let declared_application_assignment =
            self.declared_same_application_assignment_types(left_idx, right_idx);
        let (compat_source_type, compat_target_type, declared_application_any_target_accepted) =
            if let Some((source_declared, target_declared)) = declared_recursive_tuple_assignment {
                (source_declared, target_declared, false)
            } else if let Some((source_declared, target_declared)) = declared_application_assignment
            {
                let accepts_any_target =
                    self.declared_application_any_target_accepts(source_declared, target_declared);
                (source_declared, target_declared, accepts_any_target)
            } else {
                (right_type, left_type, false)
            };

        if !has_explicit_jsdoc_left_type
            && (self.is_control_flow_typed_any_assignment_target(left_idx)
                || self.is_evolving_array_element_assignment_target(left_idx)
                || self.is_checked_js_implicit_any_member_assignment_target(left_idx)
                || self.is_checked_js_constructor_provisional_this_property_assignment(
                    left_idx, right_idx,
                ))
        {
            return right_type;
        }

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ensure_relation_input_ready(right_type);
        self.ensure_relation_input_ready(left_type);
        self.ensure_relation_input_ready(compat_source_type);
        self.ensure_relation_input_ready(compat_target_type);

        let mut is_not_iterable = false;
        if is_array_destructuring {
            // TS2488: Array destructuring assignments require an iterable RHS.
            let is_iterable =
                self.check_destructuring_iterability(left_idx, right_type, NodeIndex::NONE);
            is_not_iterable = !is_iterable;
            if !is_not_iterable {
                self.check_tuple_destructuring_bounds(left_idx, right_type);
            }
        }

        // TS1186: Check for rest elements with initializers in destructuring assignments.
        if is_destructuring {
            self.check_rest_element_initializer(left_idx);
            self.check_rest_element_trailing_comma(left_idx);
            self.check_destructuring_rest_position(left_idx);
        }

        // tsc suppresses TS2322 alongside TS2540 (readonly named property writes)
        // and emits it alongside TS2542 (readonly index signature writes). The
        // distinction is structural — a fixed readonly tuple element like
        // `arr[0]` is a named property even though it uses element-access
        // syntax — so we rely on the diagnostic kind returned by
        // `check_readonly_assignment` rather than on the syntax of `left_idx`.
        let readonly_diag = if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx)
        } else {
            ReadonlyAssignmentDiagnostic::None
        };
        let suppress_for_readonly = readonly_diag.suppresses_type_mismatch();

        if !is_const && self.error_top_level_js_this_computed_element_assignment(left_idx) {
            return right_type;
        }

        if !is_const && left_type != TypeId::ANY {
            // For destructuring assignments (both object and array patterns),
            // skip the whole-object assignability check. tsc processes each
            // property/element individually, which correctly handles private
            // members and other access-controlled properties.
            let mut check_assignability = !is_destructuring && !suppress_for_readonly;

            if is_destructuring && !is_not_iterable {
                self.check_object_destructuring_assignment_from_source_type(
                    left_idx,
                    right_type,
                    Some(right_idx),
                );
            }

            if declared_application_any_target_accepted {
                check_assignability = false;
            }

            if check_assignability {
                let widened_left =
                    crate::query_boundaries::common::widen_type(self.ctx.types, left_type);
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

                        if is_compound_like
                            && self
                                .compound_assignment_relation_outcome(right_type, widened_left)
                                .related
                        {
                            check_assignability = false;
                        }
                    }
                }
            }

            // TS2322: Polymorphic `this` type assignment check.
            // When the LHS is `this.prop` where the property's raw type in the class
            // is `ThisType` (e.g., `self = this`), the RHS must also be `this`-typed.
            // Concrete class types (C, D) are not assignable to the polymorphic `this`.
            if check_assignability
                && let Some(error_emitted) =
                    self.check_polymorphic_this_property_assignment(left_idx, right_idx, right_type)
                && error_emitted
            {
                check_assignability = false;
            }

            if check_assignability
                && let Some((source_declared, target_declared)) =
                    declared_recursive_tuple_assignment
            {
                self.error_type_not_assignable_at_with_anchor(
                    source_declared,
                    target_declared,
                    left_idx,
                );
                check_assignability = false;
            }

            if check_assignability
                && let Some((source_declared, target_declared)) = declared_application_assignment
                && self.same_type_alias_application_args_reject(source_declared, target_declared)
            {
                self.error_type_not_assignable_at_with_anchor(
                    source_declared,
                    target_declared,
                    left_idx,
                );
                check_assignability = false;
            }

            self.check_assignment_compatibility(
                left_idx,
                right_idx,
                compat_source_type,
                compat_target_type,
                check_assignability, // check_assignability
                true,
            );

            if left_type != TypeId::UNKNOWN {
                // Check excess properties when the RHS is a direct object literal
                // OR when the RHS type is fresh (e.g., from a chained assignment
                // like `obj1 = obj2 = { x: 1, y: 2 }` where the inner assignment
                // preserves the freshness of the object literal).
                let is_direct_literal = self
                    .ctx
                    .arena
                    .get(right_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
                if is_direct_literal {
                    self.check_object_literal_excess_properties(right_type, left_type, right_idx);
                    // tsc treats `({ } = { x: 0, y: 0 })` (destructuring assignment
                    // with an empty pattern) as a strict excess-property check,
                    // unlike `var { } = …` which silently accepts. The shared
                    // excess-property helper bails on empty targets because `{}` is
                    // wide for normal assignability; the destructuring-assignment
                    // semantic is narrower and every source property is excess.
                    if is_destructuring && !is_array_destructuring {
                        self.check_destructuring_assignment_empty_pattern_excess(
                            left_idx, right_type, right_idx,
                        );
                    }
                } else if crate::query_boundaries::common::is_fresh_object_type(
                    self.ctx.types,
                    right_type,
                ) {
                    // Fresh type from non-literal RHS (e.g., chained assignment).
                    // Walk through the RHS to find the underlying object literal
                    // so the diagnostic points at the excess property name.
                    let literal_idx = self.find_rhs_object_literal(right_idx);
                    self.check_object_literal_excess_properties(
                        right_type,
                        left_type,
                        literal_idx.unwrap_or(right_idx),
                    );
                }
            }
        }

        right_type
    }

    /// Emit TS2353 for every property on the RHS object literal when the
    /// destructuring assignment LHS pattern is empty (`({} = …)`). The shared
    /// excess-property helper bails on empty targets because `{}` is wide for
    /// normal assignability, but tsc applies a strict check for the empty
    /// destructuring pattern: every source property is excess. Patterns with
    /// at least one named binding (or a rest element) flow through the regular
    /// excess path and this helper is a no-op for them.
    pub(crate) fn check_destructuring_assignment_empty_pattern_excess(
        &mut self,
        left_idx: NodeIndex,
        right_type: TypeId,
        right_idx: NodeIndex,
    ) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        if left_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(pattern) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };
        // Only the empty-pattern case — non-empty patterns are handled by the
        // standard excess-property loop against the inferred pattern type.
        if !pattern.elements.nodes.is_empty() {
            return;
        }
        let Some(source_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, right_type)
        else {
            return;
        };
        let empty_target =
            crate::query_boundaries::assignability::assignability_empty_object_type(self.ctx.types);
        for source_prop in &source_shape.properties {
            let report_idx = self
                .find_object_literal_property_element(right_idx, source_prop.name)
                .unwrap_or(right_idx);
            let prop_name = self.object_literal_property_display_name(
                report_idx,
                self.ctx.types.resolve_atom(source_prop.name).as_ref(),
            );
            self.error_excess_property_at(&prop_name, empty_target, report_idx);
        }
    }

    /// Walk through the RHS of an assignment to find the underlying object literal.
    /// For chained assignments like `obj1 = obj2 = { x: 1, y: 2 }`, this walks
    /// through binary `=` expressions to reach the object literal at the end.
    pub(crate) fn find_rhs_object_literal(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..10 {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(current);
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let bin = self.ctx.arena.get_binary_expr(node)?;
                current = bin.right;
                continue;
            }
            // Parenthesized expression
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            break;
        }
        None
    }

    /// Check if an assignment to `this.prop` violates polymorphic `this` type semantics.
    ///
    /// In TypeScript, properties declared with `this` type (e.g., `self = this`)
    /// require the assigned value to also be `this`-typed. Concrete class types
    /// are not assignable to the polymorphic `this` type.
    ///
    /// Returns `Some(true)` if TS2322 was emitted, `Some(false)` if the target
    /// has `ThisType` but the source is compatible, or `None` if this check
    /// doesn't apply.
    fn check_polymorphic_this_property_assignment(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        right_type: TypeId,
    ) -> Option<bool> {
        // Check if LHS is a property access expression
        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;

        // Check if receiver is `this` keyword
        let expr_node = self.ctx.arena.get(access.expression)?;
        if expr_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        // Must be inside a class
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let _class_idx = class_info.class_idx;

        // Get the property name
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        let property_name = self.ctx.types.intern_string(&ident.escaped_text);

        // Get the concrete class type for property lookup
        let concrete_this = self.current_this_type()?;

        let raw_result = crate::query_boundaries::property_access::resolve_property_access_raw_this(
            self.ctx.types,
            concrete_this,
            property_name,
        );

        let raw_type = match raw_result {
            crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
                type_id
            }
            _ => return None,
        };

        if !crate::query_boundaries::common::is_this_type(self.ctx.types, raw_type) {
            return None;
        }

        // RHS is this-typed if its computed type is canonical `ThisType` and is
        // not a property access (excluded: a concrete field flow-narrows to
        // `this`, still related by tsc against its declared type), or `this`.
        let rhs_prop = self.ctx.arena.get(right_idx).map(|n| n.kind)
            == Some(syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION);
        if (!rhs_prop && crate::query_boundaries::common::is_this_type(self.ctx.types, right_type))
            || self.expression_has_this_type(right_idx)
        {
            return Some(false); // Compatible - both are this-typed
        }

        let source_display =
            self.raw_this_property_assignment_rhs_display(right_idx)
                .or_else(|| {
                    crate::query_boundaries::diagnostics::simple_intersection_head_for_this_assignment_display(
                        self.ctx.types,
                        right_type,
                    )
                    .map(|head| self.format_type_for_assignability_message(head))
                })
                .unwrap_or_else(|| self.format_type_for_assignability_message(right_type));
        let template = get_message_template(diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, &[&source_display, "this"]);
        if let Some((start, end)) = self.get_node_span(left_idx) {
            self.push_error_at(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
        Some(true)
    }

    /// Check if an expression produces a `this`-typed value.
    ///
    /// Returns true if the expression is the `this` keyword, or a property
    /// access on `this` where the property's raw type is `ThisType` or IS
    /// the current class instance type (semantically equivalent to `this`).
    fn expression_has_this_type(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // `this` keyword is always this-typed
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        // Check for `this.prop` where prop has ThisType or is the class instance type
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            let is_this_receiver = self
                .ctx
                .arena
                .get(access.expression)
                .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);

            if is_this_receiver
                && let Some(concrete_this) = self.current_this_type()
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let raw =
                    crate::query_boundaries::property_access::resolve_property_access_raw_this(
                        self.ctx.types,
                        concrete_this,
                        self.ctx.types.intern_string(&ident.escaped_text),
                    );
                if let crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id,
                    ..
                } = raw
                {
                    // Property is this-typed if the raw type IS `ThisType`
                    if crate::query_boundaries::common::is_this_type(self.ctx.types, type_id) {
                        return true;
                    }
                    // For properties with class instance type (like `self2: D`),
                    // check the property's INITIALIZER to see if it was assigned
                    // from a this-typed expression. This distinguishes:
                    // - `self2 = this.self` → this-typed (initialized from this)
                    // - `d = new D()` → NOT this-typed (new instance)
                    if let Some(init_is_this) =
                        self.property_initializer_is_this_typed(&ident.escaped_text)
                    {
                        return init_is_this;
                    }
                }
            }
        }

        false
    }

    /// Check if a property's initializer is a this-typed expression.
    ///
    /// Looks up the property declaration in the current class and checks if
    /// its initializer is `this`, `this.prop` (where prop is this-typed),
    /// or `this.method()` (where method returns `this`).
    fn property_initializer_is_this_typed(&self, prop_name: &str) -> Option<bool> {
        let class_info = self.ctx.enclosing_class.as_ref()?;
        let member_nodes = &class_info.member_nodes;

        // Find the property declaration with this name
        for &member_idx in member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(prop.name) else {
                continue;
            };
            let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            if name_ident.escaped_text != prop_name {
                continue;
            }

            // Found the property. Check if it has an initializer.
            let init_idx = prop.initializer;
            if init_idx.is_none() {
                return Some(false);
            }

            // Check if the initializer is this-typed
            return Some(self.initializer_is_this_expression(init_idx));
        }

        // Property not found in current class - might be inherited
        // For inherited properties, defer to the raw type check
        None
    }

    /// Check if an initializer expression is `this` or a this-typed property access.
    fn initializer_is_this_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Direct `this` keyword
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        // `this.prop` where the property's raw type is ThisType
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            let is_this_receiver = self
                .ctx
                .arena
                .get(access.expression)
                .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);

            if is_this_receiver
                && let Some(concrete_this) = self.current_this_type()
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let raw =
                    crate::query_boundaries::property_access::resolve_property_access_raw_this(
                        self.ctx.types,
                        concrete_this,
                        self.ctx.types.intern_string(&ident.escaped_text),
                    );
                if let crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id,
                    ..
                } = raw
                {
                    // Check if the accessed property has ThisType
                    if crate::query_boundaries::common::is_this_type(self.ctx.types, type_id) {
                        return true;
                    }
                    // Recursively check if the accessed property was
                    // initialized from a this-typed expression
                    if let Some(init_is_this) =
                        self.property_initializer_is_this_typed(&ident.escaped_text)
                    {
                        return init_is_this;
                    }
                }
            }
        }

        // `this.method()` where method returns `this`
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
            && let Some(callee_node) = self.ctx.arena.get(call.expression)
            && callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
        {
            let is_this_receiver = self
                .ctx
                .arena
                .get(access.expression)
                .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);

            if is_this_receiver
                && let Some(concrete_this) = self.current_this_type()
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                // Check if the method's return type is ThisType
                let raw =
                    crate::query_boundaries::property_access::resolve_property_access_raw_this(
                        self.ctx.types,
                        concrete_this,
                        self.ctx.types.intern_string(&ident.escaped_text),
                    );
                if let crate::query_boundaries::common::PropertyAccessResult::Success {
                    type_id,
                    ..
                } = raw
                {
                    // Check if this is a callable with ThisType return
                    if let Some(callable) = crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        for sig in &callable.call_signatures {
                            if crate::query_boundaries::common::is_this_type(
                                self.ctx.types,
                                sig.return_type,
                            ) {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn check_tuple_destructuring_bounds(&mut self, left_idx: NodeIndex, right_type: TypeId) {
        let rhs = crate::query_boundaries::common::unwrap_readonly(self.ctx.types, right_type);

        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        let Some(array_lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };

        // Single tuple case
        if let Some(tuple_elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, rhs)
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
                if self.array_destructuring_element_has_default_initializer(element_idx) {
                    continue;
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
            }
            return;
        }

        // Union of tuples case: check if ALL members are out of bounds
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, rhs) {
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
                if self.array_destructuring_element_has_default_initializer(element_idx) {
                    continue;
                }

                let all_out_of_bounds = !members.is_empty()
                    && members.iter().all(|&m| {
                        let m = crate::query_boundaries::common::unwrap_readonly(self.ctx.types, m);
                        if let Some(elems) =
                            crate::query_boundaries::common::tuple_elements(self.ctx.types, m)
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
                }
            }
        }
    }

    pub(crate) fn enclosing_statement_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                || node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                return Some(current);
            }
            current = self.ctx.arena.get_extended(current)?.parent;
        }
        None
    }

    fn array_destructuring_element_has_default_initializer(&self, element_idx: NodeIndex) -> bool {
        let element_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(element_idx);
        self.ctx
            .arena
            .get(element_idx)
            .and_then(|node| self.ctx.arena.get_binary_expr(node))
            .is_some_and(|bin| bin.operator_token == SyntaxKind::EqualsToken as u16)
    }

    /// TS2462: A rest element must be last in a destructuring pattern.
    ///
    /// Enforces the "rest must be last" rule for both array (`[...a, b] = x`)
    /// and object (`({ ...a, b } = x)`) destructuring *assignment* targets, at
    /// every nesting level. The parser owns the binding-pattern form
    /// (`let { ...a, b } = x`, enforced in `type_checking::core`); assignment
    /// targets parse as plain array/object literals — where a spread is legal
    /// expression syntax anywhere — so the pattern-context rule is only
    /// enforceable here. Array rest parses as `SpreadElement`, object rest as
    /// `SpreadAssignment`; either one is illegal unless it is the final element.
    ///
    /// Mirrors `check_rest_element_trailing_comma`'s recursion so a mis-placed
    /// rest is reported inside nested targets (`[x, { ...a, y }] = z`,
    /// `({ p: [...a, b] } = z)`, `[a, [...b, x]] = z`), matching tsc. Anchored
    /// at the offending spread element.
    fn check_destructuring_rest_position(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        // A `pattern = default` assignment target: the destructuring pattern is
        // on the left; the right side is a value expression, not a target.
        if let Some(bin) = self.ctx.arena.get_binary_expr(node)
            && bin.operator_token == SyntaxKind::EqualsToken as u16
        {
            self.check_destructuring_rest_position(bin.left);
            return;
        }

        let is_array = node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
        let is_object = node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
        if !is_array && !is_object {
            return;
        }
        let Some(lit) = self.ctx.arena.get_literal_expr(node) else {
            return;
        };

        let elements: Vec<NodeIndex> = lit.elements.nodes.clone();
        let last = elements.len().saturating_sub(1);
        for (i, &element_idx) in elements.iter().enumerate() {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let is_rest = element_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || element_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT;
            if is_rest && i < last {
                self.error_at_node_msg(
                    element_idx,
                    diagnostic_codes::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    &[],
                );
            }

            // Recurse into nested destructuring targets: object property values,
            // rest operands, and nested array/object patterns (the latter reach
            // the array/object branch above; a `= default` element unwraps via
            // the binary-`=` handling at the top of this method).
            if element_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_property_assignment(element_node) {
                    self.check_destructuring_rest_position(prop.initializer);
                }
            } else if is_rest {
                if let Some(spread) = self.ctx.arena.get_spread(element_node) {
                    self.check_destructuring_rest_position(spread.expression);
                }
            } else {
                self.check_destructuring_rest_position(element_idx);
            }
        }
    }

    /// TS1013: A rest element in a destructuring *assignment* target may not
    /// be followed by a trailing comma (`([...c,] = a)`, `({...d,} = {})`).
    ///
    /// The parser owns the binding-pattern form (`let [...e,] = x`);
    /// assignment targets parse as plain array/object literals — where a
    /// trailing comma is legal expression syntax — so the pattern-context rule
    /// is only enforceable here, and it applies at every nesting level
    /// (`[x, [...a,]] = y` errors on the inner pattern). Anchored at the comma
    /// token with a 1-char span, matching tsc.
    fn check_rest_element_trailing_comma(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };
        let rest_kind = if left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            syntax_kind_ext::SPREAD_ELEMENT
        } else if left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            syntax_kind_ext::SPREAD_ASSIGNMENT
        } else {
            return;
        };
        let Some(lit) = self.ctx.arena.get_literal_expr(left_node) else {
            return;
        };
        let elements: Vec<NodeIndex> = lit.elements.nodes.clone();
        if lit.elements.has_trailing_comma
            && let Some(&last_idx) = elements.last()
            && let Some(last_node) = self.ctx.arena.get(last_idx)
            && last_node.kind == rest_kind
        {
            // The trailing comma is the last `,` before the closing
            // bracket/brace. Scan backwards from the closer: the rest
            // element's own `end` can swallow the separator token, and the
            // element may contain interior commas (`[...[a, b],] = x`), so
            // `rfind` over the element-to-closer window is the only anchor
            // that always lands on the separator.
            let comma_pos = self
                .ctx
                .arena
                .source_files
                .first()
                .and_then(|sf| sf.text.get(last_node.pos as usize..left_node.end as usize))
                .and_then(|text| text.rfind(','))
                .map_or(last_node.end, |off| last_node.pos + off as u32);
            self.error_at_position(
                comma_pos,
                1,
                diagnostic_messages::A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA,
                diagnostic_codes::A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA,
            );
        }
        // Recurse into nested destructuring targets: array elements, object
        // property values, rest operands, and `= default` left sides.
        for element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || element_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_rest_element_trailing_comma(element_idx);
            } else if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || element_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                if let Some(spread) = self.ctx.arena.get_spread(element_node) {
                    self.check_rest_element_trailing_comma(spread.expression);
                }
            } else if element_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                if let Some(prop) = self.ctx.arena.get_property_assignment(element_node) {
                    self.check_rest_element_trailing_comma(prop.initializer);
                }
            } else if let Some(bin) = self.ctx.arena.get_binary_expr(element_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                self.check_rest_element_trailing_comma(bin.left);
            }
        }
    }

    /// TS1186: A rest element cannot have an initializer.
    ///
    /// In assignment destructuring, `[...x = a] = b` is parsed as a spread of
    /// the assignment expression `x = a`. TypeScript detects this and emits
    /// TS1186 when the spread expression is a binary `=` assignment.
    fn check_rest_element_initializer(&mut self, left_idx: NodeIndex) {
        let Some(left_node) = self.ctx.arena.get(left_idx) else {
            return;
        };

        let elements = if left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else if left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            self.ctx
                .arena
                .get_literal_expr(left_node)
                .map(|lit| &lit.elements.nodes as &[NodeIndex])
        } else {
            None
        };

        let Some(elements) = elements else { return };
        for &element_idx in elements {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            // Check spread elements and spread assignments
            if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT
                && element_node.kind != syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                continue;
            }
            let spread_expr = self
                .ctx
                .arena
                .get_spread(element_node)
                .map(|s| s.expression)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_unary_expr_ex(element_node)
                        .map(|u| u.expression)
                });
            let Some(spread_expr) = spread_expr else {
                continue;
            };
            // If the spread expression is a binary assignment (x = a), emit TS1186.
            // tsc anchors this at the `=` operator token, not at the spread element's
            // `...` prefix or the left-hand name. Scan from the left operand's end to
            // find the `=` position.
            if let Some(spread_node) = self.ctx.arena.get(spread_expr)
                && spread_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(spread_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                // Find the `=` token position between left and right operands
                let eq_pos = self.ctx.arena.get(bin.left).map(|left_node| {
                    let search_start = left_node.end as usize;
                    self.ctx
                        .arena
                        .source_files
                        .first()
                        .and_then(|sf| {
                            sf.text[search_start..]
                                .find('=')
                                .map(|offset| (search_start + offset) as u32)
                        })
                        .unwrap_or(left_node.end)
                });
                if let Some(pos) = eq_pos {
                    let message = tsz_common::diagnostics::get_message_template(
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    )
                    .unwrap_or("");
                    self.error_at_position(
                        pos,
                        1,
                        message,
                        diagnostic_codes::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                    );
                }
            }
        }
    }

    pub(crate) fn check_assignment_compatibility(
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
            if (source_type != generic_target
                && !self
                    .generic_element_write_relation_outcome(source_type, generic_target)
                    .related)
                || (source_type != generic_target
                    && !crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        source_type,
                    )
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        generic_target,
                    ))
            {
                self.error_type_not_assignable_at_with_display_types(
                    source_type,
                    generic_target,
                    left_idx,
                );
            }
            return;
        }

        // tsc anchors some void-assignment diagnostics to the function identifier on
        // the RHS when the assignment target is `void` and the RHS is a function
        // symbol reference (e.g. `function f<T>(a: T) { ... }; x = f;`).
        // Use `has_call_signatures` instead of `is_callable_type` to exclude class
        // constructor types (which only have construct/new signatures). TSC anchors
        // class assignments (`x = C;`) at the LHS, not the RHS.
        if target_type == TypeId::VOID
            && crate::query_boundaries::common::has_call_signatures(self.ctx.types, source_type)
            && self.is_identifier_rhs(right_idx)
        {
            let _ = self.check_assignable_or_report_at_exact_anchor(
                source_type,
                target_type,
                right_idx,
                right_idx,
            );
            return;
        }

        // TS2322 anchoring should point at the assignment target (LHS), not the RHS expression.
        // This aligns diagnostic fingerprints with tsc for assignment-compatibility suites.
        let _ = self.check_assignable_or_report_at(source_type, target_type, right_idx, left_idx);
    }

    fn is_function_reference(&self, node_idx: NodeIndex) -> bool {
        self.is_identifier_rhs(node_idx)
    }

    fn is_identifier_rhs(&self, node_idx: NodeIndex) -> bool {
        let node_idx = self.ctx.arena.skip_parenthesized_and_assertions(node_idx);
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        true
    }

    fn deferred_generic_element_write_target(
        &mut self,
        left_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<TypeId> {
        if source_type == TypeId::ANY || source_type == TypeId::NEVER {
            return None;
        }

        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let object_type = self
            .resolve_identifier_symbol(access.expression)
            .and_then(|sym_id| self.assignment_target_declared_type(sym_id))
            .filter(|declared| {
                crate::query_boundaries::common::is_type_parameter(self.ctx.types, *declared)
                    || crate::query_boundaries::common::is_this_type(self.ctx.types, *declared)
            })
            .unwrap_or_else(|| self.get_type_of_node(access.expression));
        if !crate::query_boundaries::common::is_type_parameter(self.ctx.types, object_type) {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let index_type = self.get_type_of_node(access.name_or_argument);
        self.ctx.preserve_literal_types = prev_preserve;

        if let Some(write_target) =
            self.constraint_keyof_write_target_for_type_param(index_type, object_type)
        {
            return Some(write_target);
        }

        if !self.is_valid_index_for_type_param(index_type, object_type) {
            return None;
        }
        Some(
            crate::query_boundaries::assignability::assignability_index_access_type(
                self.ctx.types,
                object_type,
                index_type,
            ),
        )
    }
}
