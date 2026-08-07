//! "All value returns are an explicit `any` assertion" detection for
//! `CheckerState`.
//!
//! Split out of `return_type.rs`, which sits at the checker's 2000-line
//! boundary. A body whose every value-returning path is a literal
//! `return x as any` / `return <any>x` is treated specially by overload
//! resolution (`state_checking_members::overload_compatibility`), so this walk
//! is a self-contained feature rather than part of return-type *inference*.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Whether the body has at least one value-returning path and every such
    /// path returns an explicit `any` assertion (`return x as any`).
    pub(crate) fn has_only_explicit_any_assertion_returns(&mut self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let mut saw_value_return = false;
        let mut all_value_returns_explicit_any = true;
        self.collect_explicit_any_assertion_returns(
            body_idx,
            &mut saw_value_return,
            &mut all_value_returns_explicit_any,
        );
        saw_value_return && all_value_returns_explicit_any
    }

    fn collect_explicit_any_assertion_returns(
        &mut self,
        stmt_idx: NodeIndex,
        saw_value_return: &mut bool,
        all_value_returns_explicit_any: &mut bool,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(return_data) = self.ctx.arena.get_return_statement(node)
                    && return_data.expression.is_some()
                {
                    *saw_value_return = true;
                    if !self.is_explicit_any_assertion_expression(return_data.expression) {
                        *all_value_returns_explicit_any = false;
                    }
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_explicit_any_assertion_returns(
                            stmt,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        if_data.then_statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            if_data.else_statement,
                            saw_value_return,
                            all_value_returns_explicit_any,
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
                                self.collect_explicit_any_assertion_returns(
                                    stmt,
                                    saw_value_return,
                                    all_value_returns_explicit_any,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_explicit_any_assertion_returns(
                        try_data.try_block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.catch_clause,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_explicit_any_assertion_returns(
                            try_data.finally_block,
                            saw_value_return,
                            all_value_returns_explicit_any,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_explicit_any_assertion_returns(
                        catch_data.block,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_explicit_any_assertion_returns(
                        loop_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_explicit_any_assertion_returns(
                        for_in_of_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled_data) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_explicit_any_assertion_returns(
                        labeled_data.statement,
                        saw_value_return,
                        all_value_returns_explicit_any,
                    );
                }
            }
            _ => {}
        }
    }

    fn is_explicit_any_assertion_expression(&mut self, expr_idx: NodeIndex) -> bool {
        let mut current = expr_idx;
        while let Some(node) = self.ctx.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION)
                && let Some(assertion) = self.ctx.arena.get_type_assertion(node)
            {
                return self.get_type_from_type_node(assertion.type_node) == TypeId::ANY;
            }
            return false;
        }
        false
    }
}
