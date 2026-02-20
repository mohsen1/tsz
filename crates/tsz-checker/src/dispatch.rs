//! Expression type computation dispatcher.
//!
//! This module provides the `ExpressionDispatcher` which handles the dispatch
//! of type computation requests to appropriate specialized methods based on
//! the syntax node kind.

use crate::query_boundaries::dispatch as query;
use crate::query_boundaries::generic_checker as generic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Dispatcher for expression type computation.
///
/// `ExpressionDispatcher` handles the dispatch of type computation for different
/// node kinds, delegating to specialized methods in `CheckerState`.
pub struct ExpressionDispatcher<'a, 'b> {
    /// Reference to the checker state.
    pub checker: &'a mut CheckerState<'b>,
}

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    /// Create a new expression dispatcher.
    pub const fn new(checker: &'a mut CheckerState<'b>) -> Self {
        Self { checker }
    }

    /// Resolve a literal type: preserve the narrow literal if we're in a const
    /// assertion, contextual typing expects it, or we're computing a type for
    /// a compound expression (conditional/logical) that should preserve literals.
    fn resolve_literal(&mut self, literal_type: Option<TypeId>, widened: TypeId) -> TypeId {
        match literal_type {
            Some(lit)
                if self.checker.ctx.in_const_assertion
                    || self.checker.ctx.preserve_literal_types
                    || self.checker.contextual_literal_type(lit).is_some() =>
            {
                lit
            }
            _ => widened,
        }
    }

    fn get_expected_yield_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;

        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };

        // Prefer syntactic extraction from the explicit annotation first.
        // This preserves `TYield` exactly as written (e.g. `IterableIterator<number>`
        // => `number`) even if semantic base resolution currently widens it.
        let declared_return_node = self.checker.ctx.arena.get(declared_return_type_node)?;
        if declared_return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            let declared_return_type = self
                .checker
                .get_type_from_type_node(declared_return_type_node);
            return self
                .checker
                .get_generator_yield_type_argument(declared_return_type);
        }
        let type_ref = self.checker.ctx.arena.get_type_ref(declared_return_node)?;
        let type_name_node = self.checker.ctx.arena.get(type_ref.type_name)?;
        let type_name = self
            .checker
            .ctx
            .arena
            .get_identifier(type_name_node)
            .map(|ident| ident.escaped_text.as_str())?;

        if !matches!(
            type_name,
            "Generator"
                | "AsyncGenerator"
                | "Iterator"
                | "AsyncIterator"
                | "IterableIterator"
                | "AsyncIterableIterator"
        ) {
            let declared_return_type = self
                .checker
                .get_type_from_type_node(declared_return_type_node);
            return self
                .checker
                .get_generator_yield_type_argument(declared_return_type);
        }

        if let Some(first_arg) = type_ref
            .type_arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
        {
            return Some(self.checker.get_type_from_type_node(first_arg));
        }

        let declared_return_type = self
            .checker
            .get_type_from_type_node(declared_return_type_node);
        self.checker
            .get_generator_yield_type_argument(declared_return_type)
    }

    fn type_node_includes_undefined(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return true;
        }

        if node.kind == syntax_kind_ext::UNION_TYPE
            && let Some(composite) = self.checker.ctx.arena.get_composite_type(node)
        {
            return composite
                .types
                .nodes
                .iter()
                .copied()
                .any(|member| self.type_node_includes_undefined(member));
        }

        false
    }

    fn explicit_generator_yield_allows_undefined(&mut self, idx: NodeIndex) -> Option<bool> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;

        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };

        let declared_return_node = self.checker.ctx.arena.get(declared_return_type_node)?;
        if declared_return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.checker.ctx.arena.get_type_ref(declared_return_node)?;
        let type_name_node = self.checker.ctx.arena.get(type_ref.type_name)?;
        let type_name = self
            .checker
            .ctx
            .arena
            .get_identifier(type_name_node)
            .map(|ident| ident.escaped_text.as_str())?;
        if !matches!(
            type_name,
            "Generator"
                | "AsyncGenerator"
                | "Iterator"
                | "AsyncIterator"
                | "IterableIterator"
                | "AsyncIterableIterator"
        ) {
            return None;
        }

        let first_arg = type_ref.type_arguments.as_ref()?.nodes.first().copied()?;
        Some(self.type_node_includes_undefined(first_arg))
    }

    /// Get the declared generator type (`Generator<TYield, TReturn, TNext>`) for the enclosing generator function.
    /// Returns `None` if not in a generator or if the generator has no explicit type annotation.
    fn get_expected_generator_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;

        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };

        // Get the declared generator type
        let declared_return_type = self
            .checker
            .get_type_from_type_node(declared_return_type_node);
        Some(declared_return_type)
    }

    fn get_type_of_yield_expression(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(yield_expr) = self.checker.ctx.arena.get_unary_expr_ex(node) else {
            return TypeId::ERROR;
        };

        // If yield is outside a generator function, the parser already emitted TS1163.
        // Return ANY without evaluating the operand to avoid cascading TS2304 errors.
        let is_in_generator = self
            .checker
            .find_enclosing_function(idx)
            .and_then(|fn_idx| self.checker.ctx.arena.get(fn_idx))
            .is_some_and(|fn_node| {
                if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                    func.asterisk_token
                } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                    method.asterisk_token
                } else {
                    false
                }
            });
        if !is_in_generator {
            return TypeId::ANY;
        }

        let yielded_type = if yield_expr.expression.is_none() {
            TypeId::UNDEFINED
        } else {
            // Set contextual type for yield expression from the generator's yield type.
            // This allows `yield (num) => ...` to contextually type arrow params.
            // For `yield *expr`, the expression is an iterable of the yield type,
            // so wrap the contextual type in Array<T> to contextually type array elements.
            let prev_contextual = self.checker.ctx.contextual_type;
            if let Some(yield_ctx) = self.checker.ctx.current_yield_type() {
                if yield_expr.asterisk_token {
                    // yield *[x => ...] needs Array<TYield> as contextual type
                    // so each array element gets TYield as its contextual type
                    let array_of_yield = self.checker.ctx.types.factory().array(yield_ctx);
                    self.checker.ctx.contextual_type = Some(array_of_yield);
                } else {
                    self.checker.ctx.contextual_type = Some(yield_ctx);
                }
            }
            let expression_type = self.checker.get_type_of_node(yield_expr.expression);
            self.checker.ctx.contextual_type = prev_contextual;
            if yield_expr.asterisk_token {
                let is_async_generator = self
                    .checker
                    .find_enclosing_function(idx)
                    .and_then(|fn_idx| self.checker.ctx.arena.get(fn_idx))
                    .is_some_and(|fn_node| {
                        if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                            func.is_async && func.asterisk_token
                        } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node)
                        {
                            self.checker.has_async_modifier(&method.modifiers)
                                && method.asterisk_token
                        } else {
                            false
                        }
                    });

                if is_async_generator {
                    tsz_solver::operations::get_async_iterable_element_type(
                        self.checker.ctx.types,
                        expression_type,
                    )
                } else {
                    tsz_solver::operations::get_iterator_info(
                        self.checker.ctx.types,
                        expression_type,
                        false,
                    )
                    .map_or(TypeId::ANY, |info| info.yield_type)
                }
            } else {
                expression_type
            }
        };

        if let Some(expected_yield_type) = self.get_expected_yield_type(idx) {
            let error_node = if yield_expr.expression.is_none() {
                idx
            } else {
                yield_expr.expression
            };

            self.checker.ensure_relation_input_ready(yielded_type);
            self.checker
                .ensure_relation_input_ready(expected_yield_type);

            let resolved_expected_yield_type = self.checker.resolve_lazy_type(expected_yield_type);
            let syntactic_yield_allows_undefined = self
                .explicit_generator_yield_allows_undefined(idx)
                .unwrap_or(false);

            let bare_yield_requires_error = yield_expr.expression.is_none()
                && expected_yield_type != TypeId::ANY
                && expected_yield_type != TypeId::UNKNOWN
                && expected_yield_type != TypeId::ERROR
                && expected_yield_type != TypeId::VOID  // Allow bare yield for void
                && !syntactic_yield_allows_undefined
                && !tsz_solver::type_queries::type_includes_undefined(self.checker.ctx.types, expected_yield_type)
                && !tsz_solver::type_queries::type_includes_undefined(self.checker.ctx.types, resolved_expected_yield_type);

            // TS delegates nuanced `yield*` compatibility through iterator protocols.
            // Avoid direct TS2322 checks here to prevent false positives.
            if yield_expr.asterisk_token {
                return TypeId::ANY;
            }

            if bare_yield_requires_error {
                self.checker.check_assignable_or_report(
                    yielded_type,
                    expected_yield_type,
                    error_node,
                );
            } else if !self.checker.type_contains_error(expected_yield_type)
                && !self.checker.check_assignable_or_report(
                    yielded_type,
                    expected_yield_type,
                    error_node,
                )
            {
                // Diagnostic emitted by check_assignable_or_report.
            }
        }

        // TypeScript models `yield` result type as the value received by `.next(...)` (TNext).
        // Extract TNext from Generator<TYield, TReturn, TNext> or AsyncGenerator<TYield, TReturn, TNext>.
        if let Some(generator_type) = self.get_expected_generator_type(idx) {
            // Use contextual helper to extract TNext (argument index 2)
            let ctx = tsz_solver::ContextualTypeContext::with_expected(
                self.checker.ctx.types,
                generator_type,
            );
            if let Some(next_type) = ctx.get_generator_next_type() {
                return next_type;
            }
        }

        // Fallback to `any` if no generator context is available
        TypeId::ANY
    }

    /// Dispatch type computation based on node kind.
    ///
    /// This method examines the syntax node kind and dispatches to the
    /// appropriate specialized type computation method.
    pub fn dispatch_type_computation(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        let _is_function_declaration = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;

        match node.kind {
            // Identifiers
            k if k == SyntaxKind::Identifier as u16 => self.checker.get_type_of_identifier(idx),
            k if k == SyntaxKind::ThisKeyword as u16 => {
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    // TS2465: 'this' cannot be referenced in a computed property name.
                    // Check this first — it takes priority over other `this` errors.
                    if self
                        .checker
                        .is_this_in_class_member_computed_property_name(idx)
                    {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                            diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
                        );
                        return TypeId::ANY;
                    }
                    // TS2331: 'this' cannot be referenced in a module or namespace body
                    if self.checker.is_this_in_namespace_body(idx) {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                            diagnostic_codes::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                        );
                        return TypeId::ANY;
                    }
                    // TS17009: 'super' must be called before accessing 'this'
                    if self
                        .checker
                        .is_this_before_super_in_derived_constructor(idx)
                    {
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                            diagnostic_codes::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
                        );
                    }
                }
                if let Some(this_type) = self.checker.current_this_type() {
                    self.checker.apply_flow_narrowing(idx, this_type)
                } else if let Some(ref class_info) = self.checker.ctx.enclosing_class {
                    // Inside a class but no explicit this type on stack -
                    // return the class instance type (e.g., for constructor default params)
                    if let Some(class_node) = self.checker.ctx.arena.get(class_info.class_idx)
                        && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                    {
                        let class_instance = self
                            .checker
                            .get_class_instance_type(class_info.class_idx, class_data);
                        return self.checker.apply_flow_narrowing(idx, class_instance);
                    }
                    TypeId::ANY
                } else if self.checker.this_has_contextual_owner(idx).is_some() {
                    // `this` in a class or object literal member but enclosing_class
                    // not yet set. Suppress TS2683 - `this` is contextually typed.
                    TypeId::ANY
                } else if self.checker.ctx.no_implicit_this()
                    && !self.checker.is_js_file()
                    && self
                        .checker
                        .find_enclosing_non_arrow_function(idx)
                        .is_some()
                {
                    // TS2683: 'this' implicitly has type 'any'
                    // Suppressed in JS files: tsc infers `this` for constructor/prototype
                    // patterns and JSDoc-typed functions.
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    );
                    TypeId::ANY
                } else {
                    TypeId::ANY
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => {
                self.checker.get_type_of_super_keyword(idx)
            }

            // Literals — preserve literal types when contextual typing expects them.
            k if k == SyntaxKind::NumericLiteral as u16 => self.resolve_literal(
                self.checker.literal_type_from_initializer(idx),
                TypeId::NUMBER,
            ),
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                // TS2737: bigint literals require target >= ES2020 in non-ambient contexts.
                if (self.checker.ctx.compiler_options.target as u32)
                    < (tsz_common::common::ScriptTarget::ES2020 as u32)
                    && !self.checker.is_ambient_declaration(idx)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020,
                        diagnostic_codes::BIGINT_LITERALS_ARE_NOT_AVAILABLE_WHEN_TARGETING_LOWER_THAN_ES2020,
                    );
                }
                self.resolve_literal(
                    self.checker.literal_type_from_initializer(idx),
                    TypeId::BIGINT,
                )
            }
            k if k == SyntaxKind::StringLiteral as u16 => self.resolve_literal(
                self.checker.literal_type_from_initializer(idx),
                TypeId::STRING,
            ),
            k if k == SyntaxKind::TrueKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(true);
                self.resolve_literal(Some(literal_type), TypeId::BOOLEAN)
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(false);
                self.resolve_literal(Some(literal_type), TypeId::BOOLEAN)
            }
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,

            // Binary expressions
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.checker.get_type_of_binary_expression(idx)
            }

            // Call expressions
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.checker.get_type_of_call_expression(idx)
            }

            // New expressions
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.checker.get_type_of_new_expression(idx)
            }

            // Class expressions
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                if let Some(class) = self.checker.ctx.arena.get_class(node).cloned() {
                    self.checker.check_class_expression(idx, &class);

                    // When a class extends a type parameter and adds no new instance members,
                    // type it as the type parameter to maintain generic compatibility
                    if let Some(base_type_param) = self
                        .checker
                        .get_extends_type_parameter_if_transparent(&class)
                    {
                        base_type_param
                    } else {
                        self.checker.get_class_constructor_type(idx, &class)
                    }
                } else {
                    // Return ANY to prevent cascading TS2571 errors
                    TypeId::ANY
                }
            }

            // Property access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.checker.get_type_of_property_access(idx)
            }

            // Element access
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.checker.get_type_of_element_access(idx)
            }

            // Conditional expression (ternary)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.checker.get_type_of_conditional_expression(idx)
            }

            // Variable declaration
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.checker.get_type_of_variable_declaration(idx)
            }

            // Function declaration
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.checker.get_type_of_function(idx)
            }

            // Function expression
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                if self.checker.is_js_file() {
                    self.checker.check_js_grammar_function(idx, node);
                }
                // TS1100: Invalid use of 'eval'/'arguments' as function expression name
                if let Some(func) = self.checker.ctx.arena.get_function(node)
                    && !func.name.is_none()
                    && let Some(name_node) = self.checker.ctx.arena.get(func.name)
                    && let Some(ident) = self.checker.ctx.arena.get_identifier(name_node)
                    && (ident.escaped_text == "eval" || ident.escaped_text == "arguments")
                    && self.checker.is_strict_mode_for_node(idx)
                {
                    self.checker.error_at_node_msg(
                        func.name,
                        crate::diagnostics::diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                        &[&ident.escaped_text],
                    );
                }
                self.checker.get_type_of_function(idx)
            }

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                if self.checker.is_js_file() {
                    self.checker.check_js_grammar_function(idx, node);
                }
                self.checker.get_type_of_function(idx)
            }

            // Array literal
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.checker.get_type_of_array_literal(idx)
            }

            // Object literal
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.checker.get_type_of_object_literal(idx)
            }

            // Prefix unary expression
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.checker.get_type_of_prefix_unary(idx)
            }

            // Postfix unary expression - ++ and -- require numeric operand and valid l-value
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr(node) {
                    // TS1100: Invalid use of 'eval'/'arguments' in strict mode.
                    // Must come before TS2356 to match TSC's diagnostic priority.
                    let mut emitted_strict = false;
                    if let Some(operand_node) = self.checker.ctx.arena.get(unary.operand)
                        && operand_node.kind == SyntaxKind::Identifier as u16
                        && let Some(id_data) = self.checker.ctx.arena.get_identifier(operand_node)
                        && (id_data.escaped_text == "eval" || id_data.escaped_text == "arguments")
                        && self.checker.is_strict_mode_for_node(unary.operand)
                    {
                        use crate::diagnostics::diagnostic_codes;
                        self.checker.error_at_node_msg(
                            unary.operand,
                            diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                            &[&id_data.escaped_text],
                        );
                        emitted_strict = true;
                    }

                    // TSC checks arithmetic type BEFORE lvalue — if the type check
                    // fails (TS2356), the lvalue check (TS2357) is skipped.
                    let operand_type = self.checker.get_type_of_node(unary.operand);
                    let mut arithmetic_ok = true;

                    if !emitted_strict {
                        use tsz_solver::BinaryOpEvaluator;
                        let evaluator = BinaryOpEvaluator::new(self.checker.ctx.types);
                        let is_valid = evaluator.is_arithmetic_operand(operand_type);

                        if !is_valid {
                            arithmetic_ok = false;
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.checker.error_at_node(
                                unary.operand,
                                diagnostic_messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                                diagnostic_codes::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                            );
                        }
                    }

                    // Only check lvalue and assignment restrictions when arithmetic
                    // type is valid (matches TSC: TS2357 is skipped when TS2356 fires).
                    if arithmetic_ok {
                        let emitted_lvalue = self
                            .checker
                            .check_increment_decrement_operand(unary.operand);

                        if !emitted_lvalue {
                            // TS2588: Cannot assign to 'x' because it is a constant.
                            let is_const = self.checker.check_const_assignment(unary.operand);

                            // TS2630: Cannot assign to 'x' because it is a function.
                            self.checker.check_function_assignment(unary.operand);

                            // TS2540: Cannot assign to readonly property
                            if !is_const {
                                self.checker.check_readonly_assignment(unary.operand, idx);
                            }
                        }
                    }
                }

                TypeId::NUMBER
            }

            // typeof expression
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,

            // void expression
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,

            // await expression - unwrap Promise<T> to get T, with contextual typing (Phase 6 - tsz-3)
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                self.checker.get_type_of_await_expression(idx)
            }

            // yield expression
            k if k == syntax_kind_ext::YIELD_EXPRESSION => self.get_type_of_yield_expression(idx),

            // Parenthesized expression - just pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    // Check if expression is missing (parse error: empty parentheses)
                    if paren.expression.is_none() {
                        // Parse error - return ERROR to suppress cascading errors
                        return TypeId::ERROR;
                    }
                    // In JS/checkJs, inline JSDoc casts like `/** @type {T} */(expr)`
                    // should behave as type assertions and produce the annotated type.
                    if let Some(jsdoc_type) = self.checker.jsdoc_type_annotation_for_node(idx) {
                        let _ = self.checker.get_type_of_node(paren.expression);
                        jsdoc_type
                    } else {
                        self.checker.get_type_of_node(paren.expression)
                    }
                } else {
                    // Missing parenthesized data - propagate error
                    TypeId::ERROR
                }
            }

            // Type assertions / `as` / `satisfies`
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                if let Some(assertion) = self.checker.ctx.arena.get_type_assertion(node) {
                    // Check for const assertion BEFORE type-checking the expression
                    // so we can set the context flag to preserve literal types
                    let is_const_assertion =
                        if let Some(type_node) = self.checker.ctx.arena.get(assertion.type_node) {
                            type_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16
                        } else {
                            false
                        };

                    // Set the in_const_assertion flag to preserve literal types in nested expressions
                    let prev_in_const_assertion = self.checker.ctx.in_const_assertion;
                    if is_const_assertion {
                        self.checker.ctx.in_const_assertion = true;
                    }

                    // In recovery scenarios we may not have a type node; fall back to the expression type.
                    if assertion.type_node.is_none() {
                        let expr_type = self.checker.get_type_of_node(assertion.expression);
                        self.checker.ctx.in_const_assertion = prev_in_const_assertion;
                        expr_type
                    } else {
                        // Check for duplicate properties in type literal nodes (TS2300)
                        self.checker
                            .check_type_for_parameter_properties(assertion.type_node);

                        let asserted_type =
                            self.checker.get_type_from_type_node(assertion.type_node);

                        // Set contextual type before checking expression for both
                        // type assertions and `satisfies`. This enables contextual typing
                        // for lambdas, object literals, etc. inside `<T>(expr)` / `expr as T` / `expr satisfies T`.
                        let prev_contextual_type = self.checker.ctx.contextual_type;
                        if !is_const_assertion {
                            self.checker.ctx.contextual_type = Some(asserted_type);
                        }

                        // Always type-check the expression for side effects / diagnostics.
                        let expr_type = self.checker.get_type_of_node(assertion.expression);

                        // Restore contextual type
                        self.checker.ctx.contextual_type = prev_contextual_type;
                        self.checker.ctx.in_const_assertion = prev_in_const_assertion;

                        if is_const_assertion {
                            // as const: apply const assertion to the expression type
                            use tsz_solver::widening::apply_const_assertion;
                            apply_const_assertion(self.checker.ctx.types, expr_type)
                        } else if k == syntax_kind_ext::SATISFIES_EXPRESSION {
                            // `satisfies` keeps the expression type at runtime, but checks assignability.
                            // This is different from `as` which coerces the type.
                            self.checker.ensure_relation_input_ready(expr_type);
                            self.checker.ensure_relation_input_ready(asserted_type);
                            if !self.checker.type_contains_error(asserted_type) {
                                let _ = self.checker.check_assignable_or_report(
                                    expr_type,
                                    asserted_type,
                                    assertion.expression,
                                );
                            }
                            expr_type
                        } else {
                            // `expr as T` / `<T>expr` yields `T`.
                            // TS2352: Check if conversion may be a mistake (types don't sufficiently overlap)
                            self.checker.ensure_relation_input_ready(expr_type);
                            self.checker.ensure_relation_input_ready(asserted_type);

                            // Don't check if either type is error, any, unknown, or never.
                            // TS also skips this warning for unconcretized type-parameter
                            // assertions like `{}` as T, which are common in generic code.
                            let should_check = !self.checker.type_contains_error(expr_type)
                                && !self.checker.type_contains_error(asserted_type)
                                && expr_type != TypeId::ANY
                                && asserted_type != TypeId::ANY
                                && expr_type != TypeId::UNKNOWN
                                && asserted_type != TypeId::UNKNOWN
                                && expr_type != TypeId::NEVER
                                && asserted_type != TypeId::NEVER
                                // Skip TS2352 when either type contains type parameters
                                // (not just *is* a type parameter). TSC's comparableRelation
                                // erases generics and resolves constraints, so assertions like
                                // `x as <T>(x: T) => T` or `key as keyof T` are allowed.
                                && !generic_query::contains_type_parameters(
                                    self.checker.ctx.types,
                                    expr_type,
                                )
                                && !generic_query::contains_type_parameters(
                                    self.checker.ctx.types,
                                    asserted_type,
                                );

                            if should_check {
                                // TS2352 is emitted if neither type is assignable to the other
                                // (i.e., the types don't "sufficiently overlap").
                                // TSC uses isTypeComparableTo which is more relaxed than assignability:
                                // types are comparable if they share at least one common property.
                                let source_to_target =
                                    self.checker.is_assignable_to(expr_type, asserted_type);
                                let target_to_source =
                                    self.checker.is_assignable_to(asserted_type, expr_type);
                                if !source_to_target && !target_to_source {
                                    // TSC uses isTypeComparableTo which decomposes unions
                                    // and checks per-member overlap. For `X as A | B`, it
                                    // suffices if X overlaps with ANY member (A or B).
                                    let mut have_overlap = false;

                                    // Decompose target union: any member assignable in either direction?
                                    if let Some(members) =
                                        query::union_members(self.checker.ctx.types, asserted_type)
                                    {
                                        for member in members {
                                            if self.checker.is_assignable_to(member, expr_type)
                                                || self.checker.is_assignable_to(expr_type, member)
                                            {
                                                have_overlap = true;
                                                break;
                                            }
                                        }
                                    }

                                    // Decompose source union: any member assignable in either direction?
                                    if !have_overlap
                                        && let Some(members) =
                                            query::union_members(self.checker.ctx.types, expr_type)
                                    {
                                        for member in members {
                                            if self.checker.is_assignable_to(member, asserted_type)
                                                || self
                                                    .checker
                                                    .is_assignable_to(asserted_type, member)
                                            {
                                                have_overlap = true;
                                                break;
                                            }
                                        }
                                    }

                                    // Final fallback: check structural property overlap
                                    if !have_overlap {
                                        let evaluated_expr =
                                            self.checker.evaluate_type_for_assignability(expr_type);
                                        let evaluated_asserted = self
                                            .checker
                                            .evaluate_type_for_assignability(asserted_type);
                                        have_overlap = query::types_are_comparable(
                                            self.checker.ctx.types,
                                            evaluated_expr,
                                            evaluated_asserted,
                                        );
                                    }

                                    if !have_overlap {
                                        self.checker.error_type_assertion_no_overlap(
                                            expr_type,
                                            asserted_type,
                                            idx,
                                        );
                                    }
                                }
                            }

                            asserted_type
                        }
                    }
                } else {
                    TypeId::ERROR
                }
            }

            // Template expression (e.g., `hello ${name}`)
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.checker.get_type_of_template_expression(idx)
            }

            // No-substitution template literal - always preserve literal type.
            // Widening happens at binding sites, not at expression evaluation.
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => self.resolve_literal(
                self.checker.literal_type_from_initializer(idx),
                TypeId::STRING,
            ),

            // =========================================================================
            // Type Nodes - Delegate to TypeNodeChecker
            // =========================================================================

            // Type nodes that need binder resolution - delegate to get_type_from_type_node
            // which handles special cases with proper symbol resolution
            k if k == syntax_kind_ext::TYPE_REFERENCE => self.checker.get_type_from_type_node(idx),

            // Type nodes handled by TypeNodeChecker
            k if k == syntax_kind_ext::UNION_TYPE
                || k == syntax_kind_ext::INTERSECTION_TYPE
                || k == syntax_kind_ext::ARRAY_TYPE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                || k == syntax_kind_ext::TYPE_LITERAL
                || k == syntax_kind_ext::TYPE_QUERY
                || k == syntax_kind_ext::TYPE_OPERATOR =>
            {
                let mut checker = crate::TypeNodeChecker::new(&mut self.checker.ctx);
                checker.check(idx)
            }

            // Keyword types - when recovered into value positions, TypeScript emits TS2693.
            k if k == SyntaxKind::NumberKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("number", idx);
                    TypeId::ERROR
                } else {
                    TypeId::NUMBER
                }
            }
            k if k == SyntaxKind::StringKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("string", idx);
                    TypeId::ERROR
                } else {
                    TypeId::STRING
                }
            }
            k if k == SyntaxKind::BooleanKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("boolean", idx);
                    TypeId::ERROR
                } else {
                    TypeId::BOOLEAN
                }
            }
            k if k == SyntaxKind::VoidKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("void", idx);
                    TypeId::ERROR
                } else {
                    TypeId::VOID
                }
            }
            k if k == SyntaxKind::AnyKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("any", idx);
                    TypeId::ERROR
                } else {
                    TypeId::ANY
                }
            }
            k if k == SyntaxKind::NeverKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("never", idx);
                    TypeId::ERROR
                } else {
                    TypeId::NEVER
                }
            }
            k if k == SyntaxKind::UnknownKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("unknown", idx);
                    TypeId::ERROR
                } else {
                    TypeId::UNKNOWN
                }
            }
            k if k == SyntaxKind::UndefinedKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("undefined", idx);
                    TypeId::ERROR
                } else {
                    TypeId::UNDEFINED
                }
            }
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if k == SyntaxKind::ObjectKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("object", idx);
                    TypeId::ERROR
                } else {
                    TypeId::OBJECT
                }
            }
            k if k == SyntaxKind::BigIntKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("bigint", idx);
                    TypeId::ERROR
                } else {
                    TypeId::BIGINT
                }
            }
            k if k == SyntaxKind::SymbolKeyword as u16 => {
                if self.checker.is_keyword_type_used_as_value_position(idx) {
                    self.checker.error_type_only_value_at("symbol", idx);
                    TypeId::ERROR
                } else {
                    TypeId::SYMBOL
                }
            }

            // Qualified name (A.B.C) - resolve namespace member access
            k if k == syntax_kind_ext::QUALIFIED_NAME => self.checker.resolve_qualified_name(idx),

            // Declaration nodes - not expressions, return VOID to avoid wasted work.
            // These are handled by check_statement → check_interface_declaration / check_class_declaration.
            // get_type_of_node may be called on them (e.g., for index signature compatibility checks),
            // but they don't have a meaningful expression type.
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::ENUM_DECLARATION
                || k == syntax_kind_ext::MODULE_DECLARATION =>
            {
                TypeId::VOID
            }

            // JSX Elements (Rule #36: JSX Intrinsic Lookup)
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(jsx) = self.checker.ctx.arena.get_jsx_element(node) {
                    self.checker
                        .get_type_of_jsx_opening_element(jsx.opening_element)
                } else {
                    TypeId::ERROR
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => {
                self.checker.get_type_of_jsx_opening_element(idx)
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                // JSX fragments resolve to JSX.Element type
                self.checker.get_jsx_element_type(idx)
            }

            // Non-null assertion: x!
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                // TS8013: Non-null assertions can only be used in TypeScript files
                if self.checker.is_js_file() {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.checker.error_at_node(
                        idx,
                        diagnostic_messages::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
                }
                // Get the operand type (strip the ! assertion — removes null/undefined)
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr_ex(node) {
                    let operand_type = self.checker.get_type_of_node(unary.expression);
                    tsz_solver::remove_nullish(
                        self.checker.ctx.types.as_type_database(),
                        operand_type,
                    )
                } else {
                    TypeId::ERROR
                }
            }

            // Type predicate nodes appear in function return type positions
            // (`x is T`) and are not value expressions.
            k if k == syntax_kind_ext::TYPE_PREDICATE => TypeId::BOOLEAN,

            // ExpressionWithTypeArguments: `expr<T>` used as a standalone expression
            // (e.g., `List<number>.makeChild()`). Evaluate the inner expression
            // to trigger name resolution (TS2304) even though the overall node
            // produces a parse error (TS1477).
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(data) = self.checker.ctx.arena.get_expr_type_args(node) {
                    self.checker.get_type_of_node(data.expression)
                } else {
                    TypeId::ERROR
                }
            }

            // Default case - unknown node kind is an error
            _ => {
                tracing::warn!(
                    idx = idx.0,
                    kind = node.kind,
                    "dispatch_type_computation: unknown expression kind"
                );
                TypeId::ERROR
            }
        }
    }
}
