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
                && !crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    return_context,
                )
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    return_context,
                ))
    }

    pub(crate) fn function_has_wrapped_self_call_in_return_expression(
        &self,
        function_idx: NodeIndex,
        body_idx: NodeIndex,
    ) -> bool {
        let Some(sym_id) = self.ctx.binder.get_node_symbol(function_idx) else {
            return false;
        };

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            return self.statement_has_wrapped_self_call_in_return(body_idx, sym_id, true);
        }

        self.expression_has_wrapped_self_call_in_return(body_idx, sym_id, true)
    }

    pub(crate) fn function_like_initializer_has_wrapped_self_call_in_return_expression(
        &self,
        init_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(init_node) else {
            return false;
        };

        if func.body.is_none() {
            return false;
        }

        let body_idx = func.body;
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        let has_circular_return_in_all_paths = |sym_id: tsz_binder::SymbolId| -> bool {
            if body_node.kind == syntax_kind_ext::BLOCK {
                self.function_body_has_wrapped_self_call_in_every_return(body_idx, sym_id, false)
            } else {
                self.expression_has_wrapped_self_call_in_return(body_idx, sym_id, false)
            }
        };

        // Only check the outer variable's symbol (function_sym), not the
        // function expression's own name binding (init_sym).  A named function
        // expression referencing itself via its name (e.g.,
        // `const F = function Named() { return new Named(); }`) is NOT
        // circular in the TS7023 sense — the function's name is its own
        // complete, non-circular binding.  Only references to the enclosing
        // variable would create genuine circular return-type inference.
        has_circular_return_in_all_paths(function_sym)
    }

    /// Check if ALL return expressions in a function body are direct (non-wrapped)
    /// self-calls. Used to detect purely recursive functions like
    /// `function fn2(n) { return fn2(n); }` whose return type should be `never`.
    ///
    /// Returns `false` if any return is NOT a direct self-call (has a base case),
    /// or if any self-call is wrapped (goes through array/property access etc.),
    /// or if the body has no return statements.
    pub(crate) fn all_returns_are_direct_self_calls(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
    ) -> bool {
        // Every return must have a self-call (direct or wrapped)
        if !self.function_body_has_wrapped_self_call_in_every_return(body_idx, function_sym, false)
        {
            return false;
        }
        // None of the returns should be wrapped (they must all be direct)
        !self.function_has_wrapped_self_call_in_return_expression_for_sym(body_idx, function_sym)
    }

    /// Check if any return expression in a function body has a WRAPPED self-call
    /// (goes through array access, property access, etc.).
    fn function_has_wrapped_self_call_in_return_expression_for_sym(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            return self.statement_has_wrapped_self_call_in_return(body_idx, function_sym, true);
        }

        self.expression_has_wrapped_self_call_in_return(body_idx, function_sym, true)
    }

    fn function_body_has_wrapped_self_call_in_every_return(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
        require_wrapped_call: bool,
    ) -> bool {
        let mut return_exprs = Vec::new();
        self.collect_initializer_return_expressions_in_function_body(body_idx, &mut return_exprs);

        if return_exprs.is_empty() {
            return false;
        }

        return_exprs.into_iter().all(|expr_idx| {
            self.expression_has_wrapped_self_call_in_return(
                expr_idx,
                function_sym,
                require_wrapped_call,
            )
        })
    }

    fn collect_initializer_return_expressions_in_function_body(
        &self,
        body_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return_exprs.push(body_idx);
            return;
        }

        if let Some(block) = self.ctx.arena.get_block(body_node) {
            for &stmt_idx in &block.statements.nodes {
                self.collect_initializer_return_expressions_in_function_body_statement(
                    stmt_idx,
                    return_exprs,
                );
            }
        }
    }

    fn collect_initializer_return_expressions_in_function_body_statement(
        &self,
        stmt_idx: NodeIndex,
        return_exprs: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    return_exprs.push(ret.expression);
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_initializer_return_expressions_in_function_body_statement(
                            stmt,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        if_data.then_statement,
                        return_exprs,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_initializer_return_expressions_in_function_body_statement(
                            if_data.else_statement,
                            return_exprs,
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
                                self.collect_initializer_return_expressions_in_function_body_statement(
                                    stmt,
                                    return_exprs,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        try_data.try_block,
                        return_exprs,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_initializer_return_expressions_in_function_body_statement(
                            try_data.catch_clause,
                            return_exprs,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_initializer_return_expressions_in_function_body_statement(
                            try_data.finally_block,
                            return_exprs,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        catch_data.block,
                        return_exprs,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        loop_data.statement,
                        return_exprs,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        loop_data.statement,
                        return_exprs,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_initializer_return_expressions_in_function_body_statement(
                        labeled.statement,
                        return_exprs,
                    );
                }
            }
            _ => {}
        }
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

    fn statement_has_wrapped_self_call_in_return(
        &self,
        stmt_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
        require_wrapped_call: bool,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => self
                .ctx
                .arena
                .get_return_statement(node)
                .is_some_and(|ret| {
                    ret.expression.is_some()
                        && self.expression_has_wrapped_self_call_in_return(
                            ret.expression,
                            function_sym,
                            require_wrapped_call,
                        )
                }),
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block.statements.nodes.iter().copied().any(|stmt| {
                    self.statement_has_wrapped_self_call_in_return(
                        stmt,
                        function_sym,
                        require_wrapped_call,
                    )
                })
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.statement_has_wrapped_self_call_in_return(
                            if_data.then_statement,
                            function_sym,
                            require_wrapped_call,
                        ) || (if_data.else_statement.is_some()
                            && self.statement_has_wrapped_self_call_in_return(
                                if_data.else_statement,
                                function_sym,
                                require_wrapped_call,
                            ))
                    })
            }
            syntax_kind_ext::SWITCH_STATEMENT => self
                .ctx
                .arena
                .get_switch(node)
                .and_then(|switch_data| self.ctx.arena.get(switch_data.case_block))
                .and_then(|case_block_node| self.ctx.arena.get_block(case_block_node))
                .is_some_and(|case_block| {
                    case_block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|clause_idx| {
                            self.ctx
                                .arena
                                .get(clause_idx)
                                .and_then(|clause_node| self.ctx.arena.get_case_clause(clause_node))
                                .is_some_and(|clause| {
                                    clause.statements.nodes.iter().copied().any(|stmt| {
                                        self.statement_has_wrapped_self_call_in_return(
                                            stmt,
                                            function_sym,
                                            require_wrapped_call,
                                        )
                                    })
                                })
                        })
                }),
            syntax_kind_ext::TRY_STATEMENT => {
                self.ctx.arena.get_try(node).is_some_and(|try_data| {
                    self.statement_has_wrapped_self_call_in_return(
                        try_data.try_block,
                        function_sym,
                        require_wrapped_call,
                    ) || (try_data.catch_clause.is_some()
                        && self.statement_has_wrapped_self_call_in_return(
                            try_data.catch_clause,
                            function_sym,
                            require_wrapped_call,
                        ))
                        || (try_data.finally_block.is_some()
                            && self.statement_has_wrapped_self_call_in_return(
                                try_data.finally_block,
                                function_sym,
                                require_wrapped_call,
                            ))
                })
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                self.ctx
                    .arena
                    .get_catch_clause(node)
                    .is_some_and(|catch_data| {
                        self.statement_has_wrapped_self_call_in_return(
                            catch_data.block,
                            function_sym,
                            require_wrapped_call,
                        )
                    })
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                self.ctx.arena.get_loop(node).is_some_and(|loop_data| {
                    self.statement_has_wrapped_self_call_in_return(
                        loop_data.statement,
                        function_sym,
                        require_wrapped_call,
                    )
                })
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                self.ctx.arena.get_for_in_of(node).is_some_and(|loop_data| {
                    self.statement_has_wrapped_self_call_in_return(
                        loop_data.statement,
                        function_sym,
                        require_wrapped_call,
                    )
                })
            }
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_some_and(|labeled| {
                    self.statement_has_wrapped_self_call_in_return(
                        labeled.statement,
                        function_sym,
                        require_wrapped_call,
                    )
                }),
            _ => false,
        }
    }

    fn expression_has_wrapped_self_call_in_return(
        &self,
        expr_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
        require_wrapped_call: bool,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        if let Some(sym_id) = (node.kind == SyntaxKind::Identifier as u16)
            .then(|| self.resolve_identifier_symbol(expr_idx))
            .flatten()
            && sym_id == function_sym
        {
            return self.identifier_flows_through_wrapped_call(expr_idx, require_wrapped_call);
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
            return false;
        }

        self.ctx
            .arena
            .get_children(expr_idx)
            .into_iter()
            .any(|child_idx| {
                self.expression_has_wrapped_self_call_in_return(
                    child_idx,
                    function_sym,
                    require_wrapped_call,
                )
            })
    }

    fn identifier_flows_through_wrapped_call(
        &self,
        ident_idx: NodeIndex,
        require_wrapped_call: bool,
    ) -> bool {
        let mut current = ident_idx;
        let mut saw_wrapper = false;

        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::NON_NULL_EXPRESSION
                | syntax_kind_ext::AS_EXPRESSION
                | syntax_kind_ext::TYPE_ASSERTION
                | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    current = parent_idx;
                }
                syntax_kind_ext::CALL_EXPRESSION => {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent_node)
                        .is_some_and(|call| {
                            call.expression == current && (saw_wrapper || !require_wrapped_call)
                        });
                }
                syntax_kind_ext::NEW_EXPRESSION => {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent_node)
                        .is_some_and(|call| {
                            call.expression == current && (saw_wrapper || !require_wrapped_call)
                        });
                }
                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                    return self
                        .ctx
                        .arena
                        .get_tagged_template(parent_node)
                        .is_some_and(|tagged| {
                            tagged.tag == current && (saw_wrapper || !require_wrapped_call)
                        });
                }
                syntax_kind_ext::RETURN_STATEMENT => return false,
                _ => {
                    saw_wrapper = true;
                    current = parent_idx;
                }
            }
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
