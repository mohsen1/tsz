use super::FlowAnalyzer;
use super::flow_dp::FlowConditionDpMemos;
use crate::query_boundaries::common::{NarrowingContext, TypeGuard, TypeofKind};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::flow_analysis::{
    self as flow_query, empty_object_type, is_union_type, is_unit_type,
    is_unknown_narrowing_literal,
};
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{FlowNodeId, SymbolId, symbol_flags};
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(crate) struct BinaryFlowNarrowingContext<'m> {
    antecedent_id: FlowNodeId,
    allow_untyped_comparison_fallback: bool,
    dp_memos: Option<&'m mut FlowConditionDpMemos>,
}

impl<'a> FlowAnalyzer<'a> {
    fn narrow_to_falsy_via_flow_boundary(&self, type_id: TypeId) -> TypeId {
        let env_borrow = self.type_environment.as_ref().map(|env| env.borrow());
        flow_query::narrow_to_falsy(self.interner, env_borrow.as_deref(), type_id)
    }

    fn narrow_by_typeof_result_via_flow_boundary(
        &self,
        type_id: TypeId,
        typeof_result: &str,
        is_true_branch: bool,
    ) -> TypeId {
        let env_borrow = self.type_environment.as_ref().map(|env| env.borrow());
        flow_query::narrow_by_typeof_result(
            self.interner,
            env_borrow.as_deref(),
            type_id,
            typeof_result,
            is_true_branch,
        )
    }

    fn narrow_with_guard_via_flow_boundary(
        &self,
        narrowing: &NarrowingContext<'_>,
        type_id: TypeId,
        guard: &TypeGuard,
        is_true_branch: bool,
    ) -> TypeId {
        // Recover cross-file `Application(UnresolvedTypeName, args)` residue before
        // applying any guard (`typeof` / `instanceof` / `in` / predicate) so the
        // solver can read the union members. This is the central chokepoint for
        // the "solver-first" guard path, which equality/discriminant narrowing in
        // `narrow_by_binary_expr` does not reach. The `in`-operator guard in
        // particular targets the whole object (`'right' in a`); when `a` is a
        // cross-file discriminated-union element obtained through indexed access
        // (`as[0]`), its members stay `Application(UnresolvedTypeName, ..)` that
        // the solver-side `NarrowingContext` resolver (a `TypeEnvironment`) cannot
        // resolve, so member-presence filtering keeps the whole union and a later
        // `.right` access surfaces a false `TS2339`/`TS2322`. The `CheckerContext`
        // resolver walks the merged binder graph to recover the members. Gated
        // structurally on the residue (cached per `TypeId`), so resolved flow
        // types are untouched (#14756).
        let type_id = self.recover_unresolved_application_for_narrowing(type_id);
        flow_query::narrow_with_guard_in_context(narrowing, type_id, guard, is_true_branch)
    }

    fn union_logical_condition_branches(&self, types: Vec<TypeId>) -> TypeId {
        let mut members = Vec::with_capacity(types.len());
        let mut saw_reachable = false;

        for ty in types {
            if ty != TypeId::NEVER {
                saw_reachable = true;
            }
            if !members.contains(&ty) {
                members.push(ty);
            }
        }

        if saw_reachable {
            members.retain(|&ty| ty != TypeId::NEVER);
        }

        match members.len() {
            0 => TypeId::NEVER,
            1 => members[0],
            _ => self.interner.union_preserve_members(members),
        }
    }

    pub(crate) fn narrow_by_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        if matches!(type_id, TypeId::UNKNOWN | TypeId::ANY)
            && self.is_matching_reference(switch_expr, target)
        {
            let case_expr = self.skip_parenthesized(case_expr);
            if let Some(sym_id) = self.binder.resolve_identifier(self.arena, case_expr)
                && let Some(case_type) = self.annotation_comparison_type(sym_id)
            {
                return case_type;
            }
        }

        let binary = BinaryExprData {
            left: switch_expr,
            operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
            right: case_expr,
        };

