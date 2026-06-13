//! Cache invalidation helpers for `CheckerState`.
//!
//! These methods manage targeted and recursive clearing of cached type
//! information. They are used when contextual type information changes
//! (e.g., during generic call inference rounds or contextual retyping of
//! function parameters) and previously cached results must be discarded.

use super::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Maximum recursion depth for the contextual-retry cache-invalidation walker
/// family. Real expression trees nest far below this; the cap exists only to
/// turn a pathological self-referential expression-child link (which would
/// otherwise recurse until the native stack overflows) into early termination.
/// Chosen well above any legitimate expression nesting so the cap is never hit
/// on a finite (acyclic) walk, keeping the guard byte-identical on every
/// non-crashing input.
const MAX_CONTEXTUAL_RETRY_DEPTH: u32 = 2048;

thread_local! {
    /// Active recursion depth of the contextual-retry cache-invalidation walker
    /// ([`CheckerState::invalidate_expression_for_contextual_retry`] and
    /// [`CheckerState::invalidate_function_like_for_contextual_retry`]). These
    /// descend through expression children, normally a finite tree. A
    /// pathological arena can expose a cyclic child link; bounding depth makes
    /// the walk finite instead of overflowing the native stack. A depth bound
    /// (rather than a visited set) is used because the walker legitimately
    /// re-invalidates the same node across overload-retry rounds, so "already
    /// seen" is not equivalent to "cyclic" — only unbounded depth is. Thread-
    /// local because each file is checked on a single worker thread.
    static CONTEXTUAL_RETRY_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Kill-switch: set `TSZ_DISABLE_CONTEXTUAL_RETRY_CYCLE_GUARD` to a non-empty,
/// non-`0` value to bypass the depth guard. Used to prove the guard is
/// byte-identical on inputs that do not overflow (the guard only ever
/// short-circuits recursion deeper than `MAX_CONTEXTUAL_RETRY_DEPTH`, which no
/// finite expression tree reaches, so disabling it must not change diagnostics
/// for any non-crashing input).
fn contextual_retry_cycle_guard_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_CONTEXTUAL_RETRY_CYCLE_GUARD")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Reset the contextual-retry recursion depth.
///
/// [`ContextualRetryGuard`] already decrements on scope exit (including panic
/// unwind), so the counter is normally zero between top-level invalidation
/// requests. It is reset at independent-compilation boundaries anyway so a
/// future non-unwinding bail-out can never leak a non-zero depth across rows.
pub(crate) fn reset_contextual_retry_path() {
    CONTEXTUAL_RETRY_DEPTH.with(|d| d.set(0));
}

#[cfg(test)]
fn contextual_retry_depth_for_test() -> u32 {
    CONTEXTUAL_RETRY_DEPTH.with(std::cell::Cell::get)
}

/// RAII guard that increments the contextual-retry recursion depth for the
/// duration of one walker frame and decrements it on drop (including panic
/// unwind).
struct ContextualRetryGuard {
    tracked: bool,
}

impl ContextualRetryGuard {
    /// Enter one walker frame. Returns `Some(guard)` while depth is within the
    /// cap (caller should recurse), or `None` once the cap is reached (caller
    /// must stop — the only way to reach it is a non-terminating cyclic link).
    /// When the kill-switch is set, always returns a non-tracking guard so the
    /// walk runs exactly as before.
    fn enter() -> Option<Self> {
        if contextual_retry_cycle_guard_disabled() {
            return Some(Self { tracked: false });
        }
        let next = CONTEXTUAL_RETRY_DEPTH.with(|d| {
            let v = d.get();
            if v >= MAX_CONTEXTUAL_RETRY_DEPTH {
                None
            } else {
                d.set(v + 1);
                Some(v + 1)
            }
        });
        next.map(|_| Self { tracked: true })
    }
}

impl Drop for ContextualRetryGuard {
    fn drop(&mut self) {
        if self.tracked {
            CONTEXTUAL_RETRY_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn clear_binding_name_symbol_cache_recursive(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(name_idx) {
            self.ctx.symbol_types.remove(&sym_id);
        }

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == SyntaxKind::Identifier as u16 {
            return;
        }

        if (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            && let Some(pattern) = self.ctx.arena.get_binding_pattern(name_node)
        {
            for &element_idx in &pattern.elements.nodes {
                if element_idx.is_none() {
                    continue;
                }
                if let Some(sym_id) = self.ctx.binder.get_node_symbol(element_idx) {
                    self.ctx.symbol_types.remove(&sym_id);
                }
                if let Some(element_node) = self.ctx.arena.get(element_idx)
                    && let Some(element) = self.ctx.arena.get_binding_element(element_node)
                {
                    self.clear_binding_name_symbol_cache_recursive(element.name);
                    if element.initializer.is_some() {
                        self.clear_type_cache_recursive(element.initializer);
                    }
                }
            }
        }
    }

    /// Clear the contextual resolution cache.
    ///
    /// This is used before recomputing argument types during generic call
    /// inference rounds so that stale contextual type resolutions are not reused.
    pub(crate) fn clear_contextual_resolution_cache(&mut self) {
        self.ctx
            .flow_shared
            .narrowing_cache
            .contextual_resolve_cache
            .borrow_mut()
            .clear();
    }

    // -----------------------------------------------------------------------
    // Targeted invalidation helpers
    //
    // These replace blanket `clear_type_cache_recursive` in hot paths where
    // the subsequent re-evaluation uses `get_type_of_node_with_request` with
    // a non-empty request.  A non-empty request bypasses `node_types` for
    // non-audited node kinds and uses `request_node_types` for audited kinds,
    // so deep recursive clearing of children is unnecessary — only the
    // top-level node and context-sensitive bookkeeping need updating.
    // -----------------------------------------------------------------------

    /// Invalidate a single node's cached type entries without recursion.
    ///
    /// Removes the node from both `node_types` (empty-request cache) and
    /// `request_node_types` (request-aware cache).  Does **not** touch
    /// children, symbol types, or implicit-any tracking.
    pub(crate) fn invalidate_node_type_cache(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }
        self.ctx.request_cache_counters.targeted_nodes_invalidated += 1;
        self.ctx.node_types.remove(&idx.0);
        if !self.ctx.request_node_types.is_empty() {
            self.ctx
                .request_node_types
                .retain(|(node_idx, _), _| *node_idx != idx.0);
        }
    }

    /// Update implicit-any closure tracking for a node being invalidated.
    ///
    /// Mirrors the bookkeeping in `clear_type_cache_recursive` but for a
    /// single node only.
    fn invalidate_implicit_any_tracking(&mut self, idx: NodeIndex) {
        if self.ctx.implicit_any_contextual_closures.contains(&idx) {
            self.ctx.implicit_any_checked_closures.insert(idx);
        } else {
            self.ctx.implicit_any_checked_closures.remove(&idx);
        }
    }

    /// Invalidate parameter symbol types for a single parameter node.
    ///
    /// Clears the parameter's symbol(s) from `symbol_types` so they will be
    /// re-inferred from the new contextual signature.  Also invalidates the
    /// parameter default initializer node (if any).
    fn invalidate_function_param_symbols(&mut self, param_idx: NodeIndex) {
        if let Some(param_node) = self.ctx.arena.get(param_idx)
            && let Some(param) = self.ctx.arena.get_parameter(param_node)
        {
            for sym_id in self
                .parameter_symbol_ids(param_idx, param.name)
                .into_iter()
                .flatten()
            {
                self.ctx.symbol_types.remove(&sym_id);
            }
            if param.initializer.is_some() {
                self.invalidate_node_type_cache(param.initializer);
            }
        }
    }

    /// Invalidate a function-like node for contextual retry.
    ///
    /// This covers function expressions, methods, and accessors that are about
    /// to be re-evaluated under a non-empty typing request. The function-like
    /// node itself, parameter symbol types, and its body are invalidated, while
    /// avoiding blanket recursive clearing of unrelated siblings.
    pub(crate) fn invalidate_function_like_for_contextual_retry(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        // Share the contextual-retry depth guard with
        // `invalidate_expression_for_contextual_retry`: that walker recurses into
        // this method for function-expression arguments (e.g. the comparator in
        // `xs.sort(cmp)`), and a cyclic expression-child link can pass through
        // here, so the shared depth bound must span both to make the cycle
        // finite.
        let Some(_retry_guard) = ContextualRetryGuard::enter() else {
            return;
        };

        self.ctx.request_cache_counters.targeted_invalidation_calls += 1;
        self.invalidate_node_type_cache(idx);
        self.invalidate_implicit_any_tracking(idx);

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                if let Some(func) = self.ctx.arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        self.invalidate_function_param_symbols(param_idx);
                    }
                    if let Some(body_node) = self.ctx.arena.get(func.body) {
                        if body_node.kind == syntax_kind_ext::BLOCK {
                            self.clear_type_cache_recursive(func.body);
                        } else {
                            self.invalidate_expression_for_contextual_retry(func.body);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    for &param_idx in &method.parameters.nodes {
                        self.invalidate_function_param_symbols(param_idx);
                    }
                    self.clear_type_cache_recursive(method.body);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    for &param_idx in &accessor.parameters.nodes {
                        self.invalidate_function_param_symbols(param_idx);
                    }
                    self.clear_type_cache_recursive(accessor.body);
                }
            }
            _ => self.invalidate_expression_for_contextual_retry(idx),
        }
    }

    /// Invalidate a call argument expression for contextual retry.
    ///
    /// This is the targeted replacement for `clear_type_cache_recursive` in
    /// call argument refresh paths.  It leverages the fact that
    /// `get_type_of_node_with_request` with a non-empty request bypasses
    /// `node_types` for non-audited node kinds, so only the top-level node
    /// and context-sensitive bookkeeping need clearing.
    ///
    /// For contextually-sensitive forms (function expressions, object/array
    /// literals), this also clears the immediate subtree that depends on the
    /// contextual parameter type — but does NOT perform a deep recursive walk.
    pub(crate) fn invalidate_expression_for_contextual_retry(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        // Guard against pathological self-referential expression-child links.
        // This method and `invalidate_function_like_for_contextual_retry`
        // recurse over expression children, normally a finite tree. A small
        // number of inputs produce an arena whose expression-child links form a
        // cycle (observed on a chained-call shape such as
        // `[...xs].sort(cmp).slice(0, n)` reached through cross-file generic
        // instantiation), which would otherwise recurse until the native stack
        // overflows. A shared depth bound (`MAX_CONTEXTUAL_RETRY_DEPTH`) makes
        // the walk finite. A depth bound rather than a visited set is used
        // because the walker legitimately re-invalidates the same node across
        // overload-retry rounds, so the cap is only ever reached by a
        // non-terminating cyclic link — it is byte-identical on every finite
        // (acyclic) input.
        let Some(_retry_guard) = ContextualRetryGuard::enter() else {
            return;
        };

        self.ctx.request_cache_counters.targeted_invalidation_calls += 1;

        let Some(node) = self.ctx.arena.get(idx) else {
            self.invalidate_node_type_cache(idx);
            return;
        };

        match node.kind {
            // ---- Context-sensitive forms that need deeper clearing ----
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.invalidate_function_like_for_contextual_retry(idx);
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                // Clear the object literal node and each property.  For
                // property initializers that are function expressions, those
                // need recursive body clearing (same reason as above).
                // Simple initializers get only a node-level clear.
                self.invalidate_node_type_cache(idx);
                self.invalidate_implicit_any_tracking(idx);
                if let Some(obj) = self.ctx.arena.get_literal_expr(node) {
                    for &prop_idx in &obj.elements.nodes {
                        self.invalidate_node_type_cache(prop_idx);
                        if let Some(prop_node) = self.ctx.arena.get(prop_idx) {
                            if let Some(prop) = self.ctx.arena.get_property_assignment(prop_node) {
                                // Recurse into the initializer — if it's a function,
                                // the function branch handles deep clearing.
                                self.invalidate_expression_for_contextual_retry(prop.initializer);
                            } else if let Some(method) = self.ctx.arena.get_method_decl(prop_node) {
                                // Method declarations in object literals also need
                                // body clearing. `this` keyword nodes inside the body
                                // cache their types from the first evaluation pass;
                                // when contextual types change (e.g., ThisType<T>
                                // markers becoming available during overload retry),
                                // stale `this` types cause false TS2322 errors.
                                self.invalidate_implicit_any_tracking(prop_idx);
                                for &param_idx in &method.parameters.nodes {
                                    self.invalidate_function_param_symbols(param_idx);
                                }
                                self.clear_type_cache_recursive(method.body);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                // Clear the array and each element.  Function-expression
                // elements get recursive body clearing via recursion.
                self.invalidate_node_type_cache(idx);
                self.invalidate_implicit_any_tracking(idx);
                if let Some(array) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &array.elements.nodes {
                        self.invalidate_expression_for_contextual_retry(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                self.invalidate_implicit_any_tracking(idx);
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.invalidate_expression_for_contextual_retry(call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg_idx in &args.nodes {
                            self.invalidate_expression_for_contextual_retry(arg_idx);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.invalidate_expression_for_contextual_retry(access.expression);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    self.invalidate_expression_for_contextual_retry(access.expression);
                    self.invalidate_expression_for_contextual_retry(access.name_or_argument);
                }
            }
            // ---- Wrapper expressions: recurse into inner ----
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.invalidate_expression_for_contextual_retry(paren.expression);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    // Condition doesn't depend on contextual type, skip it.
                    self.invalidate_expression_for_contextual_retry(cond.when_true);
                    self.invalidate_expression_for_contextual_retry(cond.when_false);
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                self.invalidate_node_type_cache(idx);
                if let Some(spread) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.invalidate_expression_for_contextual_retry(spread.expression);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.invalidate_node_type_cache(idx);
                if let Some(as_expr) = self.ctx.arena.get_type_assertion(node) {
                    self.invalidate_expression_for_contextual_retry(as_expr.expression);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                self.invalidate_node_type_cache(idx);
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.invalidate_expression_for_contextual_retry(unary.expression);
                }
            }
            // ---- Simple expressions: node-level clear only ----
            _ => {
                // For simple expressions (identifier, binary, call, access,
                // literals, etc.), the node-level clear is sufficient.  These
                // evaluate the same regardless of contextual type — the request
                // path bypasses node_types for non-audited kinds, and children
                // that don't depend on context correctly hit node_types.
                self.invalidate_node_type_cache(idx);
            }
        }
    }

    /// Invalidate a function body for contextual parameter retyping.
    ///
    /// Recursively clears the body cache so expressions that reference
    /// parameters get recomputed with the new parameter types.  This is
    /// narrower than a blanket `clear_type_cache_recursive` on the entire
    /// function expression because it targets the body only — the function
    /// node itself and parameter symbol types are managed externally by
    /// `cache_parameter_types` (which must be called BEFORE this helper).
    pub(crate) fn invalidate_function_body_for_param_retyping(&mut self, body: NodeIndex) {
        self.ctx.request_cache_counters.targeted_invalidation_calls += 1;

        // For expression-bodied arrows, use targeted invalidation since the
        // body is a single expression tree. Block bodies use recursive clearing
        // because they contain statement lists referencing changed param types.
        if let Some(body_node) = self.ctx.arena.get(body)
            && body_node.kind != syntax_kind_ext::BLOCK
        {
            self.invalidate_expression_for_contextual_retry(body);
            return;
        }
        self.clear_type_cache_recursive(body);
    }

    /// Invalidate an initializer expression for a context change.
    ///
    /// Used in variable-checking paths when the contextual type changes
    /// (e.g., JSDoc blocks callable context) or when the checked pass needs
    /// to revisit a previously-cached initializer.  For function-like
    /// initializers, clears parameter symbol types and recursively clears
    /// the body.  For other forms, clears the initializer node only.
    pub(crate) fn invalidate_initializer_for_context_change(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        self.ctx.request_cache_counters.targeted_invalidation_calls += 1;

        self.invalidate_node_type_cache(idx);
        self.invalidate_implicit_any_tracking(idx);

        if let Some(init_node) = self.ctx.arena.get(idx) {
            match init_node.kind {
                k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    if let Some(func) = self.ctx.arena.get_function(init_node) {
                        for &param_idx in &func.parameters.nodes {
                            self.invalidate_function_param_symbols(param_idx);
                        }
                        // For expression-bodied arrows, use targeted invalidation.
                        // Block bodies need recursive clearing since they contain
                        // statements that reference the changed parameter types.
                        if let Some(body_node) = self.ctx.arena.get(func.body) {
                            if body_node.kind == syntax_kind_ext::BLOCK {
                                self.clear_type_cache_recursive(func.body);
                            } else {
                                self.invalidate_expression_for_contextual_retry(func.body);
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::NEW_EXPRESSION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    // These are re-checked in maybe_clear_checked_initializer_type_cache;
                    // the node-level clear is sufficient here.
                }
                _ => {}
            }
        }
    }

    /// Clear type cache for a node and all its children recursively.
    ///
    /// This is used when we need to recompute types with different contextual information,
    /// such as when checking return statements with contextual return types.
    pub(crate) fn clear_type_cache_recursive(&mut self, idx: NodeIndex) {
        self.ctx
            .request_cache_counters
            .clear_type_cache_recursive_calls += 1;

        if idx.is_none() {
            return;
        }

        // Do not clear the cache for a node that is currently being resolved.
        // Clearing would cause the recompute to hit the circular-reference guard
        // in `get_type_of_node_with_request` and produce `TypeId::ERROR`, which
        // would then overwrite the correct cached type. This is observable when
        // a class property initializer (e.g. `prop = (x: T) => U`) is in the
        // middle of being typed: a re-entry to `build_class_instance_type` from
        // inside the initializer's compute would otherwise wipe the
        // in-progress entry and replace it with ERROR.
        if self.ctx.node_resolution_stack.contains(&idx) {
            return;
        }

        // PERF: Skip clearing for nodes that never benefit from contextual retyping.
        // `null` is always TypeId::NULL, `true`/`false` are always boolean literals,
        // and regex literals are always RegExp. These never change under any context.
        // NOTE: String/numeric literals are NOT safe to skip — contextual typing
        // determines whether they widen ("hello" → string vs staying as "hello").
        // Identifiers are also not safe — they can resolve differently under context.
        if let Some(node) = self.ctx.arena.get(idx) {
            let k = node.kind;
            if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
            {
                return;
            }
        }

        // Clear this node's cache
        self.ctx.node_types.remove(&idx.0);
        self.ctx.flow_narrowed_nodes.remove(&idx.0);
        if !self.ctx.request_node_types.is_empty() {
            self.ctx
                .request_node_types
                .retain(|(node_idx, _), _| *node_idx != idx.0);
        }
        if self.ctx.implicit_any_contextual_closures.contains(&idx) {
            self.ctx.implicit_any_checked_closures.insert(idx);
        } else {
            self.ctx.implicit_any_checked_closures.remove(&idx);
        }

        // Recursively clear children
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::CASE_BLOCK
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
            {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.clear_type_cache_recursive(stmt_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.clear_type_cache_recursive(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT =>
            {
                if let Some(stmt) = self.ctx.arena.get_return_statement(node)
                    && stmt.expression.is_some()
                {
                    self.clear_type_cache_recursive(stmt.expression);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(array) = self.ctx.arena.get_literal_expr(node) {
                    for &elem_idx in &array.elements.nodes {
                        self.clear_type_cache_recursive(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(obj) = self.ctx.arena.get_literal_expr(node) {
                    for &prop_idx in &obj.elements.nodes {
                        self.clear_type_cache_recursive(prop_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    self.clear_type_cache_recursive(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.clear_type_cache_recursive(paren.expression);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.clear_type_cache_recursive(call.expression);
                    if let Some(ref args) = call.arguments {
                        for &arg_idx in &args.nodes {
                            self.clear_type_cache_recursive(arg_idx);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    // Don't recurse into `super` — its cached type from
                    // build_type_environment is correct and clearing it
                    // causes false errors in static contexts.
                    let is_super = self
                        .ctx
                        .arena
                        .get(access.expression)
                        .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16);
                    if !is_super {
                        self.clear_type_cache_recursive(access.expression);
                    }
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
                    self.clear_type_cache_recursive(bin.left);
                    self.clear_type_cache_recursive(bin.right);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.clear_type_cache_recursive(cond.condition);
                    self.clear_type_cache_recursive(cond.when_true);
                    self.clear_type_cache_recursive(cond.when_false);
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                if let Some(spread) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.clear_type_cache_recursive(spread.expression);
                }
            }
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(as_expr) = self.ctx.arena.get_type_assertion(node) {
                    self.clear_type_cache_recursive(as_expr.expression);
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr_ex(node) {
                    self.clear_type_cache_recursive(unary.expression);
                }
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                if let Some(func) = self.ctx.arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            for sym_id in self
                                .parameter_symbol_ids(param_idx, param.name)
                                .into_iter()
                                .flatten()
                            {
                                self.ctx.symbol_types.remove(&sym_id);
                            }
                            if param.initializer.is_some() {
                                self.clear_type_cache_recursive(param.initializer);
                            }
                        }
                    }
                    self.clear_type_cache_recursive(func.body);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    for &param_idx in &method.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            for sym_id in self
                                .parameter_symbol_ids(param_idx, param.name)
                                .into_iter()
                                .flatten()
                            {
                                self.ctx.symbol_types.remove(&sym_id);
                            }
                            if param.initializer.is_some() {
                                self.clear_type_cache_recursive(param.initializer);
                            }
                        }
                    }
                    self.clear_type_cache_recursive(method.body);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    for &param_idx in &accessor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            for sym_id in self
                                .parameter_symbol_ids(param_idx, param.name)
                                .into_iter()
                                .flatten()
                            {
                                self.ctx.symbol_types.remove(&sym_id);
                            }
                            if param.initializer.is_some() {
                                self.clear_type_cache_recursive(param.initializer);
                            }
                        }
                    }
                    self.clear_type_cache_recursive(accessor.body);
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.clear_type_cache_recursive(stmt_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.clear_type_cache_recursive(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node) {
                    self.clear_type_cache_recursive(ret.expression);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
            {
                if let Some(var) = self.ctx.arena.get_variable(node) {
                    for &decl_idx in &var.declarations.nodes {
                        self.clear_type_cache_recursive(decl_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.ctx.arena.get_variable_declaration(node) {
                    if let Some(sym_id) = self.ctx.binder.get_node_symbol(idx) {
                        self.ctx.symbol_types.remove(&sym_id);
                    }
                    self.clear_binding_name_symbol_cache_recursive(decl.name);
                    self.clear_type_cache_recursive(decl.initializer);
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.clear_type_cache_recursive(if_stmt.expression);
                    self.clear_type_cache_recursive(if_stmt.then_statement);
                    self.clear_type_cache_recursive(if_stmt.else_statement);
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_stmt) = self.ctx.arena.get_for_in_of(node) {
                    self.clear_type_cache_recursive(for_stmt.initializer);
                    self.clear_type_cache_recursive(for_stmt.expression);
                    self.clear_type_cache_recursive(for_stmt.statement);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod contextual_retry_guard_tests {
    use super::{
        ContextualRetryGuard, MAX_CONTEXTUAL_RETRY_DEPTH, contextual_retry_depth_for_test,
        reset_contextual_retry_path,
    };

    /// A fresh thread starts at depth zero, and a single entered frame raises
    /// the active depth by exactly one, restoring it on drop. This is the base
    /// invariant the whole stack-overflow guard relies on.
    #[test]
    fn enter_increments_and_drop_decrements() {
        reset_contextual_retry_path();
        assert_eq!(contextual_retry_depth_for_test(), 0);
        {
            let _g = ContextualRetryGuard::enter().expect("first frame must enter");
            assert_eq!(contextual_retry_depth_for_test(), 1);
            {
                let _g2 = ContextualRetryGuard::enter().expect("nested frame must enter");
                assert_eq!(contextual_retry_depth_for_test(), 2);
            }
            assert_eq!(contextual_retry_depth_for_test(), 1);
        }
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }

    /// Any walk shallower than the cap always enters successfully — the cap is
    /// chosen so legitimate (finite) expression trees never hit it, which is
    /// what keeps the guard byte-identical on non-crashing inputs.
    #[test]
    fn frames_below_cap_always_enter() {
        reset_contextual_retry_path();
        let mut guards = Vec::new();
        for expected in 1..=64u32 {
            let g = ContextualRetryGuard::enter().expect("below-cap frame must enter");
            assert_eq!(contextual_retry_depth_for_test(), expected);
            guards.push(g);
        }
        drop(guards);
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }

    /// At the cap, `enter` returns `None` so the caller stops recursing. This is
    /// the cycle/stack-overflow cutoff: a non-terminating cyclic expression-child
    /// link is the only way to reach the cap, and reaching it terminates the
    /// walk instead of overflowing the native stack.
    #[test]
    fn entering_at_cap_returns_none() {
        reset_contextual_retry_path();
        // Hold guards up to the cap so the depth is exactly at the limit.
        let mut guards = Vec::new();
        for _ in 0..MAX_CONTEXTUAL_RETRY_DEPTH {
            guards.push(ContextualRetryGuard::enter().expect("frame below cap must enter"));
        }
        assert_eq!(
            contextual_retry_depth_for_test(),
            MAX_CONTEXTUAL_RETRY_DEPTH
        );
        // The next frame is refused, breaking the recursion.
        assert!(
            ContextualRetryGuard::enter().is_none(),
            "a frame at the cap must be refused so a cyclic walk terminates"
        );
        // A refused frame must not change the depth.
        assert_eq!(
            contextual_retry_depth_for_test(),
            MAX_CONTEXTUAL_RETRY_DEPTH
        );
        drop(guards);
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }

    /// `reset_contextual_retry_path` zeroes a leaked depth, guaranteeing row
    /// isolation even if a future non-unwinding bail-out leaves the counter
    /// non-zero.
    #[test]
    fn reset_zeroes_leaked_depth() {
        reset_contextual_retry_path();
        // Simulate a leaked frame by forgetting the guard (no drop runs).
        let g = ContextualRetryGuard::enter().expect("frame must enter");
        std::mem::forget(g);
        assert_eq!(contextual_retry_depth_for_test(), 1);
        reset_contextual_retry_path();
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }
}
