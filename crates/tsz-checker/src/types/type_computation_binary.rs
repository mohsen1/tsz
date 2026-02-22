//! Binary expression type computation.
//! Extracted from `type_computation.rs` — handles all binary operators including
//! arithmetic, comparison, logical, assignment, nullish coalescing, and comma.

use crate::diagnostics::Diagnostic;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a binary expression.
    ///
    /// Handles all binary operators including arithmetic, comparison, logical,
    /// assignment, nullish coalescing, and comma operators.
    pub(crate) fn get_type_of_binary_expression(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;
        use tsz_solver::{BinaryOpEvaluator, BinaryOpResult};
        let factory = self.ctx.types.factory();

        // Hot path: pure `+` chains with stable primitive operands are common in
        // generated benchmark fixtures. We still check every operand node (so
        // operand diagnostics are preserved), but skip generic per-node binary
        // operator evaluation when the final result is deterministic.
        if let Some(root_node) = self.ctx.arena.get(idx)
            && root_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(root_binary) = self.ctx.arena.get_binary_expr(root_node)
            && root_binary.operator_token == SyntaxKind::PlusToken as u16
        {
            let mut all_plus = true;
            let mut operand_nodes = Vec::new();
            let mut pending = vec![idx];

            while let Some(node_idx) = pending.pop() {
                let Some(node) = self.ctx.arena.get(node_idx) else {
                    all_plus = false;
                    break;
                };

                if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(binary) = self.ctx.arena.get_binary_expr(node)
                {
                    if binary.operator_token == SyntaxKind::PlusToken as u16 {
                        pending.push(binary.right);
                        pending.push(binary.left);
                        continue;
                    }
                    all_plus = false;
                    break;
                }

                operand_nodes.push(node_idx);
            }

            if all_plus && operand_nodes.len() > 1 {
                let mut all_number = true;
                let mut all_bigint = true;
                let mut all_string = true;
                let mut has_any = false;

                for node_idx in operand_nodes {
                    let ty = self.get_type_of_node(node_idx);
                    if ty == TypeId::ERROR {
                        return TypeId::ERROR;
                    }
                    has_any |= ty == TypeId::ANY;
                    all_number &= ty == TypeId::NUMBER;
                    all_bigint &= ty == TypeId::BIGINT;
                    all_string &= ty == TypeId::STRING;
                }

                if all_number {
                    return TypeId::NUMBER;
                }
                if all_bigint {
                    return TypeId::BIGINT;
                }
                if all_string {
                    return TypeId::STRING;
                }
                if has_any {
                    return TypeId::ANY;
                }
            }
        }

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        let mut stack = vec![(idx, false)];
        let mut type_stack: Vec<TypeId> = Vec::new();

        while let Some((node_idx, visited)) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                // Return UNKNOWN instead of ANY when node cannot be found
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                type_stack.push(self.get_type_of_node(node_idx));
                continue;
            }

            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                // Return UNKNOWN instead of ANY when binary expression cannot be extracted
                type_stack.push(TypeId::UNKNOWN);
                continue;
            };

            let left_idx = binary.left;
            let right_idx = binary.right;
            let op_kind = binary.operator_token;

            // TS5076: Check for mixing ?? with || or && without parentheses.
            // Only check on first visit to avoid duplicates from the stack-based iteration.
            if !visited {
                let is_nullish_coalescing = op_kind == SyntaxKind::QuestionQuestionToken as u16;
                let is_logical = op_kind == SyntaxKind::BarBarToken as u16
                    || op_kind == SyntaxKind::AmpersandAmpersandToken as u16;

                if is_nullish_coalescing || is_logical {
                    // Check left operand: is it a binary expr with a conflicting operator?
                    if let Some(left_node) = self.ctx.arena.get(left_idx)
                        && left_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(left_binary) = self.ctx.arena.get_binary_expr(left_node)
                    {
                        let left_op = left_binary.operator_token;
                        let left_is_nullish = left_op == SyntaxKind::QuestionQuestionToken as u16;
                        let left_is_logical = left_op == SyntaxKind::BarBarToken as u16
                            || left_op == SyntaxKind::AmpersandAmpersandToken as u16;

                        if (is_nullish_coalescing && left_is_logical)
                            || (is_logical && left_is_nullish)
                        {
                            // Determine operator names for the error message
                            let left_op_str = if left_is_nullish {
                                "??"
                            } else if left_op == SyntaxKind::BarBarToken as u16 {
                                "||"
                            } else {
                                "&&"
                            };
                            let right_op_str = if is_nullish_coalescing {
                                "??"
                            } else if op_kind == SyntaxKind::BarBarToken as u16 {
                                "||"
                            } else {
                                "&&"
                            };
                            if let Some(loc) = self.get_source_location(left_idx) {
                                use crate::diagnostics::{
                                    Diagnostic, diagnostic_codes, diagnostic_messages,
                                    format_message,
                                };
                                self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), format_message(diagnostic_messages::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES, &[left_op_str, right_op_str]), diagnostic_codes::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES));
                            }
                        }
                    }

                    // Check right operand
                    if let Some(right_node) = self.ctx.arena.get(right_idx)
                        && right_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                        && let Some(right_binary) = self.ctx.arena.get_binary_expr(right_node)
                    {
                        let right_op = right_binary.operator_token;
                        let right_is_nullish = right_op == SyntaxKind::QuestionQuestionToken as u16;
                        let right_is_logical = right_op == SyntaxKind::BarBarToken as u16
                            || right_op == SyntaxKind::AmpersandAmpersandToken as u16;

                        if (is_nullish_coalescing && right_is_logical)
                            || (is_logical && right_is_nullish)
                        {
                            let outer_op_str = if is_nullish_coalescing {
                                "??"
                            } else if op_kind == SyntaxKind::BarBarToken as u16 {
                                "||"
                            } else {
                                "&&"
                            };
                            let inner_op_str = if right_is_nullish {
                                "??"
                            } else if right_op == SyntaxKind::BarBarToken as u16 {
                                "||"
                            } else {
                                "&&"
                            };
                            if let Some(loc) = self.get_source_location(right_idx) {
                                use crate::diagnostics::{
                                    Diagnostic, diagnostic_codes, diagnostic_messages,
                                    format_message,
                                };
                                self.ctx.diagnostics.push(Diagnostic::error(self.ctx.file_name.clone(), loc.start, loc.length(), format_message(diagnostic_messages::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES, &[inner_op_str, outer_op_str]), diagnostic_codes::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES));
                            }
                        }
                    }
                }
            }

            if !visited {
                if self.is_assignment_operator(op_kind) {
                    let assign_type = if op_kind == SyntaxKind::EqualsToken as u16 {
                        self.check_assignment_expression(left_idx, right_idx, node_idx)
                    } else {
                        self.check_compound_assignment_expression(
                            left_idx, right_idx, op_kind, node_idx,
                        )
                    };
                    type_stack.push(assign_type);
                    continue;
                }

                // For &&, the right operand gets the contextual type of the whole
                // expression (inherited from parent, e.g. assignment target).
                // For || and ??, the right operand gets the outer contextual type
                // if available, falling back to the left type (minus nullish).
                // This enables contextual typing of callbacks:
                //   let x: (a: string) => string;
                //   x = y && (a => a);           // a: string from assignment context
                //   let g = f || (x => { ... }); // x: string from left type fallback
                if op_kind == SyntaxKind::AmpersandAmpersandToken as u16 {
                    // && passes outer contextual type to the right operand only.
                    // The left operand gets no contextual type.
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = None;
                    let left_type = self.get_type_of_node(left_idx);
                    self.ctx.contextual_type = prev_context;
                    let right_type = self.get_type_of_node(right_idx);

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }
                if op_kind == SyntaxKind::BarBarToken as u16
                    || op_kind == SyntaxKind::QuestionQuestionToken as u16
                {
                    let left_type = self.get_type_of_node(left_idx);
                    // Right operand: use left type (minus nullish) as contextual type
                    let prev_context = self.ctx.contextual_type;
                    let non_nullish = self.ctx.types.remove_nullish(left_type);
                    if non_nullish != TypeId::NEVER && non_nullish != TypeId::UNKNOWN {
                        self.ctx.contextual_type = Some(non_nullish);
                    }
                    let right_type = self.get_type_of_node(right_idx);
                    self.ctx.contextual_type = prev_context;

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }

                // For comma operator: left gets no contextual type,
                // right gets the outer contextual type
                if op_kind == SyntaxKind::CommaToken as u16 {
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = None;
                    let left_type = self.get_type_of_node(left_idx);
                    self.ctx.contextual_type = prev_context;
                    let right_type = self.get_type_of_node(right_idx);

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }

                stack.push((node_idx, true));
                stack.push((right_idx, false));
                stack.push((left_idx, false));
                continue;
            }

            // Return UNKNOWN instead of ANY when type_stack is empty
            let right_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            let left_type = type_stack.pop().unwrap_or(TypeId::UNKNOWN);
            if op_kind == SyntaxKind::CommaToken as u16 {
                // TS2695: Emit when left side has no side effects
                // TypeScript suppresses this diagnostic when allowUnreachableCode is enabled
                // TypeScript DOES emit this even when left operand has type errors or is typed as any
                if self.ctx.compiler_options.allow_unreachable_code != Some(true)
                    && self.is_side_effect_free(left_idx)
                    && !self.is_indirect_call(node_idx, left_idx, right_idx)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        left_idx,
                        diagnostic_messages::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                        diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                    );
                }
                type_stack.push(right_type);
                continue;
            }
            if op_kind == SyntaxKind::InKeyword as u16 {
                if let Some(left_node) = self.ctx.arena.get(left_idx)
                    && left_node.kind == SyntaxKind::PrivateIdentifier as u16
                {
                    self.check_private_identifier_in_expression(left_idx, right_type);
                }

                // TS2322: The right-hand side of an 'in' expression must be assignable to 'object'
                // This prevents using 'in' with primitives like string | number
                if right_type != TypeId::ANY && right_type != TypeId::ERROR {
                    let _ = self.check_assignable_or_report(right_type, TypeId::OBJECT, right_idx);
                }

                type_stack.push(TypeId::BOOLEAN);
                continue;
            }
            // instanceof always produces boolean
            if op_kind == SyntaxKind::InstanceOfKeyword as u16 {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let eval_left = self.evaluate_type_for_assignability(left_type);
                if eval_left != TypeId::ERROR {
                    let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                    if !evaluator.is_valid_instanceof_left_operand(eval_left)
                        && let Some(left_node) = self.ctx.arena.get(left_idx)
                    {
                        let message = format_message(
                                diagnostic_messages::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP,
                                &[],
                            );
                        self.ctx.diagnostics.push(Diagnostic::error(
                                self.ctx.file_name.clone(),
                                left_node.pos,
                                left_node.end - left_node.pos,
                                message,
                                diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP,
                            ));
                    }
                }

                let eval_right = self.evaluate_type_for_assignability(right_type);
                if eval_right != TypeId::ERROR {
                    let mut is_valid_rhs = false;

                    let func_ty_opt = self
                        .ctx
                        .binder
                        .file_locals
                        .get("Function")
                        .map(|sym_id| self.get_type_of_symbol(sym_id))
                        .or_else(|| self.resolve_lib_type_by_name("Function"));

                    if let Some(func_ty) = func_ty_opt {
                        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                        is_valid_rhs = evaluator.is_valid_instanceof_right_operand(
                            eval_right,
                            func_ty,
                            &mut |src, tgt| self.is_assignable_to(src, tgt),
                        );
                    } else if eval_right == TypeId::ANY
                        || eval_right == TypeId::UNKNOWN
                        || eval_right == TypeId::FUNCTION
                    {
                        is_valid_rhs = true;
                    }

                    if !is_valid_rhs && let Some(right_node) = self.ctx.arena.get(right_idx) {
                        let message = format_message(
                                diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA,
                                &[],
                            );
                        self.ctx.diagnostics.push(Diagnostic::error(
                                self.ctx.file_name.clone(),
                                right_node.pos,
                                right_node.end - right_node.pos,
                                message,
                                diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA,
                            ));
                    }
                }

                type_stack.push(TypeId::BOOLEAN);
                continue;
            }

            // Logical AND: `a && b`
            if op_kind == SyntaxKind::AmpersandAmpersandToken as u16 {
                self.check_truthy_or_falsy_with_type(left_idx, left_type);
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }
                let result = match evaluator.evaluate(left_type, right_type, "&&") {
                    BinaryOpResult::Success(ty) => ty,
                    BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
                };
                type_stack.push(result);
                continue;
            }

            // Logical OR: `a || b`
            if op_kind == SyntaxKind::BarBarToken as u16 {
                // TS2872/TS2873: left side of `||` can be syntactically always truthy/falsy.
                self.check_truthy_or_falsy_with_type(left_idx, left_type);

                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                let result = match evaluator.evaluate(left_type, right_type, "||") {
                    BinaryOpResult::Success(ty) => ty,
                    BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
                };
                type_stack.push(result);
                continue;
            }

            // Nullish coalescing: `a ?? b`
            if op_kind == SyntaxKind::QuestionQuestionToken as u16 {
                // TS2872: This kind of expression is always truthy.
                self.check_always_truthy(left_idx, left_type);

                // Propagate error types (don't collapse to unknown)
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                let (non_nullish, cause) = self.split_nullish_type(left_type);
                if cause.is_none() {
                    type_stack.push(left_type);
                } else {
                    let result = match non_nullish {
                        None => right_type,
                        Some(non_nullish) => factory.union(vec![non_nullish, right_type]),
                    };
                    type_stack.push(result);
                }
                continue;
            }
            // TS17006: Unary expression not allowed as left-hand side of `**`.
            // `-x ** y` is ambiguous, so TSC forbids it. The parser produces
            // Binary(PrefixUnary(-, x), **, y), so check if left_idx is a unary.
            if op_kind == SyntaxKind::AsteriskAsteriskToken as u16 {
                if let Some(left_node) = self.ctx.arena.get(left_idx)
                    && left_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    && let Some(left_unary) = self.ctx.arena.get_unary_expr(left_node)
                {
                    let op_name = match left_unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some("-"),
                        k if k == SyntaxKind::PlusToken as u16 => Some("+"),
                        k if k == SyntaxKind::TildeToken as u16 => Some("~"),
                        k if k == SyntaxKind::ExclamationToken as u16 => Some("!"),
                        k if k == SyntaxKind::TypeOfKeyword as u16 => Some("typeof"),
                        k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                        k if k == SyntaxKind::DeleteKeyword as u16 => Some("delete"),
                        _ => None,
                    };
                    if let Some(op_name) = op_name {
                        self.error_at_node_msg(
                                    left_idx,
                                    crate::diagnostics::diagnostic_codes::AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN,
                                    &[op_name],
                                );
                    }
                }

                // TS2791: bigint exponentiation requires target >= ES2016.
                // Only fire when both types are specifically bigint-like,
                // not when either is `any`/`unknown` (TSC skips the bigint branch for those).
                if (self.ctx.compiler_options.target as u32)
                    < (tsz_common::common::ScriptTarget::ES2016 as u32)
                    && left_type != TypeId::ANY
                    && right_type != TypeId::ANY
                    && left_type != TypeId::UNKNOWN
                    && right_type != TypeId::UNKNOWN
                    && self.is_subtype_of(left_type, TypeId::BIGINT)
                    && self.is_subtype_of(right_type, TypeId::BIGINT)
                {
                    self.error_at_node_msg(
                        node_idx,
                        crate::diagnostics::diagnostic_codes::EXPONENTIATION_CANNOT_BE_PERFORMED_ON_BIGINT_VALUES_UNLESS_THE_TARGET_OPTION_IS,
                        &[],
                    );
                }
            }

            // TS2367: Check for comparisons with no overlap
            let is_equality_op = matches!(
                op_kind,
                k if k == SyntaxKind::EqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsToken as u16
                    || k == SyntaxKind::EqualsEqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
            );

            // For TS2367, get the narrow types (literals) not the widened types
            let left_narrow = self
                .literal_type_from_initializer(left_idx)
                .unwrap_or(left_type);
            let right_narrow = self
                .literal_type_from_initializer(right_idx)
                .unwrap_or(right_type);

            let is_left_nan = self.is_identifier_reference_to_global_nan(left_idx);
            let is_right_nan = self.is_identifier_reference_to_global_nan(right_idx);

            if is_equality_op && (is_left_nan || is_right_nan) {
                let condition_result = match op_kind {
                    k if k == SyntaxKind::EqualsEqualsToken as u16
                        || k == SyntaxKind::EqualsEqualsEqualsToken as u16 =>
                    {
                        "false"
                    }
                    _ => "true",
                };
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let message = format_message(
                    diagnostic_messages::THIS_CONDITION_WILL_ALWAYS_RETURN,
                    &[condition_result],
                );
                self.error_at_node(
                    node_idx,
                    &message,
                    diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN,
                );
            } else if is_equality_op
                && left_narrow != TypeId::ERROR
                && right_narrow != TypeId::ERROR
                && left_narrow != TypeId::NEVER
                && right_narrow != TypeId::NEVER
                && self.types_have_no_overlap(left_narrow, right_narrow)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let left_str = self.format_type(left_narrow);
                let right_str = self.format_type(right_narrow);
                let message = format_message(
                    diagnostic_messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA,
                    &[&left_str, &right_str],
                );
                self.error_at_node(
                    node_idx,
                    &message,
                    diagnostic_codes::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA,
                );
            }

            let op_str = match op_kind {
                k if k == SyntaxKind::PlusToken as u16 => "+",
                k if k == SyntaxKind::MinusToken as u16 => "-",
                k if k == SyntaxKind::AsteriskToken as u16 => "*",
                k if k == SyntaxKind::AsteriskAsteriskToken as u16 => "**",
                k if k == SyntaxKind::SlashToken as u16 => "/",
                k if k == SyntaxKind::PercentToken as u16 => "%",
                k if k == SyntaxKind::LessThanToken as u16 => "<",
                k if k == SyntaxKind::GreaterThanToken as u16 => ">",
                k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
                k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
                k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
                k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
                k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
                k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
                // && and || are handled above
                k if k == SyntaxKind::AmpersandToken as u16
                    || k == SyntaxKind::BarToken as u16
                    || k == SyntaxKind::CaretToken as u16
                    || k == SyntaxKind::LessThanLessThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanToken as u16
                    || k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 =>
                {
                    // Bitwise operators require integer operands (number, bigint, any, or enum)
                    // Emit TS2362/TS2363 if operands are not valid
                    let op_str = match op_kind {
                        k if k == SyntaxKind::AmpersandToken as u16 => "&",
                        k if k == SyntaxKind::BarToken as u16 => "|",
                        k if k == SyntaxKind::CaretToken as u16 => "^",
                        k if k == SyntaxKind::LessThanLessThanToken as u16 => "<<",
                        k if k == SyntaxKind::GreaterThanGreaterThanToken as u16 => ">>",
                        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                            ">>>"
                        }
                        _ => "?",
                    };

                    let emitted_nullish_error = self.check_and_emit_nullish_binary_operands(
                        left_idx, right_idx, left_type, right_type, op_str,
                    );

                    // Evaluate types to resolve unevaluated conditional/mapped types
                    let eval_left = self.evaluate_type_for_binary_ops(left_type);
                    let eval_right = self.evaluate_type_for_binary_ops(right_type);
                    let right_narrow = self
                        .literal_type_from_initializer(right_idx)
                        .unwrap_or(eval_right);
                    if matches!(op_str, "<<" | ">>" | ">>>")
                        && let Some(n) = tsz_solver::type_queries::get_number_literal_value(
                            self.ctx.types,
                            right_narrow,
                        )
                        && n.abs() >= 32.0
                    {
                        let left_text = if let Some(left_node) = self.ctx.arena.get(left_idx) {
                            if let Some(src) = self.ctx.arena.source_files.first() {
                                src.text
                                    .get(left_node.pos as usize..left_node.end as usize)
                                    .unwrap_or("expr")
                                    .to_string()
                            } else {
                                "expr".to_string()
                            }
                        } else {
                            "expr".to_string()
                        };
                        let shift_amount = ((n as i64) % 32).to_string();
                        self.error_at_node_msg(
                                    node_idx,
                                    crate::diagnostics::diagnostic_codes::THIS_OPERATION_CAN_BE_SIMPLIFIED_THIS_SHIFT_IS_IDENTICAL_TO,
                                    &[&left_text, op_str, &shift_amount],
                                );
                    }

                    let result = evaluator.evaluate(eval_left, eval_right, op_str);
                    let result_type = match result {
                        BinaryOpResult::Success(result_type) => result_type,
                        BinaryOpResult::TypeError { .. } => {
                            // Don't emit errors if either operand is ERROR - prevents cascading errors
                            if left_type != TypeId::ERROR && right_type != TypeId::ERROR {
                                // Emit appropriate error for arithmetic type mismatch
                                self.emit_binary_operator_error(
                                    node_idx,
                                    left_idx,
                                    right_idx,
                                    left_type,
                                    right_type,
                                    op_str,
                                    emitted_nullish_error,
                                );
                            }
                            TypeId::UNKNOWN
                        }
                    };
                    type_stack.push(result_type);
                    continue;
                }
                _ => {
                    type_stack.push(TypeId::UNKNOWN);
                    continue;
                }
            };

            // Check for boxed primitive types in arithmetic operations BEFORE evaluating types.
            // Boxed types (Number, String, Boolean) are interface types from lib.d.ts
            // and are NOT valid for arithmetic operations. We must check BEFORE calling
            // evaluate_type_for_binary_ops because that function converts boxed types
            // to primitives (Number → number), which would make our check fail.
            let is_arithmetic_op = matches!(op_str, "+" | "-" | "*" | "/" | "%" | "**");

            // TS18050: Emit errors for null/undefined operands BEFORE returning results or evaluating further
            let emitted_nullish_error = self.check_and_emit_nullish_binary_operands(
                left_idx, right_idx, left_type, right_type, op_str,
            );

            if is_arithmetic_op {
                let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
                let right_is_nullish =
                    right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;

                let left_is_boxed = self.is_boxed_primitive_type(left_type);
                let right_is_boxed = self.is_boxed_primitive_type(right_type);

                // If one operand is null/undefined and strict_null_checks is on, tsc prioritizes TS18050
                // over the boxed primitive error (TS2362/TS2363/TS2365).
                let skip_boxed_error = self.ctx.compiler_options.strict_null_checks
                    && (left_is_nullish || right_is_nullish);

                if (left_is_boxed || right_is_boxed) && !skip_boxed_error {
                    // Emit appropriate error based on operator
                    if op_str == "+" {
                        // TS2365: Operator '+' cannot be applied to types 'T' and 'U'
                        // Use the existing error reporter which handles + specially
                        let left_str = self.format_type(left_type);
                        let right_str = self.format_type(right_type);
                        if let Some(node) = self.ctx.arena.get(node_idx) {
                            let message = format!(
                                "Operator '{op_str}' cannot be applied to types '{left_str}' and '{right_str}'."
                            );
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                message,
                                2365, // TS2365
                            );
                        }
                    } else {
                        // TS2362/TS2363: Left/right hand side must be number/bigint/enum
                        // Emit separate errors for left and right operands
                        if left_is_boxed && let Some(node) = self.ctx.arena.get(left_idx) {
                            let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                message,
                                2362, // TS2362
                            );
                        }
                        if right_is_boxed && let Some(node) = self.ctx.arena.get(right_idx) {
                            let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                message,
                                2363, // TS2363
                            );
                        }
                    }
                    type_stack.push(TypeId::UNKNOWN);
                    continue;
                }
            }

            // Hot path: exact primitive arithmetic pairs do not require
            // generic binary-op evaluation.
            if is_arithmetic_op {
                let direct_result = match op_str {
                    "+" | "-" | "*" | "/" | "%" | "**"
                        if left_type == TypeId::NUMBER && right_type == TypeId::NUMBER =>
                    {
                        Some(TypeId::NUMBER)
                    }
                    "+" if left_type == TypeId::STRING && right_type == TypeId::STRING => {
                        Some(TypeId::STRING)
                    }
                    "+" | "-" | "*" | "/" | "%" | "**"
                        if left_type == TypeId::BIGINT && right_type == TypeId::BIGINT =>
                    {
                        Some(TypeId::BIGINT)
                    }
                    _ => None,
                };

                if let Some(result_type) = direct_result {
                    type_stack.push(result_type);
                    continue;
                }
            }

            // Evaluate types to resolve unevaluated conditional/mapped types before
            // passing to the solver. e.g., DeepPartial<number> | number → number
            let eval_left = self.evaluate_type_for_binary_ops(left_type);
            let eval_right = self.evaluate_type_for_binary_ops(right_type);

            let result = evaluator.evaluate(eval_left, eval_right, op_str);
            let result_type = match result {
                BinaryOpResult::Success(result_type) => result_type,
                BinaryOpResult::TypeError { left, right, op } => {
                    // Check if this is actually valid because we have enum types
                    // The evaluator doesn't have access to symbol information, so it can't
                    // detect enum types. We need to check here at the checker layer.
                    let left_is_enum = self.is_enum_type(left_type);
                    let right_is_enum = self.is_enum_type(right_type);
                    let is_arithmetic_op = matches!(op_str, "+" | "-" | "*" | "/" | "%" | "**");

                    // If both operands are enum types and this is an arithmetic operation,
                    // treat it as valid (enum members are numbers for numeric enums)
                    if is_arithmetic_op && left_is_enum && right_is_enum {
                        // For + operation, result is number; for other ops, also number
                        TypeId::NUMBER
                    } else if is_arithmetic_op
                        && left_is_enum
                        && evaluator.is_arithmetic_operand(right)
                    {
                        // Enum op number => number
                        TypeId::NUMBER
                    } else if is_arithmetic_op
                        && right_is_enum
                        && evaluator.is_arithmetic_operand(left)
                    {
                        // Number op enum => number
                        TypeId::NUMBER
                    } else {
                        // For equality operators (==, !=, ===, !==), tsc allows comparison
                        // when the types are comparable (assignable in either direction).
                        // For relational operators (<, >, <=, >=), tsc allows comparison
                        // if both are assignable to number/bigint, or if neither are, they must
                        // be comparable.
                        let is_comparable = if matches!(op_str, "==" | "!=" | "===" | "!==") {
                            self.is_type_comparable_to(eval_left, eval_right)
                        } else if matches!(op_str, "<" | ">" | "<=" | ">=") {
                            if eval_left == TypeId::ANY || eval_right == TypeId::ANY {
                                true
                            } else {
                                let number_or_bigint =
                                    self.ctx.types.union(vec![TypeId::NUMBER, TypeId::BIGINT]);
                                let left_to_num =
                                    self.is_assignable_to(eval_left, number_or_bigint);
                                let right_to_num =
                                    self.is_assignable_to(eval_right, number_or_bigint);

                                if left_to_num && right_to_num {
                                    true
                                } else if !left_to_num && !right_to_num {
                                    self.is_type_comparable_to(eval_left, eval_right)
                                } else {
                                    false
                                }
                            }
                        } else {
                            false
                        };

                        if is_comparable {
                            TypeId::BOOLEAN
                        } else {
                            // Don't emit errors if either operand is ERROR - prevents cascading errors
                            if left != TypeId::ERROR && right != TypeId::ERROR {
                                // Use original types for error messages (more informative)
                                self.emit_binary_operator_error(
                                    node_idx,
                                    left_idx,
                                    right_idx,
                                    left_type,
                                    right_type,
                                    op,
                                    emitted_nullish_error,
                                );
                            }
                            TypeId::UNKNOWN
                        }
                    }
                }
            };

            // Check for type overlap for equality/inequality operators (TS2367)
            let is_equality_op = matches!(
                op_kind,
                k if k == SyntaxKind::EqualsEqualsToken as u16
                    || k == SyntaxKind::EqualsEqualsEqualsToken as u16
            );
            let is_inequality_op = matches!(
                op_kind,
                k if k == SyntaxKind::ExclamationEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
            );

            if is_equality_op || is_inequality_op {
                let is_left_nan = self.is_identifier_reference_to_global_nan(left_idx);
                let is_right_nan = self.is_identifier_reference_to_global_nan(right_idx);

                // Check if the types have any overlap (skip if NaN, handled above)
                // Also skip if either type is `never` — tsc doesn't emit TS2367 for never.
                if !is_left_nan
                    && !is_right_nan
                    && left_type != TypeId::NEVER
                    && right_type != TypeId::NEVER
                    && !self.are_types_overlapping(left_type, right_type)
                {
                    // TS2367: This condition will always return 'false'/'true'
                    self.error_comparison_no_overlap(
                        left_type,
                        right_type,
                        is_equality_op,
                        node_idx,
                    );
                }
            }

            type_stack.push(result_type);
        }

        type_stack.pop().unwrap_or(TypeId::UNKNOWN)
    }
}
