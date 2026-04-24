//! Arithmetic operand validation for assignment operators.
//!
//! Handles validation of arithmetic operations including:
//! - TS2362/TS2363: Arithmetic operand type validation
//! - TS2447: Boolean bitwise operator errors
//! - TS2365: Compound assignment type compatibility (bigint/number mixing)

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if an operand type is valid for arithmetic operations.
    ///
    /// Returns true if the type is number, bigint, any, or an enum type.
    /// This is used to validate operands for TS2362/TS2363 errors.
    fn is_arithmetic_operand(&self, type_id: TypeId) -> bool {
        // Check if this is an enum type (Lazy/DefId to an enum symbol)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // Check if the symbol is an enum (ENUM flags)
            use tsz_binder::symbol_flags;
            if symbol.has_any_flags(symbol_flags::ENUM) {
                return true;
            }
        }

        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        evaluator.is_arithmetic_operand(type_id)
    }

    /// Check and emit TS2362/TS2363 errors for arithmetic operations.
    ///
    /// For operators like -, *, /, %, **, -=, *=, /=, %=, **=,
    /// validates that operands are of type number, bigint, any, or enum.
    /// Emits appropriate errors when operands are invalid.
    /// Returns true if any error was emitted.
    pub(crate) fn check_arithmetic_operands(
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

        // Strip null/undefined before checking arithmetic validity.
        // tsc calls checkNonNullType() first (emitting TS18048/TS2532), then checks the
        // remaining type. So `number | undefined` → strip to `number` → valid arithmetic.
        // Pure null/undefined becomes NEVER after stripping, which is also valid.
        // However, `void` as a standalone type should NOT be stripped — TSC checks `void`
        // directly against `number | bigint` and emits TS2362/TS2363 when it fails.
        // Stripping `void` to `never` would falsely pass the arithmetic check.
        let left_stripped = if left_eval == TypeId::VOID {
            left_eval
        } else {
            crate::query_boundaries::common::remove_nullish(self.ctx.types, left_eval)
        };
        let right_stripped = if right_eval == TypeId::VOID {
            right_eval
        } else {
            crate::query_boundaries::common::remove_nullish(self.ctx.types, right_eval)
        };
        let left_is_valid = self.is_arithmetic_operand(left_stripped);
        let right_is_valid = self.is_arithmetic_operand(right_stripped);

        let mut emitted = false;

        // Skip per-side emission when that side already resolved to ERROR
        // (e.g. TS2304 for an undeclared identifier). tsc still validates the
        // other side — `kj **= \`${x}\`` produces TS2304 and TS2363 for the
        // template RHS even though `kj` is unresolved.
        if !left_is_valid && left_type != TypeId::ERROR {
            self.error_at_node(
                left_idx,
                "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        if !right_is_valid && right_type != TypeId::ERROR {
            self.error_at_node(
                right_idx,
                "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
            );
            emitted = true;
        }

        emitted
    }

    /// Emit TS2447 error for boolean bitwise operators (&, |, ^, &=, |=, ^=).
    pub(crate) fn emit_boolean_operator_error(
        &mut self,
        node_idx: NodeIndex,
        op_str: &str,
        suggestion: &str,
    ) {
        let message = format!(
            "The '{op_str}' operator is not allowed for boolean types. Consider using '{suggestion}' instead."
        );
        self.error_at_node(
            node_idx,
            &message,
            diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
        );
    }

    /// TS2365: Check for bigint/number type mixing in compound assignment operators.
    /// When both operands are individually valid arithmetic types but the binary operation
    /// would fail (e.g., bigint -= number), emit TS2365.
    pub(crate) fn check_compound_assignment_type_compatibility(
        &mut self,
        expr_idx: NodeIndex,
        operator: u16,
        left_read_type: TypeId,
        right_type: TypeId,
        emitted_operator_error: &mut bool,
    ) {
        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        let eval_left = self.evaluate_type_for_binary_ops(left_read_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);
        if let Some(binary_op) =
            crate::query_boundaries::common::map_compound_assignment_to_binary(operator)
        {
            let result = evaluator.evaluate(eval_left, eval_right, binary_op);
            if let crate::query_boundaries::type_computation::core::BinaryOpResult::TypeError {
                ..
            } = result
            {
                let compound_op_str = match operator {
                    k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
                    k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
                    k if k == SyntaxKind::SlashEqualsToken as u16 => "/=",
                    k if k == SyntaxKind::PercentEqualsToken as u16 => "%=",
                    k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**=",
                    k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<=",
                    k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>=",
                    k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => {
                        ">>>="
                    }
                    k if k == SyntaxKind::AmpersandEqualsToken as u16 => "&=",
                    k if k == SyntaxKind::BarEqualsToken as u16 => "|=",
                    k if k == SyntaxKind::CaretEqualsToken as u16 => "^=",
                    _ => "?=",
                };
                let left_diag = self.widen_enum_member_type(
                    crate::query_boundaries::common::widen_literal_type(
                        self.ctx.types,
                        left_read_type,
                    ),
                );
                let right_diag = self.widen_enum_member_type(
                    crate::query_boundaries::common::widen_literal_type(self.ctx.types, right_type),
                );
                let left_str = self.format_type(left_diag);
                let right_str = self.format_type(right_diag);
                let message = format!(
                    "Operator '{compound_op_str}' cannot be applied to types '{left_str}' and '{right_str}'."
                );
                self.error_at_node(
                    expr_idx,
                    &message,
                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
                );
                *emitted_operator_error = true;
            }
        }
    }
}
