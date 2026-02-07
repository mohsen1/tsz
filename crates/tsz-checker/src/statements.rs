//! Statement Type Checking
//!
//! Handles control flow statements and dispatches declarations.
//! This module separates statement checking logic from the monolithic CheckerState.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Trait for statement checking callbacks.
///
/// This trait defines the interface that CheckerState must implement
/// to allow StatementChecker to delegate type checking and other operations.
pub trait StatementCheckCallbacks {
    /// Get access to the node arena for AST traversal.
    fn arena(&self) -> &NodeArena;

    /// Get the type of a node (expression or type annotation).
    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId;

    /// Check a variable statement.
    fn check_variable_statement(&mut self, stmt_idx: NodeIndex);

    /// Check a variable declaration list.
    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex);

    /// Check a variable declaration.
    fn check_variable_declaration(&mut self, decl_idx: NodeIndex);

    /// Check a return statement.
    fn check_return_statement(&mut self, stmt_idx: NodeIndex);

    /// Check unreachable code in a block.
    fn check_unreachable_code_in_block(&mut self, stmts: &[NodeIndex]);

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

    /// Assign types for for-in/for-of initializers.
    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
    );

    /// Get element type for for-of loop.
    fn for_of_element_type(&mut self, expr_type: TypeId) -> TypeId;

    /// Check for-of iterability.
    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    );

    /// Recursively check a nested statement (callback to check_statement).
    fn check_statement(&mut self, stmt_idx: NodeIndex);

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

    /// Check a break statement for validity.
    /// TS1105: A 'break' statement can only be used within an enclosing iteration statement.
    fn check_break_statement(&mut self, stmt_idx: NodeIndex);

    /// Check a continue statement for validity.
    /// TS1104: A 'continue' statement can only be used within an enclosing iteration statement.
    fn check_continue_statement(&mut self, stmt_idx: NodeIndex);

    /// Enter an iteration statement (for/while/do-while/for-in/for-of).
    /// Increments iteration_depth for break/continue validation.
    fn enter_iteration_statement(&mut self);

    /// Leave an iteration statement.
    /// Decrements iteration_depth.
    fn leave_iteration_statement(&mut self);

    /// Enter a switch statement.
    /// Increments switch_depth for break validation.
    fn enter_switch_statement(&mut self);

    /// Leave a switch statement.
    /// Decrements switch_depth.
    fn leave_switch_statement(&mut self);

    /// Save current iteration/switch context and reset it.
    /// Used when entering a function body (function creates new context).
    /// Returns the saved (iteration_depth, switch_depth, had_outer_loop).
    fn save_and_reset_control_flow_context(&mut self) -> (u32, u32, bool);

    /// Restore previously saved iteration/switch context.
    /// Used when leaving a function body.
    fn restore_control_flow_context(&mut self, saved: (u32, u32, bool));

    /// Enter a labeled statement.
    /// Pushes a label onto the label stack for break/continue validation.
    /// `is_iteration` should be true if the labeled statement wraps an iteration statement.
    fn enter_labeled_statement(&mut self, label: String, is_iteration: bool);

    /// Leave a labeled statement.
    /// Pops the label from the label stack.
    fn leave_labeled_statement(&mut self);

    /// Get the text of a node (used for getting label names).
    fn get_node_text(&self, idx: NodeIndex) -> Option<String>;

    /// Check for declarations in single-statement position (TS1156).
    /// Called when a statement in a control flow construct (if/while/do/for) body
    /// is a declaration that requires a block context.
    fn check_declaration_in_statement_position(&mut self, stmt_idx: NodeIndex);
}

/// Statement type checker that dispatches to specialized handlers.
///
/// This is a zero-sized struct that only provides the dispatching logic.
/// All state and type checking operations are delegated back to the
/// implementation of StatementCheckCallbacks (typically CheckerState).
pub struct StatementChecker;

impl StatementChecker {
    /// Create a new statement checker.
    pub fn new() -> Self {
        Self
    }

