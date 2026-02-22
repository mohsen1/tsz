//! Condition-based type narrowing for `FlowAnalyzer`.
//!
//! Handles switch clause narrowing, binary/logical expression narrowing,
//! typeof/instanceof/in guards, and boolean comparison narrowing.

use crate::control_flow::FlowAnalyzer;
use crate::query_boundaries::flow_analysis::is_unit_type;
use tsz_binder::{FlowNodeId, SymbolId, symbol_flags};
use tsz_parser::parser::node::BinaryExprData;
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, TypeGuard, TypeId, TypeofKind};

impl<'a> FlowAnalyzer<'a> {
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
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        if cond_node.kind == SyntaxKind::Identifier as u16
            && let Some((sym_id, initializer)) = self.const_condition_initializer(condition_idx)
            && !visited_aliases.contains(&sym_id)
        {
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
                        || self.is_typeof_target(bin.right, target);

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
                            return narrowing.narrow_type(type_id, &guard, effective_sense);
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
                        let result = narrowing.narrow_type(type_id, &guard, is_true_branch);
                        trace!(?result, "Guard application result");
                        return result;
                    }
                }

                return type_id;
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

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
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
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                        if let Some(callee_node) = self.arena.get(call.expression)
                            && let Some(access) = self.arena.get_access_expr(callee_node)
                            && access.question_dot_token
                            && self.is_matching_reference(access.expression, target)
                        {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                    }
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(cond_node) {
                    // Handle optional chaining: y?.a
                    if access.question_dot_token
                        && is_true_branch
                        && self.is_matching_reference(access.expression, target)
                    {
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
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
                    // Even if narrowed is NEVER, it means no branch matches, so returning NEVER is correct
                    return narrowed;
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                if self.is_matching_reference(condition_idx, target) {
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
                if self.is_matching_reference(condition_idx, target) {
                    return narrowing.narrow_type(type_id, &TypeGuard::Truthy, is_true_branch);
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
        let node = self.skip_parenthesized_non_recursive(node);
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node_data.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    /// Skip parentheses (non-recursive to avoid issues with circular references).
    fn skip_parenthesized_non_recursive(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            // Limit iterations to prevent infinite loops
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return idx;
                };
                idx = paren.expression;
            } else {
                return idx;
            }
        }
        idx
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
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let mut flags = decl_node.flags as u32;
        if (flags & (node_flags::LET | node_flags::CONST)) == 0 {
            let Some(ext) = self.arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                return false;
            }
            flags |= parent_node.flags as u32;
        }
        (flags & node_flags::CONST) != 0
    }

    /// Check if a symbol is const (immutable) vs mutable (let/var).
    ///
    /// This is used for loop widening: const variables preserve narrowing through loops,
    /// while mutable variables are widened to the declared type to account for mutations.
    pub(crate) fn is_const_symbol(&self, sym_id: SymbolId) -> bool {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let symbol = match self.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false, // Assume mutable if we can't determine
        };

        // Check the value declaration
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false; // Assume mutable if no declaration
        }

        let decl_node = match self.arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(ext) = self.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
        {
            let flags = parent_node.flags as u32;
            return (flags & node_flags::CONST) != 0;
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        (flags & node_flags::CONST) != 0
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

        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            // Use unified narrow_type API with TypeGuard::Typeof for both branches
            if let Some(typeof_kind) = TypeofKind::parse(type_name) {
                return narrowing.narrow_type(
                    type_id,
                    &TypeGuard::Typeof(typeof_kind),
                    effective_truth,
                );
            }
            // Unknown typeof string (e.g., host-defined types), no narrowing
            return type_id;
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

            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
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
                    if is_optional && effective_truth {
                        let narrowed = narrowing.narrow_excluding_type(base_type, TypeId::NULL);
                        base_type = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                    }
                    return narrowing.narrow_by_discriminant_for_type(
                        base_type,
                        &property_path,
                        literal_type,
                        effective_truth,
                    );
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
                            true,
                        );
                    } else if is_unit_type(self.interner, right_type) {
                        return narrowing.narrow_type(
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
                if let Some(left_type) = get_other_flow_type(bin.left) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            true,
                        );
                    } else if is_unit_type(self.interner, left_type) {
                        return narrowing.narrow_type(
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

        if operator == SyntaxKind::AmpersandAmpersandToken as u16 {
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

        if operator == SyntaxKind::BarBarToken as u16 {
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

        None
    }
}
