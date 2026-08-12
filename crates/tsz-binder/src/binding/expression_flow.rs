//! Binder expression flow graph construction.

use crate::state::BinderState;
use crate::{SymbolId, flow_flags, symbol_flags};
use std::sync::Arc;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl BinderState {
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

            // Prefix unary (e.g., typeof x, !x, ++x, delete x.y)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_expression(arena, unary.operand);
                    let is_increment_decrement = unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16;
                    // `delete o.a` mutates `o.a`, so a later read widens back to the
                    // declared type (re-including `undefined` for an optional prop).
                    // tsc's `bindDeleteExpressionFlow` records a flow mutation for the
                    // operand only when it is a property-access reference; element and
                    // other operands are left unmutated, so narrowing is reset for
                    // `delete o.a` but not for `delete o[k]`, matching tsc exactly.
                    let is_delete_of_property_access = unary.operator
                        == SyntaxKind::DeleteKeyword as u16
                        && Self::is_property_access_reference(arena, unary.operand);
                    if (is_increment_decrement || is_delete_of_property_access)
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
                            self.optional_chain_branch_base(arena, idx)
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
                            self.optional_chain_branch_base(arena, idx)
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

    /// Whether `idx` is a property-access expression (`o.a`, `o.a.b`), the only
    /// operand shape tsc treats as a flow mutation for `delete`
    /// (`bindDeleteExpressionFlow`). Element access (`o[k]`) and other operands
    /// are intentionally excluded so `delete o[k]` does not reset narrowing,
    /// matching tsc. Parentheses are not skipped, mirroring tsc's direct
    /// `node.expression.kind` check.
    fn is_property_access_reference(arena: &NodeArena, idx: NodeIndex) -> bool {
        arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
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

        // `x.y = void 0` / `x.y = undefined` still DECLARES the expando member
        // in tsc 7.0.2 (verified against the pinned oracle): the member is
        // visible to later reads (no TS2339), and only its inferred *type*
        // collapses to implicit `any` (TS7008 at the write, handled by the
        // checker's `collect_expando_property_assignment_type`, which already
        // excludes a void-zero/undefined RHS from contributing a concrete
        // type). An earlier version of this function bailed out of recording
        // the member entirely for such a RHS, encoding an assumption that tsc
        // rejects the member outright; it does not, so the member must still
        // be recorded here.

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
                        .map(|ident| ident.escaped_text.to_string())
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
                                .or_else(|| Some(ident.escaped_text.to_string()))
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

        // The RHS shapes that make the ASSIGNED MEMBER itself a further
        // expando host, mirroring tsc's `getExpandoInitializer`: an empty
        // object literal, a function/arrow expression, or a class
        // expression. `a.b = { k: 1 }` still declares `b`, but `b` is a
        // closed shape — a later `a.b.c = e` is a real property write
        // (TS2339 under `noImplicitAny`), not a nested expando declaration.
        fn rhs_is_expando_host_shape(arena: &NodeArena, rhs: NodeIndex) -> bool {
            arena.get(rhs).is_some_and(|node| {
                node.is_function_expression_or_arrow()
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            }) || arena.is_empty_object_literal(rhs)
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
            // The root identifier is not bound in this file's scope: it is a
            // cross-file global (JS script files share top-level `var`s), a
            // forward/out-of-order declaration, or declared in a sibling file.
            // Single-file binding cannot see that root, so `resolve_identifier`
            // returns `None` even though the write legitimately extends an
            // object/function host in another file (e.g. `Outer.Inner = class
            // {}` in one file over `var Outer = {}` in another). Record the
            // syntactic write keyed by `obj_key` so the checker's cross-file
            // expando surface can consume it; the checker re-gates host
            // capability at read time (`root_symbol_supports_js_expando_read`
            // resolves the root cross-file), so a non-object/non-callable or
            // genuinely undeclared root still reports TS2339.
            self.record_unresolved_root_expando_write(
                arena,
                lhs_node.kind,
                &obj_key,
                &prop_name,
                rhs,
            );
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
        // A class's prototype is the closed instance type: unlike its own
        // static side (`Base.newProp = 2`, a genuine expando), a write
        // through `Base.prototype…` for a member the class doesn't declare
        // must still report TS2339. Function and namespace roots keep the
        // permissive prototype-expando path — a JS constructor's prototype
        // is genuinely open.
        let is_class_prototype_chain =
            is_js_class_root && obj_key.split('.').any(|segment| segment == "prototype");
        if ((is_function_or_namespace_root && (symbol.flags & symbol_flags::CLASS) == 0)
            || is_js_class_root)
            && !is_prototype_element_access
            && !is_class_prototype_chain
        {
            // A NESTED object chain (`a.b.…x.p = e`, `obj_key` has a dot)
            // declares an expando in a TS file only when the chain's ROOT is
            // itself a callable expando-host — i.e. carries FUNCTION. A bare
            // namespace/value-module root declares nothing even though the
            // deeper member is function-typed: `declare namespace app { function
            // foo(): void }` with `app.foo.bar = e` is TS2339 at the write and
            // at every later read, whereas a function-merged root
            // (`function app(){}; namespace app { export function foo(){} }`)
            // stays a valid host. Symmetrically a member that is itself an
            // expando cannot carry a further expando (`foo.bar = {}; foo.bar.baz
            // = e` is TS2339 on `baz`). JS files keep the permissive model.
            if !is_js_like_source && obj_key.contains('.') {
                if (symbol.flags & symbol_flags::FUNCTION) == 0 {
                    return;
                }
                if let Some((parent_key, member_name)) = obj_key.rsplit_once('.')
                    && self
                        .expando_properties
                        .get(parent_key)
                        .is_some_and(|members| members.contains(member_name))
                {
                    return;
                }
            }
            // In JS files, a nested object chain (`a.b.…x.p = e`, `obj_key` has a
            // dot) declares an expando member only when the immediate base link
            // (`a.b.…x`) is itself an assignment-declared expando HOST — its own
            // declaring RHS was an empty literal, function, or class expression
            // (`expando_host_members`). A merely-declared member with a closed
            // RHS (`a.b = { k: 1 }`) does not qualify, and neither does an
            // `Object.defineProperty(root, 'seg', …)` base, which never records
            // at all — tsc types both as their literal shape and reports TS2339
            // on the nested write. `prototype` chains are exempt: `prototype` is
            // a built-in member handled by the dedicated prototype-expando paths.
            if is_js_like_source
                && !obj_key.split('.').any(|segment| segment == "prototype")
                && let Some((parent_key, member_name)) = obj_key.rsplit_once('.')
                && !self
                    .expando_host_members
                    .get(parent_key)
                    .and_then(|members| members.get(member_name))
                    .copied()
                    .unwrap_or(false)
            {
                return;
            }
            // Nearest function-like/module container, `NONE` for the source
            // file itself (blocks and loop/if heads are transparent).
            fn nearest_expando_container(arena: &NodeArena, start: NodeIndex) -> NodeIndex {
                let mut current = start;
                for _ in 0..256 {
                    let Some(ext) = arena.get_extended(current) else {
                        return NodeIndex::NONE;
                    };
                    let parent = ext.parent;
                    if parent.is_none() {
                        return NodeIndex::NONE;
                    }
                    let Some(node) = arena.get(parent) else {
                        return NodeIndex::NONE;
                    };
                    if node.is_function_like() || node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        return parent;
                    }
                    current = parent;
                }
                NodeIndex::NONE
            }
            // In TS files, `fn.prop = e` declares an expando property only
            // when the assignment's enclosing container equals the container
            // of `fn`'s declaration — an assignment inside another function's
            // body (or parameter default), or targeting a namespace-declared
            // function from file scope, is TS2339 in tsc. Block/if/loop
            // nesting shares the file container and stays declared. JS files
            // keep the permissive expando model.
            if !is_js_like_source
                && symbol.value_declaration.is_some()
                && nearest_expando_container(arena, lhs)
                    != nearest_expando_container(arena, symbol.value_declaration)
            {
                return;
            }
            let rhs_is_host = rhs_is_expando_host_shape(arena, rhs);
            self.expando_host_members
                .entry(obj_key.clone())
                .or_default()
                .entry(prop_name.clone())
                .and_modify(|host| *host &= rhs_is_host)
                .or_insert(rhs_is_host);
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
            // Only an EMPTY object literal (`var X = {}`) hosts expando members,
            // mirroring tsc's `getExpandoInitializer` (`properties.length === 0`)
            // and the checker's read/write predicates. A non-empty literal
            // (`var X = { a: 1 }`) is a closed shape, so `X.b = …` is a real
            // property write, not an expando declaration. Class/function
            // expression initializers stay hosts regardless.
            let is_expando_init = is_function_like
                || (is_property_access_lhs
                    && !has_type_annotation
                    && (init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                        || arena.is_empty_object_literal(var_decl.initializer)));
            if is_expando_init {
                // Mirror the function-root branch: in a JS file a nested chain
                // declares its member only when the immediate base link is an
                // assignment-declared expando HOST (`expando_host_members` —
                // its own declaring RHS was an empty literal, function, or
                // class expression). This blocks closed-shape bases
                // (`M.sub = { a: 1 }` followed by `M.sub.b = e`) and
                // `Object.defineProperty(root, 'seg', …)` bases, which tsc
                // types as their literal shape and rejects the nested write
                // with TS2339 under `noImplicitAny`. `prototype` chains are
                // exempt (dedicated prototype-expando handling).
                if is_js_like_source
                    && !obj_key.split('.').any(|segment| segment == "prototype")
                    && let Some((parent_key, member_name)) = obj_key.rsplit_once('.')
                    && !self
                        .expando_host_members
                        .get(parent_key)
                        .and_then(|members| members.get(member_name))
                        .copied()
                        .unwrap_or(false)
                {
                    return;
                }
                let rhs_is_host = rhs_is_expando_host_shape(arena, rhs);
                self.expando_host_members
                    .entry(obj_key.clone())
                    .or_default()
                    .entry(prop_name.clone())
                    .and_modify(|host| *host &= rhs_is_host)
                    .or_insert(rhs_is_host);
                Arc::make_mut(&mut self.expando_properties)
                    .entry(obj_key)
                    .or_default()
                    .insert(prop_name);
            }
        }
    }

    /// Record an expando write whose root identifier does not resolve in this
    /// file's scope during single-file binding — a cross-file global, a
    /// forward/out-of-order declaration, or a sibling-file host. The checker
    /// aggregates `expando_properties` across every file's binder by string
    /// key and re-resolves the root cross-file before honoring the read, so
    /// recording the raw syntactic write here is safe: a non-object /
    /// non-callable or genuinely undeclared root still fails the checker's
    /// `root_symbol_supports_js_expando_read` gate and reports TS2339.
    ///
    /// Recording is restricted to JS-like sources and to the
    /// class-expression / function / object-literal RHS shapes that make the
    /// member an assignment-declared type or callable host, mirroring the
    /// positive shape the co-located object-var branch requires. Element-access
    /// writes and prototype members keep their dedicated handling.
    fn record_unresolved_root_expando_write(
        &mut self,
        arena: &NodeArena,
        lhs_kind: u16,
        obj_key: &str,
        prop_name: &str,
        rhs: NodeIndex,
    ) {
        if lhs_kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        // Only a *simple* root (`Root.member = …`, no interior dots) is a
        // genuinely cross-file top-level host whose bare-read member access has
        // no other resolution and would otherwise surface a spurious TS2339. A
        // nested base (`Root.ns.member = …`, `obj_key` contains a dot) is itself
        // an assignment-declared expando member resolved by the checker's
        // cross-file nested-expando walk, which already sees every member
        // regardless of RHS. Recording a partial member set under the nested key
        // here would shadow that walk with an incomplete, closed object type and
        // drop the members this predicate cannot classify (e.g. IIFE-call
        // initializers), so leave nested hosts to the existing walk.
        if obj_key.contains('.') {
            return;
        }
        // The CommonJS `module` / `exports` sentinels never resolve as user
        // symbols, so they would otherwise reach this unresolved-root path.
        // `module.exports = …` is a whole-module export assignment (and
        // `module.exports.x = …` / `exports.x = …` are handled by the dedicated
        // CommonJS branch above), not an object-var expando; recording an
        // `expando["module"] = {"exports"}` entry here would shadow that export
        // machinery and break callable `module.exports()` / `require(...)`.
        if obj_key == "module" || obj_key == "exports" {
            return;
        }
        let is_js_like_source = arena.source_files.first().is_some_and(|source_file| {
            let file_name = source_file.file_name.to_ascii_lowercase();
            !source_file.is_declaration_file
                && (file_name.ends_with(".js")
                    || file_name.ends_with(".jsx")
                    || file_name.ends_with(".mjs")
                    || file_name.ends_with(".cjs"))
        });
        if !is_js_like_source {
            return;
        }
        let Some(rhs_node) = arena.get(rhs) else {
            return;
        };
        let rhs_is_expando_host = rhs_node.is_function_expression_or_arrow()
            || rhs_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION;
        if !rhs_is_expando_host {
            return;
        }
        Arc::make_mut(&mut self.expando_properties)
            .entry(obj_key.to_string())
            .or_default()
            .insert(prop_name.to_string());
    }
}