    /// Check a statement node.
    ///
    /// This dispatches to specialized handlers based on statement kind.
    /// The `state` parameter provides both the arena for AST access and
    /// callbacks for type checking operations.
    pub fn check<S: StatementCheckCallbacks>(stmt_idx: NodeIndex, state: &mut S) {
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
                state.check_variable_statement(stmt_idx);
            }
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                // Extract expression index before mutable operations
                let expr_idx = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_expression_statement(node).map(|e| e.expression)
                };
                if let Some(expression) = expr_idx {
                    // TS1359: Check for await expressions outside async function
                    state.check_await_expression(expression);
                    // Then get the type for normal type checking
                    state.get_type_of_node(expression);
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                // Extract all needed data before mutable operations
                let if_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_if_statement(node).map(|if_stmt| {
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
                    state.get_type_of_node(expression);
                    // Check then branch
                    state.check_declaration_in_statement_position(then_stmt);
                    state.check_statement(then_stmt);
                    // Check else branch if present
                    if !else_stmt.is_none() {
                        state.check_declaration_in_statement_position(else_stmt);
                        state.check_statement(else_stmt);
                    }
                }
            }
            syntax_kind_ext::RETURN_STATEMENT => {
                state.check_return_statement(stmt_idx);
            }
            syntax_kind_ext::BLOCK => {
                // Extract statements before mutable operations
                let stmts = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_block(node).map(|b| b.statements.nodes.clone())
                };
                if let Some(stmts) = stmts {
                    // Check for unreachable code before checking individual statements
                    state.check_unreachable_code_in_block(&stmts);
                    for inner_stmt in &stmts {
                        state.check_statement(*inner_stmt);
                    }
                    // Check for function overload implementations in blocks
                    state.check_function_implementations(&stmts);
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                state.check_function_declaration(stmt_idx);
            }
            syntax_kind_ext::WHILE_STATEMENT | syntax_kind_ext::DO_STATEMENT => {
                // Extract loop data before mutable operations
                let loop_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_loop(node).map(|l| (l.condition, l.statement))
                };
                if let Some((condition, statement)) = loop_data {
                    state.get_type_of_node(condition);
                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement(statement);
                    state.leave_iteration_statement();
                }
            }
            syntax_kind_ext::FOR_STATEMENT => {
                // Extract loop data before mutable operations
                let loop_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena
                        .get_loop(node)
                        .map(|l| (l.initializer, l.condition, l.incrementor, l.statement))
                };
                if let Some((initializer, condition, incrementor, statement)) = loop_data {
                    if !initializer.is_none() {
                        // Check if initializer is a variable declaration list
                        let is_var_decl_list = {
                            let arena = state.arena();
                            arena
                                .get(initializer)
                                .map(|n| n.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST)
                                .unwrap_or(false)
                        };
                        if is_var_decl_list {
                            state.check_variable_declaration_list(initializer);
                        } else {
                            state.get_type_of_node(initializer);
                        }
                    }
                    if !condition.is_none() {
                        state.get_type_of_node(condition);
                    }
                    if !incrementor.is_none() {
                        state.get_type_of_node(incrementor);
                    }
                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement(statement);
                    state.leave_iteration_statement();
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                // Extract for-in/of data before mutable operations
                let for_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena
                        .get_for_in_of(node)
                        .map(|f| (f.expression, f.initializer, f.await_modifier, f.statement))
                };
                let is_for_of = kind == syntax_kind_ext::FOR_OF_STATEMENT;

                if let Some((expression, initializer, await_modifier, statement)) = for_data {
                    // Bug #9: Check await_modifier is only used in async context
                    // for-await-of requires async function context
                    if await_modifier {
                        state.check_await_expression(expression);
                    }

                    // Determine the element type for the loop variable (for-of) or key type (for-in).
                    let expr_type = state.get_type_of_node(expression);
                    let loop_var_type = if is_for_of {
                        // Check if the expression is iterable and emit TS2488/TS2504 if not
                        state.check_for_of_iterability(expr_type, expression, await_modifier);
                        state.for_of_element_type(expr_type)
                    } else {
                        // `for (x in obj)` iterates keys (string in TS).
                        TypeId::STRING
                    };

                    // Check if initializer is a variable declaration
                    let is_var_decl_list = {
                        let arena = state.arena();
                        arena
                            .get(initializer)
                            .map(|n| n.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST)
                            .unwrap_or(false)
                    };
                    if is_var_decl_list {
                        state.assign_for_in_of_initializer_types(initializer, loop_var_type);
                        state.check_variable_declaration_list(initializer);
                    } else {
                        state.get_type_of_node(initializer);
                    }
                    state.enter_iteration_statement();
                    state.check_declaration_in_statement_position(statement);
                    state.check_statement(statement);
                    state.leave_iteration_statement();
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                // Extract switch data before mutable operations
                let switch_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_switch(node).map(|s| (s.expression, s.case_block))
                };

                if let Some((expression, case_block)) = switch_data {
                    state.get_type_of_node(expression);

                    // Extract case clauses
                    let clauses = {
                        let arena = state.arena();
                        if let Some(cb_node) = arena.get(case_block) {
                            if let Some(cb) = arena.get_block(cb_node) {
                                Some(cb.statements.nodes.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(clauses) = clauses {
                        // Track if there's a default clause (for exhaustiveness checking)
                        let mut has_default = false;

                        // Enter switch context for break validation
                        state.enter_switch_statement();

                        for clause_idx in clauses {
                            // Extract clause data
                            let clause_data = {
                                let arena = state.arena();
                                if let Some(clause_node) = arena.get(clause_idx) {
                                    arena
                                        .get_case_clause(clause_node)
                                        .map(|c| (c.expression, c.statements.nodes.clone()))
                                } else {
                                    None
                                }
                            };

                            if let Some((clause_expr, clause_stmts)) = clause_data {
                                // Check if this is a default clause (expression is NONE)
                                if clause_expr.is_none() {
                                    has_default = true;
                                } else {
                                    // Check case expression
                                    state.get_type_of_node(clause_expr);
                                }
                                // Check statements in the case
                                for inner_stmt_idx in clause_stmts {
                                    state.check_statement(inner_stmt_idx);
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
                    let node = arena.get(stmt_idx).unwrap();
                    arena
                        .get_try(node)
                        .map(|t| (t.try_block, t.catch_clause, t.finally_block))
                };

                if let Some((try_block, catch_clause, finally_block)) = try_data {
                    state.check_statement(try_block);

                    if !catch_clause.is_none() {
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
                            if !var_decl.is_none() {
                                state.check_variable_declaration(var_decl);
                            }
                            state.check_statement(block);
                        }
                    }
                    if !finally_block.is_none() {
                        state.check_statement(finally_block);
                    }
                }
            }
            syntax_kind_ext::THROW_STATEMENT => {
                // Extract operand before mutable operations
                let operand = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena.get_unary_expr(node).map(|u| u.operand)
                };
                if let Some(operand) = operand {
                    state.get_type_of_node(operand);
                }
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                state.check_interface_declaration(stmt_idx);
            }
            syntax_kind_ext::EXPORT_DECLARATION => {
                state.check_export_declaration(stmt_idx);
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                state.check_type_alias_declaration(stmt_idx);
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                state.check_enum_duplicate_members(stmt_idx);
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
                state.check_import_declaration(stmt_idx);
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                state.check_import_equals_declaration(stmt_idx);
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                state.check_module_declaration(stmt_idx);
            }
            syntax_kind_ext::CLASS_DECLARATION => {
                state.check_class_declaration(stmt_idx);
            }
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => {
                state.check_function_declaration(stmt_idx);
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                // Extract labeled statement data before mutable operations
                let labeled_data = {
                    let arena = state.arena();
                    let node = arena.get(stmt_idx).unwrap();
                    arena
                        .get_labeled_statement(node)
                        .map(|l| (l.label, l.statement))
                };

                if let Some((label_idx, statement_idx)) = labeled_data {
                    // Get the label name
                    let label_name = state.get_node_text(label_idx).unwrap_or_default();

                    // Determine if the labeled statement wraps an iteration statement
                    // This checks recursively through nested labels (e.g., target1: target2: while(...))
                    let is_iteration = {
                        let arena = state.arena();
                        Self::is_iteration_or_nested_iteration(arena, statement_idx)
                    };

                    // Push label onto stack
                    state.enter_labeled_statement(label_name, is_iteration);

                    // Check the contained statement
                    state.check_statement(statement_idx);

                    // Pop label from stack
                    state.leave_labeled_statement();
                }
            }
            _ => {
                // Catch-all for other statement types
                state.get_type_of_node(stmt_idx);
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
        if stmt_node.kind == syntax_kind_ext::LABELED_STATEMENT {
            if let Some(labeled) = arena.get_labeled_statement(stmt_node) {
                return Self::is_iteration_or_nested_iteration(arena, labeled.statement);
            }
        }

        false
    }
}

impl Default for StatementChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // Tests require the full CheckerState implementation of StatementCheckCallbacks
    // which is defined in state.rs
}
