//! Binary expression type computation.
//! Extracted from `core.rs` — handles all binary operators including
//! arithmetic, comparison, logical, assignment, nullish coalescing, and comma.

use crate::context::TypingRequest;
use crate::query_boundaries::type_computation::core::{
    WriteTargetLogicalOperator, WriteTargetLogicalResult,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Result of syntactic nullishness analysis, mirroring tsc's `PredicateSemantics`.
/// This is a purely syntactic check -- it does NOT look at types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SyntacticNullishness {
    /// The expression is always nullish (e.g., `null`, `undefined`).
    #[allow(dead_code)]
    Always,
    /// The expression may or may not be nullish (e.g., identifiers, calls, property accesses).
    Sometimes,
    /// The expression is never nullish (e.g., literals, arithmetic results, `??` results).
    Never,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_literal_index_access_property_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let (object_type, index_type) =
            crate::query_boundaries::common::index_access_parts(self.ctx.types, type_id)?;
        let atom = crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            index_type,
        )?;
        let property_name = self.ctx.types.resolve_atom(atom);

        self.contextual_object_literal_property_type(object_type, property_name.as_ref())
            .or_else(|| {
                self.ctx
                    .types
                    .contextual_property_type(object_type, property_name.as_ref())
            })
    }

    pub(crate) fn reduce_literal_index_access_property_types(&mut self, type_id: TypeId) -> TypeId {
        if let Some(resolved) = self.resolve_literal_index_access_property_type(type_id) {
            return resolved;
        }

        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        else {
            return type_id;
        };

        let mut changed = false;
        let reduced = members
            .into_iter()
            .map(|member| {
                if let Some(resolved) = self.resolve_literal_index_access_property_type(member) {
                    changed = true;
                    resolved
                } else {
                    member
                }
            })
            .collect::<Vec<_>>();

        if changed {
            self.ctx.types.factory().union_preserve_members(reduced)
        } else {
            type_id
        }
    }

    pub(crate) fn get_type_of_write_target_base_expression(&mut self, idx: NodeIndex) -> TypeId {
        // PERF: For non-binary expressions, the write context doesn't change the
        // result type compared to the normal path. Check the node_types cache first
        // to avoid redundant type resolution through the full property-access pipeline.
        // This is especially impactful for deep optional chains like `a?.b?.c?.d`
        // where each level recursively calls this method on its base expression.
        let logical_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let is_binary = self
            .ctx
            .arena
            .get(logical_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION);
        if !is_binary && let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }
        if let Some(node) = self.ctx.arena.get(logical_idx)
            && node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && matches!(
                binary.operator_token,
                k if k == SyntaxKind::BarBarToken as u16
                    || k == SyntaxKind::QuestionQuestionToken as u16
            )
        {
            let left_type = self.get_type_of_node_with_request(binary.left, &TypingRequest::NONE);
            let right_type = self.get_type_of_node_with_request(binary.right, &TypingRequest::NONE);
            let operator = if binary.operator_token == SyntaxKind::BarBarToken as u16 {
                WriteTargetLogicalOperator::LogicalOr
            } else {
                WriteTargetLogicalOperator::NullishCoalescing
            };
            match crate::query_boundaries::type_computation::core::write_target_logical_result_type(
                self.ctx.types,
                operator,
                left_type,
                right_type,
            ) {
                Some(WriteTargetLogicalResult::Type(result)) => return result,
                Some(WriteTargetLogicalResult::FallbackToLogicalExpression) => {
                    return self.get_type_of_node_with_request(
                        logical_idx,
                        &TypingRequest::for_write_context(),
                    );
                }
                None => {}
            }
        }

        self.get_type_of_node_with_request(idx, &TypingRequest::for_write_context())
    }

    /// Mirrors tsc's `getSyntacticNullishnessSemantics`. This is a purely syntactic check
    /// that determines whether an expression can ever be nullish, WITHOUT consulting the
    /// type system. For example, a variable `foo: string` returns `Sometimes` (it could
    /// theoretically be reassigned at runtime), while a literal `"hello"` returns `Never`.
    #[allow(dead_code)]
    fn get_syntactic_nullishness(&self, idx: NodeIndex) -> SyntacticNullishness {
        let Some(node) = self.ctx.arena.get(idx) else {
            return SyntacticNullishness::Sometimes;
        };

        let kind = node.kind;

        // Skip parenthesized expressions (tsc's skipOuterExpressions)
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_syntactic_nullishness(paren.expression);
        }

        // Non-null assertions (!): always Never
        if kind == syntax_kind_ext::NON_NULL_EXPRESSION {
            return SyntacticNullishness::Never;
        }

        // Type assertions (as/satisfies/<T>x): tsc skips these via skipOuterExpressions
        if kind == syntax_kind_ext::AS_EXPRESSION
            || kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || kind == syntax_kind_ext::TYPE_ASSERTION
        {
            return SyntacticNullishness::Sometimes;
        }

        // Expressions that may produce null/undefined at runtime
        if kind == syntax_kind_ext::AWAIT_EXPRESSION
            || kind == syntax_kind_ext::CALL_EXPRESSION
            || kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::META_PROPERTY
            || kind == syntax_kind_ext::NEW_EXPRESSION
            || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::YIELD_EXPRESSION
            || kind == SyntaxKind::ThisKeyword as u16
        {
            return SyntacticNullishness::Sometimes;
        }

        // Binary expressions
        if kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
        {
            let op = binary.operator_token;
            // ||, ||=, &&, &&= can produce null/undefined
            if op == SyntaxKind::BarBarToken as u16
                || op == SyntaxKind::BarBarEqualsToken as u16
                || op == SyntaxKind::AmpersandAmpersandToken as u16
                || op == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            {
                return SyntacticNullishness::Sometimes;
            }
            // For ??, ??=, =, comma: result nullishness is determined by right operand
            if op == SyntaxKind::CommaToken as u16
                || op == SyntaxKind::EqualsToken as u16
                || op == SyntaxKind::QuestionQuestionToken as u16
                || op == SyntaxKind::QuestionQuestionEqualsToken as u16
            {
                return self.get_syntactic_nullishness(binary.right);
            }
            // All other binary operators (arithmetic, comparison, bitwise, etc.)
            // never produce null/undefined
            return SyntacticNullishness::Never;
        }

        // Conditional expression: union of true and false branches
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            let when_true = self.get_syntactic_nullishness(cond.when_true);
            let when_false = self.get_syntactic_nullishness(cond.when_false);
            if when_true == SyntacticNullishness::Never && when_false == SyntacticNullishness::Never
            {
                return SyntacticNullishness::Never;
            }
            if when_true == SyntacticNullishness::Always
                && when_false == SyntacticNullishness::Always
            {
                return SyntacticNullishness::Always;
            }
            return SyntacticNullishness::Sometimes;
        }

        // null keyword
        if kind == SyntaxKind::NullKeyword as u16 {
            return SyntacticNullishness::Always;
        }

        // Identifier: check if it's `undefined`
        if kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node)
                && ident.escaped_text == "undefined"
            {
                return SyntacticNullishness::Always;
            }
            return SyntacticNullishness::Sometimes;
        }

        // Everything else: literals (string, number, boolean, bigint, regex, template,
        // object literal, array literal, function expression, arrow function, class expression,
        // etc.) are never nullish.
        SyntacticNullishness::Never
    }

    /// Get the type of a binary expression.
    ///
    /// Handles all binary operators including arithmetic, comparison, logical,
    /// assignment, nullish coalescing, and comma operators.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_binary_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_binary_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_binary_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::query_boundaries::type_computation::core::BinaryOpResult;
        use tsz_scanner::SyntaxKind;

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
                let mut operand_types = Vec::with_capacity(operand_nodes.len());
                let mut has_error = false;

                for node_idx in operand_nodes {
                    let ty = self.get_type_of_node(node_idx);
                    if ty == TypeId::ERROR {
                        has_error = true;
                        // Continue checking remaining operands to emit diagnostics
                        // for all unresolved names (e.g., both `e` in `e + e`).
                    }
                    operand_types.push(ty);
                }

                if has_error {
                    return TypeId::ERROR;
                }

                if let Some(ty) =
                    crate::query_boundaries::type_computation::core::evaluate_plus_chain(
                        self.ctx.types,
                        &operand_types,
                    )
                {
                    return ty;
                }
            }
        }

        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
        // PERF: Use SmallVec to avoid heap allocation for simple binary expressions.
        // Most binary ops (??. ||, &&, +=, etc.) have exactly 1 stack frame and 2 types.
        // Only deep + chains or nested binary expressions spill to heap.
        let mut stack: smallvec::SmallVec<[(NodeIndex, bool); 4]> =
            smallvec::smallvec![(idx, false)];
        let mut type_stack: smallvec::SmallVec<[TypeId; 4]> = smallvec::SmallVec::new();

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
                            self.error_at_node_msg(
                                left_idx,
                                crate::diagnostics::diagnostic_codes::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES,
                                &[left_op_str, right_op_str],
                            );
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
                            self.error_at_node_msg(
                                right_idx,
                                crate::diagnostics::diagnostic_codes::AND_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES,
                                &[outer_op_str, inner_op_str],
                            );
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
                    //
                    // Preserve literal types for the left operand: tsc's
                    // checkExpression returns the FRESH literal type for literal
                    // expressions (e.g., `"baz"` stays `"baz"`, not widened to
                    // `string`). This matters for `||`/`&&`/`??` because the
                    // logical evaluator uses truthiness narrowing — a widened
                    // `string` cannot be narrowed to NEVER on the falsy branch,
                    // so the result wrongly unions in the right operand.
                    let prev_preserve = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let left_type =
                        self.get_type_of_node_with_request(left_idx, &TypingRequest::NONE);
                    self.ctx.preserve_literal_types = prev_preserve;
                    let right_type = self.get_type_of_node_with_request(right_idx, request);

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }
                if op_kind == SyntaxKind::BarBarToken as u16
                    || op_kind == SyntaxKind::QuestionQuestionToken as u16
                {
                    // Preserve literal types for the left operand — see comment
                    // on the && branch above for the rationale.
                    let prev_preserve = self.ctx.preserve_literal_types;
                    self.ctx.preserve_literal_types = true;
                    let left_type = self.get_type_of_node(left_idx);
                    self.ctx.preserve_literal_types = prev_preserve;
                    let outer_context = request.contextual_type;
                    let right_ctx_idx = self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
                    let right_accepts_context =
                        self.ctx.arena.get(right_ctx_idx).is_some_and(|right_node| {
                            matches!(
                                right_node.kind,
                                syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    | syntax_kind_ext::CONDITIONAL_EXPRESSION
                            )
                        });
                    // Right operand: prefer the whole-expression contextual type
                    // inherited from the parent only for expression forms that
                    // are context-sensitive. Identifiers and other ordinary
                    // expressions should be checked once as part of the whole
                    // logical expression result, matching tsc's single
                    // assignment-level diagnostic for `var x: T = a || b`.
                    // Fall back to the left operand with nullish removed when
                    // there is no outer context.
                    let right_request = if outer_context.is_none() {
                        let evaluated_left = self.evaluate_type_with_env(left_type);
                        let mut non_nullish = self.ctx.types.remove_nullish(evaluated_left);
                        // When the left type was flow-narrowed to only null/undefined
                        // (e.g., after `f || ...` on a previous line), non_nullish
                        // becomes NEVER. Fall back to the declared type of the left
                        // operand so the right operand still gets contextual typing.
                        if non_nullish == TypeId::NEVER
                            && let Some(sym_id) = self.resolve_identifier_symbol(left_idx)
                        {
                            let declared = self.get_type_of_symbol(sym_id);
                            let ev = self.evaluate_type_with_env(declared);
                            let dn = self.ctx.types.remove_nullish(ev);
                            if dn != TypeId::NEVER {
                                non_nullish = dn;
                            }
                        }
                        if right_accepts_context
                            && non_nullish != TypeId::NEVER
                            && non_nullish != TypeId::UNKNOWN
                        {
                            request.read().normal_origin().contextual(non_nullish)
                        } else {
                            TypingRequest::NONE
                        }
                    } else if right_accepts_context {
                        request.read()
                    } else {
                        TypingRequest::NONE
                    };
                    let right_type = self.get_type_of_node_with_request(right_idx, &right_request);

                    let should_check_contextual_right =
                        outer_context.is_some() && right_accepts_context && {
                            let mut parent_idx = self
                                .ctx
                                .arena
                                .get_extended(node_idx)
                                .map(|ext| ext.parent)
                                .unwrap_or(NodeIndex::NONE);
                            let mut check = true;
                            for _ in 0..4 {
                                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                                    break;
                                };
                                if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                                    parent_idx = self
                                        .ctx
                                        .arena
                                        .get_extended(parent_idx)
                                        .map(|ext| ext.parent)
                                        .unwrap_or(NodeIndex::NONE);
                                    continue;
                                }
                                if matches!(
                                    parent_node.kind,
                                    syntax_kind_ext::AS_EXPRESSION
                                        | syntax_kind_ext::TYPE_ASSERTION
                                        | syntax_kind_ext::SATISFIES_EXPRESSION
                                        | syntax_kind_ext::CASE_CLAUSE
                                        | syntax_kind_ext::SPREAD_ELEMENT
                                        | syntax_kind_ext::SPREAD_ASSIGNMENT
                                ) {
                                    // Suppress contextual assignability check when:
                                    // - Case clauses: use comparability (TS2678), not
                                    //   assignability, for the switch discriminant.
                                    // - Type assertions/satisfies: explicit type override.
                                    // - Spread elements/assignments: the RHS of ?? inside
                                    //   a spread doesn't need to independently satisfy the
                                    //   contextual type because properties merge with
                                    //   earlier ones in the containing object literal.
                                    check = false;
                                } else if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                                    && let Some(parent_binary) =
                                        self.ctx.arena.get_binary_expr(parent_node)
                                    && matches!(
                                        parent_binary.operator_token,
                                        k if k == SyntaxKind::BarBarToken as u16
                                            || k == SyntaxKind::AmpersandAmpersandToken as u16
                                            || k == SyntaxKind::QuestionQuestionToken as u16
                                            || k == SyntaxKind::CommaToken as u16
                                    )
                                {
                                    check = false;
                                }
                                break;
                            }
                            check
                        };
                    if should_check_contextual_right
                        && right_type != TypeId::ANY
                        && right_type != TypeId::ERROR
                        && right_type != TypeId::UNKNOWN
                    {
                        let _ = self.check_assignable_or_report_at_exact_anchor(
                            right_type,
                            outer_context.expect("guarded by outer_context.is_some() check"),
                            right_idx,
                            right_idx,
                        );
                    }

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }

                // For comma operator: left gets no contextual type,
                // right gets the outer contextual type
                if op_kind == SyntaxKind::CommaToken as u16 {
                    let left_type =
                        self.get_type_of_node_with_request(left_idx, &TypingRequest::NONE);
                    let right_type = self.get_type_of_node_with_request(right_idx, request);

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
                // Use node-level error flags instead of file-level has_parse_errors:
                // Grammar errors like TS1171 (comma in computed property) are emitted by
                // our parser but by tsc's checker, so they set has_parse_errors in our
                // pipeline but shouldn't suppress TS2695. Only suppress when the binary
                // expression itself has structural parse errors (e.g., `(a, new)`).
                let node_has_parse_error = self
                    .ctx
                    .arena
                    .get(node_idx)
                    .is_some_and(|n| n.this_node_has_error() || n.this_or_subtree_has_error());
                // Also suppress TS2695 when the comma expression is inside a bare
                // block statement (not a function/method body).  This matches tsc's
                // behavior: `{ a, b } = fn()` is parsed as a block followed by `=`,
                // and the comma inside the block is always a malformed destructuring
                // pattern, never a legitimate comma operator.  Suppress unconditionally
                // regardless of parse-error state (has_parse_errors is program-level
                // and may miss file-local grammar errors like TS2809).
                let in_bare_block = self.is_inside_bare_block(node_idx);
                if !node_has_parse_error
                    && !in_bare_block
                    && self.ctx.compiler_options.allow_unreachable_code != Some(true)
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
                let result = self.check_in_operator(left_idx, right_idx, right_type);
                type_stack.push(result);
                continue;
            }
            // instanceof always produces boolean
            if op_kind == SyntaxKind::InstanceOfKeyword as u16 {
                let result =
                    self.check_instanceof_operator(left_idx, right_idx, left_type, right_type);
                type_stack.push(result);
                continue;
            }

            // Logical AND: `a && b`
            if op_kind == SyntaxKind::AmpersandAmpersandToken as u16 {
                // Skip TS2845 enum member checks — tsc only emits those in condition contexts.
                self.check_truthy_or_falsy_with_type_no_enum(left_idx, left_type);
                let callable_truthiness_body = self.find_callable_truthiness_body(idx);
                self.check_callable_truthiness(left_idx, callable_truthiness_body);
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }
                let result = match evaluator.evaluate_with_context(
                    left_type,
                    right_type,
                    "&&",
                    request.contextual_type,
                ) {
                    BinaryOpResult::Success(ty) => ty,
                    BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
                };
                type_stack.push(result);
                continue;
            }

            // Logical OR: `a || b`
            if op_kind == SyntaxKind::BarBarToken as u16 {
                // TS2872/TS2873: left side of `||` can be syntactically always truthy/falsy.
                // Skip TS2845 enum member checks — tsc only emits those in condition contexts.
                self.check_truthy_or_falsy_with_type_no_enum(left_idx, left_type);
                let callable_truthiness_body = self.find_callable_truthiness_body(idx);
                if callable_truthiness_body.is_some() {
                    self.check_callable_truthiness(left_idx, callable_truthiness_body);
                }

                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                let left_type = self.reduce_literal_index_access_property_types(left_type);
                let right_type = self.reduce_literal_index_access_property_types(right_type);
                let result = match evaluator.evaluate(left_type, right_type, "||") {
                    BinaryOpResult::Success(ty) => ty,
                    BinaryOpResult::TypeError { .. } => TypeId::UNKNOWN,
                };
                type_stack.push(result);
                continue;
            }

            // Nullish coalescing: `a ?? b`
            if op_kind == SyntaxKind::QuestionQuestionToken as u16 {
                let callable_truthiness_body = self.find_callable_truthiness_body(idx);
                if callable_truthiness_body.is_some() {
                    self.check_callable_truthiness(left_idx, callable_truthiness_body);
                }
                // Propagate error types (don't collapse to unknown)
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                let left_type = self.reduce_literal_index_access_property_types(left_type);
                let right_type = self.reduce_literal_index_access_property_types(right_type);
                // Evaluate the left type to resolve type aliases (Applications)
                // before splitting nullish parts. For example, `Maybe<T> = null | undefined | T`
                // stored as an Application needs to be expanded so that the nullish split
                // can see through the alias to extract the non-nullable component.
                let evaluated_left = self.evaluate_type_with_env(left_type);
                let (non_nullish, cause) = self.split_nullish_type(evaluated_left);
                let left_is_top_type =
                    evaluated_left == TypeId::UNKNOWN || evaluated_left == TypeId::ANY;
                let diagnostics = self.nullish_coalescing_left_diagnostics(
                    left_idx,
                    non_nullish,
                    cause,
                    left_is_top_type,
                );

                if let Some(diag_idx) = diagnostics.never_nullish_diag {
                    use crate::diagnostics::diagnostic_codes;
                    // TS2869: the left operand is never nullish, so the right
                    // operand of `??` is unreachable. tsc anchors the error
                    // at the left operand, skipping parentheses to the inner
                    // expression (e.g., `(expr) ?? ""` → anchored at `expr`).
                    self.error_at_node(
                        diag_idx,
                        "Right operand of ?? is unreachable because the left operand is never nullish.",
                        diagnostic_codes::RIGHT_OPERAND_OF_IS_UNREACHABLE_BECAUSE_THE_LEFT_OPERAND_IS_NEVER_NULLISH,
                    );
                    type_stack.push(left_type);
                    continue;
                }

                if let Some(diag_idx) = diagnostics.always_nullish_diag {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        diag_idx,
                        "This expression is always nullish.",
                        diagnostic_codes::THIS_EXPRESSION_IS_ALWAYS_NULLISH,
                    );
                    // Fall through to the shared result-type computation so
                    // downstream typing matches tsc when the asserted type
                    // still has a non-nullish slice (e.g.,
                    // `(null as string | null) ?? "x"` produces `string`
                    // via subtype reduction of `string | "x"`).
                }

                let result =
                    self.nullish_coalescing_result_type(evaluated_left, non_nullish, right_type);
                type_stack.push(result);
                continue;
            }
            // TS17006/TS17007: Certain expressions not allowed as left-hand side of `**`.
            // `-x ** y` is ambiguous; `<T>x ** y` is also forbidden by the grammar.
            // When these grammar errors fire, skip remaining type-checks to prevent
            // false-positive arithmetic diagnostics (e.g., TS2362 from `typeof x`).
            if op_kind == SyntaxKind::AsteriskAsteriskToken as u16 {
                let mut lhs_grammar_error = false;
                // Check for unary expression or type assertion on LHS of **
                if let Some(left_node) = self.ctx.arena.get(left_idx) {
                    if left_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                        && let Some(left_unary) = self.ctx.arena.get_unary_expr(left_node)
                        && let Some(op_name) = Self::unary_operator_name(left_unary.operator)
                    {
                        // TS17006: Unary expression as LHS.
                        self.error_at_node_msg(
                            left_idx,
                            crate::diagnostics::diagnostic_codes::AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN,
                            &[op_name],
                        );
                        lhs_grammar_error = true;
                    } else if left_node.kind == syntax_kind_ext::TYPE_ASSERTION {
                        // TS17007: `<T>x ** y` is not allowed.
                        self.error_at_node_msg(
                            left_idx,
                            crate::diagnostics::diagnostic_codes::A_TYPE_ASSERTION_EXPRESSION_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTI,
                            &[],
                        );
                        lhs_grammar_error = true;
                    }
                }
                // Case 2: parent of `**` is a forbidden unary (e.g. `delete(x ** y)`).
                if !lhs_grammar_error
                    && let Some(parent_idx) =
                        self.ctx.arena.get_extended(node_idx).map(|e| e.parent)
                    && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                    && parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    && let Some(parent_unary) = self.ctx.arena.get_unary_expr(parent_node)
                    && let Some(op_name) = Self::unary_operator_name(parent_unary.operator)
                {
                    self.error_at_node_msg(
                        parent_idx,
                        crate::diagnostics::diagnostic_codes::AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN,
                        &[op_name],
                    );
                    lhs_grammar_error = true;
                }
                if lhs_grammar_error {
                    // Skip arithmetic type-checking to avoid false-positive diagnostics
                    // caused by the grammar-error expression types (e.g., typeof -> string).
                    type_stack.push(TypeId::ERROR);
                    continue;
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

            // TS2839: Check for object literal equality comparison.
            // "This condition will always return 'false'/'true' since JavaScript
            // compares objects by reference, not value."
            // Fires when at least one operand is an object/array/regex/function/class literal.
            // In JS files, only fires for strict equality (===, !==), not loose (==, !=).
            {
                let is_strict_eq = op_kind == SyntaxKind::EqualsEqualsEqualsToken as u16
                    || op_kind == SyntaxKind::ExclamationEqualsEqualsToken as u16;
                let is_loose_eq = op_kind == SyntaxKind::EqualsEqualsToken as u16
                    || op_kind == SyntaxKind::ExclamationEqualsToken as u16;

                if is_strict_eq || is_loose_eq {
                    let left_is_literal = self.is_literal_expression_of_object(left_idx);
                    let right_is_literal = self.is_literal_expression_of_object(right_idx);

                    if (left_is_literal || right_is_literal) && (!self.is_js_file() || is_strict_eq)
                    {
                        let is_eq = op_kind == SyntaxKind::EqualsEqualsToken as u16
                            || op_kind == SyntaxKind::EqualsEqualsEqualsToken as u16;
                        let result = if is_eq { "false" } else { "true" };
                        self.error_at_node_msg(
                            node_idx,
                            crate::diagnostics::diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE,
                            &[result],
                        );
                    }
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

            // For TS2367, get the narrow types (literals) not the widened types.
            // For typeof expressions, use the typeof result type (union of all
            // valid typeof return strings) so that comparisons like
            // `typeof x == "Object"` correctly detect no overlap.
            // When the pre-narrowed type of either operand is `any`, skip flow
            // narrowing for TS2367 purposes. `any` overlaps with everything, so
            // comparisons like `var1.constructor === Number` (where `var1: any`)
            // must not trigger TS2367 even if a prior `if (var1.constructor ===
            // String)` caused flow narrowing to produce a specific type for
            // `var1.constructor`.
            let left_comparison_type = if left_type == TypeId::ANY {
                left_type
            } else if self.ctx.arena.get(left_idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || n.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            }) {
                self.apply_flow_narrowing(left_idx, left_type)
            } else {
                left_type
            };
            let right_comparison_type = if right_type == TypeId::ANY {
                right_type
            } else if self.ctx.arena.get(right_idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || n.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            }) {
                self.apply_flow_narrowing(right_idx, right_type)
            } else {
                right_type
            };

            let left_narrow = self
                .typeof_result_type_if_typeof(left_idx)
                .or_else(|| self.literal_type_from_initializer(left_idx))
                .unwrap_or(left_comparison_type);
            let right_narrow = self
                .typeof_result_type_if_typeof(right_idx)
                .or_else(|| self.literal_type_from_initializer(right_idx))
                .unwrap_or(right_comparison_type);

            let is_left_nan = self.is_identifier_reference_to_global_nan(left_idx);
            let is_right_nan = self.is_identifier_reference_to_global_nan(right_idx);

            // TS2839: Object/array literal equality always compares by reference
            let left_is_object_or_array_literal = self
                .ctx
                .arena
                .get(left_idx)
                .map(|n| {
                    n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                })
                .unwrap_or(false);
            let right_is_object_or_array_literal = self
                .ctx
                .arena
                .get(right_idx)
                .map(|n| {
                    n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                })
                .unwrap_or(false);

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
                && (left_is_object_or_array_literal || right_is_object_or_array_literal)
                && (!self.is_js_file()
                    || op_kind == SyntaxKind::EqualsEqualsEqualsToken as u16
                    || op_kind == SyntaxKind::ExclamationEqualsEqualsToken as u16)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let condition_result = match op_kind {
                    k if k == SyntaxKind::EqualsEqualsToken as u16
                        || k == SyntaxKind::EqualsEqualsEqualsToken as u16 =>
                    {
                        "false"
                    }
                    _ => "true",
                };
                let message = format_message(
                    diagnostic_messages::THIS_CONDITION_WILL_ALWAYS_RETURN_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE,
                    &[condition_result],
                );
                self.error_at_node(
                    node_idx,
                    &message,
                    diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE,
                );
            } else if is_equality_op
                && left_narrow != TypeId::ERROR
                && right_narrow != TypeId::ERROR
                && left_narrow != TypeId::NEVER
                && right_narrow != TypeId::NEVER
                && self.types_have_no_overlap(left_narrow, right_narrow)
                // Suppress TS2367 when the DECLARED type of either operand has overlap
                // with the other. This handles loop narrowing: e.g., `code: 0 | 1 = 0;
                // while (...) { code = code === 1 ? 0 : 1; }` — flow narrows `code` to `0`
                // but the declared type `0 | 1` overlaps with `1`. tsc widens at the loop
                // boundary; we compensate by checking declared types here.
                && !self.declared_type_has_overlap_in_loop(
                    node_idx,
                    left_idx,
                    left_narrow,
                    right_narrow,
                )
                && !self.declared_type_has_overlap_in_loop(
                    node_idx,
                    right_idx,
                    right_narrow,
                    left_narrow,
                )
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                // tsc widens literal types to their base primitives when comparing
                // types from different primitive families (e.g., string vs number).
                // For same-family comparisons (e.g., `"foo"` vs `"bar"`), literal
                // types are preserved in the error message.
                let (left_display, right_display) = if let Some(type_param) = self
                    .ts2367_explicit_unknown_like_intersection_type_param_display(
                        left_narrow,
                        right_narrow,
                    ) {
                    (
                        type_param,
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            right_narrow,
                        ),
                    )
                } else if let Some(type_param) = self
                    .ts2367_explicit_unknown_like_intersection_type_param_display(
                        right_narrow,
                        left_narrow,
                    )
                {
                    (
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            left_narrow,
                        ),
                        type_param,
                    )
                } else {
                    self.widen_for_ts2367_cross_family_display(left_narrow, right_narrow)
                };
                // tsc shows unique symbols as `typeof varName` in comparison overlap errors
                // (distinct from index-type errors like TS2538/TS7053 where it uses `unique symbol`).
                let left_str = self.format_type_for_ts2367_display(left_display);
                let right_str = self.format_type_for_ts2367_display(right_display);
                let (left_str, right_str) = if left_str == right_str {
                    // Fall back to disambiguated pair formatting when names collide
                    self.format_type_pair(left_display, right_display)
                } else {
                    (left_str, right_str)
                };
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

                    // TS18046: unknown cannot be used with bitwise operators
                    // Only emit under strictNullChecks; otherwise fall through to normal checks.
                    if left_type == TypeId::UNKNOWN || right_type == TypeId::UNKNOWN {
                        let mut emitted = false;
                        if left_type == TypeId::UNKNOWN {
                            emitted |= self.error_is_of_type_unknown(left_idx);
                        }
                        if right_type == TypeId::UNKNOWN {
                            emitted |= self.error_is_of_type_unknown(right_idx);
                        }
                        if emitted {
                            type_stack.push(TypeId::ERROR);
                            continue;
                        }
                        // Without strictNullChecks, fall through to normal handling
                    }

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
                        && let Some(n) = crate::query_boundaries::common::number_literal_value(
                            self.ctx.types,
                            right_narrow,
                        )
                        && n.abs() >= 32.0
                        // tsc only surfaces TS6807 ("This operation can be simplified")
                        // for shifts that participate in compile-time constant
                        // evaluation, which in practice means enum member
                        // initializers. Plain expression statements like
                        // `1 << 32;` do not get the suggestion. Walk to the
                        // nearest enum-member ancestor; only emit when one is
                        // found.
                        && {
                            let mut ancestor = self.ctx.arena.get_extended(node_idx).map(|e| e.parent);
                            let mut in_enum_member = false;
                            while let Some(idx) = ancestor {
                                if idx.is_none() {
                                    break;
                                }
                                if let Some(node) = self.ctx.arena.get(idx)
                                    && node.kind == tsz_parser::parser::syntax_kind_ext::ENUM_MEMBER
                                {
                                    in_enum_member = true;
                                    break;
                                }
                                ancestor = self.ctx.arena.get_extended(idx).map(|e| e.parent);
                            }
                            in_enum_member
                        }
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

                    // TS2362/TS2363: Per-operand validity check for bitwise operators.
                    // Same issue as arithmetic: when one operand is `any`, the evaluator
                    // returns Success but tsc still validates the other operand individually.
                    if !emitted_nullish_error {
                        let left_any_like = eval_left == TypeId::ANY || eval_left == TypeId::ERROR;
                        let right_any_like =
                            eval_right == TypeId::ANY || eval_right == TypeId::ERROR;

                        if (left_any_like || right_any_like)
                            && left_type != TypeId::ERROR
                            && right_type != TypeId::ERROR
                        {
                            if left_any_like
                                && !evaluator.is_arithmetic_operand(eval_right)
                                && !self.is_enum_type(right_type)
                            {
                                self.error_at_node(
                                    right_idx,
                                    "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                                    crate::diagnostics::diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                                );
                            }
                            if right_any_like
                                && !evaluator.is_arithmetic_operand(eval_left)
                                && !self.is_enum_type(left_type)
                            {
                                self.error_at_node(
                                    left_idx,
                                    "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                                    crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                                );
                            }
                        }
                    }

                    let result = evaluator.evaluate(eval_left, eval_right, op_str);
                    // Equality and relational operators always produce boolean,
                    // even when the operands don't overlap (TS2367).
                    // The TS2367 diagnostic is a warning about the comparison
                    // being unintentional, but the expression type is still boolean.
                    let is_comparison_op = matches!(
                        op_str,
                        "==" | "!=" | "===" | "!==" | "<" | ">" | "<=" | ">="
                    );
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
                            if is_comparison_op {
                                TypeId::BOOLEAN
                            } else if matches!(op_str, "&" | "|" | "^" | "<<" | ">>" | ">>>") {
                                self.operator_error_result_type(
                                    left_type,
                                    right_type,
                                    TypeId::NUMBER,
                                )
                            } else {
                                TypeId::UNKNOWN
                            }
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

            // TS18046: Emit "'x' is of type 'unknown'" when unknown is used with
            // arithmetic, relational, or bitwise operators. Equality operators (==, !=,
            // ===, !==) are allowed on unknown and do not trigger TS18046.
            // Only emit under strictNullChecks; otherwise fall through to normal checks.
            let is_non_equality_op = !matches!(op_str, "==" | "!=" | "===" | "!==");
            if is_non_equality_op && (left_type == TypeId::UNKNOWN || right_type == TypeId::UNKNOWN)
            {
                let mut emitted = false;
                if self.ctx.compiler_options.strict_null_checks {
                    if left_type == TypeId::UNKNOWN {
                        emitted |= self.error_is_of_type_unknown(left_idx);
                    }
                    if right_type == TypeId::UNKNOWN {
                        emitted |= self.error_is_of_type_unknown(right_idx);
                    }
                } else {
                    // In non-strict mode, unknown participates in normal operator
                    // compatibility checks and emits TS2365/related diagnostics.
                    self.emit_binary_operator_error(
                        node_idx, left_idx, right_idx, left_type, right_type, op_str, false,
                    );
                    type_stack.push(TypeId::UNKNOWN);
                    continue;
                }
                if emitted {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }
            }

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

                // If one operand is null/undefined, tsc prioritizes TS18050
                // over the boxed primitive error (TS2362/TS2363/TS2365).
                let skip_boxed_error = left_is_nullish || right_is_nullish;

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
                        // Emit separate errors for left and right operands.
                        // tsc checks both operands independently — when one is boxed
                        // and the other is also invalid (e.g., boolean ** Number), both
                        // errors must be emitted.
                        let evaluator = crate::query_boundaries::common::new_binary_op_evaluator(
                            self.ctx.types,
                        );
                        if left_is_boxed && let Some(node) = self.ctx.arena.get(left_idx) {
                            let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                message,
                                2362, // TS2362
                            );
                        } else {
                            // Strip null/undefined before checking — tsc calls checkNonNullType()
                            // first (emitting TS18048/TS2532), then checks the remaining type.
                            let left_stripped = crate::query_boundaries::common::remove_nullish(
                                self.ctx.types,
                                left_type,
                            );
                            if !evaluator.is_arithmetic_operand(left_stripped)
                                && !self.is_enum_type(left_stripped)
                                && let Some(node) = self.ctx.arena.get(left_idx)
                            {
                                let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    message,
                                    2362, // TS2362
                                );
                            }
                        }
                        if right_is_boxed && let Some(node) = self.ctx.arena.get(right_idx) {
                            let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                message,
                                2363, // TS2363
                            );
                        } else {
                            let right_stripped = crate::query_boundaries::common::remove_nullish(
                                self.ctx.types,
                                right_type,
                            );
                            if !evaluator.is_arithmetic_operand(right_stripped)
                                && !self.is_enum_type(right_stripped)
                                && let Some(node) = self.ctx.arena.get(right_idx)
                            {
                                let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    message,
                                    2363, // TS2363
                                );
                            }
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

            // TS2469: For relational operators (<, >, <=, >=), emit TS2469 when
            // operands are symbol-typed. tsc rejects symbol in comparisons.
            // This must be checked before the evaluator because the comparability
            // fallback would incorrectly accept `symbol < symbol`.
            // Also check constraint-resolved types for `S extends symbol`.
            if matches!(op_str, "<" | ">" | "<=" | ">=") {
                let resolve_tp = |t: TypeId| -> TypeId {
                    crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, t)
                        .filter(|&c| c != TypeId::UNKNOWN && c != t)
                        .unwrap_or(t)
                };
                let left_is_symbol = evaluator.is_symbol_like(left_type)
                    || evaluator.is_symbol_like(resolve_tp(left_type));
                let right_is_symbol = evaluator.is_symbol_like(right_type)
                    || evaluator.is_symbol_like(resolve_tp(right_type));
                if left_is_symbol || right_is_symbol {
                    use crate::diagnostics::diagnostic_codes;
                    let target_idx = if left_is_symbol { left_idx } else { right_idx };
                    self.error_at_node_msg(
                        target_idx,
                        diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                        &[op_str],
                    );
                    type_stack.push(TypeId::BOOLEAN);
                    continue;
                }
            }

            // Evaluate types to resolve unevaluated conditional/mapped types before
            // passing to the solver. e.g., DeepPartial<number> | number → number
            let eval_left = self.evaluate_type_for_binary_ops(left_type);
            let eval_right = self.evaluate_type_for_binary_ops(right_type);

            // TS2362/TS2363: Per-operand validity check for arithmetic operators.
            // When one operand is `any`, the evaluator returns Success(NUMBER) but tsc
            // still requires the OTHER operand to be a valid arithmetic type (any, number,
            // bigint, or enum). Without this pre-check, `any * T` silently passes.
            if is_arithmetic_op && op_str != "+" && !emitted_nullish_error {
                let left_any_like = eval_left == TypeId::ANY || eval_left == TypeId::ERROR;
                let right_any_like = eval_right == TypeId::ANY || eval_right == TypeId::ERROR;

                if left_any_like || right_any_like {
                    if left_any_like
                        && !evaluator.is_arithmetic_operand(eval_right)
                        && !self.is_enum_type(right_type)
                    {
                        self.error_at_node(
                            right_idx,
                            "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                            crate::diagnostics::diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        );
                    }
                    if right_any_like
                        && !evaluator.is_arithmetic_operand(eval_left)
                        && !self.is_enum_type(left_type)
                    {
                        self.error_at_node(
                            left_idx,
                            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.",
                            crate::diagnostics::diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
                        );
                    }
                }
            }

            // For relational operators, widen literal/enum types to their base types
            // before comparison (matching tsc's getBaseTypeOfLiteralTypeForComparison).
            // e.g., enum E { A, B } → number, "hello" → string
            let (cmp_left, cmp_right) = if matches!(op_str, "<" | ">" | "<=" | ">=") {
                (
                    crate::query_boundaries::common::get_base_type_for_comparison(
                        self.ctx.types,
                        eval_left,
                    ),
                    crate::query_boundaries::common::get_base_type_for_comparison(
                        self.ctx.types,
                        eval_right,
                    ),
                )
            } else {
                (eval_left, eval_right)
            };

            let result = evaluator.evaluate(cmp_left, cmp_right, op_str);
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
                    } else if is_arithmetic_op
                        && self.resolve_indexed_access_binary_op(eval_left, eval_right, op)
                    {
                        // IndexAccess types (T[K]) resolved through assignability
                        // e.g., T[K] where T extends Record<K, number> is number-like
                        TypeId::NUMBER
                    } else {
                        // For equality operators (==, !=, ===, !==), tsc allows comparison
                        // when the types are comparable (assignable in either direction).
                        // For relational operators (<, >, <=, >=), tsc requires both
                        // operands to be assignable to number/bigint/string, or if neither
                        // is, they must be comparable via the comparable relation.
                        // cmp_left/cmp_right are widened for relational operators
                        // (matching tsc's getBaseTypeOfLiteralTypeForComparison) and
                        // unchanged for equality operators.
                        let is_comparable = if matches!(op_str, "==" | "!=" | "===" | "!==") {
                            self.is_type_comparable_to(cmp_left, cmp_right)
                        } else if matches!(op_str, "<" | ">" | "<=" | ">=") {
                            if cmp_left == TypeId::ANY || cmp_right == TypeId::ANY {
                                true
                            } else {
                                let number_or_bigint =
                                    self.ctx.types.union2(TypeId::NUMBER, TypeId::BIGINT);
                                let left_to_num = self.is_assignable_to(cmp_left, number_or_bigint);
                                let right_to_num =
                                    self.is_assignable_to(cmp_right, number_or_bigint);

                                if left_to_num && right_to_num {
                                    true
                                } else if !left_to_num && !right_to_num {
                                    // Use the Comparable relation: bidirectional assignability
                                    // plus union/intersection decomposition, all-optional
                                    // property overlap, and constructor-only object checks.
                                    self.is_type_comparable_to(cmp_left, cmp_right)
                                } else {
                                    false
                                }
                            }
                        } else {
                            false
                        };

                        if !is_comparable {
                            // Don't emit errors if either operand is ERROR - prevents cascading errors
                            if left != TypeId::ERROR && right != TypeId::ERROR {
                                // For relational ops, use widened types in error messages
                                // (matching tsc: enum members show as 'number', not 'E').
                                // For equality/arithmetic ops, use original types so
                                // widen_type_for_operator_display can preserve enum names.
                                let (err_left, err_right) =
                                    if matches!(op_str, "<" | ">" | "<=" | ">=") {
                                        (cmp_left, cmp_right)
                                    } else {
                                        (left_type, right_type)
                                    };
                                self.emit_binary_operator_error(
                                    node_idx,
                                    left_idx,
                                    right_idx,
                                    err_left,
                                    err_right,
                                    op,
                                    emitted_nullish_error,
                                );
                            }
                        }
                        // Result type depends on operator category and error state:
                        // - Equality/relational → boolean
                        // - Arithmetic with error (incompatible types) → any
                        //   tsc returns `any` for failed arithmetic ops so that
                        //   downstream checks (e.g., TS2538 for destructuring keys)
                        //   see `any` rather than a misleading concrete type.
                        // - Arithmetic without error (comparable) → number
                        if !is_comparable && is_arithmetic_op {
                            TypeId::ANY
                        } else if is_arithmetic_op {
                            TypeId::NUMBER
                        } else {
                            TypeId::BOOLEAN
                        }
                    }
                }
            };

            // NOTE: TS2367 overlap checking for equality/inequality operators is handled
            // entirely by the narrowed-type check above (lines 605-624) which uses
            // `types_have_no_overlap` with `literal_type_from_initializer` narrowing.
            // A second check using `are_types_overlapping` was removed because it used
            // raw (unnarrowed) types and a different overlap function, producing false
            // positives for empty object types ({}) vs type parameters and other cases.

            type_stack.push(result_type);
        }

        type_stack.pop().unwrap_or(TypeId::UNKNOWN)
    }
}

#[cfg(test)]
#[path = "binary_tests.rs"]
mod tests;
