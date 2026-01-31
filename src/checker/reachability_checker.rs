//! Code Reachability Checking Module
//!
//! This module contains methods for analyzing code reachability and control flow.
//! It handles:
//! - Block fall-through analysis
//! - Unreachable code detection
//! - Statement fall-through analysis
//! - Switch/try/loop fall-through
//!
//! This module extends CheckerState with reachability-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;

// =============================================================================
// Reachability Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Block Analysis
    // =========================================================================

    /// Check if execution can fall through a block of statements.
    ///
    /// Returns true if execution can continue after the block, false if it always exits.
    pub(crate) fn block_falls_through(&mut self, statements: &[NodeIndex]) -> bool {
        for &stmt_idx in statements {
            if !self.statement_falls_through(stmt_idx) {
                return false;
            }
        }
        true
    }

    /// Check for unreachable code after return/throw statements in a block.
    ///
    /// Emits TS7027 for any statements that come after a return or throw,
    /// or after expressions of type 'never'.
    pub(crate) fn check_unreachable_code_in_block(&mut self, statements: &[NodeIndex]) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // TS7027 is suppressed when allowUnreachableCode is enabled
        if self.ctx.compiler_options.allow_unreachable_code {
            return;
        }

        let mut unreachable = false;
        for &stmt_idx in statements {
            if unreachable {
                // Skip empty statements and function declarations -
                // they don't trigger TS7027 in TypeScript
                let should_skip = if let Some(node) = self.ctx.arena.get(stmt_idx) {
                    node.kind == syntax_kind_ext::EMPTY_STATEMENT
                        || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                } else {
                    false
                };
                if !should_skip {
                    self.error_at_node(
                        stmt_idx,
                        diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                        diagnostic_codes::UNREACHABLE_CODE_DETECTED,
                    );
                }
            } else {
                let Some(node) = self.ctx.arena.get(stmt_idx) else {
                    continue;
                };
                match node.kind {
                    syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => {
                        unreachable = true;
                    }
                    syntax_kind_ext::EXPRESSION_STATEMENT => {
                        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                            continue;
                        };
                        let expr_type = self.get_type_of_node(expr_stmt.expression);
                        if expr_type.is_never() {
                            unreachable = true;
                        }
                    }
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                            continue;
                        };
                        for &decl_idx in &var_stmt.declarations.nodes {
                            let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                                continue;
                            };
                            let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                                continue;
                            };
                            for &list_decl_idx in &var_list.declarations.nodes {
                                let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                                    continue;
                                };
                                let Some(decl) =
                                    self.ctx.arena.get_variable_declaration(list_decl_node)
                                else {
                                    continue;
                                };
                                if decl.initializer.is_none() {
                                    continue;
                                }
                                let init_type = self.get_type_of_node(decl.initializer);
                                if init_type.is_never() {
                                    unreachable = true;
                                    break;
                                }
                            }
                            if unreachable {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // =========================================================================
    // Statement Analysis
    // =========================================================================

    /// Check if execution can fall through a statement.
    ///
    /// Returns true if execution can continue after the statement.
    pub(crate) fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return true;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT | syntax_kind_ext::THROW_STATEMENT => false,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| self.block_falls_through(&block.statements.nodes))
                .unwrap_or(true),
            syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
                    return true;
                };
                let expr_type = self.get_type_of_node(expr_stmt.expression);
                !expr_type.is_never()
            }
            syntax_kind_ext::VARIABLE_STATEMENT => {
                let Some(var_stmt) = self.ctx.arena.get_variable(node) else {
                    return true;
                };
                for &decl_idx in &var_stmt.declarations.nodes {
                    let Some(list_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_list) = self.ctx.arena.get_variable(list_node) else {
                        continue;
                    };
                    for &list_decl_idx in &var_list.declarations.nodes {
                        let Some(list_decl_node) = self.ctx.arena.get(list_decl_idx) else {
                            continue;
                        };
                        let Some(decl) = self.ctx.arena.get_variable_declaration(list_decl_node)
                        else {
                            continue;
                        };
                        if decl.initializer.is_none() {
                            continue;
                        }
                        let init_type = self.get_type_of_node(decl.initializer);
                        if init_type.is_never() {
                            return false;
                        }
                    }
                }
                true
            }
            syntax_kind_ext::IF_STATEMENT => {
                let Some(if_data) = self.ctx.arena.get_if_statement(node) else {
                    return true;
                };
                let then_falls = self.statement_falls_through(if_data.then_statement);
                if if_data.else_statement.is_none() {
                    return true;
                }
                let else_falls = self.statement_falls_through(if_data.else_statement);
                then_falls || else_falls
            }
            syntax_kind_ext::SWITCH_STATEMENT => self.switch_falls_through(stmt_idx),
            syntax_kind_ext::TRY_STATEMENT => self.try_falls_through(stmt_idx),
            syntax_kind_ext::CATCH_CLAUSE => self
                .ctx
                .arena
                .get_catch_clause(node)
                .map(|catch_data| self.statement_falls_through(catch_data.block))
                .unwrap_or(true),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(node),
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => true,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.statement_falls_through(labeled.statement))
                .unwrap_or(true),
            _ => true,
        }
    }

    // =========================================================================
    // Control Flow Analysis
    // =========================================================================

    /// Check if a switch statement falls through.
    ///
    /// Returns true if execution can continue after the switch.
    pub(crate) fn switch_falls_through(&mut self, switch_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(switch_idx) else {
            return true;
        };
        let Some(switch_data) = self.ctx.arena.get_switch(node) else {
            return true;
        };
        let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block) else {
            return true;
        };
        let Some(case_block) = self.ctx.arena.get_block(case_block_node) else {
            return true;
        };

        let mut has_default = false;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                has_default = true;
            }
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };
            if self.block_falls_through(&clause.statements.nodes) {
                return true;
            }
        }

        !has_default
    }

    /// Check if a try statement falls through.
    ///
    /// Returns true if execution can continue after the try statement.
    pub(crate) fn try_falls_through(&mut self, try_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(try_idx) else {
            return true;
        };
        let Some(try_data) = self.ctx.arena.get_try(node) else {
            return true;
        };

        let try_falls = self.statement_falls_through(try_data.try_block);
        let catch_falls = if !try_data.catch_clause.is_none() {
            self.statement_falls_through(try_data.catch_clause)
        } else {
            false
        };

        if !try_data.finally_block.is_none() {
            let finally_falls = self.statement_falls_through(try_data.finally_block);
            if !finally_falls {
                return false;
            }
        }

        try_falls || catch_falls
    }

    /// Check if a loop statement falls through.
    ///
    /// Returns true if execution can continue after the loop.
    pub(crate) fn loop_falls_through(&mut self, node: &crate::parser::node::Node) -> bool {
        let Some(loop_data) = self.ctx.arena.get_loop(node) else {
            return true;
        };

        let condition_always_true = if loop_data.condition.is_none() {
            true
        } else {
            self.is_true_condition(loop_data.condition)
        };

        if condition_always_true && !self.contains_break_statement(loop_data.statement) {
            return false;
        }

        true
    }

    /// Check if a condition is always true.
    pub(crate) fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        node.kind == SyntaxKind::TrueKeyword as u16
    }

    /// Check if a statement contains a break statement.
    pub(crate) fn contains_break_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            syntax_kind_ext::BREAK_STATEMENT => true,
            syntax_kind_ext::BLOCK => self
                .ctx
                .arena
                .get_block(node)
                .map(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .any(|&stmt| self.contains_break_statement(stmt))
                })
                .unwrap_or(false),
            syntax_kind_ext::IF_STATEMENT => self
                .ctx
                .arena
                .get_if_statement(node)
                .map(|if_data| {
                    self.contains_break_statement(if_data.then_statement)
                        || (!if_data.else_statement.is_none()
                            && self.contains_break_statement(if_data.else_statement))
                })
                .unwrap_or(false),
            syntax_kind_ext::SWITCH_STATEMENT => false,
            syntax_kind_ext::TRY_STATEMENT => self
                .ctx
                .arena
                .get_try(node)
                .map(|try_data| {
                    self.contains_break_statement(try_data.try_block)
                        || (!try_data.catch_clause.is_none()
                            && self.contains_break_statement(try_data.catch_clause))
                        || (!try_data.finally_block.is_none()
                            && self.contains_break_statement(try_data.finally_block))
                })
                .unwrap_or(false),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT
            | syntax_kind_ext::FOR_IN_STATEMENT
            | syntax_kind_ext::FOR_OF_STATEMENT => false,
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .map(|labeled| self.contains_break_statement(labeled.statement))
                .unwrap_or(false),
            _ => false,
        }
    }
}