        self.narrow_by_binary_expr(
            type_id,
            &binary,
            target,
            true,
            narrowing,
            BinaryFlowNarrowingContext {
                antecedent_id: FlowNodeId::NONE,
                allow_untyped_comparison_fallback: true,
                dp_memos: None,
            },
        )
    }

    pub(crate) fn narrow_by_switch_case_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        clause_idx: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
        has_fallthrough: bool,
    ) -> TypeId {
        let fallback =
            || self.narrow_by_switch_clause(type_id, switch_expr, case_expr, target, narrowing);
        let Some(case_block) = self
            .arena
            .get_extended(clause_idx)
            .map(|ext| ext.parent)
            .filter(|parent| parent.is_some())
        else {
            return fallback();
        };
        let Some(case_block_data) = self
            .arena
            .get(case_block)
            .and_then(|node| self.arena.get_block(node))
        else {
            return fallback();
        };

        if let Some(typeof_operand) = self.get_typeof_operand(self.skip_parenthesized(switch_expr))
            && (self.is_matching_reference(typeof_operand, target)
                || self.is_optional_chain_containing_target(typeof_operand, target))
        {
            if self.is_optional_chain_containing_target(typeof_operand, target) {
                if self.literal_string_from_node(case_expr) == Some("undefined") {
                    return type_id;
                }
                return flow_boundary::narrow_optional_chain(
                    self.interner.as_type_database(),
                    type_id,
                );
            }

            let mut narrowed = type_id;
            let mut saw_current = false;

            for &idx in &case_block_data.statements.nodes {
                let Some(clause_node) = self.arena.get(idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };

                if idx == clause_idx {
                    saw_current = true;
                    if clause.expression.is_none() {
                        break;
                    }
                    let Some(typeof_result) = self.literal_string_from_node(case_expr) else {
                        break;
                    };
                    return self.narrow_by_typeof_result_via_flow_boundary(
                        narrowed,
                        typeof_result,
                        true,
                    );
                }

                if clause.expression.is_none() {
                    continue;
                }

                let Some(typeof_result) = self.literal_string_from_node(clause.expression) else {
                    break;
                };

                narrowed =
                    self.narrow_by_typeof_result_via_flow_boundary(narrowed, typeof_result, false);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }

            if saw_current {
                return narrowed;
            }
        }

        let target_is_switch_expr = self.is_matching_reference(switch_expr, target);
        let discriminant_info = if !target_is_switch_expr {
            self.discriminant_property_info(switch_expr, target)
        } else {
            None
        };

        if target_is_switch_expr || discriminant_info.is_some() {
            if !has_fallthrough
                && let Some(current_lit_type) = self.cached_case_clause_literal_type(case_expr)
            {
                // When every clause in the switch is a pairwise-distinct literal
                // label (the common discriminated-union dispatch), the
                // "all earlier cases are distinct literals" predecessor check
                // below is satisfied for *every* clause. Decide it once per
                // switch (O(N) total, O(1) per clause via the memo) instead of
                // re-scanning all predecessors on each clause (O(N^2) overall).
                let previous_cases_are_distinct_literals =
                    if self.switch_case_block_all_distinct_literals(case_block) {
                        true
                    } else {
                        // Mixed/duplicate/non-literal switch: fall back to the
                        // exact per-clause scan. This is identical to the
                        // memoized result for the all-distinct case and only
                        // runs for switches the memo declines.
                        let mut distinct = true;
                        for &idx in &case_block_data.statements.nodes {
                            if idx == clause_idx {
                                break;
                            }
                            let Some(clause_node) = self.arena.get(idx) else {
                                continue;
                            };
                            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                                continue;
                            };
                            if clause.expression.is_none() {
                                distinct = false;
                                break;
                            }
                            let Some(prev_lit_type) =
                                self.cached_case_clause_literal_type(clause.expression)
                            else {
                                distinct = false;
                                break;
                            };
                            if prev_lit_type == current_lit_type {
                                distinct = false;
                                break;
                            }
                        }
                        distinct
                    };

                if previous_cases_are_distinct_literals {
                    return self.narrow_by_switch_clause(
                        type_id,
                        switch_expr,
                        case_expr,
                        target,
                        narrowing,
                    );
                }
            }

            let mut excluded_types: Vec<TypeId> = Vec::new();
            let mut all_literals = true;

            for &idx in &case_block_data.statements.nodes {
                if idx == clause_idx {
                    break;
                }
                let Some(clause_node) = self.arena.get(idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                if clause.expression.is_none() {
                    continue; // Skip default clause
                }

                if let Some(lit_type) = self.cached_case_clause_literal_type(clause.expression) {
                    excluded_types.push(lit_type);
                } else if let Some(node_types) = self.node_types
                    && let Some(&expr_type) = node_types.get(&clause.expression.0)
                {
                    excluded_types.push(expr_type);
                } else {
                    all_literals = false;
                    break;
                }
            }

            if all_literals {
                let narrowed = if excluded_types.is_empty() {
                    type_id
                } else if target_is_switch_expr {
                    flow_query::narrow_excluding_types_in_context(
                        narrowing,
                        type_id,
                        &excluded_types,
                    )
                } else if let Some((ref path, _)) = discriminant_info {
                    flow_query::narrow_by_excluding_discriminant_values_in_context(
                        narrowing,
                        type_id,
                        path,
                        &excluded_types,
                    )
                } else {
                    type_id
                };
                return self.narrow_by_switch_clause(
                    narrowed,
                    switch_expr,
                    case_expr,
                    target,
                    narrowing,
                );
            }
        }

        // Fall back to sequential narrowing for complex cases
        let mut narrowed = type_id;
        let mut saw_current = false;

        for &idx in &case_block_data.statements.nodes {
            let Some(clause_node) = self.arena.get(idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };

            if idx == clause_idx {
                saw_current = true;
                if clause.expression.is_some() {
                    narrowed = self.narrow_by_switch_clause(
                        narrowed,
                        switch_expr,
                        case_expr,
                        target,
                        narrowing,
                    );
                }
                break;
            }

            if clause.expression.is_none() {
                continue;
            }

            let binary = BinaryExprData {
                left: switch_expr,
                operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
                right: clause.expression,
            };
            narrowed = self.narrow_by_binary_expr(
                narrowed,
                &binary,
                target,
                false,
                narrowing,
                BinaryFlowNarrowingContext {
                    antecedent_id: FlowNodeId::NONE,
                    allow_untyped_comparison_fallback: true,
                    dp_memos: None,
                },
            );
        }

        if saw_current {
            narrowed
        } else {
            self.narrow_by_switch_clause(type_id, switch_expr, case_expr, target, narrowing)
        }
    }

    pub(crate) fn narrow_by_default_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_block: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return type_id;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return type_id;
        };

        // For switch(true), each case expression is an independent condition.
        // The default clause should be narrowed by applying the negation (false branch)
        // of every case expression, equivalent to an if-else chain's final else.
        if self.is_switch_true(switch_expr) {
            let mut narrowed = type_id;
            for &clause_idx in &case_block.statements.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                if clause.expression.is_none() {
                    continue; // Skip the default clause itself
                }
                // Apply the false branch of this case condition
                narrowed = self.narrow_type_by_condition(
                    narrowed,
                    clause.expression,
                    target,
                    false, // false branch = condition is not true
                    FlowNodeId::NONE,
                );
            }
            return narrowed;
        }

        // For `switch (typeof x)`, the default clause excludes runtime `typeof`
        // domains, not the string literal case expression types themselves.
        if let Some(typeof_operand) = self.get_typeof_operand(self.skip_parenthesized(switch_expr))
            && (self.is_matching_reference(typeof_operand, target)
                || self.is_optional_chain_containing_target(typeof_operand, target))
        {
            if self.is_optional_chain_containing_target(typeof_operand, target) {
                return type_id;
            }

            let mut narrowed = type_id;
            let mut applied = false;

            for &clause_idx in &case_block.statements.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                let case_expr = clause.expression;
                if case_expr.is_none() {
                    continue;
                }
                let Some(typeof_result) = self.literal_string_from_node(case_expr) else {
                    applied = false;
                    break;
                };

                applied = true;
                narrowed =
                    self.narrow_by_typeof_result_via_flow_boundary(narrowed, typeof_result, false);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }

            if applied {
                return narrowed;
            }
        }

        // Fast path: if this switch does not reference the target (directly or via discriminant
        // property access like switch(x.kind) when narrowing x), it cannot affect target's type.
        let target_is_switch_expr = self.is_matching_reference(switch_expr, target);
        let mut discriminant_info = None;

        if !target_is_switch_expr {
            // A discriminant path is only reported when it is accessed directly
            // on `target` (e.g. `switch (x.kind)` narrowing `x`); its absence
            // means this switch cannot affect `target`'s type.
            discriminant_info = self.discriminant_property_info(switch_expr, target);
            if discriminant_info.is_none() {
                return type_id;
            }
        }

        // Excluding finitely many case literals from broad primitive domains does not narrow.
        // Example: number minus {0, 1, 2, ...} is still number.
        if target_is_switch_expr
            && matches!(
                type_id,
                TypeId::NUMBER | TypeId::STRING | TypeId::BIGINT | TypeId::SYMBOL | TypeId::OBJECT
            )
        {
            return type_id;
        }

        // OPTIMIZATION: For direct switches on the target (switch(x) {...}) OR discriminant switches (switch(x.kind)),
        // collect all case types first and exclude them in a single O(N) pass.
        // This avoids O(N²) behavior when there are many case clauses.
        if target_is_switch_expr || discriminant_info.is_some() {
            // Collect all case expression types
            let mut excluded_types: Vec<TypeId> = Vec::new();
            for &clause_idx in &case_block.statements.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                if clause.expression.is_none() {
                    continue; // Skip default clause
                }

                // Try to get the type of the case expression
                // First try literal extraction (fast path for constants)
                if let Some(lit_type) = self.literal_type_from_node(clause.expression) {
                    excluded_types.push(lit_type);
                } else if let Some(node_types) = self.node_types {
                    // Fall back to computed node types
                    if let Some(&expr_type) = node_types.get(&clause.expression.0) {
                        excluded_types.push(expr_type);
                    }
                }
            }

            if !excluded_types.is_empty() {
                if target_is_switch_expr {
                    // Use batched narrowing for O(N) instead of O(N²)
                    return flow_query::narrow_excluding_types_in_context(
                        narrowing,
                        type_id,
                        &excluded_types,
                    );
                } else if let Some((path, is_optional)) = discriminant_info {
                    if is_optional && excluded_types.contains(&TypeId::UNDEFINED) {
                        return type_id;
                    }
                    // Use batched discriminant narrowing
                    return flow_query::narrow_by_excluding_discriminant_values_in_context(
                        narrowing,
                        type_id,
                        &path,
                        &excluded_types,
                    );
                }
            }
        }

        // Fall back to sequential narrowing for complex cases
        // (e.g., switch(x.kind) where we need property-based narrowing)
        let mut narrowed = type_id;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }

            let binary = BinaryExprData {
                left: switch_expr,
                operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
                right: clause.expression,
            };
            narrowed = self.narrow_by_binary_expr(
                narrowed,
                &binary,
                target,
                false,
                narrowing,
                BinaryFlowNarrowingContext {
                    antecedent_id: FlowNodeId::NONE,
                    allow_untyped_comparison_fallback: true,
                    dp_memos: None,
                },
            );
        }

        narrowed
    }

    pub(crate) fn narrow_type_by_condition_inner(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut AliasCycleTracker,
        mut dp_memos: Option<&mut FlowConditionDpMemos>,
    ) -> TypeId {
        let condition_idx = self.skip_parenthesized(condition_idx);
        let Some(cond_node) = self.arena.get(condition_idx) else {
            return type_id;
        };

        // Fast path: most binary operators never contribute to flow narrowing.
        // Skip context setup and guard extraction for those operators.
        if cond_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(cond_node)
            && !matches!(
                bin.operator_token,
                k if k == SyntaxKind::AmpersandAmpersandToken as u16
                    || k == SyntaxKind::BarBarToken as u16
                    || k == SyntaxKind::QuestionQuestionToken as u16
                    || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                    || k == SyntaxKind::BarBarEqualsToken as u16
                    || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                    || k == SyntaxKind::EqualsToken as u16
                    || k == SyntaxKind::InstanceOfKeyword as u16
                    || k == SyntaxKind::InKeyword as u16
                    || k == SyntaxKind::EqualsEqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsEqualsToken as u16
                    || k == SyntaxKind::EqualsEqualsToken as u16
                    || k == SyntaxKind::ExclamationEqualsToken as u16
            )
        {
            return type_id;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        // This enables proper resolution of Lazy types (type aliases) during narrowing
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            self.make_narrowing_context().with_resolver(&*env_borrow)
        } else {
            self.make_narrowing_context()
        };

        if type_id == TypeId::UNKNOWN
            && let Some(current_exclusion) =
                self.typeof_exclusion_for_condition(condition_idx, target, is_true_branch)
        {
            let prior_exclusions = if let Some(memos) = dp_memos.as_deref_mut() {
                self.antecedent_typeof_exclusion_mask_with_memo(
                    antecedent_id,
                    target,
                    &mut memos.typeof_exclusions,
                )
            } else {
                self.antecedent_typeof_exclusion_mask(antecedent_id, target)
            };
            let exclusions = prior_exclusions | Self::typeof_exclusion_bit(current_exclusion);
            if exclusions == Self::ALL_TYPEOF_EXCLUSIONS {
                return empty_object_type(self.interner);
            }
        }

        if cond_node.kind == SyntaxKind::Identifier as u16
            // Direct truthiness checks (`if (x)`, `x && ...`, `x! && ...`) must narrow
            // the reference itself. Alias recursion is only for `const alias = guard`.
            && !self.is_matching_reference(condition_idx, target)
            && let Some((sym_id, initializer)) = self.const_condition_initializer(condition_idx)
            && !visited_aliases.contains(&sym_id)
        {
            // Before applying alias narrowing, check if the target reference
            // (or its base) has been assigned to since the alias was declared.
            // If so, the alias condition may not reflect the current state of
            // the reference, so we skip alias narrowing.
            //
            // Example:
            //   const isString = typeof obj.x === 'string';
            //   obj = { x: 42 };  // obj reassigned
            //   if (isString) {
            //       obj.x;  // Should NOT be narrowed to string
            //   }
            if self.is_alias_reference_mutated(sym_id, target, antecedent_id) {
                return type_id;
            }

            visited_aliases.push(sym_id);
            let narrowed = self.narrow_type_by_condition_inner(
                type_id,
                initializer,
                target,
                is_true_branch,
                antecedent_id,
                visited_aliases,
                None,
            );
            visited_aliases.pop(sym_id);
            return narrowed;
        }

        match cond_node.kind {
            // typeof x === "string", x instanceof Class, "prop" in x, etc.
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(cond_node) {
                    // Handle logical operators (&&, ||) with special recursion
                    if let Some(narrowed) = self.narrow_by_logical_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }

                    // Handle boolean comparison: `expr === true`, `expr === false`,
                    // `expr !== true`, `expr !== false`, and reversed variants.
                    // TypeScript treats comparing a type guard result to true/false as
                    // preserving/inverting the type guard:
                    //   if (x instanceof Error === false) { ... }
                    //   if (isString(x) === true) { ... }
                    if let Some(narrowed) = self.narrow_by_boolean_comparison(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }

                    // Fast-path: avoid expensive generic guard extraction when the
                    // comparison does not directly target this reference.
                    //
                    // Example hot path:
                    //   if (e.kind === "type42") { ... } while narrowing `e`
                    //
                    // `extract_type_guard` first targets `e.kind`, which won't match `e`,
                    // then we still do full binary narrowing below. Skip the extraction in
                    // that common mismatch case and go straight to `narrow_by_binary_expr`.
                    let maybe_direct_guard_target = self.is_matching_reference(bin.left, target)
                        || self.is_matching_reference(bin.right, target)
                        || self
                            .get_constructor_property_base(bin.left)
                            .is_some_and(|base| self.is_matching_reference(base, target))
                        || self
                            .get_constructor_property_base(bin.right)
                            .is_some_and(|base| self.is_matching_reference(base, target))
                        || self.is_typeof_target(bin.left, target)
                        || self.is_typeof_target(bin.right, target)
                        || self.is_optional_chain_containing_target(bin.left, target)
                        || self.is_optional_chain_containing_target(bin.right, target);

                    // CRITICAL: Use Solver-First architecture for direct binary guards
                    // when the guard target can actually match our reference.
                    if maybe_direct_guard_target
                        && let Some((guard, guard_target, is_optional)) =
                            self.extract_type_guard(condition_idx)
                    {
                        let effective_sense = if bin.operator_token
                            == SyntaxKind::ExclamationEqualsEqualsToken as u16
                            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                        {
                            !is_true_branch
                        } else {
                            is_true_branch
                        };
                        let short_circuit_can_satisfy_guard = is_optional
                            && effective_sense
                            && self.optional_chain_guard_can_be_satisfied_by_short_circuit(&guard);
                        // Check if the guard applies to our target reference
                        if self.is_matching_reference(guard_target, target)
                            && !short_circuit_can_satisfy_guard
                        {
                            // CRITICAL: Invert sense for inequality operators (!== and !=)
                            // This applies to ALL guards, not just typeof
                            // For `x !== "string"` or `x.kind !== "circle"`, the true branch should EXCLUDE
                            // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                            let result = self.narrow_with_guard_via_flow_boundary(
                                &narrowing,
                                type_id,
                                &guard,
                                effective_sense,
                            );
                            if effective_sense
                                && matches!(guard, TypeGuard::Typeof(TypeofKind::Object))
                                && if let Some(memos) = dp_memos.as_mut() {
                                    self.antecedent_chain_excludes_null_for_target_with_memo(
                                        antecedent_id,
                                        target,
                                        &mut memos.null_exclusions,
                                    )
                                } else {
                                    self.antecedent_chain_excludes_null_for_target(
                                        antecedent_id,
                                        target,
                                    )
                                }
                            {
                                return flow_query::narrow_excluding_type_in_context(
                                    &narrowing,
                                    result,
                                    TypeId::NULL,
                                );
                            }
                            return result;
                        }

                        // Optional-chain intermediate narrowing for binary expressions.
                        // Fall through to binary narrowing for additional discriminant effects.
                        if self.contains_optional_chain(guard_target)
                            && self.is_optional_chain_prefix(guard_target, target)
                        {
                            let effective_sense = if bin.operator_token
                                == SyntaxKind::ExclamationEqualsEqualsToken as u16
                                || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                            {
                                !is_true_branch
                            } else {
                                is_true_branch
                            };
                            let chain_completed = match &guard {
                                TypeGuard::NullishEquality => !effective_sense,
                                _ => effective_sense,
                            };
                            let short_circuit_can_satisfy_guard = effective_sense
                                && self
                                    .optional_chain_guard_can_be_satisfied_by_short_circuit(&guard);
                            if chain_completed && !short_circuit_can_satisfy_guard {
                                let narrowed = flow_boundary::narrow_optional_chain(
                                    self.interner.as_type_database(),
                                    type_id,
                                );
                                // Fall through to narrow_by_binary_expr with the pre-narrowed type
                                return self.narrow_by_binary_expr(
                                    narrowed,
                                    bin,
                                    target,
                                    is_true_branch,
                                    &narrowing,
                                    BinaryFlowNarrowingContext {
                                        antecedent_id,
                                        allow_untyped_comparison_fallback: false,
                                        dp_memos: dp_memos.as_deref_mut(),
                                    },
                                );
                            }
                        }
                    }

                    // CRITICAL: Try bidirectional narrowing for x === y where both are references
                    // This handles cases that don't match traditional type guard patterns
                    // Example: if (x === y) { x } should narrow x based on y's type
                    let narrowed = self.narrow_by_binary_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        &narrowing,
                        BinaryFlowNarrowingContext {
                            antecedent_id,
                            allow_untyped_comparison_fallback: false,
                            dp_memos,
                        },
                    );
                    return narrowed;
                }
            }

            // User-defined type guards: isString(x), obj.isString(), assertsIs(x), etc.
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(narrowed) = self.narrow_call_expression_condition(
                    type_id,
                    cond_node,
                    condition_idx,
                    target,
                    is_true_branch,
                    &narrowing,
                ) {
                    return narrowed;
                }
            }

            // Prefix unary: !x
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(cond_node) {
                    // !x inverts the narrowing
                    if unary.operator == SyntaxKind::ExclamationToken as u16 {
                        return self.narrow_type_by_condition_inner(
                            type_id,
                            unary.operand,
                            target,
                            !is_true_branch,
                            antecedent_id,
                            visited_aliases,
                            None,
                        );
                    }
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(cond_node) {
                    // Handle optional chaining truthiness: `if (a?.b?.c)`.
                    // A truthy optional-chain result means every chain prefix that can short-circuit
                    // must be non-nullish on this branch.
                    if self.access_expr_is_optional_chain(cond_node, access)
                        && is_true_branch
                        && (self.is_matching_reference(access.expression, target)
                            || self.is_optional_chain_prefix(condition_idx, target))
                    {
                        return flow_boundary::narrow_optional_chain(
                            self.interner.as_type_database(),
                            type_id,
                        );
                    }
                }
                // When the access is the left operand of a `??`/`??=`, the
                // branches gate on the value being *nullish*, not *falsy* — tsc
                // routes such a condition through `narrowTypeByOptionality`
                // instead of `narrowTypeByTruthiness`. This matters for a
                // discriminated-union receiver: `r.fullPath ?? r.from` reaches
                // `r.from` on the branch where `r.fullPath` is nullish, which
                // discriminates `r` to the member whose `fullPath` is `undefined`
                // (a falsy-but-non-nullish `string` value must NOT keep that
                // member). Truthiness narrowing would keep both members and leak
                // `undefined` into the result type (false TS2322).
                let coalesce_left = self.condition_is_nullish_coalesce_left(condition_idx);

                // Handle discriminant narrowing for properties.
                // For `if (x.flag)` where x is a discriminated union like
                // `{flag: "hello"; data: string} | {flag: ""; data: number}`,
                // narrow x based on whether `flag` is truthy or falsy (or, for a
                // `??` left, non-nullish or nullish).
                if let Some(property_path) = self.discriminant_property(condition_idx, target) {
                    let narrowed = if coalesce_left {
                        flow_query::narrow_by_property_nullishness_in_context(
                            &narrowing,
                            type_id,
                            &property_path,
                            is_true_branch,
                        )
                    } else {
                        flow_query::narrow_by_property_truthiness_in_context(
                            &narrowing,
                            type_id,
                            &property_path,
                            is_true_branch,
                        )
                    };
                    // For union types, NEVER means all members were filtered out
                    // (no member has a truthy/falsy property), which is valid.
                    // For non-union types (class, mapped, generic), NEVER often
                    // means the solver couldn't resolve the property (e.g.
                    // Readonly<P> where P is a type parameter).  tsc does not
                    // narrow non-union base types by property truthiness, so
                    // fall through instead of collapsing to NEVER.
                    if narrowed != TypeId::NEVER || is_union_type(self.interner, type_id) {
                        return narrowed;
                    }
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                let condition_ref = self.arena.skip_parenthesized_and_assertions(condition_idx);
                if self.is_matching_reference(condition_ref, target) {
                    if coalesce_left {
                        // `??` left: keep only the non-nullish part (true/short-
                        // circuit branch) or the nullish part (false/right
                        // branch), matching tsc's `narrowTypeByOptionality`
                        // matching-reference arm (`NEUndefinedOrNull` /
                        // `EQUndefinedOrNull`).
                        let (non_nullish, nullish) = tsz_solver::narrowing::split_nullish_type(
                            self.interner.as_type_database(),
                            type_id,
                        );
                        let part = if is_true_branch { non_nullish } else { nullish };
                        return part.unwrap_or(TypeId::NEVER);
                    }
                    if is_true_branch {
                        // Full truthiness narrowing (same as a symbol reference):
                        // tsc's narrowTypeByTruthiness does not special-case
                        // member-access references. Besides stripping null/undefined
                        // it also narrows `unknown` to `{}` and `boolean` to `true`,
                        // which a bare non-nullish observation misses (witnessed by
                        // ts-rest `standard-schema-utils.ts`: `if (result.value)`
                        // then `{ ...result.value }` must spread a `{}`, not the
                        // un-narrowed `unknown`).
                        return flow_query::narrow_to_truthy_in_context(
                            &narrowing,
                            type_id,
                            is_true_branch,
                        );
                    }
                    // False branch - keep only falsy types (use Solver for NaN handling)
                    return self.narrow_to_falsy_via_flow_boundary(type_id);
                }
            }

            // Truthiness check: if (x)
            // Use Solver-First architecture: delegate truthiness to the flow boundary.
            _ => {
                let condition_ref = self.arena.skip_parenthesized_and_assertions(condition_idx);
                let matches = self.is_matching_reference(condition_ref, target);
                if matches {
                    // A bare-identifier `??`/`??=` left gates the right operand
                    // on nullishness, not truthiness (tsc's
                    // `narrowTypeByOptionality` matching-reference arm). e.g.
                    // `x ?? x.foo` with `x: T | undefined` sees `x: T` only when
                    // `x` is non-nullish.
                    if self.condition_is_nullish_coalesce_left(condition_idx) {
                        let (non_nullish, nullish) = tsz_solver::narrowing::split_nullish_type(
                            self.interner.as_type_database(),
                            type_id,
                        );
                        let part = if is_true_branch { non_nullish } else { nullish };
                        return part.unwrap_or(TypeId::NEVER);
                    }
                    return flow_query::narrow_to_truthy_in_context(
                        &narrowing,
                        type_id,
                        is_true_branch,
                    );
                }

                if let Some((base, prop_names)) = self.binding_element_property_alias(condition_ref)
                    && self.is_matching_reference(base, target)
                    && let Some(alias_sym_id) =
                        self.binder.resolve_identifier(self.arena, condition_ref)
                    && !self.is_alias_reference_mutated(alias_sym_id, target, antecedent_id)
                {
                    let narrowed = flow_query::narrow_by_property_truthiness_in_context(
                        &narrowing,
                        type_id,
                        &prop_names,
                        is_true_branch,
                    );
                    if narrowed != TypeId::NEVER || is_union_type(self.interner, type_id) {
                        return narrowed;
                    }
                }
            }
        }

        type_id
    }

    /// Check if a node is a property access or element access expression.
    ///
    /// This is used to prevent discriminant guards from being applied to property
    /// access results. Discriminant guards (like `obj.kind === "a"`) should only
    /// narrow the base object (`obj`), not property access results (like `obj.value`).
    fn is_property_or_element_access(&self, node: NodeIndex) -> bool {
        let node = self.arena.skip_parenthesized(node);
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node_data.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    /// If `node` is a direct `*.constructor` access, return the object expression
    /// on the left side of the access chain.
    pub(super) fn get_constructor_property_base(&self, node: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.skip_parenthesized_and_assertions(node);
        let node_data = self.arena.get(node)?;
        if node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node_data)?;
            let is_constructor_prop = self
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|name| name.escaped_text.as_str() == "constructor");
            if is_constructor_prop {
                return Some(access.expression);
            }
        } else if node_data.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node_data)?;
            if self.literal_string_from_node(access.name_or_argument) == Some("constructor") {
                return Some(access.expression);
            }
        }
        None
    }

    pub(crate) fn const_condition_initializer(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(SymbolId, NodeIndex)> {
        let sym_id = self.binder.resolve_identifier(self.arena, ident_idx)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }
        let mut decl_idx = symbol.primary_declaration()?;
        let mut decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.arena.get_extended(decl_idx)?.parent;
            decl_node = self.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if decl.initializer.is_none() {
            return None;
        }
        // tsc does not treat const variables with explicit type annotations as
        // narrowing aliases (e.g., `const isFoo: boolean = obj.kind === 'foo'`).
        // The type annotation widens the variable's type, breaking the link
        // between the condition expression and the original discriminant.
        if decl.type_annotation.is_some() {
            return None;
        }
        Some((sym_id, decl.initializer))
    }

    pub(crate) fn is_const_variable_declaration(&self, decl_idx: NodeIndex) -> bool {
        self.arena.is_const_variable_declaration(decl_idx)
    }

    /// Check if a symbol is const (immutable) vs mutable (let/var).
    ///
    /// This is used for loop widening: const variables preserve narrowing through loops,
    /// while mutable variables are widened to the declared type to account for mutations.
    pub(crate) fn is_const_symbol(&self, sym_id: SymbolId) -> bool {
        let symbol = match self.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false, // Assume mutable if we can't determine
        };

        let mut decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false; // Assume mutable if no declaration
        }

        // Walk up the parent chain to find the enclosing VARIABLE_DECLARATION.
        // Destructured bindings have value_declaration set to BINDING_ELEMENT or
        // Identifier inside a BINDING_ELEMENT, not the VARIABLE_DECLARATION itself.
        //
        // The walk must stop at scope-creating nodes (parameters, functions,
        // accessors, classes, source file). Otherwise a destructured parameter
        // inside a `const`-declared function — e.g.
        // `const fn = ({x}) => …` — walks past PARAMETER → ARROW_FUNCTION up
        // to VARIABLE_DECLARATION(fn) and reports the parameter as const.
        loop {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                // For simple identifiers, walk up to the parent (usually VARIABLE_DECLARATION)
                let Some(ext) = self.arena.get_extended(decl_idx) else {
                    return false;
                };
                if ext.parent.is_none() {
                    return false;
                }
                decl_idx = ext.parent;
                continue;
            }
            if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return self.arena.is_const_variable_declaration(decl_idx);
            }
            if matches!(
                decl_node.kind,
                syntax_kind_ext::PARAMETER
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::SOURCE_FILE
            ) {
                return false;
            }
            // BINDING_ELEMENT, BINDING_PATTERN, etc. — walk up
            let Some(ext) = self.arena.get_extended(decl_idx) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            decl_idx = ext.parent;
        }
    }

    /// Narrow type based on a binary expression (===, !==, typeof checks, etc.)
    ///
    /// `dp_memos` carries the per-`check_flow` flow-condition DP scratch when
    /// this call is reachable from the top-level traversal worklist; callers
    /// outside that path pass `None` and the null-exclusion check below falls
    /// back to a one-shot memo, exactly like the threaded sites in
    /// `narrow_type_by_condition_inner`.
    pub(crate) fn narrow_by_binary_expr(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        mut flow_context: BinaryFlowNarrowingContext<'_>,
    ) -> TypeId {
        let operator = bin.operator_token;

        // Unwrap assignment expressions: if (flag = (x instanceof Foo)) should narrow based on RHS
        // The assignment itself doesn't provide narrowing, but its RHS might
        if operator == SyntaxKind::EqualsToken as u16 {
            // When the assignment LHS *is* the narrowed reference, `(s = rhs)` used
            // as a condition observes the truthiness of the value just assigned to
            // `s`.  tsc narrows the assigned target by truthiness in the branches
            // (true → strip null/undefined, false → keep falsy), exactly like a
            // bare reference observation.  Mirror the matching-reference arms.
            if self.is_matching_reference(bin.left, target) {
                if is_true_branch {
                    return flow_query::narrow_to_truthy_in_context(
                        narrowing,
                        type_id,
                        is_true_branch,
                    );
                }
                return self.narrow_to_falsy_via_flow_boundary(type_id);
            }
            if self.arena.get(bin.right).is_some() {
                // Recursively narrow based on the RHS expression
                let mut visited = AliasCycleTracker::new();
                return self.narrow_type_by_condition_inner(
                    type_id,
                    bin.right,
                    target,
                    is_true_branch,
                    flow_context.antecedent_id,
                    &mut visited,
                    flow_context.dp_memos,
                );
            }
            return type_id;
        }

        if operator == SyntaxKind::InstanceOfKeyword as u16 {
            return self.narrow_by_instanceof(type_id, bin, target, is_true_branch);
        }

        if operator == SyntaxKind::InKeyword as u16 {
            return self.narrow_by_in_operator(type_id, bin, target, is_true_branch);
        }

        let Some(comparison) =
            crate::query_boundaries::operator_wrappers::classify_equality_comparison(operator)
        else {
            return type_id;
        };
        let is_strict = comparison.is_strict;

        let effective_truth = comparison.effective_truth(is_true_branch);
        // Recover any `Application(UnresolvedTypeName(name), args)` residue left by
        // cross-file alias lowering before equality/discriminant narrowing reads
        // the members. An imported `type Either<E, A> = Left<E> | Right<A>` lowers
        // its alias body before the merged binder exists, so the inner `Left`/
        // `Right` refs stay unresolved; the solver-side `NarrowingContext`
        // resolver (a `TypeEnvironment`) only knows names it was explicitly seeded
        // with and cannot resolve such imports, so a guard like
        // `a._tag === 'Right'` fails to read `_tag` and keeps the whole union,
        // surfacing a false `TS2339` on a later `.right`. The `CheckerContext`
        // resolver walks the merged binder graph and recovers the members, so
        // discriminant filtering then works (benefits any cross-file discriminated
        // union). Gated structurally on the residue, so resolved flow types are
        // untouched. The `in`/`instanceof`/`typeof`/predicate guard forms take the
        // solver-first guard path and recover at
        // `narrow_with_guard_via_flow_boundary` instead (#14756).
        let mut type_id = self.recover_unresolved_application_for_narrowing(type_id);

        // Optional-chain equality transport:
        // when an optional-chain comparison is known-equal to a value that cannot
        // come from short-circuiting, the chain base/prefix is non-nullish.
        if self.optional_chain_comparison_proves_non_nullish(
            bin,
            target,
            is_strict,
            effective_truth,
        ) {
            type_id =
                flow_boundary::narrow_optional_chain(self.interner.as_type_database(), type_id);
        }

        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            // Use unified narrow_type API with TypeGuard::Typeof for both branches
            if let Some(typeof_kind) = TypeofKind::parse(type_name) {
                // Route catch-variable typeof base through the flow observation
                // boundary (NORTH_STAR §3.3 / §22).  In the flow analyzer,
                // type_id is already the declared catch base (`any`/`unknown`).
                let is_catch_var = self
                    .binder
                    .resolve_identifier(self.arena, target)
                    .is_some_and(|sid| self.is_unknown_catch_variable_symbol(sid));
                let typeof_base_type =
                    flow_boundary::catch_variable_typeof_base_from_flow(type_id, is_catch_var);
                let narrowed = self.narrow_with_guard_via_flow_boundary(
                    narrowing,
                    typeof_base_type,
                    &TypeGuard::Typeof(typeof_kind),
                    effective_truth,
                );
                if effective_truth
                    && typeof_kind == TypeofKind::Object
                    && if let Some(memos) = flow_context.dp_memos.as_deref_mut() {
                        self.antecedent_chain_excludes_null_for_target_with_memo(
                            flow_context.antecedent_id,
                            target,
                            &mut memos.null_exclusions,
                        )
                    } else {
                        self.antecedent_chain_excludes_null_for_target(
                            flow_context.antecedent_id,
                            target,
                        )
                    }
                {
                    return flow_query::narrow_excluding_type_in_context(
                        narrowing,
                        narrowed,
                        TypeId::NULL,
                    );
                }
                return narrowed;
            }
            // Non-standard typeof string (e.g. "Object", host-defined types).
            // TypeScript behavior:
            //   - true branch: remove primitive types (string, number, boolean)
            //     because if typeof returned a non-standard string, x can't be primitive
            //   - false branch: keep only primitive types (typeof can only ever
            //     return standard strings at runtime, so the condition is always
            //     false; the complement narrows to primitives)
            if effective_truth {
                return flow_query::narrow_excluding_types_in_context(
                    narrowing,
                    type_id,
                    &[TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN],
                );
            } else {
                // Collect the primitive members from the source type
                let s = self.narrow_by_typeof_result_via_flow_boundary(type_id, "string", true);
                let n = self.narrow_by_typeof_result_via_flow_boundary(type_id, "number", true);
                let b = self.narrow_by_typeof_result_via_flow_boundary(type_id, "boolean", true);
                let mut parts = Vec::new();
                if s != TypeId::NEVER {
                    parts.push(s);
                }
                if n != TypeId::NEVER {
                    parts.push(n);
                }
                if b != TypeId::NEVER {
                    parts.push(b);
                }
                return if parts.is_empty() {
                    type_id // no primitives in the type, no narrowing
                } else if parts.len() == 1 {
                    parts[0]
                } else {
                    crate::query_boundaries::flow_analysis::union_types(self.interner, parts)
                };
            }
        }

        // typeof-based discriminant narrowing for unions:
        // `typeof a.error === 'undefined'` narrows `a` by filtering union members
        // where the `error` property is (or isn't) undefined.
        if is_strict
            && let Some((property_path, is_optional, typeof_literal)) =
                self.typeof_discriminant_path(bin.left, bin.right, target)
        {
            let discriminant_type = match typeof_literal {
                "undefined" => TypeId::UNDEFINED,
                _ => {
                    // For non-undefined typeof checks (e.g., typeof x.y === "string"),
                    // we can't use discriminant narrowing directly.
                    TypeId::NEVER
                }
            };
            if discriminant_type != TypeId::NEVER {
                // Optional-chain discriminants compared to `undefined` are special:
                // `obj?.prop` can be `undefined` due to nullish short-circuit on the base.
                // On truthy `===/!== undefined` paths, preserve the incoming type.
                // Applying discriminant narrowing here can incorrectly collapse to `never`.
                if is_optional && discriminant_type == TypeId::UNDEFINED && effective_truth {
                    return type_id;
                }
                return flow_query::narrow_by_discriminant_for_type_in_context(
                    narrowing,
                    type_id,
                    &property_path,
                    discriminant_type,
                    effective_truth,
                );
            }
        }

        if let Some(nullish) = self.nullish_comparison(bin.left, bin.right, target) {
            if is_strict {
                if effective_truth {
                    return nullish;
                }
                return flow_query::narrow_excluding_type_in_context(narrowing, type_id, nullish);
            }

            let nullish_union = self.interner.union2(TypeId::NULL, TypeId::UNDEFINED);
            if effective_truth {
                return nullish_union;
            }

            return flow_boundary::narrow_optional_chain(self.interner.as_type_database(), type_id);
        }

        // Discriminant and literal comparisons apply to all four equality operators
        // (`===`, `!==`, `==`, `!=`). The null/undefined loose equality cases are
        // handled by nullish_comparison above, so by this point any loose comparison
        // involves string/number literals where loose and strict (in)equality narrow
        // identically. Earlier we returned for non-equality operators, so reaching
        // this point means we're guaranteed to have an equality comparison; the
        // `is_strict || is_equals` gate previously here only excluded `!=` due to an
        // asymmetric truth table (false || false), so let every equality operator
        // through.
        {
            if let Some((property_path, literal_type, is_optional, base)) =
                self.discriminant_comparison(bin.left, bin.right, target)
            {
                // Determine whether we should apply discriminant narrowing.
                //
                // Two scenarios for skipping:
                // 1. INDIRECT property access: target is a sub-property of base
                //    (e.g., `if (obj.kind === "a") { obj.kind; }` — target=`obj.kind`, base=`obj`)
                //    Literal comparison handles this; discriminant narrowing would yield NEVER.
                // 2. ALIASED + MUTABLE: target is a let-bound variable with an aliased discriminant
                //    (e.g., aliased condition on a reassignable variable)
                //
                // IMPORTANT: for DIRECT discriminant narrowing where base == target,
                // we MUST allow it even when target is a property access.
                // e.g., `if (this.test.type === "a") { this.test.name; }` — target=`this.test`
                // must be narrowable since base == target == `this.test`.
                let is_aliased_discriminant = !self.is_matching_reference(base, target);
                let is_property_access = self.is_property_or_element_access(target);
                let is_mutable = self.is_mutable_variable(target);

                // Skip only when: (aliased AND (indirect property access OR mutable target))
                // Direct discriminant (is_aliased_discriminant = false) always applies.
                if !(is_aliased_discriminant && (is_property_access || is_mutable)) {
                    let mut base_type = type_id;
                    let optional_undefined_truthy =
                        is_optional && literal_type == TypeId::UNDEFINED && effective_truth;
                    if is_optional && effective_truth && !optional_undefined_truthy {
                        base_type = flow_boundary::narrow_optional_chain(
                            self.interner.as_type_database(),
                            base_type,
                        );
                    }
                    let narrowed = flow_query::narrow_by_discriminant_for_type_in_context(
                        narrowing,
                        base_type,
                        &property_path,
                        literal_type,
                        effective_truth,
                    );
                    if optional_undefined_truthy {
                        return type_id;
                    }
                    return narrowed;
                }
                // Skipped: indirect property access or aliased let-bound variable.
                // The type will be computed from the already-narrowed base or via literal comparison.
            }

            // Loose equality (==) does not narrow non-nullish literals on top types (unknown/any).
            // Nullish loose equality (x == null) is handled above as NullishEquality.
            if (is_strict || !type_id.is_any_or_unknown())
                && let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target)
            {
                if effective_truth {
                    let narrowed =
                        flow_query::narrow_to_type_in_context(narrowing, type_id, literal_type);
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    if flow_query::literal_assignable_to_in_context(
                        narrowing,
                        literal_type,
                        type_id,
                    ) {
                        return literal_type;
                    }
                    return TypeId::NEVER;
                }
                if !is_unit_type(self.interner, literal_type) {
                    return type_id;
                }
                return flow_query::narrow_excluding_type_in_context(
                    narrowing,
                    type_id,
                    literal_type,
                );
            }

            // Equality narrowing of `unknown` / `any` against a primitive-
            // intrinsic-typed value (e.g. `if (u === aString)` where
            // `u: unknown` and `aString: string`). tsc narrows `u` to
            // `string` in the true branch and leaves `u` unchanged in the
            // false branch — primitive intrinsics are NOT unit types, so
            // they cannot exclude members of a union source.
            //
            // This path is intentionally limited to `unknown` / `any`
            // sources; for any other source, primitive-intrinsic
            // comparands are not narrowing literals (see
            // `is_narrowing_literal`).
            if is_strict
                && type_id.is_any_or_unknown()
                && let Some(literal_type) =
                    self.literal_comparison_for_unknown_target(bin.left, bin.right, target)
            {
                if effective_truth {
                    return flow_query::narrow_to_type_in_context(narrowing, type_id, literal_type);
                }
                return type_id;
            }
        }

        // Bidirectional narrowing: x === y where both are references
        // This handles cases like: if (x === y) { ... }
        // where both x and y are variables (not just literals)
        if is_strict {
            // Check if target is on the left side (x === y, target is x)
            if self.is_matching_reference(bin.left, target) {
                // We need the type of the RIGHT side (y)
                if let Some(right_type) = self.flow_comparison_type(
                    bin.right,
                    flow_context.antecedent_id,
                    flow_context.allow_untyped_comparison_fallback,
                ) {
                    if effective_truth {
                        if flow_context.allow_untyped_comparison_fallback
                            && matches!(type_id, TypeId::UNKNOWN | TypeId::ANY)
                        {
                            return right_type;
                        }
                        return self.narrow_with_guard_via_flow_boundary(
                            narrowing,
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            true,
                        );
                    } else if is_unit_type(self.interner, right_type) {
                        return self.narrow_with_guard_via_flow_boundary(
                            narrowing,
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            false,
                        );
                    }
                }
            }

            // Check if target is on the right side (y === x, target is x)
            if self.is_matching_reference(bin.right, target) {
                // We need the type of the LEFT side (y)
                if let Some(left_type) = self.flow_comparison_type(
                    bin.left,
                    flow_context.antecedent_id,
                    flow_context.allow_untyped_comparison_fallback,
                ) {
                    if effective_truth {
                        if flow_context.allow_untyped_comparison_fallback
                            && matches!(type_id, TypeId::UNKNOWN | TypeId::ANY)
                        {
                            return left_type;
                        }
                        return self.narrow_with_guard_via_flow_boundary(
                            narrowing,
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            true,
                        );
                    } else if is_unit_type(self.interner, left_type) {
                        return self.narrow_with_guard_via_flow_boundary(
                            narrowing,
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            false,
                        );
                    }
                }
            }
        }

        type_id
    }

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
        let comparison = crate::query_boundaries::operator_wrappers::classify_equality_comparison(
            bin.operator_token,
        )?;

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
        let is_negated = !comparison.is_equals;
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
            None,
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
                    None,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_true,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                    None,
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
                None,
            );
            let left_true = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                true,
                antecedent_id,
                visited_aliases,
                None,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_true,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
                None,
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
            if is_true_branch {
                // x holds the truthy result → remove null/undefined
                return Some(flow_boundary::narrow_non_nullish(self.interner, type_id));
            }
            // x holds the falsy result → keep only falsy types
            return Some(self.narrow_to_falsy_via_flow_boundary(type_id));
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
                    None,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                    None,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                    None,
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
                None,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
                None,
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
                    None,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                    None,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                    None,
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
                None,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
                None,
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
                return Some(flow_query::narrow_to_truthy_in_context(
                    &narrowing,
                    type_id,
                    is_true_branch,
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
