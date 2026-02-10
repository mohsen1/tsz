//! Expression type computation dispatcher.
//!
//! This module provides the ExpressionDispatcher which handles the dispatch
//! of type computation requests to appropriate specialized methods based on
//! the syntax node kind.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Dispatcher for expression type computation.
///
/// ExpressionDispatcher handles the dispatch of type computation for different
/// node kinds, delegating to specialized methods in CheckerState.
pub struct ExpressionDispatcher<'a, 'b> {
    /// Reference to the checker state.
    pub checker: &'a mut CheckerState<'b>,
}

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    /// Create a new expression dispatcher.
    pub fn new(checker: &'a mut CheckerState<'b>) -> Self {
        Self { checker }
    }

    /// Resolve a literal type: preserve the narrow literal if we're in a const
    /// assertion or contextual typing expects it, otherwise widen to `widened`.
    fn resolve_literal(&mut self, literal_type: Option<TypeId>, widened: TypeId) -> TypeId {
        match literal_type {
            Some(lit)
                if self.checker.ctx.in_const_assertion
                    || self.checker.contextual_literal_type(lit).is_some() =>
            {
                lit
            }
            _ => widened,
        }
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
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
                    this_type
                } else if let Some(ref class_info) = self.checker.ctx.enclosing_class {
                    // Inside a class but no explicit this type on stack -
                    // return the class instance type (e.g., for constructor default params)
                    if let Some(class_node) = self.checker.ctx.arena.get(class_info.class_idx)
                        && let Some(class_data) = self.checker.ctx.arena.get_class(class_node)
                    {
                        return self
                            .checker
                            .get_class_instance_type(class_info.class_idx, class_data);
                    }
                    TypeId::ANY
                } else if self.checker.this_has_contextual_owner(idx).is_some() {
                    // `this` in a class or object literal member but enclosing_class
                    // not yet set. Suppress TS2683 - `this` is contextually typed.
                    TypeId::ANY
                } else if self.checker.ctx.no_implicit_this()
                    && self
                        .checker
                        .find_enclosing_non_arrow_function(idx)
                        .is_some()
                {
                    // TS2683: 'this' implicitly has type 'any'
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
                    self.checker.get_class_constructor_type(idx, &class)
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
                self.checker.get_type_of_function(idx)
            }

            // Arrow function
            k if k == syntax_kind_ext::ARROW_FUNCTION => self.checker.get_type_of_function(idx),

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

            // Postfix unary expression - ++ and -- require numeric operand
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr(node) {
                    // TS2588: Cannot assign to 'x' because it is a constant.
                    let is_const = self.checker.check_const_assignment(unary.operand);

                    // TS2540: Cannot assign to readonly property (e.g., namespace const export)
                    if !is_const {
                        self.checker.check_readonly_assignment(unary.operand, idx);
                    }

                    // Get operand type for validation
                    let operand_type = self.checker.get_type_of_node(unary.operand);

                    if !is_const {
                        // Check if operand is valid for increment/decrement
                        use tsz_solver::BinaryOpEvaluator;
                        let evaluator = BinaryOpEvaluator::new(self.checker.ctx.types);
                        let is_valid = evaluator.is_arithmetic_operand(operand_type);

                        if !is_valid {
                            use crate::types::diagnostics::{
                                diagnostic_codes, diagnostic_messages,
                            };
                            self.checker.error_at_node(
                                unary.operand,
                                diagnostic_messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                                diagnostic_codes::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                            );
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

            // Parenthesized expression - just pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    // Check if expression is missing (parse error: empty parentheses)
                    if paren.expression.is_none() {
                        // Parse error - return ERROR to suppress cascading errors
                        return TypeId::ERROR;
                    }
                    self.checker.get_type_of_node(paren.expression)
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
                        let asserted_type =
                            self.checker.get_type_from_type_node(assertion.type_node);

                        // For `satisfies`, set contextual type before checking expression
                        // This enables contextual typing for lambdas, object literals, etc.
                        let prev_contextual_type = self.checker.ctx.contextual_type;
                        if k == syntax_kind_ext::SATISFIES_EXPRESSION {
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
                            self.checker.ensure_application_symbols_resolved(expr_type);
                            self.checker
                                .ensure_application_symbols_resolved(asserted_type);
                            if !self.checker.type_contains_error(asserted_type)
                                && !self.checker.is_assignable_to(expr_type, asserted_type)
                                && !self.checker.should_skip_weak_union_error(
                                    expr_type,
                                    asserted_type,
                                    assertion.expression,
                                )
                            {
                                self.checker.error_type_not_assignable_with_reason_at(
                                    expr_type,
                                    asserted_type,
                                    assertion.expression,
                                );
                            }
                            expr_type
                        } else {
                            // `expr as T` / `<T>expr` yields `T`.
                            // TS2352: Check if conversion may be a mistake (types don't sufficiently overlap)
                            self.checker.ensure_application_symbols_resolved(expr_type);
                            self.checker
                                .ensure_application_symbols_resolved(asserted_type);

                            // Don't check if either type is error, any, unknown, or never
                            let should_check = !self.checker.type_contains_error(expr_type)
                                && !self.checker.type_contains_error(asserted_type)
                                && expr_type != TypeId::ANY
                                && asserted_type != TypeId::ANY
                                && expr_type != TypeId::UNKNOWN
                                && asserted_type != TypeId::UNKNOWN
                                && expr_type != TypeId::NEVER
                                && asserted_type != TypeId::NEVER;

                            if should_check {
                                // TS2352 is emitted if neither type is assignable to the other
                                // (i.e., the types don't "sufficiently overlap")
                                let source_to_target =
                                    self.checker.is_assignable_to(expr_type, asserted_type);
                                let target_to_source =
                                    self.checker.is_assignable_to(asserted_type, expr_type);

                                if !source_to_target && !target_to_source {
                                    self.checker.error_type_assertion_no_overlap(
                                        expr_type,
                                        asserted_type,
                                        idx,
                                    );
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

            // No-substitution template literal - preserve literal type when contextual typing expects it.
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                let literal_type = self.checker.literal_type_from_initializer(idx);
                if let Some(literal_type) = literal_type {
                    // Preserve literal type if in const assertion OR if contextual typing allows it
                    if self.checker.ctx.in_const_assertion
                        || self.checker.contextual_literal_type(literal_type).is_some()
                    {
                        literal_type
                    } else {
                        TypeId::STRING
                    }
                } else {
                    TypeId::STRING
                }
            }

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

            // Keyword types - handled inline for performance (these are simple constants)
            k if k == SyntaxKind::NumberKeyword as u16 => TypeId::NUMBER,
            k if k == SyntaxKind::StringKeyword as u16 => TypeId::STRING,
            k if k == SyntaxKind::BooleanKeyword as u16 => TypeId::BOOLEAN,
            k if k == SyntaxKind::VoidKeyword as u16 => TypeId::VOID,
            k if k == SyntaxKind::AnyKeyword as u16 => TypeId::ANY,
            k if k == SyntaxKind::NeverKeyword as u16 => TypeId::NEVER,
            k if k == SyntaxKind::UnknownKeyword as u16 => TypeId::UNKNOWN,
            k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::UNDEFINED,
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
            k if k == SyntaxKind::ObjectKeyword as u16 => TypeId::OBJECT,
            k if k == SyntaxKind::BigIntKeyword as u16 => TypeId::BIGINT,
            k if k == SyntaxKind::SymbolKeyword as u16 => TypeId::SYMBOL,

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
                self.checker.get_jsx_element_type()
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
