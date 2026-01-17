//! Statement Type Checking
//!
//! Handles control flow statements and dispatches declarations.
//! This module separates statement checking logic from the monolithic ThinCheckerState.

use super::context::CheckerContext;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;

/// Statement type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All statement type checking goes through this checker.
pub struct StatementChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
}

impl<'a, 'ctx> StatementChecker<'a, 'ctx> {
    /// Create a new statement checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check a statement node.
    ///
    /// This dispatches to specialized handlers based on statement kind.
    /// Currently a skeleton - logic will be migrated incrementally from ThinCheckerState.
    pub fn check(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                self.check_block(stmt_idx);
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.check_if_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::DO_STATEMENT => {
                self.check_loop_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                self.check_for_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                self.check_return_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.check_switch_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.check_try_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.check_throw_statement(stmt_idx);
            }
            // Declarations are handled by DeclarationChecker
            // Expression statements are handled by ExpressionChecker
            _ => {
                // Unhandled statement types - will be expanded incrementally
            }
        }
    }

    // Note: fallthrough analysis has been moved to ThinCheckerState in thin_checker.rs
    // These methods were delegating to control_flow module but that module has been refactored.

    /// Check a block statement.
    fn check_block(&mut self, block_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(block_idx) else {
            return;
        };

        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt_idx in &block.statements.nodes {
                self.check(stmt_idx);
            }
        }
    }

    /// Check an if statement.
    fn check_if_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
            // Check condition expression
            // (Would call ExpressionChecker here)

            // Check then branch
            self.check(if_stmt.then_statement);

            // Check else branch if present
            if !if_stmt.else_statement.is_none() {
                self.check(if_stmt.else_statement);
            }
        }
    }

    /// Check a while/do-while loop statement.
    fn check_loop_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            // Check condition expression
            // (Would call ExpressionChecker here)

            // Check body
            self.check(loop_stmt.statement);
        }
    }

    /// Check a for statement.
    fn check_for_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            // Check initializer (variable declaration or expression)
            // Check condition
            // Check incrementor
            // Check body
            self.check(loop_stmt.statement);
        }
    }

    /// Check a return statement.
    fn check_return_statement(&mut self, _stmt_idx: NodeIndex) {
        // Return type checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
    }

    /// Check a switch statement.
    fn check_switch_statement(&mut self, _stmt_idx: NodeIndex) {
        // Switch statement checking is handled by ThinCheckerState for now
        // Will be migrated when the arena provides get_switch_statement
    }

    /// Check a try statement.
    fn check_try_statement(&mut self, _stmt_idx: NodeIndex) {
        // Try statement checking is handled by ThinCheckerState for now
        // Will be migrated when the arena provides get_try_statement
    }

    /// Check a throw statement.
    fn check_throw_statement(&mut self, _stmt_idx: NodeIndex) {
        // Check the thrown expression
        // (Would call ExpressionChecker here)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_statement_checker_block() {
        let source = "{ let x = 1; }";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx =
            CheckerContext::new(parser.get_arena(), &binder, &types, "test.ts".to_string(), crate::checker::context::CheckerOptions::default());

        // Get the block statement
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = StatementChecker::new(&mut ctx);
                    checker.check(stmt_idx);
                    // Test passes if no panic
                }
            }
        }
    }
}
