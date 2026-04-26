//! Statement Type Checking
//!
//! Handles control flow statements and dispatches declarations.
//! This module separates statement checking logic from the monolithic `CheckerState`.

use crate::context::TypingRequest;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Trait for statement checking callbacks.
///
/// This trait defines the interface that `CheckerState` must implement
/// to allow `StatementChecker` to delegate type checking and other operations.
pub trait StatementCheckCallbacks {
    /// Get access to the node arena for AST traversal.
    fn arena(&self) -> &NodeArena;

    /// Get the type of a node (expression or type annotation).
    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId;

    /// Request-aware node typing. Defaults to the legacy no-request path.
    fn get_type_of_node_with_request(&mut self, idx: NodeIndex, request: &TypingRequest) -> TypeId {
        let _ = request;
        self.get_type_of_node(idx)
    }

    /// Get the type of a node without flow narrowing.
    /// Used for switch discriminants where tsc uses the declared/widened type,
    /// not the flow-narrowed type (avoids false TS2678).
    fn get_type_of_node_no_narrowing(&mut self, idx: NodeIndex) -> TypeId;

    /// Request-aware no-narrowing node typing. Defaults to the legacy path.
    fn get_type_of_node_no_narrowing_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let _ = request;
        self.get_type_of_node_no_narrowing(idx)
    }

    /// Check a variable statement.
    fn check_variable_statement(&mut self, stmt_idx: NodeIndex);

    fn check_variable_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let _ = request;
        self.check_variable_statement(stmt_idx);
    }

    /// Check a variable declaration list.
    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex);

    fn check_variable_declaration_list_with_request(
        &mut self,
        list_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let _ = request;
        self.check_variable_declaration_list(list_idx);
    }

    /// Check a variable declaration.
    fn check_variable_declaration(&mut self, decl_idx: NodeIndex);

    fn check_variable_declaration_with_request(
        &mut self,
        decl_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let _ = request;
        self.check_variable_declaration(decl_idx);
    }

    /// Check a return statement.
    fn check_return_statement(&mut self, stmt_idx: NodeIndex);

    fn check_return_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let _ = request;
        self.check_return_statement(stmt_idx);
    }

    /// Check unreachable code in a block.
    /// Check function implementations in a block.
    fn check_function_implementations(&mut self, stmts: &[NodeIndex]);

    /// Check a function declaration.
    fn check_function_declaration(&mut self, func_idx: NodeIndex);

    /// Check a class declaration.
    fn check_class_declaration(&mut self, class_idx: NodeIndex);

    /// Check an interface declaration.
    fn check_interface_declaration(&mut self, iface_idx: NodeIndex);

    /// Check an import declaration.
    fn check_import_declaration(&mut self, import_idx: NodeIndex);

    /// Check an import equals declaration.
    fn check_import_equals_declaration(&mut self, import_idx: NodeIndex);

    /// Check an export declaration.
    fn check_export_declaration(&mut self, export_idx: NodeIndex);

    /// Check a type alias declaration.
    fn check_type_alias_declaration(&mut self, type_alias_idx: NodeIndex);

    /// Check enum duplicate members.
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex);

    /// Check a module declaration.
    fn check_module_declaration(&mut self, module_idx: NodeIndex);

    /// Check an await expression (TS1359: await outside async).
    fn check_await_expression(&mut self, expr_idx: NodeIndex);

    /// Check a for-await statement (TS1103/TS1432: for-await outside async or without proper module/target).
    fn check_for_await_statement(&mut self, stmt_idx: NodeIndex);

    /// Check if a condition expression is always truthy/falsy (TS2872/TS2873).
    fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex);

    /// TS2774: Check if a non-nullable function type is tested for truthiness
    /// without being called in the body.
    fn check_callable_truthiness(&mut self, cond_expr: NodeIndex, body: Option<NodeIndex>);

    /// Check if a condition is statically true
    fn is_true_condition(&self, condition_idx: NodeIndex) -> bool;

    /// Check if a condition is statically false
    fn is_false_condition(&self, condition_idx: NodeIndex) -> bool;

    /// Report unreachable code directly for single statements
    fn report_unreachable_statement(&mut self, stmt_idx: NodeIndex);

    /// Check an expression statement for CJS+VMS import call diagnostics (TS1295).
    /// Called before normal expression type checking.
    fn check_expression_statement(&mut self, _stmt_idx: NodeIndex, _expr_idx: NodeIndex) {
        // Default: no check
    }

    /// TS2407: Check that the right-hand side of a for-in statement is of type 'any',
    /// an object type, or a type parameter.
    fn check_for_in_expression_type(&mut self, expr_type: TypeId, expression: NodeIndex) {
        // Default: no check
        let _ = (expr_type, expression);
    }

    /// Compute the type of a for-in loop variable.
    ///
    /// tsc types `for (let k in obj)` where `obj: T` (type parameter) as
    /// `k: Extract<keyof T, string>` (= `keyof T & string`), not plain `string`.
    fn compute_for_in_variable_type(&mut self, expr_type: TypeId) -> TypeId {
        // Default: plain string (overridden in CheckerState)
        let _ = expr_type;
        TypeId::STRING
    }

    /// Assign types for for-in/for-of initializers.
    /// `is_for_in` should be true for for-in loops (to emit TS2404 on type annotations).
    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
        is_for_in: bool,
    );

    /// Get element type for for-of loop.
    /// When `is_async` is true (for-await-of), unwraps Promise<T> → T from the element type.
    fn for_of_element_type(&mut self, expr_type: TypeId, is_async: bool) -> TypeId;

    /// Check for-of iterability.
    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    );

    /// Check assignability for for-in/of expression initializer (non-declaration case).
    /// For `for (v of expr)` where `v` is a pre-declared variable (not `var v`/`let v`/`const v`),
    /// this checks:
    /// - TS2588: Cannot assign to const variable
    /// - TS2322: Element type not assignable to variable type
    fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
        has_await_modifier: bool,
    );

    /// TS2491: Check if a for-in variable declaration uses a destructuring pattern.
    fn check_for_in_destructuring_pattern(&mut self, initializer: NodeIndex);

    /// TS2491: Check if a for-in expression initializer is an array/object literal.
    fn check_for_in_expression_destructuring(&mut self, initializer: NodeIndex);

    /// Begin tracking loop-variable circular return sites while computing a
    /// `for...of` iterable expression. Returns the number of tracked symbols.
    fn begin_for_of_self_reference_tracking(&mut self, _decl_list_idx: NodeIndex) -> usize {
        0
    }

    /// End `for...of` loop-variable circular return-site tracking.
    fn end_for_of_self_reference_tracking(&mut self, _tracked_symbol_count: usize) {}

    /// TS7022: Detect self-referencing for-of loop variables under noImplicitAny.
    fn check_for_of_self_reference_circularity(
        &mut self,
        _decl_list_idx: NodeIndex,
        _expression_idx: NodeIndex,
    ) {
        // Default: no check
    }

    /// Recursively check a nested statement (callback to `check_statement`).
    fn check_statement(&mut self, stmt_idx: NodeIndex);

    fn check_statement_with_request(&mut self, stmt_idx: NodeIndex, request: &TypingRequest) {
        let _ = request;
        self.check_statement(stmt_idx);
    }

    /// Check switch statement exhaustiveness (Task 12: CFA Diagnostics).
    ///
    /// Called after all switch clauses have been checked to determine if
    /// the switch is exhaustive (handles all possible cases).
    ///
    /// Parameters:
    /// - `stmt_idx`: The switch statement node
    /// - `expression`: The discriminant expression
    /// - `case_block`: The case block containing all clauses
    /// - `has_default`: Whether the switch has a default clause
    ///
    /// Default implementation does nothing (exhaustiveness checking is optional).
    fn check_switch_exhaustiveness(
        &mut self,
        _stmt_idx: NodeIndex,
        _expression: NodeIndex,
        _case_block: NodeIndex,
        _has_default: bool,
    ) {
        // Default: no exhaustiveness checking
    }

    /// Get the type of a case expression with contextual typing from the switch
    /// expression type. In tsc, case expressions use the switch discriminant type
    /// as their contextual type, which enables excess property checking (TS2353)
    /// for object literal case expressions.
    fn get_type_of_case_expression(&mut self, case_expr: NodeIndex, switch_type: TypeId) -> TypeId {
        // Default: no contextual typing
        let _ = switch_type;
        self.get_type_of_node(case_expr)
    }

    fn get_type_of_case_expression_with_request(
        &mut self,
        case_expr: NodeIndex,
        switch_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let _ = request;
        self.get_type_of_case_expression(case_expr, switch_type)
    }

    /// Check that a case expression type is comparable to the switch expression type.
    /// Emits TS2678 if the types have no overlap.
    fn check_switch_case_comparable(
        &mut self,
        switch_type: TypeId,
        case_type: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
    ) {
        // Default: no comparability checking
        let _ = (switch_type, case_type, switch_expr, case_expr);
    }

    /// Check a break statement for validity.
    /// TS1105: A 'break' statement can only be used within an enclosing iteration statement.
    fn check_break_statement(&mut self, stmt_idx: NodeIndex);

    /// Check a continue statement for validity.
    /// TS1104: A 'continue' statement can only be used within an enclosing iteration statement.
    fn check_continue_statement(&mut self, stmt_idx: NodeIndex);

    /// Get current reachability state
    fn is_unreachable(&self) -> bool;

    /// Set current reachability state
    fn set_unreachable(&mut self, value: bool);

    /// Get current reported state
    fn has_reported_unreachable(&self) -> bool;

    /// Set current reported state
    fn set_reported_unreachable(&mut self, value: bool);

    /// Check if a statement falls through
    fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool;

    /// Check if a call expression terminates control flow (callee returns never).
    /// Used for for-loop initializer/condition reachability propagation.
    fn call_expression_terminates_control_flow(&mut self, expr_idx: NodeIndex) -> bool;

    /// Report TS7027 unreachable code at an expression node (not a statement).
    /// Used for for-loop condition/incrementer that are unreachable.
    fn report_unreachable_code_at_node(&mut self, node_idx: NodeIndex);

    /// Report TS7027 at the statement inside a direct throwing IIFE, if the
    /// given expression is one.
    fn report_unreachable_code_at_terminating_iife_body(&mut self, node_idx: NodeIndex) -> bool;

    /// Enter an iteration statement (for/while/do-while/for-in/for-of).
    /// Increments `iteration_depth` for break/continue validation.
    fn enter_iteration_statement(&mut self);

    /// Leave an iteration statement.
    /// Decrements `iteration_depth`.
    fn leave_iteration_statement(&mut self);

    /// Enter a switch statement.
    /// Increments `switch_depth` for break validation.
    fn enter_switch_statement(&mut self);

    /// Leave a switch statement.
    /// Decrements `switch_depth`.
    fn leave_switch_statement(&mut self);

    /// Whether `noFallthroughCasesInSwitch` is enabled.
    fn no_fallthrough_cases_in_switch(&self) -> bool;

    /// Report TS7029 "Fallthrough case in switch." at the given clause node.
    fn report_fallthrough_case(&mut self, clause_idx: NodeIndex);

    /// Save current iteration/switch context and reset it.
    /// Used when entering a function body (function creates new context).
    /// Returns the saved (`iteration_depth`, `switch_depth`, `had_outer_loop`).
    fn save_and_reset_control_flow_context(&mut self) -> (u32, u32, bool);

    /// Restore previously saved iteration/switch context.
    /// Used when leaving a function body.
    fn restore_control_flow_context(&mut self, saved: (u32, u32, bool));

    /// Enter a labeled statement.
    /// Pushes a label onto the label stack for break/continue validation.
    /// `is_iteration` should be true if the labeled statement wraps an iteration statement.
    fn enter_labeled_statement(
        &mut self,
        label: String,
        is_iteration: bool,
        label_node: tsz_parser::parser::NodeIndex,
    );

    /// Leave a labeled statement.
    /// Pops the label from the label stack.
    fn leave_labeled_statement(&mut self);

    /// Get the text of a node (used for getting label names).
    fn get_node_text(&self, idx: NodeIndex) -> Option<String>;

    /// Check for declarations in single-statement position (TS1156).
    /// Called when a statement in a control flow construct (if/while/do/for) body
    /// is a declaration that requires a block context.
    fn check_declaration_in_statement_position(&mut self, stmt_idx: NodeIndex);

    /// TS1344: Check if a label is placed before a declaration that doesn't allow labels.
    /// Called when a labeled statement is found; `label_idx` is the label identifier,
    /// `statement_idx` is the inner statement.
    fn check_label_on_declaration(&mut self, label_idx: NodeIndex, statement_idx: NodeIndex);

    /// Check a with statement and emit TS2410.
    /// The 'with' statement is not supported in TypeScript.
    fn check_with_statement(&mut self, stmt_idx: NodeIndex);

    /// Check if a module element (import/export/namespace/ambient module) is in
    /// a valid context (`SourceFile` or `ModuleBlock`). If not, emit the appropriate
    /// grammar error (TS1231-1235, TS1258). Returns true if the context is invalid
    /// (error was emitted), false if the context is valid.
    fn check_grammar_module_element_context(&mut self, stmt_idx: NodeIndex) -> bool {
        let _ = stmt_idx;
        false
    }

    /// TS1184: Check if a `declare` modifier on a variable/class/enum declaration
    /// is inside a block context where it's not allowed. The parser does not emit
    /// TS1184; the checker handles it with this function.
    fn check_grammar_declare_in_block_context(&mut self, _stmt_idx: NodeIndex) {}
}

