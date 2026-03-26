//! Condition-based type narrowing for `FlowAnalyzer`.
//!
//! Handles switch clause narrowing, binary/logical expression narrowing,
//! typeof/instanceof/in guards, and boolean comparison narrowing.

use super::FlowAnalyzer;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::{common::union_members, flow_analysis::is_unit_type};
use tsz_binder::{FlowNodeId, SymbolId, symbol_flags};
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{GuardSense, NarrowingContext, TypeGuard, TypeId, TypeofKind};

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn narrow_by_switch_true_case_clause(
        &self,
        type_id: TypeId,
        case_block: NodeIndex,
        clause_idx: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return self.narrow_type_by_condition(
                type_id,
                case_expr,
                target,
                true,
                FlowNodeId::NONE,
            );
        };
        let Some(case_block_data) = self.arena.get_block(case_block_node) else {
            return self.narrow_type_by_condition(
                type_id,
                case_expr,
                target,
                true,
                FlowNodeId::NONE,
            );
        };

        // For switch(true), direct dispatch into case N requires:
        // - every preceding case condition is false
        // - current case condition is true
        // Fallthrough paths are unioned separately by the switch-clause handler.
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
                    narrowed = self.narrow_type_by_condition(
                        narrowed,
                        case_expr,
                        target,
                        true,
                        FlowNodeId::NONE,
                    );
                }
                break;
            }

            if clause.expression.is_some() {
                narrowed = self.narrow_type_by_condition(
                    narrowed,
                    clause.expression,
                    target,
                    false,
                    FlowNodeId::NONE,
                );
            }
        }

        if saw_current {
            narrowed
        } else {
            self.narrow_type_by_condition(type_id, case_expr, target, true, FlowNodeId::NONE)
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
        let binary = BinaryExprData {
            left: switch_expr,
            operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
            right: case_expr,
        };

        self.narrow_by_binary_expr(type_id, &binary, target, true, narrowing, FlowNodeId::NONE)
    }

    pub(crate) fn narrow_by_switch_case_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_block: NodeIndex,
        clause_idx: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return self.narrow_by_switch_clause(
                type_id,
                switch_expr,
                case_expr,
                target,
                narrowing,
            );
        };
        let Some(case_block_data) = self.arena.get_block(case_block_node) else {
            return self.narrow_by_switch_clause(
                type_id,
                switch_expr,
                case_expr,
                target,
                narrowing,
            );
        };

        if let Some(typeof_operand) = self.get_typeof_operand(self.skip_parenthesized(switch_expr))
            && self.is_matching_reference(typeof_operand, target)
        {
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
                    return narrowing.narrow_by_typeof(narrowed, typeof_result);
                }

                if clause.expression.is_none() {
                    continue;
                }

                let Some(typeof_result) = self.literal_string_from_node(clause.expression) else {
                    break;
                };

                narrowed = narrowing.narrow_by_typeof_negation(narrowed, typeof_result);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }

            if saw_current {
                return narrowed;
            }
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
                if !clause.expression.is_none() {
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
                FlowNodeId::NONE,
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
            && self.is_matching_reference(typeof_operand, target)
        {
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
                narrowed = narrowing.narrow_by_typeof_negation(narrowed, typeof_result);
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
            discriminant_info = self.discriminant_property_info(switch_expr, target);
            let switch_targets_base = discriminant_info
                .as_ref()
                .is_some_and(|(_, _, base)| self.is_matching_reference(*base, target));
            if !switch_targets_base {
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
                    return narrowing.narrow_excluding_types(type_id, &excluded_types);
                } else if let Some((path, _, _)) = discriminant_info {
                    // Use batched discriminant narrowing
                    return narrowing.narrow_by_excluding_discriminant_values(
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
                FlowNodeId::NONE,
            );
        }

        narrowed
    }

    /// Apply type narrowing based on a condition expression.
    pub(crate) fn narrow_type_by_condition(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let mut visited_aliases = Vec::new();

        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
        )
    }

    pub(crate) fn narrow_type_by_condition_inner(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
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
            );
            visited_aliases.pop();
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
                        || self.is_typeof_target(bin.left, target)
                        || self.is_typeof_target(bin.right, target)
                        || self.is_optional_chain_containing_target(bin.left, target)
                        || self.is_optional_chain_containing_target(bin.right, target);

                    // CRITICAL: Use Solver-First architecture for direct binary guards
                    // when the guard target can actually match our reference.
                    if maybe_direct_guard_target
                        && let Some((guard, guard_target, _is_optional)) =
                            self.extract_type_guard(condition_idx)
                    {
                        // Check if the guard applies to our target reference
                        if self.is_matching_reference(guard_target, target) {
                            // CRITICAL: Invert sense for inequality operators (!== and !=)
                            // This applies to ALL guards, not just typeof
                            // For `x !== "string"` or `x.kind !== "circle"`, the true branch should EXCLUDE
                            let effective_sense = if bin.operator_token
                                == SyntaxKind::ExclamationEqualsEqualsToken as u16
                                || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                            {
                                !is_true_branch
                            } else {
                                is_true_branch
                            };
                            // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                            return narrowing.narrow_type(
                                type_id,
                                &guard,
                                GuardSense::from(effective_sense),
                            );
                        }

                        // Optional chain intermediate narrowing for binary expressions:
                        // `animal?.breed?.size != null` narrows target `animal.breed` to non-nullish
                        // `typeof person?.name === 'string'` narrows target `person` to non-nullish
                        //
                        // Don't return early — fall through to narrow_by_binary_expr which may
                        // apply additional narrowing (e.g., discriminant narrowing for `o?.x === 1`).
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
                            if chain_completed {
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
                                    antecedent_id,
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
                        antecedent_id,
                    );
                    return narrowed;
                }
            }

            // User-defined type guards: isString(x), obj.isString(), assertsIs(x), etc.
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // CRITICAL: Use Solver-First architecture for call expressions
                // Extract TypeGuard from AST (Checker responsibility: WHERE + WHAT)
                if let Some((guard, guard_target, is_optional)) =
                    self.extract_type_guard(condition_idx)
                {
                    // CRITICAL: Optional chaining behavior
                    // If call is optional (obj?.method(x)), only narrow the true branch
                    // The false branch might mean the method wasn't called (obj was nullish)
                    if is_optional && !is_true_branch {
                        return type_id;
                    }

                    // Check if the guard applies to our target reference
                    if self.is_matching_reference(guard_target, target) {
                        use tracing::trace;
                        trace!(
                            ?guard,
                            ?type_id,
                            ?is_true_branch,
                            "Applying guard from call expression"
                        );
                        // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                        let result = narrowing.narrow_type(
                            type_id,
                            &guard,
                            GuardSense::from(is_true_branch),
                        );
                        trace!(?result, "Guard application result");
                        return result;
                    }

                    // Optional chain intermediate narrowing:
                    // When a type guard on `x?.y?.z` (guard_target) would make the full
                    // chain non-nullish, intermediates `x` and `x.y` (target) must also be
                    // non-nullish (because `?.` short-circuits to undefined otherwise).
                    //
                    // This applies in both branches:
                    // - TRUE branch of `isNotNull(x?.y?.z)` → chain is non-nullish
                    // - FALSE branch of `isNil(x?.y?.z)` → chain is non-nullish
                    // Matches tsc's getFlowTypeOfReferenceInOptionalChain behavior.
                    if self.contains_optional_chain(guard_target)
                        && self.is_optional_chain_prefix(guard_target, target)
                    {
                        return flow_boundary::narrow_optional_chain(
                            self.interner.as_type_database(),
                            type_id,
                        );
                    }
                }

                // Fall through to type-resolved predicate narrowing when AST-based
                // extract_type_guard didn't match (e.g. declared function predicates
                // where the callee type carries the predicate signature).
                if let Some(call) = self.arena.get_call_expr(cond_node) {
                    if let Some(narrowed) =
                        self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
                    {
                        return narrowed;
                    }
                    if is_true_branch {
                        let optional_call =
                            (cond_node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0;
                        if optional_call && self.is_matching_reference(call.expression, target) {
                            return flow_boundary::narrow_optional_chain(
                                self.interner.as_type_database(),
                                type_id,
                            );
                        }
                        if let Some(callee_node) = self.arena.get(call.expression)
                            && let Some(access) = self.arena.get_access_expr(callee_node)
                            && self.access_expr_is_optional_chain(callee_node, access)
                            && self.is_matching_reference(access.expression, target)
                        {
                            return flow_boundary::narrow_optional_chain(
                                self.interner.as_type_database(),
                                type_id,
                            );
                        }
                    }
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
                // Handle truthiness discriminant narrowing for properties
                // For `if (x.flag)` where x is a discriminated union like
                // `{flag: "hello"; data: string} | {flag: ""; data: number}`,
                // narrow x based on whether `flag` is truthy or falsy.
                if let Some(property_path) = self.discriminant_property(condition_idx, target) {
                    let narrowed = narrowing.narrow_by_property_truthiness(
                        type_id,
                        &property_path,
                        is_true_branch,
                    );
                    // For union types, NEVER means all members were filtered out
                    // (no member has a truthy/falsy property), which is valid.
                    // For non-union types (class, mapped, generic), NEVER often
                    // means the solver couldn't resolve the property (e.g.
                    // Readonly<P> where P is a type parameter).  tsc does not
                    // narrow non-union base types by property truthiness, so
                    // fall through instead of collapsing to NEVER.
                    if narrowed != TypeId::NEVER
                        || tsz_solver::is_union_type(self.interner, type_id)
                    {
                        return narrowed;
                    }
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                let condition_ref = self.arena.skip_parenthesized_and_assertions(condition_idx);
                if self.is_matching_reference(condition_ref, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                    // False branch - keep only falsy types (use Solver for NaN handling)
                    return narrowing.narrow_to_falsy(type_id);
                }
            }

            // Truthiness check: if (x)
            // Use Solver-First architecture: delegate to TypeGuard::Truthy
            _ => {
                let condition_ref = self.arena.skip_parenthesized_and_assertions(condition_idx);
                let matches = self.is_matching_reference(condition_ref, target);
                if matches {
                    let narrowed = narrowing.narrow_type(
                        type_id,
                        &TypeGuard::Truthy,
                        GuardSense::from(is_true_branch),
                    );
                    if std::env::var_os("TSZ_DEBUG_TRUTHY_NARROW").is_some() {
                        eprintln!(
                            "truthy-narrow cond={} target={} input={:?} result={:?} true_branch={}",
                            condition_idx.0, target.0, type_id, narrowed, is_true_branch
                        );
                    }
                    return narrowed;
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

    /// Check if `target` is an intermediate segment in an optional chain `chain_node`.
    ///
    /// When a type guard narrows `x?.y?.z`, intermediate segments like `x.y` and `x`
    /// should also be narrowed by removing null/undefined. This is because if
    /// `x?.y?.z` is non-nullish, all intermediate accesses must also be non-nullish.
    ///
    /// Returns `true` if `target` matches any prefix of the optional chain.
    pub(crate) fn is_optional_chain_prefix(
        &self,
        chain_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let chain_node = self.arena.skip_parenthesized_and_assertions(chain_node);
        let Some(node) = self.arena.get(chain_node) else {
            return false;
        };
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            // Check if the base expression matches target
            if self.is_matching_reference(access.expression, target) {
                return true;
            }
            // Also check: does the current chain node (e.g. animal?.breed) match
            // the target (e.g. animal.breed) when ignoring the optional dot?
            // This handles the case where the chain has `?.` but the target uses `.`.
            if self.is_matching_optional_access_reference(chain_node, target) {
                return true;
            }
            // Recurse into the base expression
            return self.is_optional_chain_prefix(access.expression, target);
        }
        false
    }

    /// Match a property/element access reference ignoring `?.` vs `.` differences.
    ///
    /// `is_matching_reference` can't match `x?.y` against `x.y` because
    /// `property_reference` returns `None` for optional chains. This helper
    /// compares the structure directly: same property name and matching base.
    fn is_matching_optional_access_reference(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a = self.arena.skip_parenthesized_and_assertions(a);
        let b = self.arena.skip_parenthesized_and_assertions(b);
        let (Some(node_a), Some(node_b)) = (self.arena.get(a), self.arena.get(b)) else {
            return false;
        };
        // Both must be the same kind of access expression
        if node_a.kind != node_b.kind {
            return false;
        }
        if node_a.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node_a.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let (Some(access_a), Some(access_b)) = (
            self.arena.get_access_expr(node_a),
            self.arena.get_access_expr(node_b),
        ) else {
            return false;
        };
        // Compare property names
        if node_a.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let ident_a = self
                .arena
                .get_identifier_at(access_a.name_or_argument)
                .map(|i| &i.escaped_text);
            let ident_b = self
                .arena
                .get_identifier_at(access_b.name_or_argument)
                .map(|i| &i.escaped_text);
            if ident_a != ident_b || ident_a.is_none() {
                return false;
            }
        } else {
            // Element access - compare using literal values
            let atom_a = self.literal_atom_from_node_or_type(access_a.name_or_argument);
            let atom_b = self.literal_atom_from_node_or_type(access_b.name_or_argument);
            if atom_a != atom_b || atom_a.is_none() {
                return false;
            }
        }
        // Base expressions must match (recursively, also ignoring optional dots)
        self.is_matching_reference(access_a.expression, access_b.expression)
            || self.is_matching_optional_access_reference(access_a.expression, access_b.expression)
    }

    /// Check if a node is part of an optional chain (has `?.` somewhere in its left spine).
    pub(crate) fn contains_optional_chain(&self, idx: NodeIndex) -> bool {
        let idx = self.arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
        {
            if (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0 {
                return true;
            }
            return self.contains_optional_chain(call.expression);
        }
        if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.arena.get_access_expr(node)
        {
            if self.access_expr_is_optional_chain(node, access) {
                return true;
            }
            return self.contains_optional_chain(access.expression);
        }
        false
    }

    const fn access_expr_is_optional_chain(
        &self,
        node: &tsz_parser::parser::node::Node,
        access: &tsz_parser::parser::node::AccessExprData,
    ) -> bool {
        access.question_dot_token || (node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0
    }

    /// Check if `expr` is an optional chain (or typeof of one) that contains `target`
    /// as an intermediate prefix. Used to let binary expression narrowing know that
    /// guard extraction is worth attempting even though `target` doesn't directly match
    /// either side of the comparison.
    fn is_optional_chain_containing_target(&self, expr: NodeIndex, target: NodeIndex) -> bool {
        let expr = self.arena.skip_parenthesized_and_assertions(expr);
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        // Handle `typeof x?.y?.z` — check the typeof operand
        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            if let Some(unary) = self.arena.get_unary_expr(node)
                && unary.operator == SyntaxKind::TypeOfKeyword as u16
            {
                return self.is_optional_chain_containing_target(unary.operand, target);
            }
            return false;
        }
        if !self.contains_optional_chain(expr) {
            return false;
        }
        if self.is_optional_chain_prefix(expr, target) {
            return true;
        }

        // Fallback for cases like `o?.["foo"]` where structural prefix matching can miss
        // the target due access-form differences; walk chain bases directly.
        let mut cur = expr;
        for _ in 0..64 {
            if self.is_matching_reference(cur, target) {
                return true;
            }
            let Some(cur_node) = self.arena.get(cur) else {
                return false;
            };
            if cur_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call) = self.arena.get_call_expr(cur_node)
            {
                cur = self
                    .arena
                    .skip_parenthesized_and_assertions(call.expression);
                continue;
            }
            if (cur_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || cur_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.arena.get_access_expr(cur_node)
            {
                cur = self
                    .arena
                    .skip_parenthesized_and_assertions(access.expression);
                continue;
            }
            return false;
        }
        false
    }

    fn optional_chain_comparison_proves_non_nullish(
        &self,
        bin: &BinaryExprData,
        target: NodeIndex,
        is_strict: bool,
        effective_truth: bool,
    ) -> bool {
        if !effective_truth {
            return false;
        }
        let Some(node_types) = self.node_types else {
            return false;
        };

        for (chain_side, other_side) in [(bin.left, bin.right), (bin.right, bin.left)] {
            if !self.is_optional_chain_containing_target(chain_side, target) {
                continue;
            }
            let Some(&other_type) = node_types.get(&other_side.0) else {
                continue;
            };
            if !self.comparison_allows_optional_chain_short_circuit(other_type, is_strict) {
                return true;
            }
        }

        false
    }

    fn comparison_allows_optional_chain_short_circuit(
        &self,
        compared_type: TypeId,
        is_strict: bool,
    ) -> bool {
        if compared_type.is_any_or_unknown() || compared_type == TypeId::ERROR {
            return true;
        }

        self.type_contains(compared_type, TypeId::UNDEFINED)
            || (!is_strict && self.type_contains(compared_type, TypeId::NULL))
    }

    fn type_contains(&self, type_id: TypeId, needle: TypeId) -> bool {
        if type_id == needle {
            return true;
        }
        union_members(self.interner, type_id)
            .map(|members| {
                members
                    .into_iter()
                    .any(|member| self.type_contains(member, needle))
            })
            .unwrap_or(false)
    }

    pub(crate) fn const_condition_initializer(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(SymbolId, NodeIndex)> {
        let sym_id = self.binder.resolve_identifier(self.arena, ident_idx)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
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

        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false; // Assume mutable if no declaration
        }

        self.arena.is_const_variable_declaration(decl_idx)
    }

    /// Narrow type based on a binary expression (===, !==, typeof checks, etc.)
    pub(crate) fn narrow_by_binary_expr(
        &self,
        type_id: TypeId,
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let operator = bin.operator_token;

        // Unwrap assignment expressions: if (flag = (x instanceof Foo)) should narrow based on RHS
        // The assignment itself doesn't provide narrowing, but its RHS might
        if operator == SyntaxKind::EqualsToken as u16 {
            if self.arena.get(bin.right).is_some() {
                // Recursively narrow based on the RHS expression
                let mut visited = Vec::new();
                return self.narrow_type_by_condition_inner(
                    type_id,
                    bin.right,
                    target,
                    is_true_branch,
                    antecedent_id,
                    &mut visited,
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

        let (is_equals, is_strict) = match operator {
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => (true, true),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => (false, true),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => (true, false),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => (false, false),
            _ => return type_id,
        };

        let effective_truth = if is_equals {
            is_true_branch
        } else {
            !is_true_branch
        };
        let mut type_id = type_id;

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
                // Route catch-variable typeof base reset through the flow
                // observation boundary (NORTH_STAR §3.3 / §22).
                let _is_catch_var = self
                    .binder
                    .resolve_identifier(self.arena, target)
                    .is_some_and(|sid| self.is_unknown_catch_variable_symbol(sid));
                // For catch variables, `type_id` is already the catch base
                // type (`any` or `unknown`), so we can use it directly as
                // the typeof narrowing base.
                let typeof_base_type = type_id;
                return narrowing.narrow_type(
                    typeof_base_type,
                    &TypeGuard::Typeof(typeof_kind),
                    GuardSense::from(effective_truth),
                );
            }
            // Non-standard typeof string (e.g. "Object", host-defined types).
            // TypeScript behavior:
            //   - true branch: remove primitive types (string, number, boolean)
            //     because if typeof returned a non-standard string, x can't be primitive
            //   - false branch: keep only primitive types (typeof can only ever
            //     return standard strings at runtime, so the condition is always
            //     false; the complement narrows to primitives)
            if effective_truth {
                return narrowing.narrow_excluding_types(
                    type_id,
                    &[TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN],
                );
            } else {
                // Collect the primitive members from the source type
                let s = narrowing.narrow_by_typeof(type_id, "string");
                let n = narrowing.narrow_by_typeof(type_id, "number");
                let b = narrowing.narrow_by_typeof(type_id, "boolean");
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
                    self.interner.union(parts)
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
                return narrowing.narrow_by_discriminant_for_type(
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
                return narrowing.narrow_excluding_type(type_id, nullish);
            }

            let nullish_union = self.interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
            if effective_truth {
                return nullish_union;
            }

            return flow_boundary::narrow_optional_chain(self.interner.as_type_database(), type_id);
        }

        if is_strict {
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
                    let narrowed = narrowing.narrow_by_discriminant_for_type(
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

            if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
                if effective_truth {
                    let narrowed = narrowing.narrow_to_type(type_id, literal_type);
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    if narrowing.literal_assignable_to(literal_type, type_id) {
                        return literal_type;
                    }
                    return TypeId::NEVER;
                }
                return narrowing.narrow_excluding_type(type_id, literal_type);
            }
        }

        // Bidirectional narrowing: x === y where both are references
        // This handles cases like: if (x === y) { ... }
        // where both x and y are variables (not just literals)
        if is_strict {
            // Helper to get flow type of the "other" node
            let get_other_flow_type = |other_node: NodeIndex| -> Option<TypeId> {
                let node_types = self.node_types?;
                let initial_type = *node_types.get(&other_node.0)?;

                // CRITICAL FIX: Use flow analysis if we have a valid flow node
                // This gets the flow-narrowed type of the other reference
                if antecedent_id.is_some() {
                    Some(self.get_flow_type(other_node, initial_type, antecedent_id))
                } else {
                    // Fallback for tests or when no flow context exists
                    Some(initial_type)
                }
            };

            // Check if target is on the left side (x === y, target is x)
            if self.is_matching_reference(bin.left, target) {
                // We need the type of the RIGHT side (y)
                if let Some(right_type) = get_other_flow_type(bin.right) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            GuardSense::Positive,
                        );
                    } else if is_unit_type(self.interner, right_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            GuardSense::Negative,
                        );
                    }
                }
            }

            // Check if target is on the right side (y === x, target is x)
            if self.is_matching_reference(bin.right, target) {
                // We need the type of the LEFT side (y)
                if let Some(left_type) = get_other_flow_type(bin.left) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            GuardSense::Positive,
                        );
                    } else if is_unit_type(self.interner, left_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            GuardSense::Negative,
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
        visited_aliases: &mut Vec<SymbolId>,
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
        visited_aliases: &mut Vec<SymbolId>,
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
            return Some(tsz_solver::utils::union_or_single(
                self.interner,
                vec![left_false, right_false],
            ));
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
                let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                return Some(narrowed);
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
                return Some(tsz_solver::utils::union_or_single(
                    self.interner,
                    vec![left_true, right_true],
                ));
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
                return Some(tsz_solver::utils::union_or_single(
                    self.interner,
                    vec![left_true, right_true],
                ));
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
        if operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || operator == SyntaxKind::BarBarEqualsToken as u16
            || operator == SyntaxKind::QuestionQuestionEqualsToken as u16
        {
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

    /// Check if the target reference (or its base) has been assigned to between
    /// the alias declaration and the current condition, which would invalidate
    /// aliased type guard narrowing.
    ///
    /// For simple identifiers (e.g., `e`): walks the flow graph from the
    /// condition's antecedent backward to the alias declaration, checking only
    /// the current flow path for assignments.
    ///
    /// For property accesses (e.g., `obj.x`, `this.x`): additionally checks
    /// ALL assignment flow nodes in the function after the alias declaration
    /// position, since property mutations can occur through paths not visible
    /// in the local flow graph.
    fn is_alias_reference_mutated(
        &self,
        alias_sym_id: SymbolId,
        target: NodeIndex,
        antecedent_id: FlowNodeId,
    ) -> bool {
        use tsz_binder::flow_flags;

        // Get the alias declaration position
        let alias_pos = match self.binder.get_symbol(alias_sym_id) {
            Some(sym) if sym.value_declaration.is_some() => self
                .arena
                .get(sym.value_declaration)
                .map(|n| n.pos)
                .unwrap_or(0),
            _ => return false,
        };

        // Walk the flow graph backward from the condition's antecedent.
        // Check if any ASSIGNMENT node on the current path targets the reference
        // (or its base). Stop when we reach nodes at or before the alias position.
        let mut visited = rustc_hash::FxHashSet::default();
        let mut stack = vec![antecedent_id];

        while let Some(flow_id) = stack.pop() {
            if flow_id.is_none() || !visited.insert(flow_id) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
                continue;
            };

            // Stop walking if this flow node is at or before the alias declaration
            if let Some(node) = self.arena.get(flow.node)
                && node.pos <= alias_pos
            {
                continue;
            }

            // Check if this is an assignment targeting our reference or its base
            if flow.has_any_flags(flow_flags::ASSIGNMENT)
                && (self.assignment_targets_reference_node(flow.node, target)
                    || self.assignment_targets_base_of_reference(flow.node, target))
            {
                return true;
            }

            // Continue to antecedents
            for &ant in &flow.antecedent {
                stack.push(ant);
            }
        }

        // For property accesses (obj.x, this.x, obj[0]), also check function-wide
        // assignments. Property accesses can be affected by mutations in other
        // branches that aren't on the current flow path.
        if self.reference_base(target).is_some() {
            return self.has_base_assignment_after_pos(target, alias_pos);
        }

        false
    }

    /// Check if any assignment flow node in the containing function targets
    /// the base of the given reference after the specified position. This is a
    /// conservative function-wide check used for property access aliases.
    ///
    /// Scoped to the containing function to avoid false positives from
    /// assignments in sibling class constructors/methods.
    fn has_base_assignment_after_pos(&self, target: NodeIndex, after_pos: u32) -> bool {
        use tsz_binder::flow_flags;

        // Find the containing function's position bounds to scope the search.
        // This prevents matching `this.x = 10` in class C11 when checking
        // an alias in class C10.
        let (fn_start, fn_end) = self.containing_function_bounds(target);

        let flow_count = self.binder.flow_nodes.len();
        for i in 0..flow_count {
            let flow_id = tsz_binder::FlowNodeId(i as u32);
            let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
                continue;
            };

            if !flow.has_any_flags(flow_flags::ASSIGNMENT) {
                continue;
            }

            let Some(node) = self.arena.get(flow.node) else {
                continue;
            };

            // Only consider assignments after the alias declaration
            if node.pos <= after_pos {
                continue;
            }

            // Only consider assignments within the same function
            if node.pos < fn_start || node.pos > fn_end {
                continue;
            }

            // Check if this assignment targets the reference itself or its base
            if self.assignment_targets_reference_node(flow.node, target)
                || self.assignment_targets_base_of_reference(flow.node, target)
            {
                return true;
            }
        }
        false
    }

    /// Get the position bounds (start, end) of the containing function-like
    /// node for the given reference. Returns (0, `u32::MAX`) if no containing
    /// function is found (source file level).
    fn containing_function_bounds(&self, reference: NodeIndex) -> (u32, u32) {
        let mut current = reference;
        while current.is_some() {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.is_function_like() {
                return (node.pos, node.end);
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        (0, u32::MAX)
    }
}
