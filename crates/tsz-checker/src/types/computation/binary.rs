//! Binary expression type computation.
//! Extracted from `core.rs` — handles all binary operators including
//! arithmetic, comparison, logical, assignment, nullish coalescing, and comma.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{BinaryOpEvaluator, TypeId};

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
    fn declared_instanceof_left_operand_type(
        &mut self,
        left_idx: NodeIndex,
        left_type: TypeId,
    ) -> TypeId {
        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
        if evaluator.is_valid_instanceof_left_operand(left_type) {
            return left_type;
        }

        let Some(node) = self.ctx.arena.get(left_idx) else {
            return left_type;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return left_type;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(left_idx) else {
            return left_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return left_type;
        };

        let mut decl_idx = symbol.value_declaration;
        let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
            return left_type;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_idx = ext.parent;
            decl_node = parent_node;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return left_type;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return left_type;
        };
        if var_decl.type_annotation.is_none() {
            return left_type;
        }
        if !self
            .ctx
            .binder
            .get_node_flow(left_idx)
            .and_then(|flow_id| self.ctx.binder.flow_nodes.get(flow_id))
            .is_some_and(|flow| flow.has_any_flags(tsz_binder::flow_flags::ASSIGNMENT))
        {
            return left_type;
        }

        let declared_type = self.get_type_of_symbol(sym_id);
        if evaluator.is_valid_instanceof_left_operand(declared_type) {
            declared_type
        } else {
            left_type
        }
    }

    pub(crate) fn get_type_of_write_target_base_expression(&mut self, idx: NodeIndex) -> TypeId {
        let logical_idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
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
            let ctx = tsz_solver::NarrowingContext::new(self.ctx.types);
            let members = if binary.operator_token == SyntaxKind::BarBarToken as u16 {
                let truthy_left = ctx.narrow_by_truthiness(left_type);
                let falsy_left = ctx.narrow_to_falsy(left_type);
                if truthy_left == TypeId::NEVER || falsy_left == TypeId::NEVER {
                    return self.get_type_of_node_with_request(
                        logical_idx,
                        &TypingRequest::for_write_context(),
                    );
                }
                vec![truthy_left, right_type]
            } else {
                let non_nullish_left =
                    ctx.narrow_by_nullishness(left_type, tsz_solver::NullishFilter::ExcludeNullish);
                let nullish_left =
                    ctx.narrow_by_nullishness(left_type, tsz_solver::NullishFilter::KeepNullish);
                if non_nullish_left == TypeId::NEVER || nullish_left == TypeId::NEVER {
                    return self.get_type_of_node_with_request(
                        logical_idx,
                        &TypingRequest::for_write_context(),
                    );
                }
                vec![non_nullish_left, right_type]
            };

            if let Some(normalized) = tsz_solver::normalize_object_union_members_for_write_target(
                self.ctx.types,
                &members,
            ) {
                return tsz_solver::utils::union_or_single(self.ctx.types, normalized);
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
    fn is_valid_in_operator_rhs(&mut self, ty: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        if matches!(
            ty,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::OBJECT
        ) {
            return true;
        }

        // For type parameters, check if their constraint is assignable to object.
        // Unconstrained type params are NOT valid (could be primitive).
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty) {
            return match crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            ) {
                Some(c) => self.is_type_param_constraint_valid_for_in(c),
                None => false,
            };
        }

        if query::is_object_like_type(self.ctx.types, ty) {
            return true;
        }

        if let Some(members) = query::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .all(|&member| self.is_valid_in_operator_rhs(member));
        }

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&member| self.is_valid_in_operator_rhs(member));
        }

        false
    }

    /// For type parameter constraints: ALL constituents must be valid for `in`.
    /// `T extends object` is valid, but `T extends object | "hello"` is not
    /// because T could be instantiated with `"hello"` (a primitive).
    fn is_type_param_constraint_valid_for_in(&mut self, constraint: TypeId) -> bool {
        // Just delegate to the main check — the constraint itself must be fully valid
        self.is_valid_in_operator_rhs(constraint)
    }

    /// Check if an AST node is a nullish coalescing expression (`??`) or a
    /// literal value (string, number, boolean, bigint, template), unwrapping
    /// parentheses. TSC only emits TS2869 for these syntactic forms; general
    /// non-nullable expressions (identifiers, property access, `&&` chains)
    /// do not trigger TS2869 even when their type is never nullish.
    fn is_nullish_coalescing_or_literal(
        arena: &tsz_parser::parser::NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        // Unwrap parentheses: (expr) -> expr
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = arena.get_parenthesized(node) {
                return Self::is_nullish_coalescing_or_literal(arena, paren.expression);
            }
            return false;
        }

        // Binary expression: check if it's a `??`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(binary) = arena.get_binary_expr(node) {
                return binary.operator_token == SyntaxKind::QuestionQuestionToken as u16;
            }
            return false;
        }

        // Literal values: string, number, bigint, template, true, false
        let kind = node.kind;
        kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == syntax_kind_ext::TEMPLATE_EXPRESSION
    }

    /// Check if a type "may represent a primitive value" for TS2638.
    /// In tsc, this check fires for:
    /// - Type parameters whose constraint is missing or could represent a primitive
    /// - Empty object types like `{}` (they accept any non-nullish value including
    ///   primitives like strings, numbers, booleans)
    /// - Unions where any member may represent a primitive
    /// - Intersections where all members may represent a primitive
    fn type_may_represent_primitive(&self, ty: TypeId) -> bool {
        // Type parameters: check if constraint is missing or could be primitive
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ty) {
            return match crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                ty,
            ) {
                None => true, // Unconstrained type param may be primitive
                Some(c) => {
                    // A type param constrained to `{}` may still be primitive
                    if tsz_solver::is_empty_object_type(self.ctx.types, c) {
                        return true;
                    }
                    self.type_may_represent_primitive(c)
                }
            };
        }

        // Empty object type `{}` may represent a primitive value — `{}` matches
        // any non-null, non-undefined value including strings, numbers, booleans.
        // tsc emits TS2638 for `"prop" in x` when x has type `{}`.
        if tsz_solver::is_empty_object_type(self.ctx.types, ty) {
            return true;
        }

        // Union: any member may represent primitive
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty) {
            return members
                .iter()
                .any(|&m| self.type_may_represent_primitive(m));
        }

        // Intersection: all members may represent primitive
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, ty)
        {
            return members
                .iter()
                .all(|&m| self.type_may_represent_primitive(m));
        }

        false
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
        use tsz_solver::BinaryOpEvaluator;
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
                let mut operand_types = Vec::with_capacity(operand_nodes.len());

                for node_idx in operand_nodes {
                    let ty = self.get_type_of_node(node_idx);
                    if ty == TypeId::ERROR {
                        return TypeId::ERROR;
                    }
                    operand_types.push(ty);
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

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);
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
                    let left_type =
                        self.get_type_of_node_with_request(left_idx, &TypingRequest::NONE);
                    let right_type = self.get_type_of_node_with_request(right_idx, request);

                    type_stack.push(left_type);
                    type_stack.push(right_type);
                    stack.push((node_idx, true));
                    continue;
                }
                if op_kind == SyntaxKind::BarBarToken as u16
                    || op_kind == SyntaxKind::QuestionQuestionToken as u16
                {
                    let left_type = self.get_type_of_node(left_idx);
                    let outer_context = request.contextual_type;
                    // Right operand: prefer the whole-expression contextual type
                    // inherited from the parent (e.g. assignment target). Fall back
                    // to the left operand with nullish removed when there is no outer
                    // context.
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
                        let right_ctx_idx =
                            self.ctx.arena.skip_parenthesized_and_assertions(right_idx);
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
                        if right_accepts_context
                            && non_nullish != TypeId::NEVER
                            && non_nullish != TypeId::UNKNOWN
                        {
                            request.read().normal_origin().contextual(non_nullish)
                        } else {
                            TypingRequest::NONE
                        }
                    } else {
                        request.read()
                    };
                    let right_type = self.get_type_of_node_with_request(right_idx, &right_request);

                    let should_check_contextual_right = outer_context.is_some() && {
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
                            ) {
                                // Case clauses use comparability (TS2678), not
                                // assignability (TS2322), to validate the case
                                // expression against the switch discriminant type.
                                // The contextual type here is only meant for type
                                // inference (e.g., object literal excess properties),
                                // not for assignability checking on || operands.
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
                if !self.ctx.has_parse_errors
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
                if let Some(left_node) = self.ctx.arena.get(left_idx)
                    && left_node.kind == SyntaxKind::PrivateIdentifier as u16
                {
                    self.check_private_identifier_in_expression(left_idx, right_type);
                }

                // TS2322: The right-hand side of an 'in' expression must be assignable to 'object'
                // This prevents using 'in' with primitives like string | number
                if !self.is_valid_in_operator_rhs(right_type) {
                    // Route through the check_assignable_or_report(...) gateway family
                    // so computation-layer mismatches stay on the centralized path.
                    let _ = self.check_assignable_or_report_at_exact_anchor(
                        right_type,
                        TypeId::OBJECT,
                        right_idx,
                        right_idx,
                    );
                } else if self.type_may_represent_primitive(right_type) {
                    // TS2638: Type '{}' may represent a primitive value, which is not
                    // permitted as the right operand of the 'in' operator.
                    let type_str = self.format_type(right_type);
                    self.error_at_node_msg(
                        right_idx,
                        tsz_common::diagnostics::diagnostic_codes::TYPE_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND,
                        &[&type_str],
                    );
                }

                type_stack.push(TypeId::BOOLEAN);
                continue;
            }
            // instanceof always produces boolean
            if op_kind == SyntaxKind::InstanceOfKeyword as u16 {
                use crate::diagnostics::diagnostic_codes;

                // TS2848: The right-hand side of an instanceof must not be an instantiation expression
                let unwrapped_right = self.ctx.arena.skip_parenthesized(right_idx);
                if let Some(right_node) = self.ctx.arena.get(unwrapped_right)
                    && right_node.kind
                        == tsz_parser::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                {
                    self.error_at_node(
                            unwrapped_right,
                            crate::diagnostics::diagnostic_messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
                            diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXP,
                        );
                }

                // `instanceof` left-operand validity is about the expression's semantic
                // value type, not the relation-normalized type used for assignability.
                // Running it through assignability evaluation can erase object members
                // from unions before we validate TS2358.
                if left_type != TypeId::ERROR {
                    let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                    let lhs_type = self.declared_instanceof_left_operand_type(left_idx, left_type);
                    if !evaluator.is_valid_instanceof_left_operand(lhs_type) {
                        self.error_at_node_msg(
                            left_idx,
                            diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP,
                            &[],
                        );
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

                    // TypeScript also allows types with [Symbol.hasInstance] as valid instanceof RHS.
                    // This is checked even when the standard callable/Function checks fail.
                    if !is_valid_rhs {
                        use crate::query_boundaries::common::PropertyAccessResult;
                        is_valid_rhs = matches!(
                            self.resolve_property_access_with_env(
                                eval_right,
                                "[Symbol.hasInstance]"
                            ),
                            PropertyAccessResult::Success { .. }
                        );
                    }

                    if !is_valid_rhs {
                        self.error_at_node_msg(
                            right_idx,
                            diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLA,
                            &[],
                        );
                    }
                }

                type_stack.push(TypeId::BOOLEAN);
                continue;
            }

            // Logical AND: `a && b`
            if op_kind == SyntaxKind::AmpersandAmpersandToken as u16 {
                // Skip TS2845 enum member checks — tsc only emits those in condition contexts.
                self.check_truthy_or_falsy_with_type_no_enum(left_idx, left_type);
                // TS2774: check for non-nullable callable tested for truthiness
                // Only check at the top-level binary expression (not nested ones)
                // to avoid duplicate diagnostics when this is inside an if-condition.
                if let Some(parent_idx) = self.ctx.arena.get_extended(idx).map(|ext| ext.parent)
                    && let Some(parent) = self.ctx.arena.get(parent_idx)
                    && parent.kind != syntax_kind_ext::BINARY_EXPRESSION
                    && parent.kind != syntax_kind_ext::IF_STATEMENT
                    && parent.kind != syntax_kind_ext::CONDITIONAL_EXPRESSION
                    && parent.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION
                {
                    self.check_callable_truthiness(idx, None);
                }
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
                // Propagate error types (don't collapse to unknown)
                if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
                    type_stack.push(TypeId::ERROR);
                    continue;
                }

                // Evaluate the left type to resolve type aliases (Applications)
                // before splitting nullish parts. For example, `Maybe<T> = null | undefined | T`
                // stored as an Application needs to be expanded so that the nullish split
                // can see through the alias to extract the non-nullable component.
                let evaluated_left = self.evaluate_type_with_env(left_type);
                let (non_nullish, cause) = self.split_nullish_type(evaluated_left);
                let left_is_top_type =
                    evaluated_left == TypeId::UNKNOWN || evaluated_left == TypeId::ANY;
                let left_is_nullish_chain_or_literal =
                    Self::is_nullish_coalescing_or_literal(self.ctx.arena, left_idx);
                if cause.is_none() && !left_is_top_type && left_is_nullish_chain_or_literal {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        right_idx,
                        "Right operand of ?? is unreachable because the left operand is never nullish.",
                        diagnostic_codes::RIGHT_OPERAND_OF_IS_UNREACHABLE_BECAUSE_THE_LEFT_OPERAND_IS_NEVER_NULLISH,
                    );
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
            // TS17006/TS17007: Certain expressions not allowed as left-hand side of `**`.
            // `-x ** y` is ambiguous; `<T>x ** y` is also forbidden by the grammar.
            // When these grammar errors fire, skip remaining type-checks to prevent
            // false-positive arithmetic diagnostics (e.g., TS2362 from `typeof x`).
            if op_kind == SyntaxKind::AsteriskAsteriskToken as u16 {
                let mut lhs_grammar_error = false;
                if let Some(left_node) = self.ctx.arena.get(left_idx) {
                    if left_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                        && let Some(left_unary) = self.ctx.arena.get_unary_expr(left_node)
                    {
                        // TS17006: Unary expression (-, +, ~, !, typeof, void, delete) as LHS.
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
                            lhs_grammar_error = true;
                        }
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
                // These expressions are ambiguous in the grammar, so tsc emits TS17006
                // pointing to the parent unary expression rather than the `**` LHS.
                // Examples: `delete temp ** 3` → `delete(temp ** 3)`, `!(3 ** 4)`.
                if !lhs_grammar_error
                    && let Some(parent_idx) =
                        self.ctx.arena.get_extended(node_idx).map(|e| e.parent)
                    && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                    && parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    && let Some(parent_unary) = self.ctx.arena.get_unary_expr(parent_node)
                {
                    let parent_op_name = match parent_unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some("-"),
                        k if k == SyntaxKind::PlusToken as u16 => Some("+"),
                        k if k == SyntaxKind::TildeToken as u16 => Some("~"),
                        k if k == SyntaxKind::ExclamationToken as u16 => Some("!"),
                        k if k == SyntaxKind::TypeOfKeyword as u16 => Some("typeof"),
                        k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                        k if k == SyntaxKind::DeleteKeyword as u16 => Some("delete"),
                        _ => None,
                    };
                    if let Some(op_name) = parent_op_name {
                        self.error_at_node_msg(
                            parent_idx,
                            crate::diagnostics::diagnostic_codes::AN_UNARY_EXPRESSION_WITH_THE_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN,
                            &[op_name],
                        );
                        lhs_grammar_error = true;
                    }
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
                // tsc preserves literal types within the same primitive family
                // (e.g., '0' and '2' for number-to-number) but widens all literals
                // to their primitive types across different families (e.g., '"foo"'
                // becomes 'string' when compared against 'number').
                let left_base = tsz_solver::type_queries::widen_literal_to_primitive(
                    self.ctx.types,
                    left_narrow,
                );
                let right_base = tsz_solver::type_queries::widen_literal_to_primitive(
                    self.ctx.types,
                    right_narrow,
                );
                let (left_display, right_display) = if left_base == right_base {
                    // Same primitive family: preserve all literals
                    (left_narrow, right_narrow)
                } else if left_base == left_narrow || right_base == right_narrow {
                    // One side is non-literal (type parameter, object, etc.):
                    // preserve both as-is. tsc only widens when both sides are
                    // literals from different primitive families.
                    (left_narrow, right_narrow)
                } else {
                    // Different families, both literal: widen to primitive types.
                    // tsc widens both sides (e.g., '"foo"' → 'string', '0' → 'number')
                    // when the operands are from different primitive families.
                    (left_base, right_base)
                };
                let (left_str, right_str) = self.format_type_pair(left_display, right_display);
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

            // TS2469: For relational operators (<, >, <=, >=), emit TS2469 when
            // operands are symbol-typed. tsc rejects symbol in comparisons.
            // This must be checked before the evaluator because the comparability
            // fallback would incorrectly accept `symbol < symbol`.
            if matches!(op_str, "<" | ">" | "<=" | ">=") {
                let left_is_symbol = evaluator.is_symbol_like(left_type);
                let right_is_symbol = evaluator.is_symbol_like(right_type);
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

            // For relational operators, widen literal/enum types to their base types
            // before comparison (matching tsc's getBaseTypeOfLiteralTypeForComparison).
            // e.g., enum E { A, B } → number, "hello" → string
            let (cmp_left, cmp_right) = if matches!(op_str, "<" | ">" | "<=" | ">=") {
                (
                    tsz_solver::get_base_type_for_comparison(self.ctx.types, eval_left),
                    tsz_solver::get_base_type_for_comparison(self.ctx.types, eval_right),
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
                        && self
                            .resolve_indexed_access_binary_op(eval_left, eval_right, op, &evaluator)
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
                                    self.ctx.types.union(vec![TypeId::NUMBER, TypeId::BIGINT]);
                                let left_to_num = self.is_assignable_to(cmp_left, number_or_bigint);
                                let right_to_num =
                                    self.is_assignable_to(cmp_right, number_or_bigint);

                                if left_to_num && right_to_num {
                                    true
                                } else if !left_to_num && !right_to_num {
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
                        // Result type depends on operator category:
                        // - Equality/relational → boolean
                        // - Arithmetic (+, -, *, /, %, **) → number
                        //   (+ could also be string, but number is a safe fallback
                        //    that avoids cascading TS2322 from boolean)
                        if is_arithmetic_op {
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

    /// If `idx` is a `typeof` expression (`PREFIX_UNARY_EXPRESSION` with `TypeOfKeyword`),
    /// return the typeof result type:
    /// `"string" | "number" | "bigint" | "boolean" | "symbol" | "undefined" | "object" | "function"`.
    /// This is used for TS2367 overlap detection so that comparisons like
    /// `typeof x == "Object"` (capital O) correctly detect no overlap.
    fn typeof_result_type_if_typeof(&self, idx: NodeIndex) -> Option<TypeId> {
        use tsz_scanner::SyntaxKind;
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return None;
        }
        let unary = self.ctx.arena.get_unary_expr(node)?;
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return None;
        }
        let factory = self.ctx.types.factory();
        let members = vec![
            factory.literal_string("string"),
            factory.literal_string("number"),
            factory.literal_string("bigint"),
            factory.literal_string("boolean"),
            factory.literal_string("symbol"),
            factory.literal_string("undefined"),
            factory.literal_string("object"),
            factory.literal_string("function"),
        ];
        Some(factory.union(members))
    }

    /// Check if an identifier node's declared type overlaps with the given comparison type.
    /// Returns true if the identifier's declared type is wider than `narrow_type` and
    /// has overlap with `other_type`. This prevents false TS2367 when flow narrowing
    /// inside loops makes the narrowed type too specific (e.g., `0` instead of `0 | 1`).
    fn declared_type_has_overlap_in_loop(
        &mut self,
        comparison_idx: NodeIndex,
        idx: NodeIndex,
        narrow_type: TypeId,
        other_type: TypeId,
    ) -> bool {
        if !self.is_inside_loop(comparison_idx) {
            return false;
        }

        let node = match self.ctx.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        // Only applies to identifiers
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        // Resolve the identifier to a symbol
        let sym_id = match self.ctx.binder.resolve_identifier(self.ctx.arena, idx) {
            Some(s) => s,
            None => return false,
        };
        // Get the symbol's value_declaration and its type (the declared type)
        let symbol = match self.ctx.binder.get_symbol(sym_id) {
            Some(s) => s,
            None => return false,
        };
        if symbol.value_declaration.is_none() {
            return false;
        }
        let declared_type = match self.ctx.node_types.get(&symbol.value_declaration.0) {
            Some(&t) => t,
            None => return false,
        };
        // Only relevant when the declared type is wider than the narrowed type
        if declared_type == narrow_type {
            return false;
        }
        // Check if the declared type overlaps with the other operand
        !self.types_have_no_overlap(declared_type, other_type)
    }

    fn is_inside_loop(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::WHILE_STATEMENT
                    || k == syntax_kind_ext::DO_STATEMENT
                    || k == syntax_kind_ext::FOR_STATEMENT
                    || k == syntax_kind_ext::FOR_IN_STATEMENT
                    || k == syntax_kind_ext::FOR_OF_STATEMENT
            ) {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Check if a binary operation with `IndexAccess` operands is valid through assignability.
    ///
    /// When the solver's `BinaryOpEvaluator` returns `TypeError` for an operation like
    /// `number + T[K]`, the `IndexAccess` type may not have been resolved through its
    /// constraint chain (e.g., T extends Record<K, number> means T[K] is number-like).
    /// This method uses the checker's assignability infrastructure to validate such cases,
    /// matching tsc's behavior of using `isTypeAssignableTo` for binary operator validation.
    fn resolve_indexed_access_binary_op(
        &mut self,
        left: TypeId,
        right: TypeId,
        op: &str,
        evaluator: &tsz_solver::BinaryOpEvaluator,
    ) -> bool {
        let left_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, left);
        let right_is_index_access =
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, right);

        if !left_is_index_access && !right_is_index_access {
            return false;
        }

        match op {
            "+" => {
                // For +, both operands must be number-like, string-like, or bigint-like
                let left_ok = evaluator.is_arithmetic_operand(left)
                    || left_is_index_access && self.is_assignable_to(left, TypeId::NUMBER);
                let right_ok = evaluator.is_arithmetic_operand(right)
                    || right_is_index_access && self.is_assignable_to(right, TypeId::NUMBER);
                left_ok && right_ok
            }
            "-" | "*" | "/" | "%" | "**" => {
                let left_ok = evaluator.is_arithmetic_operand(left)
                    || left_is_index_access && self.is_assignable_to(left, TypeId::NUMBER);
                let right_ok = evaluator.is_arithmetic_operand(right)
                    || right_is_index_access && self.is_assignable_to(right, TypeId::NUMBER);
                left_ok && right_ok
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "binary_tests.rs"]
mod tests;
