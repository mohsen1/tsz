//! Non-strict (`strictNullChecks: false`) `null`/`undefined` widening for a
//! block-bodied function's inferred return type.
//!
//! Split out of `return_type.rs`, which sits at the checker's 2000-line
//! boundary. The expression-bodied seam lives there
//! (`maybe_widen_return_contribution`); this is the block-bodied twin, called
//! per return contribution from `collect_return_types_in_statement`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Apply the non-strict `null`/`undefined` → `any` widening to a fresh
    /// return contribution, but only when every nullish leaf the widener would
    /// touch actually originates from a *widening* nullish source.
    ///
    /// tsc gives the `null` keyword and the global `undefined` the widening
    /// flavour (`nullWideningType` / `undefinedWideningType`) and propagates it
    /// through array/object-literal construction, so `getWidenedType` maps those
    /// leaves to `any` when `strictNullChecks` is off:
    /// `function f() { return [undefined]; }` infers `any[]`. A leaf that is
    /// merely *typed* `undefined` carries no widening flavour — with
    /// `declare var q: undefined`, `function f() { return [q]; }` keeps
    /// `undefined[]`. tsz has no per-type widening flag, so the provenance is
    /// recovered from the return expression's own syntax; anything the walk
    /// cannot account for keeps the unwidened contribution.
    pub(crate) fn widen_nullish_return_contribution(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
    ) -> TypeId {
        if self.ctx.strict_null_checks() {
            return type_id;
        }
        let widened =
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, type_id);
        if widened == type_id {
            return type_id;
        }
        if self.return_contribution_nullish_leaves_are_widening(expr_idx, 0) {
            widened
        } else {
            type_id
        }
    }

    /// Whether every nullish leaf reachable through a return expression's fresh
    /// literal structure comes from a widening nullish source. See
    /// [`Self::widen_nullish_return_contribution`] for the rule this recovers.
    fn return_contribution_nullish_leaves_are_widening(
        &mut self,
        expr_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        const MAX_NULLISH_PROVENANCE_DEPTH: u8 = 8;
        if depth > MAX_NULLISH_PROVENANCE_DEPTH {
            return false;
        }
        let expr_idx = self.unwrap_parenthesized_expression(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        let kind = node.kind;

        // Fresh literal structure: the widening flavour of a leaf propagates to
        // the array/object literal built around it, so recurse into the members
        // instead of judging the composite node itself.
        if kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(array) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            let elements: Vec<NodeIndex> = array.elements.nodes.clone();
            return elements.into_iter().all(|element| {
                // An elided element (`return [,,]`, parsed as `NodeIndex::NONE`)
                // is a widening source: the user wrote no value, so tsc gives
                // the hole `undefinedWideningType` exactly as it does the bare
                // `undefined` keyword, and `() => [,,]` infers `any[]`. Without
                // this the hole hits the node-lookup guard and fails closed.
                // The enclosing `all` keeps it honest — a hole is permissive on
                // its own and decisive nowhere, so one declared-`undefined`
                // sibling (`return [, q]`) still makes the whole literal
                // non-widening. Matches the mutable-binding seam's identical
                // carve-out (`mutable_binding_nullish.rs`, #16393).
                if element == NodeIndex::NONE {
                    return true;
                }
                self.return_contribution_nullish_leaves_are_widening(element, depth + 1)
            });
        }
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(object) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            let elements: Vec<NodeIndex> = object.elements.nodes.clone();
            for element_idx in elements {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    return false;
                };
                // Only a plain `name: value` member exposes its own value
                // expression; spreads, shorthands, methods and accessors are
                // judged by the leaf rule below on the member node itself, which
                // rejects anything carrying a nullish leaf.
                let member_value = if element.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    self.ctx
                        .arena
                        .get_property_assignment(element)
                        .map(|prop| prop.initializer)
                        .unwrap_or(element_idx)
                } else {
                    element_idx
                };
                if !self.return_contribution_nullish_leaves_are_widening(member_value, depth + 1) {
                    return false;
                }
            }
            return true;
        }

        // Leaf rule: a leaf whose checked type has no nullish leaf for the
        // widener to touch is always fine; one that does must be a widening
        // source (the `null` keyword or the global `undefined`).
        let Some(&leaf_type) = self.ctx.node_types.get(&expr_idx.0) else {
            return false;
        };
        let widened =
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, leaf_type);
        if widened == leaf_type {
            return true;
        }
        if kind == SyntaxKind::NullKeyword as u16 || kind == SyntaxKind::UndefinedKeyword as u16 {
            return true;
        }
        crate::flow_domain::control_flow::narrowing_helpers::is_global_undefined_identifier(
            self.ctx.arena,
            self.ctx.binder,
            expr_idx,
        )
    }

    /// Whether every value-returning path in `body_idx` is a bare `null`
    /// keyword, the global `undefined` identifier, or an empty `return;`
    /// (implicit `undefined`) — the same widening-source leaves
    /// [`Self::return_contribution_nullish_leaves_are_widening`] recognizes,
    /// checked syntactically instead of through `node_types` so this is safe
    /// to call from the pre-body-check return-type-inference seam (#17203).
    ///
    /// Distinguishes tsc's noImplicitAny return-type check: a body whose
    /// inferred return type widens to `any` purely because every return is a
    /// bare nullish contribution under non-strict null checks (`function f()
    /// { return null; }`) still reports TS7010 — unlike a body that already
    /// returns an `any`-typed operand (`return x` where `x: any`), which
    /// tsc leaves silent because no widening occurred.
    pub(crate) fn all_value_returns_are_nullish_widening_sources(
        &mut self,
        body_idx: NodeIndex,
    ) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let mut saw_return = false;
        let mut all_nullish = true;
        self.collect_nullish_only_returns(body_idx, &mut saw_return, &mut all_nullish);
        saw_return && all_nullish
    }

    fn collect_nullish_only_returns(
        &mut self,
        stmt_idx: NodeIndex,
        saw_return: &mut bool,
        all_nullish: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node) {
                    *saw_return = true;
                    if return_data.expression.is_some()
                        && !self.is_bare_nullish_return_expression(return_data.expression)
                    {
                        *all_nullish = false;
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_nullish_only_returns(stmt, saw_return, all_nullish);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_nullish_only_returns(
                        if_data.then_statement,
                        saw_return,
                        all_nullish,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_nullish_only_returns(
                            if_data.else_statement,
                            saw_return,
                            all_nullish,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_nullish_only_returns(stmt, saw_return, all_nullish);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_nullish_only_returns(try_data.try_block, saw_return, all_nullish);
                    if try_data.catch_clause.is_some() {
                        self.collect_nullish_only_returns(
                            try_data.catch_clause,
                            saw_return,
                            all_nullish,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_nullish_only_returns(
                            try_data.finally_block,
                            saw_return,
                            all_nullish,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_nullish_only_returns(catch_data.block, saw_return, all_nullish);
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_nullish_only_returns(loop_data.statement, saw_return, all_nullish);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_nullish_only_returns(
                        for_in_of_data.statement,
                        saw_return,
                        all_nullish,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_nullish_only_returns(
                        labeled_data.statement,
                        saw_return,
                        all_nullish,
                    );
                }
            }
            _ => {}
        }
    }

    fn is_bare_nullish_return_expression(&mut self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.unwrap_parenthesized_expression(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::NullKeyword as u16
            || node.kind == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        crate::flow_domain::control_flow::narrowing_helpers::is_global_undefined_identifier(
            self.ctx.arena,
            self.ctx.binder,
            expr_idx,
        )
    }
}