/// Statement type checker that dispatches to specialized handlers.
///
/// This is a zero-sized struct that only provides the dispatching logic.
/// All state and type checking operations are delegated back to the
/// implementation of `StatementCheckCallbacks` (typically `CheckerState`).
pub struct StatementChecker;

impl StatementChecker {
    /// Create a new statement checker.
    pub const fn new() -> Self {
        Self
    }

    /// Check a statement node.
    ///
    /// This dispatches to specialized handlers based on statement kind.
    /// The `state` parameter provides both the arena for AST access and
    /// callbacks for type checking operations.
    pub fn check<S: StatementCheckCallbacks>(stmt_idx: NodeIndex, state: &mut S) {
        Self::check_with_request(stmt_idx, state, &TypingRequest::NONE);
    }

    pub fn check_with_request<S: StatementCheckCallbacks>(
        stmt_idx: NodeIndex,
        state: &mut S,
        request: &TypingRequest,
    ) {
        state.report_unreachable_statement(stmt_idx);
        let non_contextual_request = request.contextual_opt(None);

        // Get node kind and extract needed data before any mutable operations
        let node_data = {
            let arena = state.arena();
            let Some(node) = arena.get(stmt_idx) else {
                return;
            };
            (node.kind, node)
        };
        let kind = node_data.0;

        match kind {
            syntax_kind_ext::VARIABLE_STATEMENT => {
                state.check_grammar_declare_in_block_context(stmt_idx);
                state.check_variable_statement_with_request(stmt_idx, request);
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // Extract expression index before mutable operations
                let expr_idx = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_expression_statement(node))
                        .map(|e| e.expression)
                };
                if let Some(expression) = expr_idx {
                    // TS1295: Check for dynamic import() in CJS+VMS
                    state.check_expression_statement(stmt_idx, expression);
                    // TS1359: Check for await expressions outside async function
                    state.check_await_expression(expression);
                    // Then get the type for normal type checking
                    state.get_type_of_node_with_request(expression, request);
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                // Extract all needed data before mutable operations
                let if_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_if_statement(node))
                        .map(|if_stmt| {
                            (
                                if_stmt.expression,
                                if_stmt.then_statement,
                                if_stmt.else_statement,
                            )
                        })
                };
                if let Some((expression, then_stmt, else_stmt)) = if_data {
                    // Check condition
                    state.check_await_expression(expression);
                    state.get_type_of_node_with_request(expression, &non_contextual_request);
                    // TS2872/TS2873: check if condition is always truthy/falsy
                    state.check_truthy_or_falsy(expression);
                    // TS2774: check for non-nullable callable tested for truthiness
                    state.check_callable_truthiness(expression, Some(then_stmt));

                    let condition_is_true = state.is_true_condition(expression);
                    let condition_is_false = state.is_false_condition(expression);

                    let prev_unreachable = state.is_unreachable();
                    let prev_reported = state.has_reported_unreachable();

                    // Check then branch
                    if condition_is_false {
                        state.set_unreachable(true);
                    }
                    state.check_declaration_in_statement_position(then_stmt);
                    state.check_statement_with_request(then_stmt, request);

                    state.set_unreachable(prev_unreachable);
                    state.set_reported_unreachable(prev_reported);

                    // Check else branch if present
                    if else_stmt.is_some() {
                        if condition_is_true {
                            state.set_unreachable(true);
                        }
                        state.check_declaration_in_statement_position(else_stmt);
                        state.check_statement_with_request(else_stmt, request);

                        state.set_unreachable(prev_unreachable);
                        state.set_reported_unreachable(prev_reported);
                    }
                }
            }
            syntax_kind_ext::RETURN_STATEMENT => {
                state.check_return_statement_with_request(stmt_idx, request);
            }
            syntax_kind_ext::BLOCK => {
                // Extract statements before mutable operations
                let stmts = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_block(node))
                        .map(|b| b.statements.nodes.clone())
                };
                if let Some(stmts) = stmts {
                    let prev_unreachable = state.is_unreachable();
                    let prev_reported = state.has_reported_unreachable();
                    for inner_stmt in &stmts {
                        state.check_statement_with_request(*inner_stmt, request);
                        if !state.statement_falls_through(*inner_stmt) {
                            state.set_unreachable(true);
                        }
                    }
                    state.set_unreachable(prev_unreachable);
                    state.set_reported_unreachable(prev_reported);
                    // Check for function overload implementations in blocks
                    state.check_function_implementations(&stmts);
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION => {
                state.check_function_declaration(stmt_idx);
            }
            syntax_kind_ext::WHILE_STATEMENT | syntax_kind_ext::DO_STATEMENT => {
                // Extract loop data before mutable operations
                let loop_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_loop(node))
                        .map(|l| (l.condition, l.statement))
                };
                if let Some((condition, statement)) = loop_data {
                    state.get_type_of_node_with_request(condition, &non_contextual_request);
                    state.check_truthy_or_falsy(condition);

                    let prev_unreachable = state.is_unreachable();
                    let prev_reported = state.has_reported_unreachable();

                    // Body is unreachable if it's a while loop with a false condition
                    if kind == syntax_kind_ext::WHILE_STATEMENT
                        && state.is_false_condition(condition)
                    {
                        state.set_unreachable(true);
                    }

                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement_with_request(statement, request);
                    state.leave_iteration_statement();

                    state.set_unreachable(prev_unreachable);
                    state.set_reported_unreachable(prev_reported);
                }
            }
            syntax_kind_ext::FOR_STATEMENT => {
                // Extract loop data before mutable operations
                let loop_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_loop(node))
                        .map(|l| (l.initializer, l.condition, l.incrementor, l.statement))
                };
                if let Some((initializer, condition, incrementor, statement)) = loop_data {
                    // Track whether the initializer terminates control flow
                    // (e.g., calls an IIFE that always throws).
                    let mut init_terminates = false;
                    if initializer.is_some() {
                        // Check if initializer is a variable declaration list
                        let is_var_decl_list = {
                            let arena = state.arena();
                            arena.get(initializer).is_some_and(|n| {
                                n.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                            })
                        };
                        if is_var_decl_list {
                            state
                                .check_variable_declaration_list_with_request(initializer, request);
                        } else {
                            state.get_type_of_node_with_request(
                                initializer,
                                &non_contextual_request,
                            );
                            // Check if the initializer expression terminates control flow
                            init_terminates =
                                state.call_expression_terminates_control_flow(initializer);
                        }
                    }

                    // If the initializer always throws, the remaining for-clause
                    // expressions are unreachable.
                    // For direct throwing IIFEs, tsc anchors the unreachable
                    // diagnostic inside the unreachable IIFE bodies, not at
                    // the `for` clause expression starts.
                    if init_terminates && condition.is_some() {
                        let reported_condition =
                            state.report_unreachable_code_at_terminating_iife_body(condition);
                        if !reported_condition {
                            state.report_unreachable_code_at_node(condition);
                        }
                        if incrementor.is_some()
                            && !state.report_unreachable_code_at_terminating_iife_body(incrementor)
                        {
                            state.report_unreachable_code_at_node(incrementor);
                        }
                    }

                    let mut condition_is_false = false;
                    let mut condition_terminates = false;
                    if condition.is_some() {
                        state.get_type_of_node_with_request(condition, &non_contextual_request);
                        state.check_truthy_or_falsy(condition);
                        condition_is_false = state.is_false_condition(condition);
                        // Check if the condition expression itself terminates control flow
                        condition_terminates =
                            state.call_expression_terminates_control_flow(condition);
                    }

                    // If the condition always throws, the incrementer is unreachable
                    if !init_terminates && condition_terminates && incrementor.is_some()
                        && !state.report_unreachable_code_at_terminating_iife_body(incrementor) {
                            state.report_unreachable_code_at_node(incrementor);
                        }

                    let prev_unreachable = state.is_unreachable();
                    let prev_reported = state.has_reported_unreachable();

                    if condition_is_false || init_terminates || condition_terminates {
                        state.set_unreachable(true);
                    }

                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement_with_request(statement, request);
                    if incrementor.is_some()
                        && !condition_is_false
                        && !init_terminates
                        && !condition_terminates
                    {
                        state.get_type_of_node_with_request(incrementor, &non_contextual_request);
                    }
                    state.leave_iteration_statement();

                    state.set_unreachable(prev_unreachable);
                    state.set_reported_unreachable(prev_reported);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                // Extract for-in/of data before mutable operations
                let for_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_for_in_of(node))
                        .map(|f| (f.expression, f.initializer, f.await_modifier, f.statement))
                };
                let is_for_of = kind == syntax_kind_ext::FOR_OF_STATEMENT;

                if let Some((expression, initializer, await_modifier, statement)) = for_data {
                    // Bug #9: Check await_modifier is only used in async context
                    // for-await-of requires async function context
                    if await_modifier {
                        state.check_await_expression(expression);
                        state.check_for_await_statement(stmt_idx);
                    }

                    // Check if initializer is a variable declaration list and detect
                    // parser-level errors that should suppress semantic expression checks:
                    // - Empty decl list (TS1123 already reported by parser)
                    // - For-in variable with initializer (TS1189 will be reported)
                    let (is_var_decl_list, has_grammar_error) = {
                        let arena = state.arena();
                        if let Some(n) = arena.get(initializer) {
                            if n.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                                let grammar_err = arena.get_variable(n).is_none_or(|v| {
                                    v.declarations.nodes.is_empty()
                                        || (!is_for_of
                                            && v.declarations.nodes.len() == 1
                                            && v.declarations.nodes.first().is_some_and(|&d| {
                                                arena.get(d).is_some_and(|dn| {
                                                    arena
                                                        .get_variable_declaration(dn)
                                                        .is_some_and(|vd| vd.initializer.is_some())
                                                })
                                            }))
                                });
                                (true, grammar_err)
                            } else {
                                (false, false)
                            }
                        } else {
                            (false, false)
                        }
                    };

                    let tracked_for_of_symbols =
                        if is_for_of && is_var_decl_list && !has_grammar_error {
                            state.begin_for_of_self_reference_tracking(initializer)
                        } else {
                            0
                        };

                    // Determine the element type for the loop variable (for-of) or key type (for-in).
                    // When there are grammar errors, skip semantic checks (TS2407 etc.)
                    // but still evaluate the expression to catch TS2304 "cannot find name".
                    let loop_var_type = if has_grammar_error {
                        // Still type-check the expression for name resolution errors,
                        // but only for for-in. For for-of with grammar errors, the
                        // expression often involves the `of` keyword itself due to
                        // parsing ambiguity (e.g., `for (var of of)`).
                        if !is_for_of {
                            state
                                .get_type_of_node_with_request(expression, &non_contextual_request);
                        }
                        if is_for_of {
                            TypeId::ANY
                        } else {
                            TypeId::STRING
                        }
                    } else {
                        let expr_type = state
                            .get_type_of_node_with_request(expression, &non_contextual_request);
                        if is_for_of {
                            // Check if the expression is iterable and emit TS2488/TS2504 if not
                            state.check_for_of_iterability(expr_type, expression, await_modifier);
                            state.for_of_element_type(expr_type, await_modifier)
                        } else {
                            // TS2407: for-in expression must be any, object type, or type parameter
                            state.check_for_in_expression_type(expr_type, expression);
                            // tsc: for-in variable type is keyof T & string when the expression
                            // has a type parameter, otherwise plain string.
                            state.compute_for_in_variable_type(expr_type)
                        }
                    };

                    if tracked_for_of_symbols > 0 {
                        state.end_for_of_self_reference_tracking(tracked_for_of_symbols);
                    }

                    if is_var_decl_list {
                        // TS2491: for-in cannot use destructuring patterns
                        if !is_for_of {
                            state.check_for_in_destructuring_pattern(initializer);
                        }
                        // TS7022/TS7023: Detect self-referencing for-of variables.
                        // This covers both direct `for (var v of v)` cycles and
                        // iterator-protocol methods whose return expressions read `v`.
                        if is_for_of {
                            state.check_for_of_self_reference_circularity(initializer, expression);
                        }
                        state.assign_for_in_of_initializer_types(
                            initializer,
                            loop_var_type,
                            !is_for_of,
                        );
                        state.check_variable_declaration_list_with_request(initializer, request);
                    } else {
                        // TS2491: for-in with expression initializer cannot be array/object literal
                        if !is_for_of {
                            state.check_for_in_expression_destructuring(initializer);
                        }
                        // Non-declaration initializer (e.g., `for (v of expr)` where v is pre-declared)
                        // Check assignability: element type must be assignable to the variable's type
                        // Also checks TS2588 (const assignment)
                        state.check_for_in_of_expression_initializer(
                            initializer,
                            loop_var_type,
                            is_for_of,
                            await_modifier,
                        );
                    }
                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement_with_request(statement, request);
                    state.leave_iteration_statement();
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                // Extract switch data before mutable operations
                let switch_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_switch(node))
                        .map(|s| (s.expression, s.case_block))
                };

                if let Some((expression, case_block)) = switch_data {
                    // Use the declared/widened type (no flow narrowing) for the switch
                    // discriminant. tsc's checkExpression returns the non-narrowed type,
                    // preventing false TS2678 when flow narrows the discriminant
                    // (e.g., `const x: number = 0` narrowed to `0` would reject `case 1`).
                    let switch_type = state.get_type_of_node_no_narrowing_with_request(
                        expression,
                        &non_contextual_request,
                    );

                    // Extract case clauses
                    let clauses = {
                        let arena = state.arena();
                        if let Some(cb_node) = arena.get(case_block) {
                            arena
                                .get_block(cb_node)
                                .map(|cb| cb.statements.nodes.clone())
                        } else {
                            None
                        }
                    };

                    if let Some(clauses) = clauses {
                        // Track if there's a default clause (for exhaustiveness checking)
                        let mut has_default = false;

                        // Collect clause data for fallthrough analysis
                        let clause_data_vec: Vec<_> = clauses
                            .iter()
                            .filter_map(|&clause_idx| {
                                let arena = state.arena();
                                let clause_node = arena.get(clause_idx)?;
                                let clause = arena.get_case_clause(clause_node)?;
                                Some((
                                    clause_idx,
                                    clause.expression,
                                    clause.statements.nodes.clone(),
                                ))
                            })
                            .collect();

                        let num_clauses = clause_data_vec.len();
                        let check_fallthrough = state.no_fallthrough_cases_in_switch();

                        // Enter switch context for break validation
                        state.enter_switch_statement();

                        for (i, (clause_idx, clause_expr, clause_stmts)) in
                            clause_data_vec.iter().enumerate()
                        {
                            // Check if this is a default clause (expression is NONE)
                            if clause_expr.is_none() {
                                has_default = true;
                            } else {
                                // Check case expression with switch type as contextual type.
                                // This enables excess property checking (TS2353) for object
                                // literal case expressions, matching tsc behavior.
                                let case_type = state.get_type_of_case_expression_with_request(
                                    *clause_expr,
                                    switch_type,
                                    request,
                                );
                                state.check_switch_case_comparable(
                                    switch_type,
                                    case_type,
                                    expression,
                                    *clause_expr,
                                );
                            }
                            let prev_unreachable = state.is_unreachable();
                            let prev_reported = state.has_reported_unreachable();
                            let mut clause_falls_through = true;
                            for inner_stmt_idx in clause_stmts {
                                state.check_statement_with_request(*inner_stmt_idx, request);
                                if !state.statement_falls_through(*inner_stmt_idx) {
                                    state.set_unreachable(true);
                                    clause_falls_through = false;
                                }
                            }
                            state.set_unreachable(prev_unreachable);
                            state.set_reported_unreachable(prev_reported);

                            // TS7029: Fallthrough case in switch
                            if check_fallthrough
                                && clause_falls_through
                                && !clause_stmts.is_empty()
                                && i < num_clauses - 1
                            {
                                // Check that the next clause has statements (empty
                                // case grouping like `case 1: case 2: break;` is ok)
                                let next_has_stmts = clause_data_vec
                                    .get(i + 1)
                                    .is_some_and(|(_, _, stmts)| !stmts.is_empty());
                                if next_has_stmts {
                                    state.report_fallthrough_case(*clause_idx);
                                }
                            }
                        }

                        // Leave switch context
                        state.leave_switch_statement();

                        // Check exhaustiveness (Task 12: CFA Diagnostics)
                        state.check_switch_exhaustiveness(
                            stmt_idx,
                            expression,
                            case_block,
                            has_default,
                        );
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                // Extract try data before mutable operations
                let try_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_try(node))
                        .map(|t| (t.try_block, t.catch_clause, t.finally_block))
                };

                if let Some((try_block, catch_clause, finally_block)) = try_data {
                    state.check_statement_with_request(try_block, request);

                    if catch_clause.is_some() {
                        // Extract catch clause data
                        let catch_data = {
                            let arena = state.arena();
                            if let Some(catch_node) = arena.get(catch_clause) {
                                arena
                                    .get_catch_clause(catch_node)
                                    .map(|c| (c.variable_declaration, c.block))
                            } else {
                                None
                            }
                        };

                        if let Some((var_decl, block)) = catch_data {
                            if var_decl.is_some() {
                                state.check_variable_declaration_with_request(var_decl, request);
                            }
                            state.check_statement_with_request(block, request);
                        }
                    }
                    if finally_block.is_some() {
                        state.check_statement_with_request(finally_block, request);
                    }
                }
            }
            syntax_kind_ext::THROW_STATEMENT => {
                // Extract operand before mutable operations.
                // Throw statements use ReturnData (same as return statements),
                // not UnaryExprData.
                let operand = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_return_statement(node))
                        .map(|r| r.expression)
                };
                if let Some(operand) = operand {
                    state.get_type_of_node_with_request(operand, &non_contextual_request);
                }
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                state.check_interface_declaration(stmt_idx);
            }
            syntax_kind_ext::EXPORT_DECLARATION => {
                state.check_grammar_module_element_context(stmt_idx);
                state.check_export_declaration(stmt_idx);
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                state.check_type_alias_declaration(stmt_idx);
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                state.check_grammar_declare_in_block_context(stmt_idx);
                state.check_enum_duplicate_members(stmt_idx);
                // Walk enum member initializer expressions for semantic checking
                // (name resolution, etc.). tsc resolves identifiers in initializers
                // and emits TS2304 for undefined names (e.g., `[e] = id++` → TS2304 for `id`).
                let initializers: Vec<NodeIndex> = {
                    let arena = state.arena();
                    if let Some(node) = arena.get(stmt_idx)
                        && let Some(enum_data) = arena.get_enum(node)
                    {
                        enum_data
                            .members
                            .nodes
                            .iter()
                            .filter_map(|&member_idx| {
                                let member_node = arena.get(member_idx)?;
                                let member_data = arena.get_enum_member(member_node)?;
                                if member_data.initializer.is_none() {
                                    None
                                } else {
                                    Some(member_data.initializer)
                                }
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                };
                for init_idx in initializers {
                    state.get_type_of_node_with_request(init_idx, &non_contextual_request);
                }
            }
            syntax_kind_ext::EMPTY_STATEMENT | syntax_kind_ext::DEBUGGER_STATEMENT => {
                // No action needed
            }
            syntax_kind_ext::BREAK_STATEMENT => {
                state.check_break_statement(stmt_idx);
            }
            syntax_kind_ext::CONTINUE_STATEMENT => {
                state.check_continue_statement(stmt_idx);
            }
            syntax_kind_ext::IMPORT_DECLARATION => {
                if !state.check_grammar_module_element_context(stmt_idx) {
                    state.check_import_declaration(stmt_idx);
                }
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if !state.check_grammar_module_element_context(stmt_idx) {
                    state.check_import_equals_declaration(stmt_idx);
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                // Note: TS1234/TS1235 are checked inside check_module_declaration
                state.check_module_declaration(stmt_idx);
            }
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                state.check_grammar_declare_in_block_context(stmt_idx);
                state.check_class_declaration(stmt_idx);
            }
            syntax_kind_ext::WITH_STATEMENT => {
                state.check_with_statement(stmt_idx);
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                // Extract labeled statement data before mutable operations
                let labeled_data = {
                    let arena = state.arena();
                    arena
                        .get(stmt_idx)
                        .and_then(|node| arena.get_labeled_statement(node))
                        .map(|l| (l.label, l.statement))
                };

                if let Some((label_idx, statement_idx)) = labeled_data {
                    // TS1344: Check if label is placed before a non-labelable declaration
                    state.check_label_on_declaration(label_idx, statement_idx);

                    // Get the label name
                    let label_name = state.get_node_text(label_idx).unwrap_or_default();

                    // Determine if the labeled statement wraps an iteration statement
                    // This checks recursively through nested labels (e.g., target1: target2: while(...))
                    let is_iteration = {
                        let arena = state.arena();
                        Self::is_iteration_or_nested_iteration(arena, statement_idx)
                    };

                    // Push label onto stack
                    state.enter_labeled_statement(label_name, is_iteration, label_idx);

                    // Check the contained statement
                    state.check_statement_with_request(statement_idx, request);

                    // Pop label from stack
                    state.leave_labeled_statement();
                }
            }
            syntax_kind_ext::EXPORT_ASSIGNMENT => {
                state.check_grammar_module_element_context(stmt_idx);
                // Export assignments are mainly checked in check_export_assignment
                // at the source file level
                state.get_type_of_node_with_request(stmt_idx, request);
            }
            _ => {
                // Catch-all for other statement types
                state.get_type_of_node_with_request(stmt_idx, &non_contextual_request);
            }
        }
    }

    /// Check if a statement is an iteration statement, either directly or through nested labels.
    /// This handles cases like `target1: target2: while(true)` where both target1 and target2
    /// should be considered as wrapping an iteration statement.
    fn is_iteration_or_nested_iteration(
        arena: &tsz_parser::parser::node::NodeArena,
        stmt_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            return false;
        };

        // Check if it's directly an iteration statement
        if matches!(
            stmt_node.kind,
            syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT
                | syntax_kind_ext::WHILE_STATEMENT
                | syntax_kind_ext::DO_STATEMENT
        ) {
            return true;
        }

        // Check if it's a labeled statement wrapping an iteration (recursively)
        if stmt_node.kind == syntax_kind_ext::LABELED_STATEMENT
            && let Some(labeled) = arena.get_labeled_statement(stmt_node)
        {
            return Self::is_iteration_or_nested_iteration(arena, labeled.statement);
        }

        false
    }
}

impl Default for StatementChecker {
    fn default() -> Self {
        Self::new()
    }
}
