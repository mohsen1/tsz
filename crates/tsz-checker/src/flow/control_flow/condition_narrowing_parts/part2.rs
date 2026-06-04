impl<'a> FlowAnalyzer<'a> {
    /// Handle boolean comparison narrowing: `expr === true`, `expr === false`,
    /// `expr !== true`, `expr !== false`, and their reversed variants.
    ///
    /// When a type guard expression is compared to `true` or `false`, TypeScript
    /// preserves the narrowing. For example:
    ///   - `x instanceof Error === false` → same as `!(x instanceof Error)`
    ///   - `isString(x) === true` → same as `isString(x)`
    ///   - `x instanceof Error !== false` → same as `x instanceof Error`
    fn narrow_by_boolean_comparison(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<TypeId> {
        // Only handle strict/loose equality/inequality operators
        let is_strict_eq = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16;
        let is_strict_neq = bin.operator_token == SyntaxKind::ExclamationEqualsEqualsToken as u16;
        let is_loose_eq = bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
        let is_loose_neq = bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16;

        if !is_strict_eq && !is_strict_neq && !is_loose_eq && !is_loose_neq {
            return None;
        }

        // Check for true/false on either side
        let (guard_expr, is_compared_to_true) = if self.is_boolean_literal(bin.right) {
            (bin.left, self.is_true_literal(bin.right))
        } else if self.is_boolean_literal(bin.left) {
            (bin.right, self.is_true_literal(bin.left))
        } else {
            return None;
        };

        // Don't intercept discriminant property comparisons like `x.kind === false`.
        // These should go through discriminant narrowing (which checks `false <: prop_type`),
        // not boolean truthiness narrowing (which checks whether prop_type can be falsy).
        // Only apply boolean comparison for complex guard expressions like
        // `x instanceof Error === false` or `isString(x) === true`.
        if self
            .relative_discriminant_path(guard_expr, target)
            .is_some()
        {
            return None;
        }

        // Don't intercept plain reference equality like `u === true` where `u`
        // is itself the narrowing target. That should go through
        // `LiteralEquality` (narrow `u` to `true`/`false`), not through
        // truthiness recursion. The latter would just check whether `u` could
        // be truthy and leave broad sources like `unknown` un-narrowed.
        if self.is_matching_reference(guard_expr, target) {
            return None;
        }

        // Determine effective sense:
        // `expr === true` in true branch → narrow as if expr is true
        // `expr === false` in true branch → narrow as if expr is false
        // `expr !== true` in true branch → narrow as if expr is false
        // `expr !== false` in true branch → narrow as if expr is true
        let is_negated = is_strict_neq || is_loose_neq;
        let effective_sense = if is_compared_to_true {
            if is_negated {
                !is_true_branch
            } else {
                is_true_branch
            }
        } else {
            // compared to false — invert
            if is_negated {
                is_true_branch
            } else {
                !is_true_branch
            }
        };

        // Recursively narrow based on the guard expression
        Some(self.narrow_type_by_condition_inner(
            type_id,
            guard_expr,
            target,
            effective_sense,
            antecedent_id,
            visited_aliases,
        ))
    }

    /// Check if a node is the literal `true` or `false`.
    fn is_boolean_literal(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized(node);
        self.arena.get(node).is_some_and(|n| {
            n.kind == SyntaxKind::TrueKeyword as u16 || n.kind == SyntaxKind::FalseKeyword as u16
        })
    }

    /// Check if a node is the literal `true`.
    fn is_true_literal(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized(node);
        self.arena
            .get(node)
            .is_some_and(|n| n.kind == SyntaxKind::TrueKeyword as u16)
    }

    pub(crate) fn narrow_by_logical_expr(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<TypeId> {
        let operator = bin.operator_token;

        // Logical assignment operators (&&=, ||=, ??=) used in conditions
        // (e.g. `if (x &&= y)`) have the same truthiness/narrowing semantics
        // as their corresponding logical operators (&&, ||, ??). The assignment
        // side-effect is handled by the ASSIGNMENT flow node separately.
        if operator == SyntaxKind::AmpersandAmpersandToken as u16
            || operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
        {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_true,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(right_true);
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let left_true = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                true,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_true,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(self.union_logical_condition_branches(vec![left_false, right_false]));
        }

        // For ||= and ??= in condition context: `if (x ||= y)` / `if (x ??= y)`
        // When the LHS matches the target reference, the assignment ensures x holds
        // the expression result. So in the true branch, x is truthy (the result was
        // truthy). This is different from plain `||`/`??` where the LHS is NOT
        // assigned the result.
        if (operator == SyntaxKind::BarBarEqualsToken as u16
            || operator == SyntaxKind::QuestionQuestionEqualsToken as u16)
            && self.is_matching_reference(bin.left, target)
        {
            let env_borrow;
            let narrowing = if let Some(env) = &self.type_environment {
                env_borrow = env.borrow();
                self.make_narrowing_context().with_resolver(&*env_borrow)
            } else {
                self.make_narrowing_context()
            };
            if is_true_branch {
                // x holds the truthy result → remove null/undefined
                return Some(flow_boundary::narrow_non_nullish(self.interner, type_id));
            }
            // x holds the falsy result → keep only falsy types
            return Some(narrowing.narrow_to_falsy(type_id));
        }
        // For non-matching references, fall through to || handling below

        if operator == SyntaxKind::BarBarToken as u16
            || operator == SyntaxKind::BarBarEqualsToken as u16
        {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(self.union_logical_condition_branches(vec![left_true, right_true]));
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(right_false);
        }

        // ??= in condition context: `if (x ??= y)` narrows like `if (x ?? y)`
        // In the true branch, the result is non-nullish — either x was non-nullish,
        // or y was assigned and was truthy.
        if operator == SyntaxKind::QuestionQuestionEqualsToken as u16
            || operator == SyntaxKind::QuestionQuestionToken as u16
        {
            // For ?? / ??=, the narrowing on the reference follows truthiness semantics:
            // true branch: result was truthy (either left was non-null, or right was truthy)
            // false branch: both left and right were falsy
            // We treat this like || for condition narrowing since the truthiness patterns match.
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(self.union_logical_condition_branches(vec![left_true, right_true]));
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(right_false);
        }

        // Logical assignment operators (&&=, ||=, ??=) used as conditions:
        // `if (x &&= y)` / `if (x ||= y)` / `if (x ??= y)`
        // The flow graph already handles the assignment semantics (two branches
        // for short-circuit vs assignment, merged at a BRANCH_LABEL). When the
        // result is used as an `if` condition, apply truthiness narrowing:
        //
        // - LHS (x): On TRUE branch, x is guaranteed truthy for all three operators.
        // - RHS (y): For &&= only, the TRUE branch also guarantees y is truthy,
        //   because &&= evaluates y only when x is truthy, and the result IS y.
        //   For ||= and ??=, the TRUE branch doesn't guarantee y was evaluated.
        if crate::query_boundaries::operator_wrappers::is_logical_compound_assignment_operator(
            operator,
        ) {
            let matches_lhs = self.is_matching_reference(bin.left, target);
            let matches_rhs = operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                && self.is_matching_reference(bin.right, target);

            if matches_lhs || matches_rhs {
                let env_borrow;
                let narrowing = if let Some(env) = &self.type_environment {
                    env_borrow = env.borrow();
                    self.make_narrowing_context().with_resolver(&*env_borrow)
                } else {
                    self.make_narrowing_context()
                };
                return Some(narrowing.narrow_type(
                    type_id,
                    &TypeGuard::Truthy,
                    GuardSense::from(is_true_branch),
                ));
            }
        }

        None
    }

    /// Variant of `literal_type_from_node` for narrowing an
    /// `unknown`/`any` source. Falls back to primitive-intrinsic
    /// acceptance when the standard path returns `None`. Callers MUST
    /// only use this when the source is `unknown`/`any` and MUST NOT
    /// exclude the result in the false branch — primitive intrinsics
    /// are not unit types.
    pub(super) fn literal_type_from_node_for_unknown_target(
        &self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if let Some(t) = self.literal_type_from_node(idx) {
            return Some(t);
        }
        let idx = self.skip_parenthesized(idx);
        let node = self.arena.get(idx)?;
        if let Some(node_types) = self.node_types
            && let Some(&type_id) = node_types.get(&idx.0)
            && let Some(t) = is_unknown_narrowing_literal(self.interner, type_id)
        {
            return Some(t);
        }
        if let Some(type_id) = self.resolve_const_identifier_type(idx, node) {
            return is_unknown_narrowing_literal(self.interner, type_id);
        }
        if let Some(sym_id) = self.binder.resolve_identifier(self.arena, idx)
            && let Some(type_id) = self.annotation_comparison_type(sym_id)
        {
            return is_unknown_narrowing_literal(self.interner, type_id);
        }
        None
    }

    /// Variant of `literal_comparison` for narrowing an `unknown`/`any`
    /// source. Same caller obligations as
    /// [`Self::literal_type_from_node_for_unknown_target`].
    fn literal_comparison_for_unknown_target(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        if self.is_matching_reference(left, target) {
            return self.literal_type_from_node_for_unknown_target(right);
        }
        if self.is_matching_reference(right, target) {
            return self.literal_type_from_node_for_unknown_target(left);
        }
        None
    }
}
