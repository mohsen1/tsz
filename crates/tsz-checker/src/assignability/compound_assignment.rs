//! Compound assignment expression checking (+=, -=, *=, &&=, ??=, etc.).

use crate::context::TypingRequest;
use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

// =============================================================================
// Compound Assignment Checking
// =============================================================================

impl<'a> CheckerState<'a> {
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
                    crate::diagnostics::diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_A,
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
        let left_read_raw = self.get_type_of_node(left_idx);
        let left_read_type = self.resolve_type_query_type(left_read_raw);

        let left_target = self.get_type_of_assignment_target(left_idx);
        let left_type = self.resolve_type_query_type(left_target);

        let request = if left_type != TypeId::ANY
            && left_type != TypeId::NEVER
            && left_type != TypeId::UNKNOWN
            && !self.type_contains_error(left_type)
        {
            TypingRequest::with_contextual_type(left_type)
        } else {
            TypingRequest::NONE
        };

        let right_raw = self.get_type_of_node_with_request(right_idx, &request);
        let right_type = self.resolve_type_query_type(right_raw);

        // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
        // No need to manually track freshness removal here.

        self.ensure_relation_input_ready(right_type);
        self.ensure_relation_input_ready(left_type);

        // Check readonly first — suppress TS2322 when TS2540/TS2542 fires.
        let is_readonly_target = if !is_const {
            self.check_readonly_assignment(left_idx, expr_idx)
        } else {
            false
        };

        // Track whether an operator error was emitted so we can suppress cascading TS2322.
        // TSC doesn't emit TS2322 when there's already an operator error (TS2447/TS2362/TS2363).
        let mut emitted_operator_error = is_const || is_function_assignment || is_readonly_target;

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

        let emitted_nullish_error = if !op_str.is_empty() {
            self.check_and_emit_nullish_binary_operands(
                left_idx,
                right_idx,
                left_read_type,
                right_type,
                op_str,
            )
        } else {
            false
        };
        emitted_operator_error |= emitted_nullish_error;

        // TS2469: For += with symbol operands, emit when one side is symbol and the
        // other is string or any. Uses "+=" in the message (not "+").
        if operator == SyntaxKind::PlusEqualsToken as u16
            && left_read_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            let left_is_symbol = evaluator.is_symbol_like(left_read_type);
            let right_is_symbol = evaluator.is_symbol_like(right_type);
            if left_is_symbol || right_is_symbol {
                let left_is_string_or_any = left_read_type == TypeId::ANY
                    || left_read_type == TypeId::STRING
                    || tsz_solver::type_queries::is_string_literal(self.ctx.types, left_read_type);
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
            && left_read_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            // Evaluate types to resolve IndexAccess/Application types before checking.
            // e.g. `T[K]` where `T extends Record<K, number>` should resolve to `number`
            // so the += operator is correctly accepted.
            let eval_left = self.evaluate_type_for_binary_ops(left_read_type);
            let eval_right = self.evaluate_type_for_binary_ops(right_type);
            let result = evaluator.evaluate(eval_left, eval_right, "+");
            if let crate::query_boundaries::type_computation::core::BinaryOpResult::TypeError {
                ..
            } = result
            {
                // For the diagnostic message, tsc uses widened types for most
                // operands (e.g., `0` → `number`, `true` → `boolean`).
                // Widen literal types to base types and enum members to
                // parent enums, matching tsc behavior for messages like
                // "Operator '+=' cannot be applied to types 'boolean' and 'number'."
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
            if left_read_type != TypeId::ERROR && right_type != TypeId::ERROR {
                let had_per_operand_error =
                    self.check_arithmetic_operands(left_idx, right_idx, left_read_type, right_type);
                emitted_operator_error |= had_per_operand_error;

                // TS2365: Check for bigint/number mixing in arithmetic compound assignments
                if !had_per_operand_error && !emitted_operator_error {
                    self.check_compound_assignment_type_compatibility(
                        expr_idx,
                        operator,
                        left_read_type,
                        right_type,
                        &mut emitted_operator_error,
                    );
                }
            }
        }

        // TS2791: bigint exponentiation assignment requires target >= ES2016.
        // Skip when either type is any/unknown (TSC skips the bigint branch for those).
        if operator == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            && (self.ctx.compiler_options.target as u32)
                < (tsz_common::common::ScriptTarget::ES2016 as u32)
            && left_read_type != TypeId::ANY
            && right_type != TypeId::ANY
            && left_read_type != TypeId::UNKNOWN
            && right_type != TypeId::UNKNOWN
            && self.is_subtype_of(left_read_type, TypeId::BIGINT)
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
        if is_boolean_bitwise_compound && !is_function_assignment && !emitted_nullish_error {
            // TS2447: For &=, |=, ^= with both boolean operands, emit special error
            let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
            let left_is_boolean = evaluator.is_boolean_like(left_read_type);
            let right_is_boolean = evaluator.is_boolean_like(right_type);
            if left_is_boolean && right_is_boolean {
                let (op_str, suggestion) = match operator {
                    k if k == SyntaxKind::AmpersandEqualsToken as u16 => ("&=", "&&"),
                    k if k == SyntaxKind::BarEqualsToken as u16 => ("|=", "||"),
                    _ => ("^=", "!=="),
                };
                self.emit_boolean_operator_error(left_idx, op_str, suggestion);
                emitted_operator_error = true;
            } else if left_read_type != TypeId::ERROR && right_type != TypeId::ERROR {
                let had_per_operand_error =
                    self.check_arithmetic_operands(left_idx, right_idx, left_read_type, right_type);
                emitted_operator_error |= had_per_operand_error;

                // TS2365: Check for bigint/number mixing in bitwise compound assignments
                if !had_per_operand_error && !emitted_operator_error {
                    self.check_compound_assignment_type_compatibility(
                        expr_idx,
                        operator,
                        left_read_type,
                        right_type,
                        &mut emitted_operator_error,
                    );
                }
            }
        } else if is_shift_compound
            && !is_function_assignment
            && !emitted_nullish_error
            && left_read_type != TypeId::ERROR
            && right_type != TypeId::ERROR
        {
            let had_per_operand_error =
                self.check_arithmetic_operands(left_idx, right_idx, left_read_type, right_type);
            emitted_operator_error |= had_per_operand_error;

            // TS2365: Check for bigint/number mixing in shift compound assignments
            if !had_per_operand_error && !emitted_operator_error {
                self.check_compound_assignment_type_compatibility(
                    expr_idx,
                    operator,
                    left_read_type,
                    right_type,
                    &mut emitted_operator_error,
                );
            }
        }

        let result_type =
            self.compound_assignment_result_type(left_read_type, right_type, operator);
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
        use crate::query_boundaries::type_computation::core::BinaryOpResult;
        use tsz_solver::BinaryOpEvaluator;

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let op_str = match operator {
            k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
            k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("**"),
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

        let bitwise_op = match operator {
            k if k == SyntaxKind::AmpersandEqualsToken as u16 => Some("&"),
            k if k == SyntaxKind::BarEqualsToken as u16 => Some("|"),
            k if k == SyntaxKind::CaretEqualsToken as u16 => Some("^"),
            k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => Some("<<"),
            k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => Some(">>"),
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => {
                Some(">>>")
            }
            _ => None,
        };
        if let Some(op) = bitwise_op {
            return match evaluator.evaluate(left_type, right_type, op) {
                BinaryOpResult::Success(result) => result,
                BinaryOpResult::TypeError { .. } => TypeId::NUMBER,
            };
        }

        // Return ANY for unknown binary operand types to prevent cascading errors
        TypeId::ANY
    }
}
