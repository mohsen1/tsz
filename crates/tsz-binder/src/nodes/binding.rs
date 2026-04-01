//! AST node binding, hoisting, and scope management.

use crate::{ContainerKind, ScopeContext, SymbolId, SymbolTable, flow_flags, symbol_flags};
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use crate::state::{BinderState, FileFeatures};

impl BinderState {
    fn declaration_span(arena: &NodeArena, declaration: NodeIndex) -> Option<(u32, u32)> {
        arena.get(declaration).map(|node| (node.pos, node.end))
    }

    pub(crate) fn is_inside_class_member_computed_property_name(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let mut current = idx;
        while current.is_some() {
            let Some(ext) = arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent) = arena.get(parent_idx) else {
                return false;
            };

            if parent.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                let computed_idx = parent_idx;
                let Some(computed_ext) = arena.get_extended(computed_idx) else {
                    return false;
                };
                let member_idx = computed_ext.parent;
                let Some(member_ext) = arena.get_extended(member_idx) else {
                    return false;
                };
                let class_idx = member_ext.parent;
                let Some(owner) = arena.get(class_idx) else {
                    return false;
                };
                if owner.kind == syntax_kind_ext::CLASS_DECLARATION
                    || owner.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return true;
                }
                current = parent_idx;
                continue;
            }

            if parent.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent.kind == syntax_kind_ext::METHOD_DECLARATION
                || parent.kind == syntax_kind_ext::CONSTRUCTOR
                || parent.kind == syntax_kind_ext::GET_ACCESSOR
                || parent.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return false;
            }

            current = parent_idx;
        }
        false
    }

    /// Collect hoisted declarations from statements.
    pub(crate) fn collect_hoisted_declarations(
        &mut self,
        arena: &NodeArena,
        statements: &NodeList,
    ) {
        self.collect_hoisted_declarations_impl(arena, statements, false);
    }

    /// Internal implementation with block tracking.
    fn collect_hoisted_declarations_impl(
        &mut self,
        arena: &NodeArena,
        statements: &NodeList,
        in_block: bool,
    ) {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = arena.get_variable(node) {
                            // VariableStatement stores declaration_list as first element
                            if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                                self.collect_hoisted_var_decl(arena, decl_list_idx);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        // Function declarations inside blocks are block-scoped when:
                        // - The file is an external module (ES6 modules), or
                        // - The scope is in strict mode ("use strict" or --alwaysStrict), or
                        // - The target is ES2015 or later.
                        // In non-strict, non-module ES3/ES5 scripts, they hoist (Annex B behavior).
                        let is_es6_or_later = self.options.target as u32
                            >= tsz_common::common::ScriptTarget::ES2015 as u32;
                        let block_scoped = in_block
                            && (self.is_external_module || self.is_strict_scope || is_es6_or_later);
                        if !block_scoped {
                            self.hoisted_functions.push(stmt_idx);
                        }
                    }
                    k if k == syntax_kind_ext::BLOCK => {
                        // Always recurse into blocks for var hoisting (var is always
                        // function-scoped regardless of target).
                        // Pass in_block=true to prevent function hoisting from blocks.
                        if let Some(block) = arena.get_block(node) {
                            self.collect_hoisted_declarations_impl(arena, &block.statements, true);
                        }
                    }
                    k if k == syntax_kind_ext::IF_STATEMENT => {
                        if let Some(if_stmt) = arena.get_if_statement(node) {
                            self.collect_hoisted_from_node(arena, if_stmt.then_statement);
                            if if_stmt.else_statement.is_some() {
                                self.collect_hoisted_from_node(arena, if_stmt.else_statement);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::WHILE_STATEMENT
                        || k == syntax_kind_ext::DO_STATEMENT =>
                    {
                        if let Some(loop_data) = arena.get_loop(node) {
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_STATEMENT => {
                        if let Some(loop_data) = arena.get_loop(node) {
                            // Hoist var declarations from initializer (e.g., `for (var i = 0; ...)`)
                            let init = loop_data.initializer;
                            if init.is_some()
                                && let Some(init_node) = arena.get(init)
                                && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            {
                                self.collect_hoisted_var_decl(arena, init);
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, loop_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::FOR_IN_STATEMENT
                        || k == syntax_kind_ext::FOR_OF_STATEMENT =>
                    {
                        if let Some(for_data) = arena.get_for_in_of(node) {
                            // Hoist var declarations from the initializer (e.g., `for (var x in obj)`)
                            let init = for_data.initializer;
                            if let Some(init_node) = arena.get(init)
                                && init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            {
                                self.collect_hoisted_var_decl(arena, init);
                            }
                            // Hoist from the loop body
                            self.collect_hoisted_from_node(arena, for_data.statement);
                        }
                    }
                    k if k == syntax_kind_ext::TRY_STATEMENT => {
                        if let Some(try_data) = arena.get_try(node) {
                            // Hoist from try block
                            self.collect_hoisted_from_node(arena, try_data.try_block);
                            // Hoist from catch clause's block
                            if try_data.catch_clause.is_some()
                                && let Some(catch_data) =
                                    arena.get_catch_clause_at(try_data.catch_clause)
                            {
                                self.collect_hoisted_from_node(arena, catch_data.block);
                            }
                            // Hoist from finally block
                            if try_data.finally_block.is_some() {
                                self.collect_hoisted_from_node(arena, try_data.finally_block);
                            }
                        }
                    }
                    k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                        if let Some(switch_data) = arena.get_switch(node) {
                            // The case_block is treated as a block - get its children (case/default clauses)
                            if let Some(block_data) = arena.get_block_at(switch_data.case_block) {
                                // Each child is a case/default clause with statements
                                for &clause_idx in &block_data.statements.nodes {
                                    if let Some(clause_data) = arena.get_case_clause_at(clause_idx)
                                    {
                                        self.collect_hoisted_declarations(
                                            arena,
                                            &clause_data.statements,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    k if k == syntax_kind_ext::LABELED_STATEMENT => {
                        if let Some(label_data) = arena.get_labeled_statement(node) {
                            self.collect_hoisted_from_node(arena, label_data.statement);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_var_decl(&mut self, arena: &NodeArena, decl_list_idx: NodeIndex) {
        if let Some(node) = arena.get(decl_list_idx)
            && let Some(list) = arena.get_variable(node)
        {
            // Check if this is a var declaration (not let/const)
            let is_var = (u32::from(node.flags) & (node_flags::LET | node_flags::CONST)) == 0;
            if is_var {
                for &decl_idx in &list.declarations.nodes {
                    if let Some(decl) = arena.get_variable_declaration_at(decl_idx) {
                        if let Some(name) = Self::get_identifier_name(arena, decl.name) {
                            self.hoisted_vars.push((name.to_string(), decl_idx));
                        } else {
                            let mut names = Vec::new();
                            Self::collect_binding_identifiers(arena, decl.name, &mut names);
                            for ident_idx in names {
                                if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                                    self.hoisted_vars.push((name.to_string(), ident_idx));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_hoisted_from_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            if node.kind == syntax_kind_ext::BLOCK {
                // Always recurse into blocks for var hoisting (var is always
                // function-scoped regardless of target).
                // Function declarations in blocks are block-scoped in ES6+ modules.
                if let Some(block) = arena.get_block(node) {
                    self.collect_hoisted_declarations_impl(arena, &block.statements, true);
                }
            } else {
                // Handle single statement (not wrapped in a block)
                // e.g., `if (x) var y = 1;` or `while (x) var i = 0;`
                // These are at the same scope level, not in a block.
                let mut stmts = tsz_parser::NodeList::new();
                stmts.nodes.push(idx);
                self.collect_hoisted_declarations(arena, &stmts);
            }
        }
    }

    /// Process hoisted function declarations.
    pub(crate) fn process_hoisted_functions(&mut self, arena: &NodeArena) {
        let functions = std::mem::take(&mut self.hoisted_functions);
        for func_idx in functions {
            if let Some(node) = arena.get(func_idx)
                && let Some(func) = arena.get_function(node)
                && let Some(name) = Self::get_identifier_name(arena, func.name)
            {
                let is_exported = Self::has_export_modifier(arena, func.modifiers.as_ref());
                let sym_id =
                    self.declare_symbol(arena, name, symbol_flags::FUNCTION, func_idx, is_exported);

                // Also add to persistent scope
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Process hoisted var declarations.
    /// Var declarations are hoisted to the top of their function/global scope.
    pub(crate) fn process_hoisted_vars(&mut self, arena: &NodeArena) {
        let hoisted_vars = std::mem::take(&mut self.hoisted_vars);
        for (name, decl_idx) in hoisted_vars {
            // Declare the var symbol with FUNCTION_SCOPED_VARIABLE flag
            // This makes it accessible before its actual declaration point
            let is_exported = Self::is_node_exported(arena, decl_idx);
            let sym_id = self.declare_symbol(
                arena,
                &name,
                symbol_flags::FUNCTION_SCOPED_VARIABLE,
                decl_idx,
                is_exported,
            );

            // Also add to persistent scope
            self.declare_in_persistent_scope(name, sym_id);
        }
    }

    /// Bind a node and its children.
    pub(crate) fn bind_node(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::RETURN_STATEMENT
        {
            tracing::debug!(idx = idx.0, kind = node.kind, "bind_node called");
        }

        self.bind_node_by_node_kind(arena, node, idx);
    }

    #[inline]
    fn bind_node_by_node_kind(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.record_flow(idx);
            }
            k if k == syntax_kind_ext::HERITAGE_CLAUSE => {
                if let Some(heritage) = arena.get_heritage_clause(node) {
                    for &type_idx in &heritage.types.nodes {
                        self.bind_node(arena, type_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = arena.get_expr_type_args(node) {
                    self.bind_expression(arena, expr.expression);
                    self.bind_type_parameters(arena, expr.type_arguments.as_ref());
                }
            }
            // Variable declarations
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = arena.get_variable(node) {
                    // Track using/await-using features for TS2318 diagnostics
                    if let Some(&decl_list_idx) = var_stmt.declarations.nodes.first() {
                        if let Some(list_node) = arena.get(decl_list_idx) {
                            let flags = u32::from(list_node.flags);
                            if (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING {
                                self.file_features.set(FileFeatures::AWAIT_USING);
                            } else if (flags & node_flags::USING) != 0 {
                                self.file_features.set(FileFeatures::USING);
                            }
                        }
                        self.bind_node(arena, decl_list_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(list) = arena.get_variable(node) {
                    for &decl_idx in &list.declarations.nodes {
                        self.bind_node(arena, decl_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                self.record_flow(idx);
                self.bind_variable_declaration(arena, node, idx);
            }

            // Function declarations
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.bind_function_declaration(arena, node, idx);
            }

            // Method declarations (in object literals)
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = arena.get_method_decl(node) {
                    if method.asterisk_token {
                        self.file_features.set(FileFeatures::GENERATORS);
                    }
                    self.bind_callable_body_with_type_params(
                        arena,
                        &method.parameters,
                        method.body,
                        idx,
                        method.type_parameters.as_ref(),
                    );
                }
            }

            // Get/Set accessors (in object literals and classes)
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = arena.get_accessor(node) {
                    self.bind_callable_body(arena, &accessor.parameters, accessor.body, idx);
                }
            }

            // Class declarations
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.record_flow(idx);
                self.bind_class_declaration(arena, node, idx);
            }
            k if k == syntax_kind_ext::CLASS_EXPRESSION => {
                self.bind_class_expression(arena, node, idx);
            }

            // Interface declarations
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.record_flow(idx);
                self.bind_interface_declaration(arena, node, idx);
            }

            // Type alias declarations
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.record_flow(idx);
                self.bind_type_alias_declaration(arena, node, idx);
            }

            // Enum declarations
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.bind_enum_declaration(arena, node, idx);
            }

            // Block - creates a new block scope
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = arena.get_block(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    for &stmt_idx in &block.statements.nodes {
                        self.bind_node(arena, stmt_idx);
                    }
                    self.exit_scope(arena);
                }
            }

            // If statement - build flow graph for type narrowing
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.record_flow(idx);
                if let Some(if_stmt) = arena.get_if_statement(node) {
                    use tracing::trace;

                    // Bind the condition expression (record identifiers in it)
                    self.bind_expression(arena, if_stmt.expression);

                    // Save the pre-condition flow
                    let pre_condition_flow = self.current_flow;
                    trace!(
                        pre_condition_flow = pre_condition_flow.0,
                        "if statement: pre_condition_flow",
                    );

                    // Create TRUE_CONDITION flow for the then branch
                    let true_flow = self.create_flow_condition(
                        flow_flags::TRUE_CONDITION,
                        pre_condition_flow,
                        if_stmt.expression,
                    );
                    trace!(
                        true_flow = true_flow.0,
                        "if statement: created TRUE_CONDITION flow",
                    );

                    // Bind the then branch with narrowed flow
                    self.current_flow = true_flow;
                    trace!("if statement: binding then branch with TRUE_CONDITION flow");
                    self.bind_node(arena, if_stmt.then_statement);
                    let after_then_flow = self.current_flow;
                    trace!(
                        after_then_flow = after_then_flow.0,
                        "if statement: after_then_flow",
                    );

                    // Handle else branch if present
                    let after_else_flow = if if_stmt.else_statement.is_none() {
                        // No else branch - false condition goes directly to merge
                        self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            if_stmt.expression,
                        )
                    } else {
                        // Create FALSE_CONDITION flow for the else branch
                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            if_stmt.expression,
                        );
                        trace!(
                            false_flow = false_flow.0,
                            "if statement: created FALSE_CONDITION flow",
                        );

                        // Bind the else branch with narrowed flow
                        self.current_flow = false_flow;
                        trace!("if statement: binding else branch with FALSE_CONDITION flow");
                        self.bind_node(arena, if_stmt.else_statement);
                        let result = self.current_flow;
                        trace!(result = result.0, "if statement: after_else_flow",);
                        result
                    };

                    // Create merge point for branches
                    let merge_label = self.create_branch_label();
                    trace!(
                        merge_label = merge_label.0,
                        "if statement: created merge label",
                    );
                    self.add_antecedent(merge_label, after_then_flow);
                    self.add_antecedent(merge_label, after_else_flow);
                    self.current_flow = merge_label;
                }
            }

            // While/do statement
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = arena.get_loop(node) {
                    let _ = self.current_flow;
                    let loop_label = self.create_loop_label();
                    if self.current_flow.is_some() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    // Create post-loop merge point for break targets
                    let post_loop = self.create_branch_label();
                    self.break_targets.push(post_loop);
                    self.continue_targets.push(loop_label);

                    if node.kind == syntax_kind_ext::DO_STATEMENT {
                        self.bind_node(arena, loop_data.statement);
                        self.bind_expression(arena, loop_data.condition);

                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.add_antecedent(loop_label, true_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.add_antecedent(post_loop, pre_condition_flow);
                        self.add_antecedent(post_loop, false_flow);
                    } else {
                        self.bind_expression(arena, loop_data.condition);

                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.current_flow = true_flow;
                        self.bind_node(arena, loop_data.statement);
                        self.add_antecedent(loop_label, self.current_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        // FIX: Don't add pre_loop_flow as antecedent to merge_label
                        // The exit path must go through false_flow to preserve narrowing
                        self.add_antecedent(post_loop, false_flow);
                    }

                    self.break_targets.pop();
                    self.continue_targets.pop();
                    self.current_flow = post_loop;
                }
            }

            // For statement
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.record_flow(idx);
                if let Some(loop_data) = arena.get_loop(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    self.bind_node(arena, loop_data.initializer);

                    let _ = self.current_flow;
                    let loop_label = self.create_loop_label();
                    if self.current_flow.is_some() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    // Create post-loop merge point for break targets
                    let post_loop = self.create_branch_label();
                    self.break_targets.push(post_loop);
                    let continue_target = if loop_data.incrementor.is_some() {
                        self.create_branch_label()
                    } else {
                        loop_label
                    };
                    self.continue_targets.push(continue_target);

                    if loop_data.condition.is_none() {
                        self.bind_node(arena, loop_data.statement);
                        self.add_antecedent(continue_target, self.current_flow);
                        if loop_data.incrementor.is_some() {
                            self.current_flow = continue_target;
                        }
                        self.bind_expression(arena, loop_data.incrementor);
                        self.add_antecedent(loop_label, self.current_flow);
                        self.add_antecedent(post_loop, loop_label);
                        self.add_antecedent(post_loop, self.current_flow);
                    } else {
                        self.bind_expression(arena, loop_data.condition);
                        let pre_condition_flow = self.current_flow;
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        self.current_flow = true_flow;
                        self.bind_node(arena, loop_data.statement);
                        self.add_antecedent(continue_target, self.current_flow);
                        if loop_data.incrementor.is_some() {
                            self.current_flow = continue_target;
                        }
                        self.bind_expression(arena, loop_data.incrementor);
                        self.add_antecedent(loop_label, self.current_flow);

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            pre_condition_flow,
                            loop_data.condition,
                        );
                        // FIX: Don't add pre_loop_flow as antecedent to merge_label
                        // The exit path must go through false_flow to preserve narrowing
                        self.add_antecedent(post_loop, false_flow);
                    }

                    self.break_targets.pop();
                    self.continue_targets.pop();
                    self.current_flow = post_loop;
                    self.exit_scope(arena);
                }
            }

            // For-in/for-of
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                self.record_flow(idx);
                if let Some(for_data) = arena.get_for_in_of(node) {
                    self.enter_scope(ContainerKind::Block, idx);
                    self.bind_node(arena, for_data.initializer);
                    let loop_label = self.create_loop_label();
                    if self.current_flow.is_some() {
                        self.add_antecedent(loop_label, self.current_flow);
                    }
                    self.current_flow = loop_label;

                    // Create post-loop merge point for break targets
                    let post_loop = self.create_branch_label();
                    self.break_targets.push(post_loop);
                    self.continue_targets.push(loop_label);

                    // Match tsc's bindForInOrForOfStatement: post-loop only gets
                    // the loop label as antecedent (representing "iterator exhausted").
                    // The end-of-body flow only feeds back to the loop label, NOT
                    // directly to post-loop. Previously, adding end-of-body to
                    // post-loop bypassed the LOOP_LABEL's fixed-point analysis and
                    // introduced un-narrowed types after the loop.
                    self.add_antecedent(post_loop, loop_label);

                    self.bind_expression(arena, for_data.expression);
                    if for_data.initializer.is_some() {
                        let flow = self.create_flow_assignment(for_data.initializer);
                        self.current_flow = flow;
                    }
                    self.bind_node(arena, for_data.statement);
                    self.add_antecedent(loop_label, self.current_flow);

                    self.break_targets.pop();
                    self.continue_targets.pop();
                    self.current_flow = post_loop;
                    self.exit_scope(arena);
                }
            }

            // Switch statement
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.bind_switch_statement(arena, node, idx);
            }

            // Try statement
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.bind_try_statement(arena, node, idx);
            }

            // Labeled statement
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = arena.get_labeled_statement(node) {
                    self.bind_node(arena, labeled.statement);
                }
            }

            // With statement
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = arena.get_with_statement(node) {
                    self.bind_node(arena, with_stmt.expression);
                    self.bind_node(arena, with_stmt.then_statement);
                }
            }

            // Import declarations
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.bind_import_declaration(arena, node, idx);
            }

            // Import equals declaration (import x = ns.member)
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.bind_import_equals_declaration(arena, node, idx);
            }

            // Export declarations - bind the exported declaration
            k if k == syntax_kind_ext::EXPORT_DECLARATION
                || k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION =>
            {
                self.bind_export_declaration(arena, node, idx);
            }
            // Export assignment - bind the assigned expression
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(assign) = arena.get_export_assignment(node) {
                    // export = expr; exports all members of expr as module exports
                    // For example: export = Utils; makes all Utils exports available
                    self.bind_node(arena, assign.expression);

                    // If the expression is an identifier, resolve it and copy its exports
                    if let Some(name) = Self::get_identifier_name(arena, assign.expression)
                        && let Some(sym_id) = self
                            .current_scope
                            .get(name)
                            .or_else(|| self.file_locals.get(name))
                    {
                        // Track the explicit `export =` target so require-import resolution
                        // can recover the assigned symbol directly.
                        self.file_locals.set("export=".to_string(), sym_id);

                        // Copy the symbol's exports to the current module's exports.
                        // This makes export = Namespace; work correctly.
                        // Only add names that don't already exist in file_locals to
                        // avoid shadowing global/ambient declarations (e.g., DOM types
                        // like ClipboardEvent should not be shadowed by React.ClipboardEvent
                        // when `export = React` appears inside `declare module "react"`).
                        if let Some(symbol) = self.symbols.get(sym_id)
                            && let Some(ref exports) = symbol.exports
                        {
                            for (export_name, &export_sym_id) in exports.iter() {
                                // Skip "default" and "export=" — the `export =` target
                                // itself IS the default export. Copying a static member
                                // named `default` would shadow the `export=` symbol and
                                // cause default-import resolution to pick up the member
                                // (e.g. `static default: "foo"`) instead of the class.
                                if export_name == "default" || export_name == "export=" {
                                    continue;
                                }
                                if self.file_locals.get(export_name).is_none() {
                                    self.file_locals.set(export_name.clone(), export_sym_id);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                self.bind_node_by_node_kind_tail(arena, node, idx);
            }
        }
    }

    #[inline]
    fn bind_node_by_node_kind_tail(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        match node.kind {
            // Module/namespace declarations
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.bind_module_declaration(arena, node, idx);
            }
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(block) = arena.get_module_block(node)
                    && let Some(ref statements) = block.statements
                {
                    for &stmt_idx in &statements.nodes {
                        self.bind_node(arena, stmt_idx);
                    }
                }
            }
            // Expression statements - record flow and traverse into the expression
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.record_flow(idx);
                if let Some(expr_stmt) = arena.get_expression_statement(node) {
                    // Use bind_expression instead of bind_node to properly record flow
                    // for identifiers within property access expressions etc.
                    self.bind_expression(arena, expr_stmt.expression);
                }
            }

            // Return/throw statements - traverse into the expression and mark unreachable
            k if k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT =>
            {
                self.record_flow(idx);
                if let Some(ret) = arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    tracing::debug!(
                        return_idx = idx.0,
                        expr_idx = ret.expression.0,
                        "Binding return expression"
                    );
                    self.bind_node(arena, ret.expression);
                }
                // For return statements inside IIFEs, redirect flow to the return
                // target label. This ensures the IIFE's return doesn't make the
                // outer function's flow unreachable.
                if node.kind == syntax_kind_ext::RETURN_STATEMENT
                    && let Some(&return_target) = self.return_targets.last()
                {
                    self.add_antecedent(return_target, self.current_flow);
                }
                self.current_flow = self.unreachable_flow;
            }

            // Break statement - jump to break target and mark unreachable
            k if k == syntax_kind_ext::BREAK_STATEMENT => {
                if let Some(&break_target) = self.break_targets.last() {
                    self.add_antecedent(break_target, self.current_flow);
                }
                self.current_flow = self.unreachable_flow;
            }

            // Continue statement - jump to continue target and mark unreachable
            k if k == syntax_kind_ext::CONTINUE_STATEMENT => {
                if let Some(&continue_target) = self.continue_targets.last() {
                    self.add_antecedent(continue_target, self.current_flow);
                }
                self.current_flow = self.unreachable_flow;
            }

            // Binary expressions - traverse into operands
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                // Delegate to bind_expression which handles short-circuit operators
                // (&&, ||, ??) with proper TRUE_CONDITION/FALSE_CONDITION flow nodes.
                // This ensures narrowing works in all expression contexts (return
                // statements, variable initializers, etc.), not just conditions.
                self.bind_expression(arena, idx);
            }

            // Conditional expressions - build flow graph for type narrowing
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
                    self.bind_node(arena, cond.when_true);
                    let after_true_flow = self.current_flow;

                    // Create FALSE_CONDITION flow for when_false branch
                    let false_flow = self.create_flow_condition(
                        flow_flags::FALSE_CONDITION,
                        pre_condition_flow,
                        cond.condition,
                    );
                    self.current_flow = false_flow;
                    self.bind_node(arena, cond.when_false);
                    let after_false_flow = self.current_flow;

                    // Create merge point for both branches
                    let merge_label = self.create_branch_label();
                    self.add_antecedent(merge_label, after_true_flow);
                    self.add_antecedent(merge_label, after_false_flow);
                    self.current_flow = merge_label;
                }
            }

            // Property access / element access
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_node(arena, access.expression);
                    self.bind_node(arena, access.name_or_argument);
                }
            }

            // Prefix/postfix unary expressions
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_node(arena, unary.operand);
                    if (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && !Self::is_inside_class_member_computed_property_name(arena, idx)
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
            }

            // Non-null expression - just bind the inner expression
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if node.has_data()
                    && let Some(unary) = arena.unary_exprs_ex.get(node.data_index as usize)
                {
                    self.bind_node(arena, unary.expression);
                }
            }

            // Await expression - create flow node for async suspension point
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
                }
                let flow = self.create_flow_await_point(idx);
                self.current_flow = flow;
            }

            // Yield expression - create flow node for generator suspension point
            k if k == syntax_kind_ext::YIELD_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
                }
                let flow = self.create_flow_yield_point(idx);
                self.current_flow = flow;
            }

            // Type assertions / as / satisfies — record flow so type-position children
            // (e.g. QualifiedName inside `typeof x.p` in `... as typeof x.p`) can
            // find the enclosing flow context via parent-walk for flow narrowing.
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.record_flow(idx);
                if node.has_data()
                    && let Some(assertion) = arena.type_assertions.get(node.data_index as usize)
                {
                    self.bind_node(arena, assertion.expression);
                }
            }

            // Decorators
            k if k == syntax_kind_ext::DECORATOR => {
                self.file_features.set(FileFeatures::DECORATORS);
                if let Some(decorator) = arena.get_decorator(node) {
                    self.bind_node(arena, decorator.expression);
                }
            }

            // Tagged templates
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data()
                    && let Some(tagged) = arena.tagged_templates.get(node.data_index as usize)
                {
                    self.bind_node(arena, tagged.tag);
                    self.bind_node(arena, tagged.template);
                }
            }

            // Template expressions
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = arena.get_template_expr(node) {
                    self.bind_node(arena, template.head);
                    for &span in &template.template_spans.nodes {
                        self.bind_node(arena, span);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = arena.get_template_span(node) {
                    self.bind_node(arena, span.expression);
                    self.bind_node(arena, span.literal);
                }
            }

            // Object/array literals
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                if let Some(lit) = arena.get_literal_expr(node) {
                    for &elem in &lit.elements.nodes {
                        self.bind_node(arena, elem);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = arena.get_property_assignment(node) {
                    self.bind_node(arena, prop.name);
                    self.bind_node(arena, prop.initializer);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(prop) = arena.get_shorthand_property(node) {
                    self.bind_node(arena, prop.name);
                    if prop.object_assignment_initializer.is_some() {
                        self.bind_node(arena, prop.object_assignment_initializer);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = arena.get_spread(node) {
                    self.bind_node(arena, spread.expression);
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = arena.get_computed_property(node) {
                    self.bind_node(arena, computed.expression);
                }
            }

            // Call expressions - traverse into callee and arguments.
            // For IIFEs, bind arguments BEFORE the function expression so that
            // argument side-effects are in the flow context before the IIFE body.
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    let callee_idx = arena.skip_parenthesized(call.expression);
                    let is_iife = arena.get(callee_idx).is_some_and(|n| {
                        n.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                            || n.kind == syntax_kind_ext::ARROW_FUNCTION
                    });

                    if is_iife {
                        // IIFE: bind arguments first (in outer flow context), then callee.
                        // This matches tsc's binding order for IIFEs.
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.bind_node(arena, arg);
                            }
                        }
                        self.bind_node(arena, call.expression);
                    } else {
                        // Normal call: bind callee first, then arguments.
                        self.bind_node(arena, call.expression);
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.bind_node(arena, arg);
                            }
                        }
                    }
                    let flow = self.create_flow_call(idx);
                    self.current_flow = flow;
                    if Self::is_array_mutation_call(arena, idx) {
                        let flow = self.create_flow_array_mutation(idx);
                        self.current_flow = flow;
                    }
                }
            }

            // New expressions - traverse into expression and arguments
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(new_expr) = arena.get_call_expr(node) {
                    self.bind_node(arena, new_expr.expression);
                    if let Some(args) = &new_expr.arguments {
                        for &arg in &args.nodes {
                            self.bind_node(arena, arg);
                        }
                    }
                }
            }

            // Parenthesized expressions - record flow and traverse into inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.record_flow(idx);
                if let Some(paren) = arena.get_parenthesized(node) {
                    self.bind_node(arena, paren.expression);
                }
            }

            // Arrow function expressions - bind body
            k if k == syntax_kind_ext::ARROW_FUNCTION => {
                tracing::debug!(arrow_idx = idx.0, "MATCHED ARROW_FUNCTION in bind_node");
                self.bind_arrow_function(arena, node, idx);
            }

            // Function expressions - bind body
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                self.bind_function_expression(arena, node, idx);
            }

            // Typeof, void expressions - record flow and traverse into operand
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION
                || k == syntax_kind_ext::VOID_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_node(arena, unary.operand);
                }
            }

            // Await, yield expressions - record flow and traverse into expression
            // Note: These use unary_exprs_ex storage with `expression` field, not unary_exprs
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION =>
            {
                self.record_flow(idx);
                if let Some(unary) = arena.get_unary_expr_ex(node) {
                    self.bind_node(arena, unary.expression);
                }
            }

            // JSX elements - recurse into children for flow graph
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(jsx) = arena.get_jsx_element(node) {
                    self.bind_node(arena, jsx.opening_element);
                    for &child in &jsx.children.nodes {
                        self.bind_node(arena, child);
                    }
                    self.bind_node(arena, jsx.closing_element);
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_OPENING_ELEMENT =>
            {
                if let Some(opening) = arena.get_jsx_opening(node) {
                    self.bind_node(arena, opening.attributes);
                }
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(fragment) = arena.get_jsx_fragment(node) {
                    for &child in &fragment.children.nodes {
                        self.bind_node(arena, child);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                if let Some(attrs) = arena.get_jsx_attributes(node) {
                    for &prop in &attrs.properties.nodes {
                        self.bind_node(arena, prop);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                if let Some(attr) = arena.get_jsx_attribute(node) {
                    self.bind_node(arena, attr.initializer);
                }
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                if let Some(spread) = arena.get_jsx_spread_attribute(node) {
                    self.bind_node(arena, spread.expression);
                }
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(expr) = arena.get_jsx_expression(node) {
                    self.bind_node(arena, expr.expression);
                }
            }

            _ => {
                // For other node types, no symbols to create
            }
        }
    }

    /// Check if a node is exported.
    /// Handles walking up the tree for `VariableDeclaration` -> `VariableStatement`.
    pub(crate) fn is_node_exported(arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        // 1. Check direct modifiers (Function, Class, Interface, Enum, Module, TypeAlias)
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = arena.get_function(node) {
                    return Self::has_export_modifier(arena, func.modifiers.as_ref());
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = arena.get_class(node) {
                    return Self::has_export_modifier(arena, class.modifiers.as_ref());
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = arena.get_interface(node) {
                    return Self::has_export_modifier(arena, iface.modifiers.as_ref());
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = arena.get_type_alias(node) {
                    return Self::has_export_modifier(arena, alias.modifiers.as_ref());
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = arena.get_enum(node) {
                    return Self::has_export_modifier(arena, enum_decl.modifiers.as_ref());
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = arena.get_module(node) {
                    return Self::has_export_modifier(arena, module.modifiers.as_ref());
                }
            }
            // 2. Handle VariableDeclaration (walk up to VariableStatement)
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                // Walk up: VariableDeclaration -> VariableDeclarationList -> VariableStatement
                if let Some(ext) = arena.get_extended(idx) {
                    let list_idx = ext.parent;
                    if let Some(list_ext) = arena.get_extended(list_idx) {
                        let stmt_idx = list_ext.parent;
                        if let Some(stmt_node) = arena.get(stmt_idx)
                            && stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                            && let Some(var_stmt) = arena.get_variable(stmt_node)
                        {
                            return Self::has_export_modifier(arena, var_stmt.modifiers.as_ref());
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Declare a symbol in the current scope, merging when allowed.
    pub(crate) fn declare_symbol(
        &mut self,
        arena: &NodeArena,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
    ) -> SymbolId {
        if let Some(existing_id) = self.current_scope.get(name) {
            // Check if the existing symbol is in the local symbol table.
            // If not (e.g., it's from a lib binder), we should create a new local symbol
            // to shadow the lib symbol with the local declaration.
            if self.symbols.get(existing_id).is_none() {
                // The existing_id is from a lib binder, not our local binder.
                // Create a new symbol in the local binder to shadow the lib symbol.
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.node_symbols.get(&ctx.container_node.0).copied());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = is_exported;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                // Update current_scope to point to the local symbol (shadowing)
                self.current_scope.set(owned_name.clone(), sym_id);
                // CRITICAL: Also update file_locals to shadow lib symbol in file-level scope
                // This ensures symbol resolution finds the local symbol instead of the lib one
                self.file_locals.set(owned_name.clone(), sym_id);
                self.node_symbols.insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }

            let existing_flags = self.symbols.get(existing_id).map_or(0, |s| s.flags);

            // In tsc, file-scope value declarations (function, var, class) shadow
            // identically-named globals from lib files — they live in different scopes.
            // Our model merges lib symbols into the file scope, so we simulate shadowing
            // by creating a new symbol instead of merging when a user function or class
            // declaration collides with a lib-originated value symbol.
            // Note: var/const/let (VARIABLE) shadowing is not yet safe because the checker
            // may need to infer the local variable's type from an expression referencing the
            // global (e.g. `const Symbol = globalThis.Symbol`), and creating a separate
            // symbol breaks that inference path. Functions and classes define their own types.
            //
            // In SCRIPT mode: interfaces and namespaces merge with globals (augmentation).
            // In MODULE mode: interfaces and type aliases shadow globals (no augmentation
            // at file scope — `declare global {}` is needed for true augmentation).
            let should_shadow_lib = if self.lib_symbol_ids.contains(&existing_id) {
                if self.is_external_module && !self.in_global_augmentation {
                    // In modules, interfaces, type aliases, and import aliases shadow lib symbols
                    // (they create module-local types/bindings, not global augmentation).
                    // Functions and classes also shadow as before.
                    // ALIAS (import declarations) must shadow to prevent cross-file contamination:
                    // without this, `import self = require(...)` in two separate modules would
                    // both merge into the global lib `self` symbol, causing false TS2300 duplicates.
                    //
                    // EXCEPTION: When inside `declare global { ... }`, interfaces and other
                    // declarations should MERGE with lib symbols, not shadow. The `declare global`
                    // block explicitly requests global augmentation even in external modules.
                    (flags
                        & (symbol_flags::FUNCTION
                            | symbol_flags::CLASS
                            | symbol_flags::INTERFACE
                            | symbol_flags::TYPE_ALIAS
                            | symbol_flags::ALIAS))
                        != 0
                } else {
                    // In scripts, only function/class shadow. Interfaces merge (augmentation).
                    (flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0
                        && (existing_flags & symbol_flags::VALUE) != 0
                        && (flags & (symbol_flags::INTERFACE | symbol_flags::MODULE)) == 0
                }
            } else {
                false
            };
            if should_shadow_lib {
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.node_symbols.get(&ctx.container_node.0).copied());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = is_exported;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                self.current_scope.set(owned_name.clone(), sym_id);
                self.file_locals.set(owned_name.clone(), sym_id);
                self.node_symbols.insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }
            // In merged namespace blocks, a non-exported variable must not merge with an
            // exported variable of the same name from a prior block. In tsc, these are
            // distinct symbols: `export var Origin: Point` in block 1 and `var Origin: string`
            // in block 2 are separate — the non-exported one is a local variable that shadows
            // the exported member within that block's scope, without affecting the namespace's
            // exported type.
            let is_in_module_scope = self
                .scope_chain
                .get(self.current_scope_idx)
                .is_some_and(|ctx| ctx.container_kind == ContainerKind::Module);
            let existing_is_exported = self.symbols.get(existing_id).is_some_and(|s| s.is_exported);
            if is_in_module_scope
                && existing_is_exported
                && !is_exported
                && (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            {
                let owned_name = name.to_string();
                let sym_id = self.symbols.alloc(flags, owned_name.clone());
                let container_sym = self
                    .scope_chain
                    .get(self.current_scope_idx)
                    .and_then(|ctx| self.node_symbols.get(&ctx.container_node.0).copied());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    let span = Self::declaration_span(arena, declaration);
                    sym.add_declaration(declaration, span);
                    if (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(declaration, span);
                    }
                    sym.is_exported = false;
                    if let Some(parent_id) = container_sym {
                        sym.parent = parent_id;
                    }
                }
                self.current_scope.set(owned_name.clone(), sym_id);
                self.node_symbols.insert(declaration.0, sym_id);
                self.declare_in_persistent_scope(owned_name, sym_id);
                return sym_id;
            }

            let can_merge = Self::can_merge_flags(existing_flags, flags);

            let combined_flags = if can_merge {
                existing_flags | flags
            } else {
                existing_flags
            };

            // Record merge event for debugging
            self.debugger
                .record_merge(name, existing_id, existing_flags, flags, combined_flags);

            if let Some(sym) = self.symbols.get_mut(existing_id) {
                if can_merge {
                    sym.flags |= flags;
                    if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                        sym.set_value_declaration(
                            declaration,
                            Self::declaration_span(arena, declaration),
                        );
                    }
                }

                sym.add_declaration(declaration, Self::declaration_span(arena, declaration));
                if is_exported {
                    sym.is_exported = true;
                }

                // Record declaration event (merge)
                self.debugger.record_declaration(
                    name,
                    existing_id,
                    combined_flags,
                    sym.declarations.len(),
                    true,
                );
            }

            self.node_symbols.insert(declaration.0, existing_id);
            self.declare_in_persistent_scope(name.to_string(), existing_id);
            return existing_id;
        }

        // For function-scoped variables (var), check if this declaration was already
        // processed during the hoisting pass. `var` declarations are hoisted to the
        // function/file scope before the main bind pass. If the current scope is a
        // block scope (e.g., for-loop), the hoisted symbol lives in a parent scope
        // and won't be found in current_scope. Look it up via node_symbols which
        // was populated during hoisting.
        if (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            && let Some(&existing_id) = self.node_symbols.get(&declaration.0)
            && self.symbols.get(existing_id).is_some_and(|sym| {
                // Only reuse the existing symbol if it was actually hoisted as a
                // function-scoped variable. Constructor parameter properties use the
                // same AST node (the Parameter) for both the class-scope PROPERTY
                // symbol and the constructor-scope parameter. Without this check,
                // the parameter binding would incorrectly reuse the PROPERTY symbol,
                // leaking it into the function scope and causing false TS2451
                // diagnostics when a static member shares the name.
                (sym.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
            })
        {
            // Already hoisted — just ensure we don't double-add the declaration
            if let Some(sym) = self.symbols.get_mut(existing_id) {
                sym.add_declaration(declaration, Self::declaration_span(arena, declaration));
                if is_exported {
                    sym.is_exported = true;
                }
            }
            self.declare_in_persistent_scope(name.to_string(), existing_id);
            return existing_id;
        }

        // Allocate the name string once and reuse via clone for all tables.
        // This reduces per-declaration heap allocations from ~5 to ~2-3.
        let owned_name = name.to_string();
        let sym_id = self.symbols.alloc(flags, owned_name.clone());
        // Set parent to the current container's symbol (namespace, class, etc.)
        let container_sym = self
            .scope_chain
            .get(self.current_scope_idx)
            .and_then(|ctx| self.node_symbols.get(&ctx.container_node.0).copied());
        if let Some(sym) = self.symbols.get_mut(sym_id) {
            let span = Self::declaration_span(arena, declaration);
            sym.add_declaration(declaration, span);
            if sym.value_declaration.is_none() && (flags & symbol_flags::VALUE) != 0 {
                sym.set_value_declaration(declaration, span);
            }
            sym.is_exported = is_exported;
            if let Some(parent_id) = container_sym {
                sym.parent = parent_id;
            }
        }
        self.current_scope.set(owned_name.clone(), sym_id);

        // Keep source-file declarations visible through file_locals.
        // This is required for nested module scopes resolving references to
        // top-level ambient symbols (e.g. `import alias = demoNS` inside `declare module`).
        //
        // IMPORTANT: Do NOT add symbols from module augmentation bodies to file_locals.
        // Module augmentation declarations (`declare module "./x" { interface Foo { ... } }`)
        // are tracked separately via `module_augmentations` and merged at type resolution time.
        // Adding them to file_locals pollutes the driver's cross-file merge, causing the
        // augmentation's symbol to overwrite the original module's exported symbol.
        if self.current_scope_id.is_some()
            && !self.in_module_augmentation
            && self
                .scopes
                .get(self.current_scope_id.0 as usize)
                .is_some_and(|scope| scope.kind == ContainerKind::SourceFile)
        {
            self.file_locals.set(owned_name.clone(), sym_id);
        }

        self.node_symbols.insert(declaration.0, sym_id);
        self.declare_in_persistent_scope(owned_name, sym_id);

        // Record declaration event (new symbol)
        self.debugger
            .record_declaration(name, sym_id, flags, 1, false);

        sym_id
    }

    /// Check if two symbol flag sets can be merged.
    /// Made public for use in checker to detect duplicate identifiers (TS2300).
    #[must_use]
    pub const fn can_merge_flags(existing_flags: u32, new_flags: u32) -> bool {
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::CLASS != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
            || (existing_flags & symbol_flags::INTERFACE != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Namespace/module can merge with interface
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow function + class merging (TypeScript allows declare function + declare class)
        if (existing_flags & symbol_flags::FUNCTION) != 0 && (new_flags & symbol_flags::CLASS) != 0
        {
            return true;
        }
        if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Allow method overloads to merge (method signature + method implementation)
        if (existing_flags & symbol_flags::METHOD) != 0 && (new_flags & symbol_flags::METHOD) != 0 {
            return true;
        }

        // Allow VARIABLE + NAMESPACE_MODULE merging.
        // TypeScript's NamespaceModuleExcludes = 0 (can merge with anything) and
        // FunctionScopedVariableExcludes doesn't include NAMESPACE_MODULE.
        // e.g., `namespace m2 { ... } var m2: { ... };`
        if (existing_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (new_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::NAMESPACE_MODULE) != 0
            && (existing_flags & symbol_flags::VARIABLE) != 0
        {
            return true;
        }

        // Allow INTERFACE to merge with VALUE symbols (e.g., `interface Object` + `declare var Object`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_ALIAS to merge with VALUE symbols
        // In TypeScript, type aliases and values exist in separate namespaces
        // and can share the same name:
        //   type Foo = number;
        //   export const Foo = 1;  // legal: Foo is both a type and a value
        if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_ALIAS) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow TYPE_PARAMETER to merge with VALUE symbols
        // e.g., `<T>(T: T) => T`
        if (existing_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::TYPE_PARAMETER) != 0
            && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        // Allow ALIAS (import) to merge with VALUE symbols.
        // In TypeScript, imports and local value declarations can share the
        // same name — the import occupies the type namespace and the local
        // declaration occupies the value namespace:
        //   import type { A } from "./a";
        //   const A: A = "a";  // legal: A is both a type and a value
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::VALUE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::VALUE) != 0 {
            return true;
        }

        // Allow ALIAS (import) to merge with local type declarations.
        // Import clauses can legally share a name with interfaces/type aliases
        // and form a single merged symbol that's usable in both namespaces:
        //   export default interface Foo {}
        //   import Foo from "./mod";
        //   export { Foo as default };
        if (existing_flags & symbol_flags::ALIAS) != 0
            && (new_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0
            && (existing_flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)) != 0
        {
            return true;
        }

        // Allow ALIAS to merge with MODULE (namespace/module).
        // In TypeScript, AliasExcludes = Alias (only conflicts with other aliases)
        // and NamespaceModuleExcludes = 0 (can merge with anything).
        // This covers `export as namespace X` + `declare namespace X` coexisting:
        //   export = React;
        //   export as namespace React;  // creates ALIAS
        //   declare namespace React {}  // creates MODULE — must merge
        if (existing_flags & symbol_flags::ALIAS) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }
        if (new_flags & symbol_flags::ALIAS) != 0 && (existing_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Allow static and instance members to have the same name
        // TypeScript allows a class to have both a static member and an instance member with the same name
        // e.g., class C { static foo; foo; }
        let existing_is_static = (existing_flags & symbol_flags::STATIC) != 0;
        let new_is_static = (new_flags & symbol_flags::STATIC) != 0;
        if existing_is_static != new_is_static {
            // One is static, one is instance - allow merge
            return true;
        }

        false
    }

    // Scope management

    pub(crate) fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        self.enter_scope_with_capacity(kind, node, 0);
    }

    /// Enter a new scope with pre-allocated capacity for the symbol table.
    /// This avoids repeated hash map resizing for scopes where the approximate
    /// member count is known (e.g., class bodies with many members).
    pub(crate) fn enter_scope_with_capacity(
        &mut self,
        kind: ContainerKind,
        node: NodeIndex,
        capacity: usize,
    ) {
        // Legacy scope chain management
        let parent = Some(self.current_scope_idx);
        self.scope_chain.push(ScopeContext::new(kind, node, parent));
        self.current_scope_idx = self.scope_chain.len() - 1;
        if capacity > 0 {
            // Take the current scope, push it, and create a pre-sized one
            let old_scope = std::mem::take(&mut self.current_scope);
            self.scope_stack.push(old_scope);
            self.current_scope = SymbolTable::with_capacity(capacity);
        } else {
            self.push_scope();
        }

        // Persistent scope management (for stateless checking)
        self.enter_persistent_scope_with_capacity(kind, node, capacity);
    }

    pub(crate) fn exit_scope(&mut self, arena: &NodeArena) {
        // Capture exports before popping if this is a module/namespace
        if let Some(ctx) = self.scope_chain.get(self.current_scope_idx) {
            match ctx.container_kind {
                ContainerKind::Module => {
                    // Find the symbol for this module/namespace
                    if let Some(sym_id) = self.node_symbols.get(&ctx.container_node.0) {
                        let export_all = self
                            .scope_chain
                            .get(self.current_scope_idx)
                            .and_then(|ctx| arena.get(ctx.container_node))
                            .and_then(|node| arena.get_module(node))
                            .is_some_and(|module| {
                                let is_external = arena.get(module.name).is_some_and(|name_node| {
                                    name_node.kind == SyntaxKind::StringLiteral as u16
                                        || name_node.kind
                                            == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                                });
                                arena.has_modifier_ref(
                                    module.modifiers.as_ref(),
                                    SyntaxKind::DeclareKeyword,
                                ) || is_external
                            });

                        // Filter exports: only include symbols with is_exported = true or EXPORT_VALUE flag
                        let mut exports = SymbolTable::new();
                        for (name, &child_id) in self.current_scope.iter() {
                            if let Some(child) = self.symbols.get(child_id) {
                                // Check explicit export flag OR if it's an EXPORT_VALUE (from export {})
                                if export_all
                                    || child.is_exported
                                    || (child.flags & symbol_flags::EXPORT_VALUE) != 0
                                {
                                    exports.set(name.clone(), child_id);
                                }
                            }
                        }

                        // Persist filtered exports
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            if let Some(ref mut existing) = symbol.exports {
                                for (name, &child_id) in exports.iter() {
                                    existing.set(name.clone(), child_id);
                                }
                            } else {
                                symbol.exports = Some(Box::new(exports));
                            }
                        }
                    }
                }
                ContainerKind::Class => {
                    // Find the symbol for this class
                    if let Some(sym_id) = self.node_symbols.get(&ctx.container_node.0) {
                        // Persist the current scope as the class's members
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            symbol.members = Some(Box::new(self.current_scope.clone()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Copy current scope to persistent scope before popping
        self.sync_current_scope_to_persistent();

        self.pop_scope();
        if let Some(ctx) = self.scope_chain.get(self.current_scope_idx)
            && let Some(parent) = ctx.parent_idx
        {
            self.current_scope_idx = parent;
        }

        // Exit persistent scope
        self.exit_persistent_scope();
    }

    pub(crate) fn push_scope(&mut self) {
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    pub(crate) fn pop_scope(&mut self) {
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }
}
