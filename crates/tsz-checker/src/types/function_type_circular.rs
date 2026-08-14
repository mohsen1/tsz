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
        no_contextual_return: bool,
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

        // A circular-return site is "lazy" (a benign self-reference that `tsc`
        // resolves on demand rather than reporting as TS7022/TS7023) when the
        // deferred function has no contextual return type AND its body does not
        // recursively invoke the resolving variable in callee position. The site
        // is still recorded normally so the variable's type still widens to
        // `any` (cycle break, unchanged from before); the lazy marker only
        // suppresses the spurious diagnostic at the emission site. See issue
        // #10675 (`define<T>(spec: T): T; const api = define({ refresh: () => api })`).
        let is_lazy = no_contextual_return
            && !self.return_body_has_callee_self_invocation(body_idx, &resolving_vars);

        for sym_id in found {
            let sites = self
                .ctx
                .pending_circular_return_sites
                .sites
                .entry(sym_id)
                .or_default();
            if !sites.contains(&function_idx) {
                sites.push(function_idx);
            }
            if is_lazy {
                let lazy_sites = self
                    .ctx
                    .pending_circular_return_sites
                    .lazy
                    .entry(sym_id)
                    .or_default();
                if !lazy_sites.contains(&function_idx) {
                    lazy_sites.push(function_idx);
                }
            }
        }
    }

    /// Whether every recorded circular-return `site` for `sym_id` is a benign
    /// lazy self-reference (see [`Self::record_pending_circular_return_sites`]).
    /// When true, the variable's TS7022/TS7023 emission is suppressed even
    /// though its type still widened to `any`, matching `tsc`, which accepts
    /// such deferred self-references without a diagnostic.
    pub(crate) fn all_circular_return_sites_are_lazy(
        &self,
        sym_id: tsz_binder::SymbolId,
        sites: &[NodeIndex],
    ) -> bool {
        if sites.is_empty() {
            return false;
        }
        let Some(lazy) = self.ctx.pending_circular_return_sites.lazy.get(&sym_id) else {
            return false;
        };
        sites.iter().all(|site| lazy.contains(site))
    }

    /// Whether `function_idx` has been recorded as a *non-lazy* circular return
    /// site for any resolving variable — the same condition that drives the
    /// TS7023 emission (a genuine implicit-`any` circularity, not a benign
    /// deferred self-reference like #10675). Used to resolve such a function's
    /// inferred return type to the circular `any` rather than the degenerate
    /// `void`/`never` that return aggregation produces once the direct self-call
    /// return is dropped.
    ///
    /// This is what distinguishes a *variable-bound* self-recursive function
    /// expression / arrow (`var f = function(n){ return f(n); }` — the variable
    /// resolution is circular, so tsc's return type is `any`) from a *clean*
    /// no-base-case recursion in a named function declaration
    /// (`function f(n){ return f(n); }` → `never`): only the former is a
    /// resolving *variable*, so only it is ever recorded here.
    pub(crate) fn function_is_nonlazy_circular_return_site(&self, function_idx: NodeIndex) -> bool {
        // Fast path for the overwhelming common case: no circular return sites are
        // recorded at all (only variable-bound self-recursion mid-resolution ever
        // populates this map), so a void/never return-type inference pays nothing.
        if self.ctx.pending_circular_return_sites.sites.is_empty() {
            return false;
        }
        self.ctx
            .pending_circular_return_sites
            .sites
            .iter()
            .any(|(sym_id, sites)| {
                sites.contains(&function_idx)
                    && !self
                        .ctx
                        .pending_circular_return_sites
                        .lazy
                        .get(sym_id)
                        .is_some_and(|lazy| lazy.contains(&function_idx))
            })
    }

    pub(super) fn return_body_has_resolving_var_in_call_like(&self, body_idx: NodeIndex) -> bool {
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
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                return block.statements.nodes.iter().copied().any(|stmt_idx| {
                    self.statement_has_resolving_var_in_call_like_return(stmt_idx, &resolving_vars)
                });
            }
            return false;
        }

        self.expression_has_resolving_var_in_call_like(body_idx, &resolving_vars)
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
            return self.statement_has_wrapped_self_call_in_return(body_idx, sym_id, true, true);
        }

        self.expression_has_wrapped_self_call_in_return(body_idx, sym_id, true, true)
    }

    /// Whether a `const`/`let`/`var` initializer that is a function /
    /// function-expression / arrow carries an EXPLICIT return-type annotation.
    /// Such a function has a known return type, so a recursive self-reference is
    /// not an implicit-`any` return circularity and must not trigger TS7023 —
    /// mirroring `tsc` and the existing exemption for annotated function
    /// declarations (`function f(): number { return f(); }` is already clean).
    pub(crate) fn function_like_initializer_has_explicit_return_annotation(
        &self,
        init_idx: NodeIndex,
    ) -> bool {
        self.ctx
            .arena
            .get(init_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .is_some_and(|func| func.type_annotation.is_some())
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
                self.function_body_has_wrapped_self_call_in_every_return(
                    body_idx, sym_id, false, true,
                )
            } else {
                self.expression_has_wrapped_self_call_in_return(body_idx, sym_id, false, true)
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
    ///
    /// A self-reference in the callee position of a `new` expression
    /// (`return new Self(...)`) never counts here, even though it is otherwise
    /// syntactically a "direct self-call": `tsc` does not treat this as
    /// non-terminating recursion. A JS "constructor function" without an
    /// explicit `@constructor`/`@class` JSDoc tag has no construct signature
    /// (TS7009), so `new Self(...)` still *produces a value* (implicitly `any`)
    /// rather than diverging — the `never` collapse this predicate feeds must
    /// not apply. See `constructorFunctionsStrict.ts`: `function A(x) { if
    /// (!(this instanceof A)) { return new A(x) } this.x = x }` must infer
    /// `A`'s return type as `any` (so `A(1)` is usable), not `never`.
    pub(crate) fn all_returns_are_direct_self_calls(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
    ) -> bool {
        // Every return must have a self-call (direct or wrapped), and `new
        // Self(...)` must not count as one (see doc comment above).
        if !self.function_body_has_wrapped_self_call_in_every_return(
            body_idx,
            function_sym,
            false,
            false,
        ) {
            return false;
        }
        // None of the returns should be wrapped (they must all be direct)
        !self.function_has_wrapped_self_call_in_return_expression_for_sym(body_idx, function_sym)
    }

    /// Check if any return expression in a function body has a WRAPPED self-call
    /// (goes through array access, property access, etc.). Used only by
    /// [`Self::all_returns_are_direct_self_calls`], so `new Self(...)` is
    /// excluded from counting as a self-call here too.
    fn function_has_wrapped_self_call_in_return_expression_for_sym(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
    ) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };

        if body_node.kind == syntax_kind_ext::BLOCK {
            return self.statement_has_wrapped_self_call_in_return(
                body_idx,
                function_sym,
                true,
                false,
            );
        }

        self.expression_has_wrapped_self_call_in_return(body_idx, function_sym, true, false)
    }

    fn function_body_has_wrapped_self_call_in_every_return(
        &self,
        body_idx: NodeIndex,
        function_sym: tsz_binder::SymbolId,
        require_wrapped_call: bool,
        count_new: bool,
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
                count_new,
            )
        })
    }

    pub(crate) fn collect_initializer_return_expressions_in_function_body(
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
        count_new: bool,
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
                            count_new,
                        )
                }),
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block.statements.nodes.iter().copied().any(|stmt| {
                    self.statement_has_wrapped_self_call_in_return(
                        stmt,
                        function_sym,
                        require_wrapped_call,
                        count_new,
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
                            count_new,
                        ) || (if_data.else_statement.is_some()
                            && self.statement_has_wrapped_self_call_in_return(
                                if_data.else_statement,
                                function_sym,
                                require_wrapped_call,
                                count_new,
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
                                            count_new,
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
                        count_new,
                    ) || (try_data.catch_clause.is_some()
                        && self.statement_has_wrapped_self_call_in_return(
                            try_data.catch_clause,
                            function_sym,
                            require_wrapped_call,
                            count_new,
                        ))
                        || (try_data.finally_block.is_some()
                            && self.statement_has_wrapped_self_call_in_return(
                                try_data.finally_block,
                                function_sym,
                                require_wrapped_call,
                                count_new,
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
                            count_new,
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
                        count_new,
                    )
                })
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                self.ctx.arena.get_for_in_of(node).is_some_and(|loop_data| {
                    self.statement_has_wrapped_self_call_in_return(
                        loop_data.statement,
                        function_sym,
                        require_wrapped_call,
                        count_new,
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
                        count_new,
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
        count_new: bool,
    ) -> bool {
        if self.expression_is_void_prefix_unary(expr_idx) {
            return false;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && !self.identifier_is_non_value_name_position(expr_idx)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && sym_id == function_sym
        {
            return self.identifier_flows_through_wrapped_call(
                expr_idx,
                require_wrapped_call,
                count_new,
            );
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
                    count_new,
                )
            })
    }

    fn identifier_flows_through_wrapped_call(
        &self,
        ident_idx: NodeIndex,
        require_wrapped_call: bool,
        count_new: bool,
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
                    if !count_new {
                        return false;
                    }
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
        if self.expression_is_void_prefix_unary(expr_idx) {
            return;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && !self.identifier_is_non_value_name_position(expr_idx)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
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

    fn statement_has_resolving_var_in_call_like_return(
        &self,
        stmt_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
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
                        && self.expression_has_resolving_var_in_call_like(
                            ret.expression,
                            resolving_vars,
                        )
                }),
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block.statements.nodes.iter().copied().any(|stmt| {
                    self.statement_has_resolving_var_in_call_like_return(stmt, resolving_vars)
                })
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.statement_has_resolving_var_in_call_like_return(
                            if_data.then_statement,
                            resolving_vars,
                        ) || (if_data.else_statement.is_some()
                            && self.statement_has_resolving_var_in_call_like_return(
                                if_data.else_statement,
                                resolving_vars,
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
                                        self.statement_has_resolving_var_in_call_like_return(
                                            stmt,
                                            resolving_vars,
                                        )
                                    })
                                })
                        })
                }),
            syntax_kind_ext::TRY_STATEMENT => {
                self.ctx.arena.get_try(node).is_some_and(|try_data| {
                    self.statement_has_resolving_var_in_call_like_return(
                        try_data.try_block,
                        resolving_vars,
                    ) || (try_data.catch_clause.is_some()
                        && self.statement_has_resolving_var_in_call_like_return(
                            try_data.catch_clause,
                            resolving_vars,
                        ))
                        || (try_data.finally_block.is_some()
                            && self.statement_has_resolving_var_in_call_like_return(
                                try_data.finally_block,
                                resolving_vars,
                            ))
                })
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                self.ctx
                    .arena
                    .get_catch_clause(node)
                    .is_some_and(|catch_data| {
                        self.statement_has_resolving_var_in_call_like_return(
                            catch_data.block,
                            resolving_vars,
                        )
                    })
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                self.ctx.arena.get_loop(node).is_some_and(|loop_data| {
                    self.statement_has_resolving_var_in_call_like_return(
                        loop_data.statement,
                        resolving_vars,
                    )
                })
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                self.ctx.arena.get_for_in_of(node).is_some_and(|loop_data| {
                    self.statement_has_resolving_var_in_call_like_return(
                        loop_data.statement,
                        resolving_vars,
                    )
                })
            }
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_some_and(|labeled| {
                    self.statement_has_resolving_var_in_call_like_return(
                        labeled.statement,
                        resolving_vars,
                    )
                }),
            _ => false,
        }
    }

    fn expression_has_resolving_var_in_call_like(
        &self,
        expr_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if self.expression_is_void_prefix_unary(expr_idx) {
            return false;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16
            && !self.identifier_is_non_value_name_position(expr_idx)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && resolving_vars.contains(&sym_id)
        {
            return self.identifier_flows_through_call_like(expr_idx);
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
                self.expression_has_resolving_var_in_call_like(child_idx, resolving_vars)
            })
    }

    fn identifier_flows_through_call_like(&self, ident_idx: NodeIndex) -> bool {
        let mut current = ident_idx;
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

            if matches!(
                parent_node.kind,
                syntax_kind_ext::CALL_EXPRESSION
                    | syntax_kind_ext::NEW_EXPRESSION
                    | syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            ) {
                return true;
            }
            if matches!(
                parent_node.kind,
                syntax_kind_ext::RETURN_STATEMENT
                    | syntax_kind_ext::FUNCTION_DECLARATION
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
            current = parent_idx;
        }
    }

    /// Whether any return expression in `body_idx` recursively *invokes* one of
    /// `resolving_vars` in **callee** position — e.g. `() => api.loop()`,
    /// `() => (0, api.loop)()`, `() => (c ? api.a : api.b)()`. Unlike
    /// [`Self::return_body_has_resolving_var_in_call_like`], a reference that is
    /// merely a call *argument* (`() => helper(api)`) does NOT count, because the
    /// callee's return type does not depend on the resolving variable.
    ///
    /// This is the precise "the deferred function's return type genuinely
    /// depends on itself" predicate used to decide whether a circular-return
    /// site is a real circularity (`tsc` reports TS7023) or a benign lazily
    /// resolved self-reference. It does not affect which sites are *recorded*;
    /// it only refines whether the recorded site is reported.
    pub(super) fn return_body_has_callee_self_invocation(
        &self,
        body_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                return block.statements.nodes.iter().copied().any(|stmt_idx| {
                    self.statement_has_callee_self_invocation(stmt_idx, resolving_vars)
                });
            }
            return false;
        }
        self.expression_has_callee_self_invocation(body_idx, resolving_vars)
    }

    fn statement_has_callee_self_invocation(
        &self,
        stmt_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
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
                        && self
                            .expression_has_callee_self_invocation(ret.expression, resolving_vars)
                }),
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .any(|stmt| self.statement_has_callee_self_invocation(stmt, resolving_vars))
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.statement_has_callee_self_invocation(
                            if_data.then_statement,
                            resolving_vars,
                        ) || (if_data.else_statement.is_some()
                            && self.statement_has_callee_self_invocation(
                                if_data.else_statement,
                                resolving_vars,
                            ))
                    })
            }
            _ => false,
        }
    }

    fn expression_has_callee_self_invocation(
        &self,
        expr_idx: NodeIndex,
        resolving_vars: &FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if self.expression_is_void_prefix_unary(expr_idx) {
            return false;
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16
            && !self.identifier_is_non_value_name_position(expr_idx)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && resolving_vars.contains(&sym_id)
        {
            return self.identifier_flows_through_callee_position(expr_idx);
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
            .any(|child_idx| self.expression_has_callee_self_invocation(child_idx, resolving_vars))
    }

    /// Whether the resolving-variable reference at `ident_idx` flows into the
    /// **callee** position of a call/new/tagged-template (`api.loop()`), walking
    /// through transparent wrappers, object-side member access, comma right
    /// operands, and conditional branches. A reference appearing only as a call
    /// *argument* (`helper(api)`) returns `false`.
    fn identifier_flows_through_callee_position(&self, ident_idx: NodeIndex) -> bool {
        let mut current = ident_idx;
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
                // Transparent wrappers: the value flows outward unchanged.
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::NON_NULL_EXPRESSION
                | syntax_kind_ext::AS_EXPRESSION
                | syntax_kind_ext::TYPE_ASSERTION
                | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    current = parent_idx;
                }
                // Member access keeps the variable on the callee path only when
                // it is the object being accessed (`api.loop`), not a computed
                // index (`obj[api]`).
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                    let is_object = self
                        .ctx
                        .arena
                        .get_access_expr(parent_node)
                        .is_some_and(|access| access.expression == current);
                    if !is_object {
                        return false;
                    }
                    current = parent_idx;
                }
                syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION => {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent_node)
                        .is_some_and(|call| call.expression == current);
                }
                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                    return self
                        .ctx
                        .arena
                        .get_tagged_template(parent_node)
                        .is_some_and(|tagged| tagged.tag == current);
                }
                // A comma expression yields its right operand, so the variable
                // stays on the callee path only through that operand
                // (`(0, api.loop)()`); the discarded left operand does not.
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let flows = self
                        .ctx
                        .arena
                        .get_binary_expr(parent_node)
                        .is_some_and(|bin| {
                            bin.operator_token == SyntaxKind::CommaToken as u16
                                && bin.right == current
                        });
                    if !flows {
                        return false;
                    }
                    current = parent_idx;
                }
                // A conditional yields whichever branch is taken, so either
                // branch keeps the variable on the callee path
                // (`(c ? api.loop : api.loop)()`); the condition does not.
                syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                    let flows = self
                        .ctx
                        .arena
                        .get_conditional_expr(parent_node)
                        .is_some_and(|cond| {
                            cond.when_true == current || cond.when_false == current
                        });
                    if !flows {
                        return false;
                    }
                    current = parent_idx;
                }
                _ => return false,
            }
        }
    }

    pub(crate) fn expression_is_void_prefix_unary(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_unary_expr(node)
                    .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
        })
    }
}
