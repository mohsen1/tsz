//! Type computation helpers, relationship queries, and format utilities.
//! This module extends `CheckerState` with additional methods for type-related
//! operations, providing cleaner APIs for common patterns.

use crate::context::TypingRequest;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::type_computation::core::{
    self as expr_ops, evaluate_contextual_structure_with,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    fn expression_is_intrinsically_non_promise_like(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.ctx.arena.get_parenthesized(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::AS_EXPRESSION => {
                self.ctx.arena.get_type_assertion(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::SATISFIES_EXPRESSION => {
                self.ctx.arena.get_type_assertion(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.ctx.arena.get_unary_expr_ex(node).is_some_and(|expr| {
                    self.expression_is_intrinsically_non_promise_like(expr.expression)
                })
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == tsz_scanner::SyntaxKind::NumericLiteral as u16
                || k == tsz_scanner::SyntaxKind::BigIntLiteral as u16
                || k == tsz_scanner::SyntaxKind::RegularExpressionLiteral as u16
                || k == tsz_scanner::SyntaxKind::TrueKeyword as u16
                || k == tsz_scanner::SyntaxKind::FalseKeyword as u16
                || k == tsz_scanner::SyntaxKind::NullKeyword as u16 =>
            {
                true
            }
            _ => false,
        }
    }

    fn contextual_type_for_conditional_branch(
        &self,
        contextual: TypeId,
        branch_idx: NodeIndex,
    ) -> TypeId {
        if !self.expression_is_intrinsically_non_promise_like(branch_idx) {
            return contextual;
        }

        let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, contextual)
        else {
            return contextual;
        };

        let mut non_promise_members = Vec::new();
        let mut saw_promise_member = false;
        for member in members {
            if self.type_ref_is_promise_like(member) {
                saw_promise_member = true;
            } else {
                non_promise_members.push(member);
            }
        }

        if saw_promise_member && !non_promise_members.is_empty() {
            self.ctx.types.factory().union(non_promise_members)
        } else {
            contextual
        }
    }

    // =========================================================================
    pub(crate) fn is_identifier_reference_to_global_nan(&self, node_idx: NodeIndex) -> bool {
        let mut current_idx = node_idx;
        while let Some(node) = self.ctx.arena.get(current_idx) {
            if node.kind == tsz_parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(expr) = self.ctx.arena.get_parenthesized(node)
            {
                current_idx = expr.expression;
                continue;
            }
            break;
        }

        if let Some(node) = self.ctx.arena.get(current_idx)
            && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
            && ident.escaped_text == "NaN"
        {
            if let Some(sym_id) = self.resolve_identifier_symbol(current_idx) {
                let is_global = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_none_or(|s| s.parent.is_none());
                return self.ctx.symbol_is_from_lib(sym_id) || is_global;
            }
            return true; // Unresolved NaN treated as global
        }
        false
    }

    /// Check if a unary expression node is the direct left-hand side of a `**` binary.
    ///
    /// Used to suppress secondary diagnostics (TS2703 from `delete`, TS2872 from `!`) when
    /// the unary expression is in a grammar-error position. When `(delete X) ** Y` or
    /// `(!X) ** Y` is processed, binary.rs will emit TS17006 for this node as the LHS of `**`.
    /// Emitting TS2703/TS2872 on top would be a false positive, so we skip them here.
    pub(crate) fn is_lhs_of_exponentiation(&self, node_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(parent_idx) = self.ctx.arena.get_extended(node_idx).map(|e| e.parent)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(parent_binary) = self.ctx.arena.get_binary_expr(parent_node)
            && parent_binary.operator_token == SyntaxKind::AsteriskAsteriskToken as u16
            && parent_binary.left == node_idx
        {
            true
        } else {
            false
        }
    }

    /// Check if a node is a "literal expression of object" — one of:
    /// `ObjectLiteralExpression`, `ArrayLiteralExpression`, `RegularExpressionLiteral`,
    /// `FunctionExpression`, or `ClassExpression`. Used for TS2839 (object equality check).
    pub(crate) fn is_literal_expression_of_object(&self, node_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;
        if let Some(node) = self.ctx.arena.get(node_idx) {
            matches!(
                node.kind,
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                    || k == SyntaxKind::RegularExpressionLiteral as u16
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
            )
        } else {
            false
        }
    }

    // Core Type Computation
    // =========================================================================

    /// Evaluate a type deeply for binary operation checking.
    ///
    /// Unlike `evaluate_type_with_resolution` which only handles the top-level type,
    /// this also evaluates individual members of union types. This is needed because
    /// types like `DeepPartial<number> | number` are stored as a union where one
    /// member is an unevaluated Application type that the solver's `NumberLikeVisitor`
    /// can't handle.
    pub(crate) fn evaluate_type_for_binary_ops(&mut self, type_id: TypeId) -> TypeId {
        let db = self.ctx.types;
        let mut evaluate_leaf = |leaf_type: TypeId| self.evaluate_type_with_resolution(leaf_type);
        let result = evaluate_contextual_structure_with(db, type_id, &mut evaluate_leaf);

        // `evaluate_contextual_structure_with` does not walk into IndexAccess nodes.
        // For patterns like `v[k]` where `v: T extends Record<K, number>`, the
        // type is `IndexAccess(T, K)` which must be evaluated iteratively via the
        // full TypeEnvironment resolver to eventually reach the concrete type `number`.
        if tsz_solver::type_queries::get_index_access_types(self.ctx.types, result).is_some() {
            let mut current = result;
            for _ in 0..3 {
                let evaluated = self.evaluate_type_with_env(current);
                if evaluated == current || evaluated == TypeId::UNKNOWN {
                    break;
                }
                current = evaluated;
                if tsz_solver::type_queries::get_index_access_types(self.ctx.types, current)
                    .is_none()
                {
                    break;
                }
            }
            if current != result && current != TypeId::UNKNOWN {
                return current;
            }
        }

        // For TypeParameter types (e.g., T extends number), resolve through constraint
        // so arithmetic validity checks can see the constraint type.
        if let Some(constraint) =
            tsz_solver::type_queries::get_type_parameter_constraint(self.ctx.types, result)
            && constraint != TypeId::UNKNOWN
            && constraint != result
        {
            return constraint;
        }

        result
    }

    /// Evaluate a contextual type that may contain unevaluated mapped/conditional types.
    ///
    /// When a generic function's parameter type is instantiated (e.g., `{ [K in keyof P]: P[K] }`
    /// with P=Props), the result may be a mapped type with `Lazy` references that need a
    /// full resolver to evaluate. The solver's default `contextual_property_type` uses
    /// `NoopResolver` and can't resolve these. This method uses the Judge (which has access
    /// to the `TypeEnvironment` resolver) to evaluate such types into concrete object types.
    pub(crate) fn evaluate_contextual_type(&self, type_id: TypeId) -> TypeId {
        let mut evaluate_leaf = |leaf_type: TypeId| self.judge_evaluate(leaf_type);
        let evaluated =
            evaluate_contextual_structure_with(self.ctx.types, type_id, &mut evaluate_leaf);
        // Keep unresolved contextual shapes available when evaluation degrades
        // to UNKNOWN (common with partially-instantiated generic conditionals).
        if evaluated == TypeId::UNKNOWN {
            type_id
        } else {
            evaluated
        }
    }

    /// Get the type of a conditional expression (ternary operator).
    ///
    /// Computes the type of `condition ? whenTrue : whenFalse`.
    /// Returns the union of the two branch types if they differ.
    ///
    /// When a contextual type is available, each branch is checked against it
    /// to catch type errors (TS2322).
    ///
    /// Uses `solver::compute_conditional_expression_type` for type computation
    /// as part of the Solver-First architecture migration.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_conditional_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_conditional_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_conditional_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(cond) = self.ctx.arena.get_conditional_expr(node) else {
            return TypeId::ERROR;
        };

        // Get condition type for type computation
        let condition_type = self.get_type_of_node(cond.condition);
        self.check_truthy_or_falsy_with_type(cond.condition, condition_type);
        // TS2774: check for non-nullable callable tested for truthiness
        self.check_callable_truthiness(cond.condition, Some(cond.when_true));

        // Apply contextual typing to each branch for better inference,
        // but don't check assignability here - that happens at the call site.
        // This allows `cond ? "a" : "b"` to infer as `"a" | "b"` and then
        // the union is checked against the contextual type.
        let contextual_type = request.contextual_type;

        // Preserve literal types in conditional branches so that
        // `const x = cond ? "a" : "b"` infers `"a" | "b"` (tsc behavior).
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;

        // tsc always evaluates BOTH branches and unions them for the result
        // type, even when the condition is a literal boolean.  This ensures
        // `var r = true ? t : u; var r = true ? u : t;` computes the same
        // union type regardless of branch order (fixing false TS2403).
        //
        // When the condition IS a literal boolean, the dead branch may contain
        // code that would emit false diagnostics (e.g. TS2454 for variables
        // that are genuinely uninitialized on that path).  We suppress
        // diagnostics from the dead branch by snapshot/restore.
        use tsz_scanner::SyntaxKind;
        let condition_is_true = self
            .ctx
            .arena
            .get(cond.condition)
            .is_some_and(|n| n.kind == SyntaxKind::TrueKeyword as u16);
        let condition_is_false = self
            .ctx
            .arena
            .get(cond.condition)
            .is_some_and(|n| n.kind == SyntaxKind::FalseKeyword as u16);

        let should_suppress_contextual_branch_assignability =
            contextual_type.is_some() && !self.assignment_source_is_return_expression(idx);
        let suppress_contextual_branch_ts2322 =
            |state: &mut Self,
             branch_idx: NodeIndex,
             snap: &crate::context::speculation::DiagnosticSnapshot| {
                if !should_suppress_contextual_branch_assignability {
                    return;
                }
                let Some(branch_node) = state.ctx.arena.get(branch_idx) else {
                    return;
                };
                let branch_start = branch_node.pos;
                let branch_end = branch_node.end;
                state.ctx.rollback_diagnostics_filtered(snap, |diag| {
                    let in_branch = diag.start >= branch_start && diag.start < branch_end;
                    !(in_branch && diag.code == 2322)
                });
            };

        // Compute branch types with the outer contextual type for inference.
        // Use per-branch requests so each branch gets its own narrowed contextual type.
        let true_ctx = contextual_type
            .map(|ctx| self.contextual_type_for_conditional_branch(ctx, cond.when_true));
        let true_request = request.contextual_opt(true_ctx);
        let when_true = if condition_is_false {
            // Dead branch — suppress diagnostics but still compute type.
            // Must save/restore BOTH the diagnostics vec AND the dedup set,
            // otherwise entries added to the dedup set would prevent the same
            // diagnostic from being emitted later by the regular checker pass
            // (e.g. TS8010 grammar errors in JS files).
            self.speculative_type_of_node(cond.when_true, &true_request)
        } else {
            let snap = self.ctx.snapshot_diagnostics();
            let ty = self.get_type_of_node_with_request(cond.when_true, &true_request);
            suppress_contextual_branch_ts2322(self, cond.when_true, &snap);
            ty
        };

        let false_ctx = contextual_type
            .map(|ctx| self.contextual_type_for_conditional_branch(ctx, cond.when_false));
        let false_request = request.contextual_opt(false_ctx);
        let when_false = if condition_is_true {
            // Dead branch — suppress diagnostics but still compute type.
            self.speculative_type_of_node(cond.when_false, &false_request)
        } else {
            let snap = self.ctx.snapshot_diagnostics();
            let ty = self.get_type_of_node_with_request(cond.when_false, &false_request);
            suppress_contextual_branch_ts2322(self, cond.when_false, &snap);
            ty
        };

        self.ctx.preserve_literal_types = prev_preserve;

        // Do NOT widen branch literal types here. In tsc, conditional expressions
        // preserve literal types (possibly "fresh") and widening is deferred to the
        // point of use: `let`/`var` declarations widen via
        // `widen_initializer_type_for_mutable_binding`, and return type inference
        // widens via `widen_literal_type` in `infer_return_type_from_body`.
        // Eagerly widening here caused false TS2322 errors when the result was
        // assigned to a `const` with a literal union annotation, e.g.:
        //   const c1 = cond ? "foo" : "bar";        // should be "foo" | "bar"
        //   const c2: "foo" | "bar" = c1;            // should pass

        // Use Solver API for type computation (Solver-First architecture)
        expr_ops::compute_conditional_expression_type(
            self.ctx.types,
            condition_type,
            when_true,
            when_false,
        )
    }

    /// Get type of prefix unary expression.
    ///
    /// Computes the type of unary expressions like `!x`, `+x`, `-x`, `~x`, `++x`, `--x`, `typeof x`.
    /// Returns boolean for `!`, number for arithmetic operators, string for `typeof`.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_prefix_unary(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_prefix_unary_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_prefix_unary_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;
        use tsz_solver::type_queries::{LiteralTypeKind, classify_literal_type};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return TypeId::ERROR;
        };

        match unary.operator {
            // ! returns boolean — also check operand for always-truthy/falsy (TS2872/TS2873)
            k if k == SyntaxKind::ExclamationToken as u16 => {
                // Type-check operand fully so inner expression diagnostics fire
                // (e.g. TS18050 for `!(null + undefined)`).
                let operand_raw = self.get_type_of_node(unary.operand);
                let operand_type = self.resolve_type_query_type(operand_raw);
                // Suppress TS2872/TS2873 when:
                // 1. Operand is an error type (inner error already reported).
                // 2. This `!` is the direct LHS of `**`: `!X ** Y` parses as `(!X) ** Y`,
                //    which is a grammar error (TS17006). Reporting "always truthy" on top
                //    would be a false positive.
                if operand_type != TypeId::ERROR && !self.is_lhs_of_exponentiation(idx) {
                    // Skip TS2845 enum member checks — tsc only emits those in condition contexts.
                    self.check_truthy_or_falsy_with_type_no_enum(unary.operand, operand_type);
                }
                TypeId::BOOLEAN
            }
            // typeof returns string but still type-check operand for flow/node types.
            k if k == SyntaxKind::TypeOfKeyword as u16 => {
                self.get_type_of_node(unary.operand);
                TypeId::STRING
            }
            // Unary + and - return number unless contextual typing expects a numeric literal.
            // Note: tsc does NOT validate operand types for unary +/- in general.
            // Unary + is a common idiom for number conversion (+someString).
            // However, tsc DOES emit TS2469 when the operand is a symbol type.
            k if k == SyntaxKind::PlusToken as u16 || k == SyntaxKind::MinusToken as u16 => {
                // Evaluate operand for side effects / flow analysis
                let operand_type = self.get_type_of_node(unary.operand);

                // TS18050: unary +/- on literal null/undefined keyword
                // tsc emits TS18050 "The value 'X' cannot be used here" for `-undefined`, `-null`,
                // `+undefined`, `+null` even without strictNullChecks.
                if (operand_type == TypeId::UNDEFINED || operand_type == TypeId::NULL)
                    && self.is_literal_null_or_undefined_node(unary.operand)
                {
                    let value_name = if operand_type == TypeId::NULL {
                        "null"
                    } else {
                        "undefined"
                    };
                    if let Some(operand_node) = self.ctx.arena.get(unary.operand) {
                        let message = format_message(
                            diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE,
                            &[value_name],
                        );
                        self.ctx.error(
                            operand_node.pos,
                            operand_node.end.saturating_sub(operand_node.pos),
                            message,
                            diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                        );
                    }
                    return TypeId::NUMBER;
                }

                // TS18046: unary +/- on unknown is not allowed (strictNullChecks only)
                if operand_type == TypeId::UNKNOWN && self.error_is_of_type_unknown(unary.operand) {
                    return TypeId::ERROR;
                }

                // TS18050: unary +/- on literal null/undefined keywords.
                // tsc emits this regardless of strictNullChecks.
                if self.is_literal_null_or_undefined_node(unary.operand) {
                    let cause = if let Some(node) = self.ctx.arena.get(unary.operand)
                        && node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
                    {
                        TypeId::NULL
                    } else {
                        TypeId::UNDEFINED
                    };
                    self.emit_nullish_operand_error(unary.operand, cause);
                    return TypeId::NUMBER;
                }

                // TS2469: unary +/- on symbol types
                {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    if evaluator.is_symbol_like(operand_type) {
                        let op_str = if k == SyntaxKind::PlusToken as u16 {
                            "+"
                        } else {
                            "-"
                        };
                        if let Some(operand_node) = self.ctx.arena.get(unary.operand) {
                            let message = format_message(
                                diagnostic_messages::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                                &[op_str],
                            );
                            self.ctx.error(
                                operand_node.pos,
                                operand_node.end.saturating_sub(operand_node.pos),
                                message,
                                diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                            );
                        }
                        return TypeId::NUMBER;
                    }
                }

                // TS2736: unary + cannot be applied to bigint types.
                // JavaScript throws at runtime for +bigint, so tsc rejects it.
                // Unary - on bigint IS valid (-1n === -(1n)).
                if k == SyntaxKind::PlusToken as u16
                    && operand_type != TypeId::ANY
                    && operand_type != TypeId::ERROR
                {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    if evaluator.is_bigint_like(operand_type)
                        && let Some(operand_node) = self.ctx.arena.get(unary.operand)
                    {
                        let type_str = self.format_type(operand_type);
                        let message = format_message(
                            diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                            &["+", &type_str],
                        );
                        self.ctx.error(
                            operand_node.pos,
                            operand_node.end.saturating_sub(operand_node.pos),
                            message,
                            diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                        );
                        return TypeId::NUMBER;
                    }
                }

                if let Some(literal_type) = self.literal_type_from_initializer(idx) {
                    if request.contextual_type.is_some_and(|ctx_type| {
                        self.contextual_type_allows_literal(ctx_type, literal_type)
                    }) {
                        return literal_type;
                    }

                    if matches!(
                        classify_literal_type(self.ctx.types, literal_type),
                        LiteralTypeKind::BigInt(_)
                    ) {
                        if unary.operator == SyntaxKind::PlusToken as u16 {
                            if let Some(node) = self.ctx.arena.get(idx) {
                                let message = format_message(
                                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                                    &["+", "bigint"],
                                );
                                self.ctx.error(
                                    node.pos,
                                    node.end.saturating_sub(node.pos),
                                    message,
                                    diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                                );
                            }
                            return TypeId::ERROR;
                        }

                        // Preserve bigint literals for unary +/- to avoid widening to number in
                        // numeric-literal assignments (`const negZero: 0n = -0n`).
                        return literal_type;
                    }
                }

                // Return bigint for bigint operands, number otherwise.
                {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    let resolved = self.evaluate_type_with_env(operand_type);
                    if evaluator.is_bigint_like(resolved) {
                        TypeId::BIGINT
                    } else {
                        TypeId::NUMBER
                    }
                }
            }
            // ~ (bitwise NOT) — returns bigint for bigint operands, number otherwise.
            // Note: tsc does NOT validate operand types for ~ in general,
            // but DOES emit TS2469 when the operand is a symbol type.
            k if k == SyntaxKind::TildeToken as u16 => {
                // Evaluate operand for side effects / flow analysis
                let operand_type = self.get_type_of_node(unary.operand);

                // TS2469: unary ~ on symbol types
                {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    if evaluator.is_symbol_like(operand_type)
                        && let Some(operand_node) = self.ctx.arena.get(unary.operand)
                    {
                        let message = format_message(
                            diagnostic_messages::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                            &["~"],
                        );
                        self.ctx.error(
                            operand_node.pos,
                            operand_node.end.saturating_sub(operand_node.pos),
                            message,
                            diagnostic_codes::THE_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                        );
                    }
                }

                // Return bigint for bigint operands, number otherwise.
                {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    let resolved = self.evaluate_type_with_env(operand_type);
                    if evaluator.is_bigint_like(resolved) {
                        TypeId::BIGINT
                    } else {
                        TypeId::NUMBER
                    }
                }
            }
            // ++ and -- require numeric operand and valid l-value
            k if k == SyntaxKind::PlusPlusToken as u16
                || k == SyntaxKind::MinusMinusToken as u16 =>
            {
                self.check_strict_mode_eval_or_arguments_assignment(unary.operand);
                if self.check_function_assignment(unary.operand) {
                    return TypeId::NUMBER;
                }

                // Get operand type for validation.
                // TSC checks arithmetic type BEFORE lvalue — if the type check
                // fails (TS2356), the lvalue check (TS2357) is skipped.
                let operand_type = self.get_type_of_node(unary.operand);

                // TS18046: ++/-- on unknown is not allowed (strictNullChecks only).
                // tsc emits TS18046 instead of TS2356 for unknown operands.
                if operand_type == TypeId::UNKNOWN && self.error_is_of_type_unknown(unary.operand) {
                    return TypeId::NUMBER;
                }

                let mut arithmetic_ok = true;

                {
                    use tsz_solver::BinaryOpEvaluator;
                    let evaluator = BinaryOpEvaluator::new(self.ctx.types);
                    let (non_nullish, nullish_cause) = self.split_nullish_type(operand_type);
                    let nullish_can_flow_to_number = non_nullish.is_none_or(|ty| {
                        let evaluated = self.evaluate_type_with_env(ty);
                        evaluator.is_arithmetic_operand(evaluated) || self.is_enum_like_type(ty)
                    });
                    if self.ctx.strict_null_checks()
                        && let Some(cause) = nullish_cause
                        && nullish_can_flow_to_number
                    {
                        arithmetic_ok = false;
                        self.emit_nullish_operand_error(unary.operand, cause);
                    }

                    // Evaluate the type to resolve Lazy(DefId) aliases before checking.
                    // Type aliases like `YesNo = Choice.Yes | Choice.No` may stay as
                    // Lazy(DefId) which the visitor can't recurse into.
                    let resolved_type = self.evaluate_type_with_env(operand_type);
                    // When strictNullChecks is off, null/undefined are silently
                    // assignable to number, so skip arithmetic check for them.
                    let is_valid = evaluator.is_arithmetic_operand(resolved_type)
                        || self.is_enum_like_type(operand_type)
                        || self.is_enum_like_type(resolved_type)
                        || (!self.ctx.strict_null_checks()
                            && (operand_type == TypeId::NULL || operand_type == TypeId::UNDEFINED));

                    if arithmetic_ok && !is_valid {
                        arithmetic_ok = false;
                        // Emit TS2356 for invalid increment/decrement operand type
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            unary.operand,
                            diagnostic_messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                            diagnostic_codes::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                        );
                    }
                }

                // Determine the result type: bigint for bigint operands, number otherwise.
                // tsc returns the same numeric type as the operand for ++/--.
                let result_type = {
                    let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
                    let resolved = self.evaluate_type_with_env(operand_type);
                    if evaluator.is_bigint_like(resolved) {
                        TypeId::BIGINT
                    } else {
                        TypeId::NUMBER
                    }
                };

                // Only check lvalue and assignment restrictions when arithmetic
                // type is valid (matches TSC: TS2357 is skipped when TS2356 fires).
                if arithmetic_ok {
                    let emitted_lvalue = self.check_increment_decrement_operand(unary.operand);

                    if !emitted_lvalue {
                        // TS2588: Cannot assign to 'x' because it is a constant.
                        let is_const = self.check_const_assignment(unary.operand);

                        // TS2630: Cannot assign to 'x' because it is a function.
                        self.check_function_assignment(unary.operand);

                        // TS2540: Cannot assign to readonly property
                        if !is_const {
                            self.check_readonly_assignment(unary.operand, idx);
                        }
                    }
                }

                result_type
            }
            // delete returns boolean and checks that operand is a property reference
            k if k == SyntaxKind::DeleteKeyword as u16 => {
                // Evaluate operand for side effects / flow analysis
                let operand_type = self.get_type_of_node(unary.operand);

                let operand_idx = self.ctx.arena.skip_parenthesized(unary.operand);

                // TS1102: delete cannot be called on an identifier in strict mode.
                let is_identifier_operand = operand_idx.is_some()
                    && self.ctx.arena.get(operand_idx).is_some_and(|operand_node| {
                        operand_node.kind == SyntaxKind::Identifier as u16
                    });
                // TSC's grammarErrorOnNode suppresses at file level via
                // hasParseDiagnostics(sourceFile), not per-node.
                let suppress_delete_identifier_error = self.has_syntax_parse_errors();
                if is_identifier_operand
                    && self.is_strict_mode_for_node(idx)
                    && !suppress_delete_identifier_error
                {
                    self.error_at_node(
                        operand_idx,
                        crate::diagnostics::diagnostic_messages::DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE,
                        crate::diagnostics::diagnostic_codes::DELETE_CANNOT_BE_CALLED_ON_AN_IDENTIFIER_IN_STRICT_MODE,
                    );
                }

                // TS2703: The operand of a 'delete' operator must be a property reference.
                // Valid operands: property access (obj.prop), element access (obj["prop"]),
                // or optional chain (obj?.prop). All other expressions are invalid.
                let is_property_reference = operand_idx.is_some()
                    && self.ctx.arena.get(operand_idx).is_some_and(|operand_node| {
                        use tsz_parser::parser::syntax_kind_ext;
                        operand_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || operand_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    });

                // Suppress TS2703 when:
                // 1. Operand has an error type (inner error already reported).
                // 2. This `delete` is the direct LHS of `**`: `delete X ** Y` parses as
                //    `(delete X) ** Y`, which is a grammar error (TS17006). Reporting
                //    TS2703 on top would be a false positive.
                let suppress_property_reference_error = self.has_syntax_parse_errors()
                    && operand_idx.is_some()
                    && self.node_span_contains_parse_error(operand_idx);
                let suppress_js_strict_mode_delete_follow_on =
                    self.is_js_file() && !self.ctx.js_strict_mode_diagnostics_enabled();
                if !is_property_reference
                    && !suppress_property_reference_error
                    && !suppress_js_strict_mode_delete_follow_on
                    && !self.is_lhs_of_exponentiation(idx)
                {
                    // tsc's grammarErrorOnNode skips parenthesized wrappers, so
                    // `delete (expr)` should point at `expr`, not `(`.
                    let error_node = self.ctx.arena.skip_parenthesized(unary.operand);
                    self.error_at_node(
                        error_node,
                        crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                        crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                    );
                }
                // TS2542: Cannot delete a readonly index signature element.
                // For `delete v[expr]` where v has a readonly index signature
                // (e.g., readonly tuples, readonly arrays, objects with readonly index sigs).
                let has_readonly_delete_error = if is_property_reference {
                    self.check_readonly_delete_operand(operand_idx)
                } else {
                    false
                };

                // TS18011: The operand of a 'delete' operator cannot be a private identifier.
                if is_property_reference
                    && let Some(operand_node) = self.ctx.arena.get(operand_idx)
                    && operand_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(operand_node)
                    && self.is_private_identifier_name(access.name_or_argument)
                {
                    self.error_at_node(
                        operand_idx,
                        crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_PRIVATE_IDENTIFIER,
                        crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_PRIVATE_IDENTIFIER,
                    );
                }

                // TS2790: In strictNullChecks, delete is only allowed for optional properties.
                // With exactOptionalPropertyTypes disabled, properties whose declared type
                // includes `undefined` are also treated as deletable.
                // tsc also exempts: any/unknown/never property types, index signature properties.
                if !has_readonly_delete_error
                    && self.ctx.compiler_options.strict_null_checks
                    && let Some(operand_node) = self.ctx.arena.get(operand_idx)
                    && (operand_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || operand_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                    && let Some(access) = self.ctx.arena.get_access_expr(operand_node)
                {
                    use crate::query_boundaries::common::PropertyAccessResult;

                    let prop_name = self
                        .ctx
                        .arena
                        .get_identifier_at(access.name_or_argument)
                        .map(|ident| ident.escaped_text.clone())
                        .or_else(|| self.get_literal_string_from_node(access.name_or_argument))
                        .or_else(|| {
                            self.get_literal_index_from_node(access.name_or_argument)
                                .map(|idx| idx.to_string())
                        });

                    if let Some(prop_name) = prop_name {
                        let mut object_type = self.get_type_of_node(access.expression);
                        let uses_optional_chain_base = access.question_dot_token
                            || crate::computation::access::is_optional_chain(
                                self.ctx.arena,
                                access.expression,
                            );
                        if uses_optional_chain_base {
                            let (non_nullish, _) = self.split_nullish_type(object_type);
                            if let Some(non_nullish) = non_nullish {
                                object_type = non_nullish;
                            }
                        }

                        if object_type != TypeId::ANY
                            && object_type != TypeId::UNKNOWN
                            && object_type != TypeId::ERROR
                            && object_type != TypeId::NEVER
                        {
                            let property_result =
                                self.resolve_property_access_with_env(object_type, &prop_name);
                            let (prop_type, from_idx_sig) = match property_result {
                                PropertyAccessResult::Success {
                                    type_id,
                                    from_index_signature,
                                    ..
                                } => {
                                    let prop_type = if uses_optional_chain_base
                                        || operand_node.kind
                                            == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                                    {
                                        type_id
                                    } else {
                                        operand_type
                                    };
                                    (prop_type, from_index_signature)
                                }
                                _ => (operand_type, false),
                            };

                            if prop_type != TypeId::ANY
                                && prop_type != TypeId::UNKNOWN
                                && prop_type != TypeId::NEVER
                                && prop_type != TypeId::ERROR
                            {
                                let is_mapped =
                                    tsz_solver::is_mapped_type(self.ctx.types, object_type);
                                if !from_idx_sig && !is_mapped {
                                    let is_optional =
                                        self.is_property_optional(object_type, &prop_name);
                                    let optional_via_undefined =
                                        !self.ctx.compiler_options.exact_optional_property_types
                                            && tsz_solver::type_queries::type_includes_undefined(
                                                self.ctx.types,
                                                prop_type,
                                            );
                                    if !is_optional && !optional_via_undefined {
                                        self.error_at_node(
                                            operand_idx,
                                            crate::diagnostics::diagnostic_messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
                                            crate::diagnostics::diagnostic_codes::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                TypeId::BOOLEAN
            }
            // void returns undefined
            k if k == SyntaxKind::VoidKeyword as u16 => {
                // Evaluate operand for side effects / flow analysis
                self.get_type_of_node(unary.operand);
                TypeId::UNDEFINED
            }
            _ => TypeId::ANY,
        }
    }

    pub(crate) fn is_strict_mode_for_node(&self, idx: NodeIndex) -> bool {
        self.ctx.is_strict_mode_for_node(idx)
    }

    /// Get type of template expression (template literal with substitutions).
    ///
    /// Type-checks all expressions within template spans to emit errors like TS2304.
    ///
    /// In TypeScript, template expressions produce:
    /// - A template literal type when the contextual type expects one (e.g., parameter
    ///   expects `` `${T}:${U}` ``), preserving type parameter information
    /// - `string` type otherwise
    ///
    /// Uses `solver::compute_template_expression_type` for type computation
    /// as part of the Solver-First architecture migration.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_template_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_template_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_template_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::STRING;
        };

        let Some(template) = self.ctx.arena.get_template_expr(node) else {
            return TypeId::STRING;
        };

        // Extract the head text (text before the first ${})
        let head_text = self
            .ctx
            .arena
            .get(template.head)
            .and_then(|n| self.ctx.arena.get_literal(n))
            .map(|lit| lit.text.clone())
            .unwrap_or_default();

        let span_request = request.read().normal_origin().contextual_opt(None);

        // Preserve literal types for template span expressions so that
        // `abc${0}abc` can resolve to the concrete string literal "abc0abc".
        // In tsc, literals in expression position always keep their literal type;
        // widening only happens at binding sites. We temporarily enable literal
        // preservation here to match that behavior.
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;

        // Type-check each template span's expression and collect types + text parts
        let mut part_types = Vec::new();
        let mut texts = vec![head_text];
        for &span_idx in &template.template_spans.nodes {
            let Some(span_node) = self.ctx.arena.get(span_idx) else {
                continue;
            };

            let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                continue;
            };

            // Type-check the expression - this will emit TS2304 if name is unresolved
            let part_type = self.get_type_of_node_with_request(span.expression, &span_request);
            part_types.push(part_type);

            // Extract the text after this expression (middle or tail)
            let tail_text = self
                .ctx
                .arena
                .get(span.literal)
                .and_then(|n| self.ctx.arena.get_literal(n))
                .map(|lit| lit.text.clone())
                .unwrap_or_default();
            texts.push(tail_text);
        }

        // Restore previous literal preservation state
        self.ctx.preserve_literal_types = prev_preserve;

        // Check if we're in a template literal context:
        // 1. Contextual type is/contains a template literal type or string literal type
        // 2. Inside a const assertion (as const)
        let in_template_context = self.ctx.in_const_assertion
            || request.contextual_type.is_some_and(|ct| {
                expr_ops::is_template_literal_contextual_type(self.ctx.types, ct)
            });

        if in_template_context {
            // Construct a template literal type preserving type parameter shapes
            expr_ops::compute_template_expression_type_contextual(
                self.ctx.types,
                &texts,
                &part_types,
            )
        } else {
            // Default: template literals produce string type
            expr_ops::compute_template_expression_type(self.ctx.types, &texts, &part_types)
        }
    }

    /// Get type of variable declaration.
    ///
    /// Computes the type of variable declarations like `let x: number = 5` or `const y = "hello"`.
    /// Returns the type annotation if present, otherwise infers from the initializer.
    pub(crate) fn get_type_of_variable_declaration(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return TypeId::ERROR;
        };

        // First check type annotation - this takes precedence
        if var_decl.type_annotation.is_some() {
            let annotation_type = self.get_type_from_type_node(var_decl.type_annotation);
            // `const k: unique symbol = Symbol()` — create a proper UniqueSymbol type
            // using the variable's binder symbol as the identity.
            if annotation_type == TypeId::SYMBOL
                && self.is_const_variable_declaration(idx)
                && self.is_unique_symbol_type_annotation(var_decl.type_annotation)
                && let Some(sym_id) = self.get_symbol_id_for_variable_name(var_decl.name)
            {
                return self
                    .ctx
                    .types
                    .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
            }
            return annotation_type;
        }

        if self.is_catch_clause_variable_declaration(idx) {
            // Route through the flow observation boundary for centralized
            // catch-variable typing policy.
            return flow_boundary::resolve_catch_variable_type(
                self.ctx.use_unknown_in_catch_variables(),
            );
        }

        // For-in variables are always typed as `string`
        if self.is_for_in_variable_declaration(idx) {
            return TypeId::STRING;
        }

        // Infer from initializer
        if var_decl.initializer.is_some() {
            let init_type = self.get_type_of_node(var_decl.initializer);

            // Rule #10: Literal Widening (with freshness)
            // For mutable bindings (let/var), widen literals to their primitive type
            // ONLY when the initializer is a "fresh" literal expression (direct literal
            // in source code). Types from variable references, narrowing, or computed
            // expressions are "non-fresh" and should NOT be widened.
            // For const bindings, preserve literal types (unless in array/object context)
            if !self.is_const_variable_declaration(idx) {
                let widened = if self.is_fresh_literal_expression(var_decl.initializer) {
                    self.widen_initializer_type_for_mutable_binding(init_type)
                } else {
                    init_type
                };
                // Route null/undefined widening through the flow observation boundary.
                return flow_boundary::widen_null_undefined_to_any(
                    self.ctx.types,
                    widened,
                    self.ctx.strict_null_checks(),
                );
            }

            // `const k = Symbol()` — infer unique symbol type.
            // In TypeScript, const declarations initialized with Symbol() get
            // a unique symbol type (typeof k), not the general `symbol` type.
            if init_type == TypeId::SYMBOL
                && self.is_symbol_call_initializer(var_decl.initializer)
                && let Some(sym_id) = self.get_symbol_id_for_variable_name(var_decl.name)
            {
                return self
                    .ctx
                    .types
                    .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
            }

            // const: preserve literal type — use the literal type from the
            // initializer directly, since get_type_of_node may have widened it
            // (e.g., `const c = 0` should be `0`, not `number`)
            if let Some(literal) = self.literal_type_from_initializer(var_decl.initializer) {
                literal
            } else {
                init_type
            }
        } else {
            // No initializer - use UNKNOWN to enforce strict checking
            // This requires explicit type annotation or prevents unsafe usage
            TypeId::UNKNOWN
        }
    }

    /// Check if an initializer expression is a `Symbol()` or `Symbol("desc")` call.
    pub(crate) fn is_symbol_call_initializer(&self, init_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
            ident.escaped_text == "Symbol"
        } else {
            false
        }
    }

    /// Get the binder SymbolId for a variable declaration's name node.
    fn get_symbol_id_for_variable_name(&self, name_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        self.ctx.binder.get_node_symbol(name_idx)
    }

    /// Get the type of an assignment target without definite assignment checks.
    ///
    /// Computes the type of the left-hand side of an assignment expression.
    /// Handles identifier resolution and type-only alias checking.
    pub(crate) fn get_type_of_assignment_target(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;

        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == SyntaxKind::Identifier as u16
        {
            // Check for local variable first (including "arguments" shadowing).
            // This handles: `const arguments = ...; arguments = foo;`
            if let Some(sym_id) = self.resolve_identifier_symbol_for_write(idx) {
                if self.alias_resolves_to_type_only(sym_id) {
                    if let Some(ident) = self.ctx.arena.get_identifier(node) {
                        self.report_wrong_meaning_diagnostic(
                            &ident.escaped_text,
                            idx,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                    }
                    return TypeId::ERROR;
                }

                if let Some(ident) = self.ctx.arena.get_identifier(node)
                    && self.check_tdz_violation(sym_id, idx, &ident.escaped_text, false)
                {
                    return TypeId::ERROR;
                }

                // Check if this is "arguments" in a function body with a local declaration
                if let Some(ident) = self.ctx.arena.get_identifier(node) {
                    if ident.escaped_text == "arguments" && self.is_in_regular_function_body(idx) {
                        // Check if the declaration is local to the current function
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && !symbol.declarations.is_empty()
                        {
                            let decl_node = symbol.declarations[0];
                            if let Some(current_fn) = self.find_enclosing_function(idx)
                                && let Some(decl_fn) = self.find_enclosing_function(decl_node)
                                && current_fn == decl_fn
                            {
                                // Local "arguments" declaration - use it
                                let declared_type = self.get_type_of_symbol(sym_id);
                                return declared_type;
                            }
                        }
                        // Symbol found but not local - fall through to IArguments check below
                    } else {
                        // Not "arguments" or not in function - use the symbol
                        let declared_type = self.get_type_of_symbol(sym_id);
                        return declared_type;
                    }
                } else {
                    // Use the resolved symbol
                    let declared_type = self.get_type_of_symbol(sym_id);
                    return declared_type;
                }
            }

            // Inside a regular function body, `arguments` is the implicit IArguments object,
            // overriding any outer `arguments` declaration (but not local ones, checked above).
            if let Some(ident) = self.ctx.arena.get_identifier(node)
                && ident.escaped_text == "arguments"
                && self.is_in_regular_function_body(idx)
            {
                let lib_binders = self.get_lib_binders();
                if let Some(sym_id) = self
                    .ctx
                    .binder
                    .get_global_type_with_libs("IArguments", &lib_binders)
                {
                    return self.type_reference_symbol_type(sym_id);
                }
                return TypeId::ANY;
            }
        }

        // Instantiation expressions on the left side (e.g. `fn<T> = ...`) are invalid (TS2364),
        // but the base expression is still a value read and must participate in
        // definite assignment checks (TS2454).
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(node)
            && expr_type_args
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty())
        {
            let base_expr = expr_type_args.expression;
            let _ = self.get_type_of_node(base_expr);

            // In assignment-target context, flow nodes may attach to the outer
            // instantiation expression rather than the inner identifier. Force
            // definite-assignment checking for `id<T> = ...` to match tsc.
            if let Some(base_node) = self.ctx.arena.get(base_expr)
                && base_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(base_expr)
            {
                let declared_type = self.get_type_of_symbol(sym_id);
                let _ = self.check_flow_usage(base_expr, declared_type, sym_id);
            }
        }

        // For non-identifier assignment targets (property access, element access, etc.),
        // we need the declared type without control-flow narrowing.
        // Example: After `if (foo[x] === undefined)`, when checking `foo[x] = 1`,
        // we should check against the declared type (e.g., `number | undefined` from index signature)
        // not the narrowed type (e.g., `undefined`).
        //
        // However, if the target is invalid (e.g. `getValue<number> = ...` parsed as BinaryExpression),
        // we should NOT skip narrowing because we want to treat it as an expression read
        // to catch errors like TS2454 (used before assigned).

        // Expando function pattern: when assigning to a property of a function
        // declaration (e.g., `foo.toString = () => {}`), tsc treats ALL property
        // assignments as creating/overriding properties on the function's expando
        // type, WITHOUT checking assignability against existing Function prototype
        // properties. Return `any` to match this behavior.
        // Note: class declarations are NOT included here — class static property
        // assignments DO check assignability in tsc.
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
            && let Some(obj_sym) =
                self.resolve_identifier_symbol_without_tracking(access.expression)
            && let Some(symbol) = self.ctx.binder.get_symbol(obj_sym)
            && (symbol.flags & tsz_binder::symbol_flags::FUNCTION) != 0
            && (symbol.flags & tsz_binder::symbol_flags::CLASS) == 0
        {
            // Still evaluate the node so side effects (diagnostics on the object) fire,
            // but return `any` for the LHS type so assignability is not checked.
            let _ = self.get_type_of_node_with_request(idx, &TypingRequest::for_write_context());
            return TypeId::ANY;
        }

        if self.is_valid_assignment_target(idx) {
            self.get_type_of_node_with_request(idx, &TypingRequest::for_write_context())
        } else {
            self.get_type_of_node(idx)
        }
    }

    /// Get the type of a class member.
    ///
    /// Computes the type for class property declarations, method declarations, and getters.
    pub(crate) fn get_type_of_class_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    return TypeId::ANY;
                };

                // Get the type: either from annotation or inferred from initializer
                if let Some(declared_type) =
                    self.effective_class_property_declared_type(member_idx, prop)
                {
                    declared_type
                } else if prop.initializer.is_some() {
                    self.get_type_of_node(prop.initializer)
                } else {
                    TypeId::ANY
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                    return TypeId::ANY;
                };
                let signature = self.call_signature_from_method(method, member_idx);
                use tsz_solver::FunctionShape;
                let factory = self.ctx.types.factory();
                factory.function(FunctionShape {
                    type_params: signature.type_params,
                    params: signature.params,
                    this_type: signature.this_type,
                    return_type: signature.return_type,
                    type_predicate: signature.type_predicate,
                    is_constructor: false,
                    is_method: true,
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                    return TypeId::ANY;
                };

                if accessor.type_annotation.is_some() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                }
            }
            _ => TypeId::ANY,
        }
    }

    /// Get the simple type of an interface member (without wrapping in object type).
    ///
    /// For property signatures: returns the property type
    /// For method signatures: returns the function type
    pub(crate) fn get_type_of_interface_member_simple(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::FunctionShape;
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ANY;
        };

        if member_node.kind == METHOD_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            let (type_params, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
            let (params, this_type) = self.extract_params_from_signature(sig);
            let (return_type, type_predicate) =
                self.return_type_and_predicate(sig.type_annotation, &params);

            let shape = FunctionShape {
                type_params,
                params,
                this_type,
                return_type,
                type_predicate,
                is_constructor: false,
                is_method: true,
            };
            self.pop_type_parameters(type_param_updates);
            return factory.function(shape);
        }

        if member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ANY;
            };

            if sig.type_annotation.is_some() {
                return self.get_type_from_type_node(sig.type_annotation);
            }
            return TypeId::ANY;
        }

        TypeId::ANY
    }

    /// Get the type of an interface member.
    ///
    /// Returns an object type containing the member. For method signatures,
    /// creates a callable type. For property signatures, creates a property type.
    pub(crate) fn get_type_of_interface_member(&mut self, member_idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use tsz_solver::{FunctionShape, PropertyInfo};
        let factory = self.ctx.types.factory();

        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return TypeId::ERROR;
        };

        if member_node.kind == METHOD_SIGNATURE || member_node.kind == PROPERTY_SIGNATURE {
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                return TypeId::ERROR;
            };
            let name = self.get_property_name(sig.name);
            let Some(name) = name else {
                return TypeId::ERROR;
            };
            let name_atom = self.ctx.types.intern_string(&name);

            if member_node.kind == METHOD_SIGNATURE {
                let (type_params, type_param_updates) =
                    self.push_type_parameters(&sig.type_parameters);
                let (params, this_type) = self.extract_params_from_signature(sig);
                let (return_type, type_predicate) =
                    self.return_type_and_predicate(sig.type_annotation, &params);

                let shape = FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate,
                    is_constructor: false,
                    is_method: true,
                };
                self.pop_type_parameters(type_param_updates);
                let method_type = factory.function(shape);

                let prop = PropertyInfo {
                    name: name_atom,
                    type_id: method_type,
                    write_type: method_type,
                    optional: sig.question_token,
                    readonly: self.has_readonly_modifier(&sig.modifiers),
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                };
                return factory.object(vec![prop]);
            }

            let type_id = if sig.type_annotation.is_some() {
                self.get_type_from_type_node(sig.type_annotation)
            } else {
                TypeId::ANY
            };
            let prop = PropertyInfo {
                name: name_atom,
                type_id,
                write_type: type_id,
                optional: sig.question_token,
                readonly: self.has_readonly_modifier(&sig.modifiers),
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            };
            return factory.object(vec![prop]);
        }

        TypeId::ANY
    }

    // =========================================================================
    // Speculative type computation helpers
    // =========================================================================

    /// Compute the type of a node speculatively: snapshots diagnostics,
    /// evaluates the node with the given request, then rolls back all
    /// diagnostics. Only the resulting `TypeId` survives.
    ///
    /// Use this for inference-contributing probes (e.g. Round 1 generic
    /// inference, dead conditional branches) where the type is needed but
    /// side-effect diagnostics must not leak.
    pub(crate) fn speculative_type_of_node(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let snap = self.ctx.snapshot_diagnostics();
        let ty = self.get_type_of_node_with_request(idx, request);
        self.ctx.rollback_diagnostics(&snap);
        ty
    }

    /// Like [`speculative_type_of_node`](Self::speculative_type_of_node) but
    /// for function-shaped nodes (methods, function expressions, arrow
    /// functions). Delegates to `get_type_of_function_with_request`.
    pub(crate) fn speculative_type_of_function(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let snap = self.ctx.snapshot_diagnostics();
        let ty = self.get_type_of_function_with_request(idx, request);
        self.ctx.rollback_diagnostics(&snap);
        ty
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_codes;

    #[test]
    fn template_expr_contextual_type_no_false_positive() {
        // Template expression `\`${scope}:${event}\`` passed to a parameter expecting
        // a template literal type should NOT produce TS2345
        let source = r#"
type Registry = { a: { a1: {} }; b: { b1: {} } };
type Keyof<T> = keyof T & string;
declare function f1<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;
function f2<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(scope: Scope, event: Event) {
  f1(`${scope}:${event}`);
}
"#;
        let errors = check_source_codes(source);
        assert!(
            !errors.contains(&2345),
            "Should not emit TS2345 for template literal matching contextual type, got: {errors:?}"
        );
    }

    #[test]
    fn generic_array_like_context_provides_element_type() {
        // When contextual type is a generic Application like ReadonlyArray<[K, V]>,
        // ensure the solver extracts the element type from the type arguments.
        // This exercises the Application → evaluation path in get_array_element_type.
        // The full Iterable<readonly [K, V]> path (used by Map constructor) is
        // validated by conformance tests (for-of37, for-of40, for-of50) since it
        // requires Symbol.iterator from lib definitions.
        let source = r#"
interface ReadonlyArray<T> {
    readonly length: number;
    readonly [n: number]: T;
}
declare function f<K, V>(entries: ReadonlyArray<readonly [K, V]>): [K, V];
const r = f([["", true]]);
"#;
        let errors = check_source_codes(source);
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
            "ReadonlyArray<readonly [K, V]> should contextually type array elements as tuples, got: {semantic_errors:?}"
        );
    }

    #[test]
    fn array_param_context_still_works() {
        // Ensure the fix doesn't break the already-working array parameter path.
        // When the parameter is a plain array type (readonly (readonly [K, V])[]),
        // contextual typing should still work without needing the fallback.
        let source = r#"
declare function f<K, V>(entries: readonly (readonly [K, V])[]): [K, V];
const result = f([["", true]]);
"#;
        let errors = check_source_codes(source);
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            !semantic_errors.contains(&2345) && !semantic_errors.contains(&2769),
            "Array parameter should contextually type elements as tuples, got: {semantic_errors:?}"
        );
    }

    #[test]
    fn template_expr_without_context_stays_string() {
        // Template expression assigned to `string` should still work (not break)
        let source = r#"
function f(x: string, y: number): string {
    return `${x} is ${y}`;
}
"#;
        let errors = check_source_codes(source);
        // Filter out TS2318 (lib not found) since test env has no lib definitions
        let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
        assert!(
            semantic_errors.is_empty(),
            "Template expression returning string should produce no semantic errors, got: {semantic_errors:?}"
        );
    }
}
