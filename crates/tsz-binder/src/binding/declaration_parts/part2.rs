impl BinderState {
    /// Get a symbol, checking lib binders if not found locally.
    /// This is used by the checker to resolve symbols that come from lib.d.ts.
    pub fn get_symbol_with_libs<'a>(
        &'a self,
        id: SymbolId,
        lib_binders: &'a [Arc<Self>],
    ) -> Option<&'a Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
        }

        // Prefer local symbols first so source-file declarations can shadow
        // lib symbols even when SymbolId values collide.
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Legacy path (for backward compatibility when lib_symbols_merged is false):
        // Prefer lib binders when the ID is known to originate from libs
        if self.lib_symbol_ids.contains(&id) {
            for lib_binder in lib_binders {
                if let Some(sym) = lib_binder.symbols.get(id) {
                    return Some(sym);
                }
            }
        }

        // Then try lib binders
        for lib_binder in lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }

        None
    }

    /// Look up a global type by name from `file_locals` and lib binders.
    ///
    /// This method is used by the checker to find built-in types like Array, Object,
    /// Function, Promise, etc. It checks:
    /// 1. Local `file_locals` (for user-defined globals or merged lib symbols)
    /// 2. Lib binders (only when `lib_symbols_merged` is false)
    ///
    /// Returns the `SymbolId` if found, None otherwise.
    pub fn get_global_type(&self, name: &str) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check lib binders directly (for backward compatibility)
        for lib_binder in self.lib_binders.iter() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Look up a global type by name, using provided lib binders.
    ///
    /// This variant is used when the checker has its own lib contexts and needs
    /// to search them explicitly.
    pub fn get_global_type_with_libs(
        &self,
        name: &str,
        lib_binders: &[Arc<Self>],
    ) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check provided lib binders (for backward compatibility)
        for lib_binder in lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        // Finally check our own lib binders
        for lib_binder in self.lib_binders.iter() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Check if a global type exists (in `file_locals` or lib binders).
    ///
    /// This is a convenience method for checking type availability without
    /// actually retrieving the symbol.
    pub fn has_global_type(&self, name: &str) -> bool {
        self.get_global_type(name).is_some()
    }

    pub fn get_node_symbol(&self, node: NodeIndex) -> Option<SymbolId> {
        self.node_symbols.get(&node.0).copied()
    }

    pub const fn get_symbols(&self) -> &SymbolArena {
        &self.symbols
    }

    /// Check if the current source file is an external module (has top-level import/export).
    /// This is used by the checker to determine if ES module semantics apply.
    pub const fn is_external_module(&self) -> bool {
        self.is_external_module
    }

    /// Check if a module specifier likely refers to an existing module that can be augmented.
    /// Rule #44: Module augmentation vs ambient module declaration detection.
    ///
    /// Returns true if:
    /// - The module specifier refers to an already declared module
    /// - The specifier looks like an external package (not a relative path)
    pub(crate) fn is_potential_module_augmentation(&self, module_specifier: &str) -> bool {
        // In external modules, relative `declare module "./x"` is always an augmentation target.
        if module_specifier.starts_with("./")
            || module_specifier.starts_with("../")
            || module_specifier == "."
            || module_specifier == ".."
        {
            return true;
        }

        // Check if we've already declared this module
        if self.declared_modules.contains(module_specifier) {
            return true;
        }

        // Check if we have exports from this module (meaning it was resolved)
        if self.module_exports.contains_key(module_specifier) {
            return true;
        }

        // External packages (not relative paths) are assumed to exist and can be augmented
        // This handles cases like `declare module 'express' { ... }`
        !module_specifier.starts_with('.') && !module_specifier.starts_with('/')
    }

    /// Get the flow node that was active at a given AST node.
    /// Used by the checker for control flow analysis.
    pub fn get_node_flow(&self, node: NodeIndex) -> Option<FlowNodeId> {
        self.node_flow.get(&node.0).copied()
    }

    /// Get the containing switch statement for a case/default clause.
    pub fn get_switch_for_clause(&self, clause: NodeIndex) -> Option<NodeIndex> {
        self.switch_clause_to_switch.get(&clause.0).copied()
    }

    /// Record the current flow node for an AST node.
    /// Called during binding to track flow position for identifiers and other expressions.
    pub(crate) fn record_flow(&mut self, node: NodeIndex) {
        if self.current_flow.is_some() {
            use tracing::trace;
            if let Some(flow_node) = self.flow_nodes.get(self.current_flow) {
                trace!(
                    node_idx = node.0,
                    flow_id = self.current_flow.0,
                    flow_flags = flow_node.flags,
                    "record_flow: associating node with flow"
                );
            }
            Arc::make_mut(&mut self.node_flow).insert(node.0, self.current_flow);
        }
    }

    pub(crate) fn with_fresh_flow<F>(&mut self, bind_body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.with_fresh_flow_inner(bind_body, false);
    }

    /// Create a fresh flow for a function body, optionally capturing the enclosing flow for closures.
    /// If `capture_enclosing` is true, the START node will point to the enclosing flow, allowing
    /// const/let variables to preserve narrowing from the outer scope.
    pub(crate) fn with_fresh_flow_inner<F>(&mut self, bind_body: F, capture_enclosing: bool)
    where
        F: FnOnce(&mut Self),
    {
        let prev_flow = self.current_flow;
        let start_flow = {
            let flow_nodes = std::sync::Arc::make_mut(&mut self.flow_nodes);
            let start_flow = flow_nodes.alloc(flow_flags::START);

            // For closures (arrow functions and function expressions), capture the enclosing flow
            // so that const/let variables can preserve narrowing from the outer scope
            if capture_enclosing
                && prev_flow.is_some()
                && let Some(start_node) = flow_nodes.get_mut(start_flow)
            {
                start_node.antecedent.push(prev_flow);
            }
            start_flow
        };

        // Save and clear return_targets so that return statements inside
        // non-IIFE functions don't redirect to an enclosing IIFE's return target.
        let prev_return_targets = std::mem::take(&mut self.return_targets);

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
        self.return_targets = prev_return_targets;
    }

    pub(crate) const fn is_assignment_operator(operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    pub(crate) fn is_array_mutation_call(arena: &NodeArena, call_idx: NodeIndex) -> bool {
        let Some(call) = arena.get_call_expr_at(call_idx) else {
            return false;
        };
        let Some(access) = arena.get_access_expr_at(call.expression) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return false;
        };
        let name = if let Some(ident) = arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    fn is_optional_chain_access(arena: &NodeArena, idx: NodeIndex) -> bool {
        let idx = arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = arena.get_access_expr(node) {
                    access.question_dot_token
                        || Self::is_optional_chain_access(arena, access.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if node.is_optional_chain() {
                    return true;
                }
                if let Some(call) = arena.get_call_expr(node) {
                    Self::is_optional_chain_access(arena, call.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn continues_optional_chain(&self, arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(ext) = arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = arena.get(parent) else {
            return false;
        };
        match parent_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                arena.get_access_expr(parent_node).is_some_and(|access| {
                    access.expression == idx && Self::is_optional_chain_access(arena, parent)
                })
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                arena.get_call_expr(parent_node).is_some_and(|call| {
                    call.expression == idx && Self::is_optional_chain_access(arena, parent)
                })
            }
            _ => false,
        }
    }

    fn optional_chain_branch_base(&self) -> FlowNodeId {
        let current = self.current_flow;
        let Some(flow) = self.flow_nodes.get(current) else {
            return current;
        };
        if (flow.flags & flow_flags::TRUE_CONDITION) != 0
            && let Some(&antecedent) = flow.antecedent.first()
            && antecedent.is_some()
        {
            return antecedent;
        }
        current
    }

    /// Bind a short-circuit binary expression (&&, ||, ??) with intermediate
    /// flow condition nodes.
    ///
    /// For `a && b`: the right operand `b` is only evaluated when `a` is truthy,
    /// so we create a `TRUE_CONDITION` node for `a` before binding `b`. This allows
    /// references in `b` to see type narrowing from `a`.
    ///
    /// For `a || b` and `a ?? b`: the right operand `b` is only evaluated when `a`
    /// is falsy/nullish, so we create a `FALSE_CONDITION` node for `a` before binding `b`.
    pub(crate) fn bind_short_circuit_expression(
        &mut self,
        arena: &NodeArena,
        idx: NodeIndex,
        left: NodeIndex,
        right: NodeIndex,
        operator: u16,
    ) {
        self.record_flow(idx);

        // Bind the left operand
        self.bind_expression(arena, left);
        let after_left_flow = self.current_flow;

        let is_assignment = operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || operator == SyntaxKind::BarBarEqualsToken as u16
            || operator == SyntaxKind::QuestionQuestionEqualsToken as u16;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16
            || operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
        {
            // For && and &&=: right side is only evaluated when left is truthy
            let true_condition =
                self.create_flow_condition(flow_flags::TRUE_CONDITION, after_left_flow, left);
            self.current_flow = true_condition;
            self.bind_expression(arena, right);
            if is_assignment && !Self::is_inside_class_member_computed_property_name(arena, idx) {
                self.current_flow = self.create_flow_assignment(idx);
            }
            let after_right_flow = self.current_flow;

            // Short-circuit path: left is falsy, right is not evaluated
            let false_condition =
                self.create_flow_condition(flow_flags::FALSE_CONDITION, after_left_flow, left);

            // Merge both paths
            let merge = self.create_branch_label();
            self.add_antecedent(merge, after_right_flow);
            self.add_antecedent(merge, false_condition);
            self.current_flow = merge;
        } else {
            // For ||, ??, ||=, ??=: right side is only evaluated when left is falsy/nullish
            let false_condition =
                self.create_flow_condition(flow_flags::FALSE_CONDITION, after_left_flow, left);
            self.current_flow = false_condition;
            self.bind_expression(arena, right);
            if is_assignment && !Self::is_inside_class_member_computed_property_name(arena, idx) {
                self.current_flow = self.create_flow_assignment(idx);
            }
            let after_right_flow = self.current_flow;

            // Short-circuit path: left is truthy, right is not evaluated
            let true_condition =
                self.create_flow_condition(flow_flags::TRUE_CONDITION, after_left_flow, left);

            // Merge both paths
            let merge = self.create_branch_label();
            self.add_antecedent(merge, after_right_flow);
            self.add_antecedent(merge, true_condition);
            self.current_flow = merge;
        }
    }

    pub(crate) fn bind_binary_expression_flow_iterative(
        &mut self,
        arena: &NodeArena,
        root: NodeIndex,
    ) {
        enum WorkItem {
            Visit(NodeIndex),
            PostAssign(NodeIndex),
        }

        let mut stack = vec![WorkItem::Visit(root)];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Visit(idx) => {
                    let Some(node) = arena.get(idx) else {
                        continue;
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        self.record_flow(idx);
                        let Some(bin) = arena.get_binary_expr(node) else {
                            continue;
                        };
                        if bin.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                            || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
                            || bin.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
                        {
                            self.bind_short_circuit_expression(
                                arena,
                                idx,
                                bin.left,
                                bin.right,
                                bin.operator_token,
                            );
                            continue;
                        }

                        if Self::is_assignment_operator(bin.operator_token) {
                            // For destructuring defaults (LHS is a pattern),
                            // bind RHS before LHS to match runtime eval order.
                            let lhs_is_destructuring = bin.operator_token
                                == SyntaxKind::EqualsToken as u16
                                && arena.get(bin.left).is_some_and(|left_node| {
                                    left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                        || left_node.kind
                                            == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                        || left_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                        || left_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                });
                            stack.push(WorkItem::PostAssign(idx));
                            if lhs_is_destructuring {
                                // Stack is LIFO: push LHS last so it runs after RHS
                                if bin.left.is_some() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                                if bin.right.is_some() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                            } else {
                                if bin.right.is_some() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if bin.left.is_some() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                            }
                            continue;
                        }
                        // Delegate short-circuit operators to proper flow handling
                        if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                            || bin.operator_token == SyntaxKind::BarBarToken as u16
                            || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                        {
                            self.bind_short_circuit_expression(
                                arena,
                                idx,
                                bin.left,
                                bin.right,
                                bin.operator_token,
                            );
                            continue;
                        }
                        if bin.right.is_some() {
                            stack.push(WorkItem::Visit(bin.right));
                        }
                        if bin.left.is_some() {
                            stack.push(WorkItem::Visit(bin.left));
                        }
                        continue;
                    }

                    self.bind_expression(arena, idx);
                }
                WorkItem::PostAssign(idx) => {
                    if !Self::is_inside_class_member_computed_property_name(arena, idx) {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
            }
        }
    }

    /// Bind an expression and record flow positions for identifiers.
    /// This is used for condition expressions in if/while/for statements.
    pub(crate) fn bind_expression(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = arena.get_binary_expr(node) {
                if bin.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                    || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
                    || bin.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
                {
                    self.bind_short_circuit_expression(
                        arena,
                        idx,
                        bin.left,
                        bin.right,
                        bin.operator_token,
                    );
                    return;
                }

                if Self::is_assignment_operator(bin.operator_token) {
                    self.record_flow(idx);
                    // For destructuring assignments (LHS is array/object literal),
                    // bind the RHS (source/default) before the LHS (pattern).
                    // This matches tsc's bindDestructuringTargetFlow: at runtime,
                    // the source/default is evaluated before the pattern is applied,
                    // so flow-sensitive reads in the default must see pre-assignment
                    // values. E.g., `[{ [(a = 1)]: b } = [9, a] as const] = []`
                    // must evaluate `[9, a]` (reading `a = 0`) before `(a = 1)`.
                    let lhs_is_destructuring = bin.operator_token == SyntaxKind::EqualsToken as u16
                        && arena.get(bin.left).is_some_and(|left_node| {
                            left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || left_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || left_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        });
                    if lhs_is_destructuring {
                        self.bind_expression(arena, bin.right);
                        self.bind_expression(arena, bin.left);
                    } else {
                        self.bind_expression(arena, bin.left);
                        self.bind_expression(arena, bin.right);
                    }
                    if !Self::is_inside_class_member_computed_property_name(arena, idx) {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                    // Detect expando property assignments (X.prop = value)
                    if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                        self.detect_expando_assignment(arena, bin.left, bin.right);
                    }
                    return;
                }

                // Handle short-circuit operators (&&, ||, ??) with intermediate
                // flow condition nodes so that the right operand sees narrowing
                // from the left operand.
                if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                    || bin.operator_token == SyntaxKind::BarBarToken as u16
                    || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                {
                    self.bind_short_circuit_expression(
                        arena,
                        idx,
                        bin.left,
                        bin.right,
                        bin.operator_token,
                    );
                    return;
                }
            }
            self.bind_binary_expression_flow_iterative(arena, idx);
            return;
        }

        // Record flow position for this node
        self.record_flow(idx);

        match node.kind {
            // Identifiers - record flow position for type narrowing
            k if k == SyntaxKind::Identifier as u16 => {
                // Already recorded above
                return;
            }

            // Prefix unary (e.g., typeof x, !x)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_expression(arena, unary.operand);
                    if (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && !Self::is_inside_class_member_computed_property_name(arena, idx)
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Property access (e.g., x.foo)
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);
                }
                return;
            }

            // Element access (e.g., x[0], x?.[expr])
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);

                    // Optional chaining short-circuits RHS evaluation.
                    // For `obj?.[expr]`, `expr` is evaluated only when `obj` is present.
                    if Self::is_optional_chain_access(arena, idx) {
                        let after_base = if self.continues_optional_chain(arena, idx)
                            || Self::is_optional_chain_access(arena, access.expression)
                        {
                            self.optional_chain_branch_base()
                        } else {
                            self.current_flow
                        };

                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            after_base,
                            access.expression,
                        );
                        self.current_flow = true_flow;
                        self.bind_expression(arena, access.name_or_argument);
                        if !self.continues_optional_chain(arena, idx) {
                            let after_element = self.current_flow;

                            let false_flow = self.create_flow_condition(
                                flow_flags::FALSE_CONDITION,
                                after_base,
                                access.expression,
                            );

                            let merge = self.create_branch_label();
                            self.add_antecedent(merge, after_element);
                            self.add_antecedent(merge, false_flow);
                            self.current_flow = merge;
                        }
                    } else {
                        self.bind_expression(arena, access.name_or_argument);
                    }
                }
                return;
            }

            // Call expression (e.g., isString(x))
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    self.bind_expression(arena, call.expression);

                    let is_optional_call = node.is_optional_chain();
                    if is_optional_call {
                        let after_callee = if self.continues_optional_chain(arena, idx)
                            || Self::is_optional_chain_access(arena, call.expression)
                        {
                            self.optional_chain_branch_base()
                        } else {
                            self.current_flow
                        };

                        // Optional calls short-circuit argument evaluation when callee is absent.
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            after_callee,
                            call.expression,
                        );
                        self.current_flow = true_flow;
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.bind_expression(arena, arg);
                            }
                        }
                        let flow = self.create_flow_call(idx);
                        self.current_flow = flow;
                        if Self::is_array_mutation_call(arena, idx) {
                            let flow = self.create_flow_array_mutation(idx);
                            self.current_flow = flow;
                        }
                        if !self.continues_optional_chain(arena, idx) {
                            let after_call = self.current_flow;

                            let false_flow = self.create_flow_condition(
                                flow_flags::FALSE_CONDITION,
                                after_callee,
                                call.expression,
                            );

                            let merge = self.create_branch_label();
                            self.add_antecedent(merge, after_call);
                            self.add_antecedent(merge, false_flow);
                            self.current_flow = merge;
                        }
                    } else {
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.bind_expression(arena, arg);
                            }
                        }
                        // Create CALL flow node for all call expressions
                        let flow = self.create_flow_call(idx);
                        self.current_flow = flow;
                        // Also create ARRAY_MUTATION flow node if it's an array mutation
                        if Self::is_array_mutation_call(arena, idx) {
                            let flow = self.create_flow_array_mutation(idx);
                            self.current_flow = flow;
                        }
                    }
                }
                return;
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = arena.get_parenthesized(node) {
                    self.bind_expression(arena, paren.expression);
                }
                return;
            }

            // Type assertion (e.g., x as string, <T>x, x satisfies T)
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = arena.get_type_assertion(node) {
                    self.bind_expression(arena, assertion.expression);
                }
                return;
            }

            // Conditional expression (ternary) - build flow graph for type narrowing
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = arena.get_conditional_expr(node) {
                    // Bind the condition expression
                    self.bind_expression(arena, cond.condition);

                    // Save pre-condition flow
                    let pre_condition_flow = self.current_flow;

                    // Create TRUE_CONDITION flow for when_true branch
                    let true_flow = self.create_flow_condition(
                        flow_flags::TRUE_CONDITION,
                        pre_condition_flow,
                        cond.condition,
                    );
                    self.current_flow = true_flow;
                    self.bind_expression(arena, cond.when_true);
                    let after_true_flow = self.current_flow;

                    // Create FALSE_CONDITION flow for when_false branch
                    let false_flow = self.create_flow_condition(
                        flow_flags::FALSE_CONDITION,
                        pre_condition_flow,
                        cond.condition,
                    );
                    self.current_flow = false_flow;
                    self.bind_expression(arena, cond.when_false);
                    let after_false_flow = self.current_flow;

                    // Create merge point for both branches
                    let merge_label = self.create_branch_label();
                    self.add_antecedent(merge_label, after_true_flow);
                    self.add_antecedent(merge_label, after_false_flow);
                    self.current_flow = merge_label;
                }
                return;
            }

            _ => {}
        }

        self.bind_node(arena, idx);
    }

    /// Detect expando property assignments of the form `X.prop = value`.
    /// Tracks both simple identifiers (`X.prop`) and dotted receiver chains
    /// (`A.B.prop`) so function members on namespaces can collect expandos.
    fn detect_expando_assignment(&mut self, arena: &NodeArena, lhs: NodeIndex, rhs: NodeIndex) {
        fn symbol_call(arena: &NodeArena, idx: NodeIndex) -> bool {
            let Some(node) = arena.get(idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return false;
            }
            let Some(call) = arena.get_call_expr(node) else {
                return false;
            };
            let Some(callee) = arena.get(call.expression) else {
                return false;
            };
            callee.kind == SyntaxKind::Identifier as u16
                && arena
                    .get_identifier(callee)
                    .is_some_and(|ident| ident.escaped_text == "Symbol")
        }

        fn is_undefined_like_rhs(arena: &NodeArena, idx: NodeIndex) -> bool {
            let Some(node) = arena.get(idx) else {
                return false;
            };

            if node.kind == SyntaxKind::Identifier as u16 {
                return arena
                    .get_identifier(node)
                    .is_some_and(|ident| ident.escaped_text == "undefined");
            }

            if node.kind != syntax_kind_ext::VOID_EXPRESSION
                && node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            {
                return false;
            }

            let Some(unary) = arena.get_unary_expr(node) else {
                return false;
            };
            if unary.operator != SyntaxKind::VoidKeyword as u16 {
                return false;
            }
            let Some(expr) = arena.get(unary.operand) else {
                return false;
            };
            matches!(expr.kind, k if k == SyntaxKind::NumericLiteral as u16)
                && arena.get_literal(expr).is_some_and(|lit| lit.text == "0")
        }

        if is_undefined_like_rhs(arena, rhs) {
            return;
        }

        fn property_access_chain(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
            if let Some(text) = arena.identifier_text_owned(idx) {
                return Some(text);
            }
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let left = property_access_chain(arena, access.expression)?;
                let right = arena.identifier_text_owned(access.name_or_argument)?;
                return Some(format!("{left}.{right}"));
            }
            None
        }

        fn root_identifier_index(arena: &NodeArena, idx: NodeIndex) -> Option<NodeIndex> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return Some(idx);
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                return root_identifier_index(arena, access.expression);
            }
            None
        }

        fn resolved_const_expando_key(
            binder: &BinderState,
            arena: &NodeArena,
            sym_id: SymbolId,
            depth: u8,
        ) -> Option<String> {
            if depth > 8 {
                return None;
            }

            let symbol = binder.symbols.get(sym_id)?;
            let decl_idx = if symbol.value_declaration.is_some() {
                symbol.value_declaration
            } else {
                symbol
                    .declarations
                    .iter()
                    .copied()
                    .find(|decl| decl.is_some())?
            };
            if !arena.is_const_variable_declaration(decl_idx) {
                return None;
            }

            let decl_node = arena.get(decl_idx)?;
            let var_decl = arena.get_variable_declaration(decl_node)?;
            let init_idx = var_decl.initializer;
            if init_idx.is_none() {
                return None;
            }
            let init_node = arena.get(init_idx)?;

            match init_node.kind {
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    arena.get_literal(init_node).map(|lit| lit.text.clone())
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = arena.get_unary_expr(init_node)?;
                    let operand = arena.get(unary.operand)?;
                    if operand.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let lit = arena.get_literal(operand)?;
                    match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{}", lit.text)),
                        k if k == SyntaxKind::PlusToken as u16 => Some(lit.text.clone()),
                        _ => None,
                    }
                }
                k if k == SyntaxKind::Identifier as u16 => {
                    let name = arena.identifier_text_owned(init_idx)?;
                    let next_sym = binder.file_locals.get(&name)?;
                    resolved_const_expando_key(binder, arena, next_sym, depth + 1)
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    symbol_call(arena, init_idx).then(|| format!("__unique_{}", sym_id.0))
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    let assertion = arena.get_type_assertion(init_node)?;
                    let inner = arena.get(assertion.expression)?;
                    match inner.kind {
                        k if k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                        {
                            arena.get_literal(inner).map(|lit| lit.text.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        fn expando_member_key(
            binder: &BinderState,
            arena: &NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            match node.kind {
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let access = arena.get_access_expr(node)?;
                    let name_node = arena.get(access.name_or_argument)?;
                    arena
                        .get_identifier(name_node)
                        .map(|ident| ident.escaped_text.clone())
                }
                syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                    let access = arena.get_access_expr(node)?;
                    let key_node = arena.get(access.name_or_argument)?;
                    match key_node.kind {
                        k if k == SyntaxKind::Identifier as u16 => {
                            let ident = arena.get_identifier(key_node)?;
                            binder
                                .file_locals
                                .get(&ident.escaped_text)
                                .and_then(|sym_id| {
                                    resolved_const_expando_key(binder, arena, sym_id, 0)
                                })
                                .or_else(|| Some(ident.escaped_text.clone()))
                        }
                        k if k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                        {
                            arena.get_literal(key_node).map(|lit| lit.text.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        let Some(lhs_node) = arena.get(lhs) else {
            return;
        };
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return;
        }
        let Some(access) = arena.get_access_expr(lhs_node) else {
            return;
        };
        let Some(prop_name) = expando_member_key(self, arena, lhs) else {
            return;
        };

        let Some(obj_key) = property_access_chain(arena, access.expression) else {
            return;
        };
        let root_name = obj_key.split('.').next().unwrap_or_default();
        if root_name.is_empty() {
            return;
        }

        // CommonJS export chains like `module.exports.foo = ...` and
        // `module.exports.foo.bar = ...` don't resolve through `file_locals`
        // because `module` is not a user-declared symbol. Track them directly
        // so the checker can reuse one expando summary path for property reads
        // and forward-reference TS2565 checks.
        if obj_key == "module.exports"
            || obj_key.starts_with("module.exports.")
            || obj_key == "exports"
            || obj_key.starts_with("exports.")
        {
            Arc::make_mut(&mut self.expando_properties)
                .entry(obj_key)
                .or_default()
                .insert(prop_name);
            return;
        }

        // Resolve the root identifier through the enclosing scope chain so nested
        // function/value roots share the same expando summary path as top-level ones.
        let Some(root_ident) = root_identifier_index(arena, access.expression) else {
            return;
        };
        let Some(sym_id) = self.resolve_identifier(arena, root_ident) else {
            return;
        };
        let Some(symbol) = self.symbols.get(sym_id) else {
            return;
        };

        let is_js_like_source = arena.source_files.first().is_some_and(|source_file| {
            let file_name = source_file.file_name.to_ascii_lowercase();
            !source_file.is_declaration_file
                && (file_name.ends_with(".js")
                    || file_name.ends_with(".jsx")
                    || file_name.ends_with(".mjs")
                    || file_name.ends_with(".cjs"))
        });

        // Track for functions and namespace-like roots. Class roots are only
        // expando-capable in JS files; TS files must keep `class C {} C.x = 1`
        // as a TS2339 error.
        //
        // Don't track prototype element-access expandos (e.g.
        // `F.prototype[sym] = val`). TSC's late-bound assignment
        // declarations are unsupported for prototype chains, so we
        // should emit TS7053 rather than suppress it.
        let is_prototype_element_access = obj_key.split('.').any(|segment| segment == "prototype")
            && lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        let is_function_or_namespace_root = (symbol.flags
            & (symbol_flags::FUNCTION
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE))
            != 0;
        let is_js_class_root = is_js_like_source && (symbol.flags & symbol_flags::CLASS) != 0;
        if ((is_function_or_namespace_root && (symbol.flags & symbol_flags::CLASS) == 0)
            || is_js_class_root)
            && !is_prototype_element_access
        {
            Arc::make_mut(&mut self.expando_properties)
                .entry(obj_key.clone())
                .or_default()
                .insert(prop_name);
            return;
        }

        // Also track for variables initialized with function/class/object-literal expressions
        // (e.g. `var X = function(){}; X.prop = 1` or `var X = {}; X.prop = 1`)
        // For typed variables, only track function/arrow inits (expando function pattern).
        if (symbol.flags & symbol_flags::VARIABLE) != 0 {
            let decl_idx = symbol.value_declaration;
            if decl_idx.is_none() {
                return;
            }
            let Some(decl_node) = arena.get(decl_idx) else {
                return;
            };
            let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
                return;
            };
            if var_decl.initializer.is_none() {
                return;
            }
            let Some(init_node) = arena.get(var_decl.initializer) else {
                return;
            };
            let has_type_annotation = var_decl.type_annotation.is_some();
            let is_function_like = init_node.is_function_expression_or_arrow();
            let is_property_access_lhs =
                lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION;
            let is_expando_init = is_function_like
                || (is_property_access_lhs
                    && !has_type_annotation
                    && (init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                        || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION));
            if is_expando_init {
                Arc::make_mut(&mut self.expando_properties)
                    .entry(obj_key)
                    .or_default()
                    .insert(prop_name);
            }
        }
    }

    /// Check if the current scope is the global (file-level) scope.
    /// Record a semantic definition entry for a top-level declaration.
    ///
    /// This captures stable identity information at bind time so the checker
    /// can pre-create solver `DefIds` during construction rather than inventing
    /// them on demand in hot paths.
    ///
    /// Only records entries for declarations at the source file scope (ScopeId(0))
    /// to avoid noise from nested declarations that are less likely to be
    /// cross-file semantic references.
    /// Collect type parameter names from a type parameter `NodeList`.
    ///
    /// Returns an empty `Vec` if `type_params` is `None` or contains no
    /// extractable names. Each entry is the escaped text of the type
    /// parameter identifier (e.g., `["T", "U"]` for `<T, U>`).
    pub(crate) fn collect_type_param_names(
        arena: &NodeArena,
        type_params: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(params) = type_params else {
            return Vec::new();
        };
        params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let node = arena.get(param_idx)?;
                let tp = arena.get_type_parameter(node)?;
                let name = Self::get_identifier_name(arena, tp.name)?;
                Some(name.to_string())
            })
            .collect()
    }

    pub(crate) fn record_semantic_def(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        type_param_count: u16,
        type_param_names: Vec<String>,
        is_exported: bool,
    ) {
        self.record_semantic_def_ext(
            sym_id,
            kind,
            name,
            declaration,
            SemanticDefDetails {
                type_param_count,
                type_param_names,
                is_exported,
                ..Default::default()
            },
        );
    }

    /// Like `record_semantic_def` but with explicit `is_declare` flag.
    pub(crate) fn record_semantic_def_with_declare(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        details: SemanticDefDetails,
    ) {
        self.record_semantic_def_ext(sym_id, kind, name, declaration, details);
    }

    /// Extended version of `record_semantic_def` that also captures enriched
    /// identity data: enum member names, const-enum flag, abstract-class flag,
    /// and split heritage names (extends vs implements).
    ///
    /// This captures stable identity information at bind time so the checker
    /// can pre-create solver `DefIds` during construction rather than inventing
    /// them on demand in hot paths.
    ///
    /// Only records entries for declarations at the source file scope (`ScopeId(0)`)
    /// to avoid noise from nested declarations that are less likely to be
    /// cross-file semantic references.
    pub(crate) fn record_semantic_def_ext(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        details: SemanticDefDetails,
    ) {
        let SemanticDefDetails {
            type_param_count,
            type_param_names,
            is_exported,
            enum_member_names,
            is_const,
            is_abstract,
            is_declare,
            extends_names,
            implements_names,
        } = details;
        // Only capture top-level declarations (source file scope or module scope)
        // and declarations inside `declare global { }` blocks.
        // Nested declarations (inside function bodies, class bodies, etc.) are not
        // recorded because they don't participate in cross-file identity.
        let is_top_level = self.current_scope_id == crate::ScopeId(0)
            || self
                .scopes
                .get(self.current_scope_id.0 as usize)
                .is_some_and(|scope| {
                    matches!(
                        scope.kind,
                        crate::ContainerKind::SourceFile | crate::ContainerKind::Module
                    )
                });
        // Declarations inside `declare global { }` blocks are semantically
        // top-level even if their scope chain doesn't directly match
        // SourceFile/Module (e.g., when the global block is nested inside
        // another module declaration). Capture them so the pre-population
        // pipeline creates stable DefIds for global augmentations.
        if !is_top_level && !self.in_global_augmentation {
            return;
        }
        // Declaration merging: keep the first declaration's core identity stable
        // (kind, name, span, file_id) but accumulate heritage and type_param_count
        // from later declarations.  This ensures the pre-populated DefinitionInfo
        // has complete heritage information (e.g., `interface A extends B {}` +
        // `interface A extends C {}` yields extends_names = ["B", "C"]).
        if let Some(existing) = std::sync::Arc::make_mut(&mut self.semantic_defs).get_mut(&sym_id) {
            // Type-side declaration merging into a value-side namespace must
            // promote the kind. A symbol like `namespace B {} interface B {}`
            // appears in TYPE positions as the interface; if the recorded kind
            // stayed `Namespace`, the type printer would emit `typeof B` instead
            // of `B`. tsc's resolver picks the type-side meaning in type
            // positions, so the binder mirrors that by upgrading the kind when
            // a Type-class declaration merges into a Namespace entry.
            if matches!(existing.kind, crate::state::SemanticDefKind::Namespace)
                && matches!(
                    kind,
                    crate::state::SemanticDefKind::Interface
                        | crate::state::SemanticDefKind::TypeAlias
                        | crate::state::SemanticDefKind::Class
                        | crate::state::SemanticDefKind::Enum
                )
            {
                existing.kind = kind;
            }
            // Accumulate new extends_names that aren't already present.
            for h in &extends_names {
                if !existing.extends_names.contains(h) {
                    existing.extends_names.push(h.clone());
                }
            }
            // Accumulate new implements_names that aren't already present.
            for h in &implements_names {
                if !existing.implements_names.contains(h) {
                    existing.implements_names.push(h.clone());
                }
            }
            // If the first declaration had no type params but this one does
            // (e.g., augmentation adds generics), update the arity and names.
            // However, do NOT merge function type params into/over a type-level
            // (interface/type alias/class) semantic def, and vice versa.
            // Function type params are function-scoped and don't represent
            // the type's generic arity.
            // E.g., `interface Mixin {}; function Mixin<T>(...) {...}` — the
            // interface has 0 type params, and the function's `T` is irrelevant.
            // Also handles the reverse: `function Mixin<T>(...); type Mixin = any;`
            // — the type alias has 0 type params and should override the function's.
            let is_type_kind = |k: &crate::state::SemanticDefKind| {
                matches!(
                    k,
                    crate::state::SemanticDefKind::Interface
                        | crate::state::SemanticDefKind::TypeAlias
                        | crate::state::SemanticDefKind::Class
                )
            };
            let is_function_kind = |k: &crate::state::SemanticDefKind| {
                matches!(k, crate::state::SemanticDefKind::Function)
            };
            let cross_function_type = (is_function_kind(&kind) && is_type_kind(&existing.kind))
                || (is_type_kind(&kind) && is_function_kind(&existing.kind));
            // When a type declaration (interface/type alias/class) merges with
            // a function, the semantic def's type_param_count should reflect the
            // TYPE declaration's params (which is the relevant arity for TS2314).
            if cross_function_type {
                // If a type declaration is merging in, update to its param count
                // (even if 0, since the type might have no params).
                if is_type_kind(&kind) {
                    existing.type_param_count = type_param_count;
                    existing.type_param_names = type_param_names;
                }
                // If function is merging into a type, don't update params (already handled)
            } else if existing.type_param_count == 0 && type_param_count > 0 {
                existing.type_param_count = type_param_count;
                existing.type_param_names = type_param_names;
            }
            // If the later declaration is exported, mark as exported.
            if is_exported {
                existing.is_exported = true;
            }
            // Accumulate enum members from later enum declarations.
            if !enum_member_names.is_empty() {
                for m in &enum_member_names {
                    if !existing.enum_member_names.contains(m) {
                        existing.enum_member_names.push(m.clone());
                    }
                }
            }
            // Promote global augmentation flag if any declaration is from declare global.
            if self.in_global_augmentation {
                existing.is_global_augmentation = true;
            }
            return;
        }
        // Determine containing namespace symbol, if any.
        // A declaration is namespace-parented when its scope is Module but not
        // the source-file root (ScopeId(0)).
        let parent_namespace = if self.current_scope_id != crate::ScopeId(0) {
            self.scopes
                .get(self.current_scope_id.0 as usize)
                .and_then(|scope| {
                    if scope.kind == crate::ContainerKind::Module {
                        // Look up the namespace symbol from the scope's container node.
                        self.get_node_symbol(scope.container_node)
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        std::sync::Arc::make_mut(&mut self.semantic_defs).insert(
            sym_id,
            crate::state::SemanticDefEntry {
                kind,
                name: name.to_string(),
                file_id: self
                    .symbols
                    .get(sym_id)
                    .map_or(u32::MAX, |s| s.decl_file_idx),
                span_start: declaration.0,
                type_param_count,
                type_param_names,
                is_exported,
                enum_member_names,
                is_const,
                is_abstract,
                extends_names,
                implements_names,
                parent_namespace,
                is_global_augmentation: self.in_global_augmentation,
                is_declare,
            },
        );
    }

    fn is_global_scope(&self) -> bool {
        // Global scope is ScopeId(0) in script files
        self.current_scope_id == crate::ScopeId(0)
    }

    /// Check whether a name in the current binder already resolves to a lib symbol.
    ///
    /// Lib symbols are merged into the local binder before user binding via
    /// `merge_lib_symbols`, so the symbol IDs in `current_scope`/`file_locals` for
    /// these names are tracked in `lib_symbol_ids`. This lets us detect "the user
    /// is declaring an interface whose name collides with a lib global" without
    /// hardcoding a static allow-list of lib types — covering DOM, `WebWorker`,
    /// `ScriptHost`, and any other ambient globals the project pulls in.
    fn name_collides_with_lib_symbol(&self, name: &str) -> bool {
        self.current_scope
            .get(name)
            .or_else(|| self.file_locals.get(name))
            .is_some_and(|sym_id| self.lib_symbol_ids.contains(&sym_id))
    }
}
