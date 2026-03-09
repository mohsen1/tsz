//! Circular return-site tracking for closure/function type resolution.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn record_pending_circular_return_sites(
        &mut self,
        function_idx: NodeIndex,
        body_idx: NodeIndex,
    ) {
        let resolving_vars: FxHashSet<_> = self
            .ctx
            .symbol_dependency_stack
            .iter()
            .copied()
            .filter(|sym_id| {
                self.ctx.binder.get_symbol(*sym_id).is_some_and(|symbol| {
                    symbol.flags
                        & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                            | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                        != 0
                })
            })
            .collect();

        if resolving_vars.is_empty() {
            return;
        }

        let mut found = FxHashSet::default();
        if let Some(body_node) = self.ctx.arena.get(body_idx) {
            if body_node.kind == syntax_kind_ext::BLOCK {
                if let Some(block) = self.ctx.arena.get_block(body_node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_resolving_var_refs_in_return_statement(
                            stmt_idx,
                            &resolving_vars,
                            &mut found,
                        );
                    }
                }
            } else {
                self.collect_resolving_var_refs_in_return_expression(
                    body_idx,
                    &resolving_vars,
                    &mut found,
                );
            }
        }

        for sym_id in found {
            let sites = self
                .ctx
                .pending_circular_return_sites
                .entry(sym_id)
                .or_default();
            if !sites.contains(&function_idx) {
                sites.push(function_idx);
            }
        }
    }

    pub(super) fn contextual_return_suppresses_circularity(
        &self,
        return_context: Option<TypeId>,
    ) -> bool {
        let Some(return_context) = return_context else {
            return false;
        };

        return_context == TypeId::ANY
            || (return_context != TypeId::UNKNOWN
                && !tsz_solver::type_queries::is_type_parameter_like(
                    self.ctx.types,
                    return_context,
                )
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    return_context,
                ))
    }

    fn collect_resolving_var_refs_in_return_statement(
        &self,
        stmt_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
        found: &mut FxHashSet<tsz_binder::SymbolId>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.collect_resolving_var_refs_in_return_expression(
                        ret.expression,
                        resolving_vars,
                        found,
                    );
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_resolving_var_refs_in_return_statement(
                            stmt,
                            resolving_vars,
                            found,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        if_data.then_statement,
                        resolving_vars,
                        found,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_resolving_var_refs_in_return_statement(
                            if_data.else_statement,
                            resolving_vars,
                            found,
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
                                self.collect_resolving_var_refs_in_return_statement(
                                    stmt,
                                    resolving_vars,
                                    found,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        try_data.try_block,
                        resolving_vars,
                        found,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_resolving_var_refs_in_return_statement(
                            try_data.catch_clause,
                            resolving_vars,
                            found,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_resolving_var_refs_in_return_statement(
                            try_data.finally_block,
                            resolving_vars,
                            found,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        catch_data.block,
                        resolving_vars,
                        found,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        loop_data.statement,
                        resolving_vars,
                        found,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        loop_data.statement,
                        resolving_vars,
                        found,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_resolving_var_refs_in_return_statement(
                        labeled.statement,
                        resolving_vars,
                        found,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_resolving_var_refs_in_return_expression(
        &self,
        expr_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
        found: &mut FxHashSet<tsz_binder::SymbolId>,
    ) {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if let Some(sym_id) = (node.kind == SyntaxKind::Identifier as u16)
            .then(|| self.resolve_identifier_symbol(expr_idx))
            .flatten()
            && resolving_vars.contains(&sym_id)
        {
            found.insert(sym_id);
            return;
        }

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return;
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_resolving_var_refs_in_return_expression(child_idx, resolving_vars, found);
        }
    }
}
