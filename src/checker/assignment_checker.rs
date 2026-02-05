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

use crate::SyntaxKind;
use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{Diagnostic, DiagnosticCategory, diagnostic_codes};
use crate::parser::NodeIndex;
use crate::solver::TypeId;

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

        self.check_readonly_assignment(left_idx, expr_idx);

        if left_type != TypeId::ANY {
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
            } else if !self.is_assignable_to(right_type, left_type)
                && !self.should_skip_weak_union_error(right_type, left_type, right_idx)
            {
                self.error_type_not_assignable_with_reason_at(right_type, left_type, right_idx);
            }

            if left_type != TypeId::UNKNOWN
                && let Some(right_node) = self.ctx.arena.get(right_idx)
                && right_node.kind == crate::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
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
        use crate::solver::BinaryOpEvaluator;

        // Check if this is an enum type (Lazy/DefId to an enum symbol)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                // Check if the symbol is an enum (ENUM flags)
                use crate::binder::symbol_flags;
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
    fn check_arithmetic_operands(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
    ) {
        let left_is_valid = self.is_arithmetic_operand(left_type);
        let right_is_valid = self.is_arithmetic_operand(right_type);

        if !left_is_valid {
            if let Some(loc) = self.get_source_location(left_idx) {
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
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
                    code: diagnostic_codes::RIGHT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                    category: DiagnosticCategory::Error,
                    message_text: "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
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

        self.check_readonly_assignment(left_idx, expr_idx);

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
            self.check_arithmetic_operands(left_idx, right_idx, left_type, right_type);
        }

        // Check bitwise compound assignments: &=, |=, ^=, <<=, >>=, >>>=
        let is_bitwise_compound = matches!(
            operator,
            k if k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
        );
        if is_bitwise_compound {
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

        if left_type != TypeId::ANY {
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
                && right_node.kind == crate::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
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
        use crate::solver::{BinaryOpEvaluator, BinaryOpResult};

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
