//! Assignment Checking Module
//!
//! This module contains methods for checking assignment expressions.
//! It handles:
//! - Simple assignment (=)
//! - Compound assignment (+=, -=, *=, etc.)
//! - Logical assignment (&&=, ||=, ??=)
//! - Arithmetic operand validation (TS2362/TS2363)
//! - Readonly property assignment checking
//!
//! This module extends CheckerState with assignment-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::CheckerState;
use crate::types::diagnostics::{Diagnostic, DiagnosticCategory, diagnostic_codes};
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
    // =========================================================================
    // Assignment Operator Utilities
    // =========================================================================

    /// Check if a token is an assignment operator (=, +=, -=, etc.)
    pub(crate) fn is_assignment_operator(&self, operator: u16) -> bool {
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
    fn is_valid_assignment_target(&self, idx: NodeIndex) -> bool {
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

    /// Check if an identifier node refers to a const variable.
    ///
    /// Returns `Some(name)` if the identifier refers to a const, `None` otherwise.
    fn get_const_variable_name(&self, ident_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(ident_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.clone();

        let sym_id = self.resolve_identifier_symbol(ident_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return None;
        }

        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }

        let decl_node = self.ctx.arena.get(value_decl)?;
        let mut decl_flags = decl_node.flags as u32;

        // If CONST/LET not directly on node, check parent (VariableDeclarationList)
        if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0 {
            if let Some(ext) = self.ctx.arena.get_extended(value_decl)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                decl_flags |= parent_node.flags as u32;
            }
        }

        if decl_flags & node_flags::CONST != 0 {
            Some(name)
        } else {
            None
        }
    }

    /// Check if the assignment target (LHS) is a const variable and emit TS2588 if so.
    ///
    /// Resolves through parenthesized expressions to find the underlying identifier.
    /// Returns `true` if a TS2588 error was emitted (caller should skip further type checks).
    pub(crate) fn check_const_assignment(&mut self, target_idx: NodeIndex) -> bool {
        let inner = self.skip_parenthesized_expression(target_idx);
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
        if !self.is_valid_assignment_target(left_idx) {
            self.error_at_node(
                left_idx,
                "The left-hand side of an assignment expression must be a variable or a property access.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY,
            );
        }

        // TS2588: Cannot assign to 'x' because it is a constant.
        // Check early - if this fires, skip type assignability checks (tsc behavior).
        let is_const = self.check_const_assignment(left_idx);

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
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx);
        }

        if !is_const && left_type != TypeId::ANY {
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    right_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !is_array_destructuring
                && !self.is_assignable_to(right_type, left_type)
                && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
            {
                // Don't emit errors if either side is ERROR - prevents cascading errors
                if right_type != TypeId::ERROR && left_type != TypeId::ERROR {
                    self.error_type_not_assignable_with_reason_at(right_type, left_type, right_idx);
                }
            }

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                self.check_object_literal_excess_properties(right_type, left_type, right_idx);
            }
        }

        right_type
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
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                // Check if the symbol is an enum (ENUM flags)
                use tsz_binder::symbol_flags;
                if (symbol.flags & symbol_flags::ENUM) != 0 {
                    return true;
                }
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
        let left_is_valid = self.is_arithmetic_operand(left_type);
        let right_is_valid = self.is_arithmetic_operand(right_type);

        if !left_is_valid {
            if let Some(loc) = self.get_source_location(left_idx) {
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                    category: DiagnosticCategory::Error,
                    message_text: "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
        }

        if !right_is_valid {
            if let Some(loc) = self.get_source_location(right_idx) {
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                    category: DiagnosticCategory::Error,
                    message_text: "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
        }

        !left_is_valid || !right_is_valid
    }

    /// Emit TS2447 error for boolean bitwise operators (&, |, ^, &=, |=, ^=).
    fn emit_boolean_operator_error(&mut self, node_idx: NodeIndex, op_str: &str, suggestion: &str) {
        if let Some(loc) = self.get_source_location(node_idx) {
            let message = format!(
                "The '{}' operator is not allowed for boolean types. Consider using '{}' instead.",
                op_str, suggestion
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
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
        if !self.is_valid_assignment_target(left_idx) {
            self.error_at_node(
                left_idx,
                "The left-hand side of an assignment expression must be a variable or a property access.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY,
            );
        }

        // TS2588: Cannot assign to 'x' because it is a constant.
        let is_const = self.check_const_assignment(left_idx);

        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let prev_context = self.ctx.contextual_type;
        if left_type != TypeId::ANY && !self.type_contains_error(left_type) {
            self.ctx.contextual_type = Some(left_type);
        }

        let right_raw = self.get_type_of_node(right_idx);
        let right_type = self.resolve_type_query_type(right_raw);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ctx.contextual_type = prev_context;

        self.ensure_application_symbols_resolved(right_type);
        self.ensure_application_symbols_resolved(left_type);

        if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx);
        }

        // Track whether an operator error was emitted so we can suppress cascading TS2322.
        // TSC doesn't emit TS2322 when there's already an operator error (TS2447/TS2362/TS2363).
        let mut emitted_operator_error = is_const;

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
        if is_arithmetic_compound {
            // Don't emit arithmetic errors if either operand is ERROR - prevents cascading errors
            if left_type != TypeId::ERROR && right_type != TypeId::ERROR {
                emitted_operator_error |=
                    self.check_arithmetic_operands(left_idx, right_idx, left_type, right_type);
            }
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
        if is_boolean_bitwise_compound {
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
        } else if is_shift_compound && left_type != TypeId::ERROR && right_type != TypeId::ERROR {
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
            if let Some((source_level, target_level)) =
                self.constructor_accessibility_mismatch_for_assignment(left_idx, right_idx)
            {
                self.error_constructor_accessibility_not_assignable(
                    assigned_type,
                    left_type,
                    source_level,
                    target_level,
                    right_idx,
                );
            } else if !self.is_assignable_to(assigned_type, left_type)
                && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
            {
                self.error_type_not_assignable_with_reason_at(assigned_type, left_type, right_idx);
            }

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
            _ => None,
        };

        if let Some(op) = op_str {
            return match evaluator.evaluate(left_type, right_type, op) {
                BinaryOpResult::Success(result) => result,
                // Return ANY instead of UNKNOWN for type errors to prevent cascading errors
                BinaryOpResult::TypeError { .. } => TypeId::ANY,
            };
        }

        if operator == SyntaxKind::QuestionQuestionEqualsToken as u16 {
            return self.ctx.types.union2(left_type, right_type);
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
}
