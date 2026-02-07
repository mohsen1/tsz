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
                if let Some(this_type) = self.checker.current_this_type() {
                    this_type
                } else if let Some(ref class_info) = self.checker.ctx.enclosing_class.clone() {
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
                } else {
                    // Not in a class - check if we're in a NON-ARROW function
                    // Arrow functions capture `this` from their enclosing scope, so they
                    // should NOT trigger TS2683. We need to skip past arrow functions
                    // to find the actual enclosing function that defines the `this` context.
                    if self.checker.ctx.no_implicit_this()
                        && self
                            .checker
                            .find_enclosing_non_arrow_function(idx)
                            .is_some()
                    {
                        // TS2683: 'this' implicitly has type 'any'
                        // Only emit when noImplicitThis is enabled
                        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.checker.error_at_node(
                            idx,
                            diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY,
                            diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY,
                        );
                        TypeId::ANY
                    } else {
                        // Outside function, only inside arrow functions, or noImplicitThis disabled
                        // Use ANY for recovery without error
                        TypeId::ANY
                    }
                }
            }
            k if k == SyntaxKind::SuperKeyword as u16 => {
                self.checker.get_type_of_super_keyword(idx)
            }

            // Literals - preserve literal types when contextual typing expects them.
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let literal_type = self.checker.literal_type_from_initializer(idx);
                if let Some(literal_type) = literal_type {
                    // Preserve literal type if in const assertion OR if contextual typing allows it
                    if self.checker.ctx.in_const_assertion
                        || self.checker.contextual_literal_type(literal_type).is_some()
                    {
                        literal_type
                    } else {
                        TypeId::NUMBER
                    }
                } else {
                    TypeId::NUMBER
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
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
            // Boolean literals - preserve literal type when contextual typing expects it.
            k if k == SyntaxKind::TrueKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(true);
                // Preserve literal type if in const assertion OR if contextual typing allows it
                if self.checker.ctx.in_const_assertion
                    || self.checker.contextual_literal_type(literal_type).is_some()
                {
                    literal_type
                } else {
                    TypeId::BOOLEAN
                }
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                let literal_type = self.checker.ctx.types.literal_boolean(false);
                // Preserve literal type if in const assertion OR if contextual typing allows it
                if self.checker.ctx.in_const_assertion
                    || self.checker.contextual_literal_type(literal_type).is_some()
                {
                    literal_type
                } else {
                    TypeId::BOOLEAN
                }
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
                if let Some(unary) = self.checker.ctx.arena.get_unary_expr_ex(node) {
                    // Get operand type for validation
                    let operand_type = self.checker.get_type_of_node(unary.expression);

                    // Check if operand is valid for increment/decrement
                    use tsz_solver::BinaryOpEvaluator;
                    let evaluator = BinaryOpEvaluator::new(self.checker.ctx.types);
                    let is_valid = evaluator.is_arithmetic_operand(operand_type);

                    if !is_valid {
                        // Emit TS2356 for invalid increment/decrement operand type
                        if let Some(loc) = self.checker.get_source_location(unary.expression) {
                            use crate::types::diagnostics::{
                                Diagnostic, DiagnosticCategory, diagnostic_codes,
                                diagnostic_messages,
                            };
                            self.checker.ctx.diagnostics.push(Diagnostic {
                                code: diagnostic_codes::ARITHMETIC_OPERAND_MUST_BE_NUMBER,
                                category: DiagnosticCategory::Error,
                                message_text:
                                    diagnostic_messages::ARITHMETIC_OPERAND_MUST_BE_NUMBER
                                        .to_string(),
                                file: self.checker.ctx.file_name.clone(),
                                start: loc.start,
                                length: loc.length(),
                                related_information: Vec::new(),
                            });
                        }
                    }
                }

                TypeId::NUMBER
            }

            // typeof expression
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,

            // void expression
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,

            // Non-null assertion (e.g., `expr!`) - strip null/undefined from inner type
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(data) = self.checker.ctx.arena.get_unary_expr_ex(node) {
                    let inner_type = self.checker.get_type_of_node(data.expression);
                    tsz_solver::remove_nullish(
                        self.checker.ctx.types.as_type_database(),
                        inner_type,
                    )
                } else {
                    TypeId::ERROR
                }
            }

            // await expression - unwrap Promise<T> to get T, with contextual typing (Phase 6 - tsz-3)
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                self.checker.get_type_of_await_expression(idx)
            }

            // Parenthesized expression - just pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.checker.ctx.arena.get_parenthesized(node) {
                    self.checker.get_type_of_node(paren.expression)
                } else {
                    // Return ANY to prevent cascading TS2571 errors
                    TypeId::ANY
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

                    // Always type-check the expression for side effects / diagnostics.
                    let expr_type = self.checker.get_type_of_node(assertion.expression);

                    // Restore the previous flag value
                    self.checker.ctx.in_const_assertion = prev_in_const_assertion;

                    // In recovery scenarios we may not have a type node; fall back to the expression type.
                    if assertion.type_node.is_none() {
                        expr_type
                    } else if is_const_assertion {
                        // as const: apply const assertion to the expression type
                        use tsz_solver::widening::apply_const_assertion;
                        apply_const_assertion(self.checker.ctx.types, expr_type)
                    } else {
                        let asserted_type =
                            self.checker.get_type_from_type_node(assertion.type_node);
                        if k == syntax_kind_ext::SATISFIES_EXPRESSION {
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
