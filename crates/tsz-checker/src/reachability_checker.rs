//! Code reachability and fall-through analysis.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // TS7027 is only emitted as an error when allowUnreachableCode is explicitly false.
        // When undefined (None), tsc emits it as a suggestion (not captured in conformance).
        // When true (Some(true)), it's fully suppressed.
        if self.ctx.compiler_options.allow_unreachable_code != Some(false) {
            return;
        }

        let mut unreachable = false;
        for &stmt_idx in statements {
            if unreachable {
                // Skip statements that don't trigger TS7027 in TypeScript:
                // - empty statements
                // - function declarations (hoisted)
                // - type/interface declarations (no runtime effect)
                // - const enum declarations (no runtime effect when preserveConstEnums is off)
                // - var declarations without initializers (hoisted, no runtime effect)
                let should_skip = if let Some(node) = self.ctx.arena.get(stmt_idx) {
                    node.kind == syntax_kind_ext::EMPTY_STATEMENT
                        || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                        || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                        || node.kind == syntax_kind_ext::MODULE_DECLARATION
                        || self.is_var_without_initializer(stmt_idx, node)
                } else {
                    false
                };
                if !should_skip {
                    self.error_at_node(
                        stmt_idx,
                        diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                        diagnostic_codes::UNREACHABLE_CODE_DETECTED,
                    );
                    // TypeScript only reports TS7027 for the first unreachable statement
                    return;
                }
            } else if !self.statement_falls_through(stmt_idx) {
                unreachable = true;
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
                .is_none_or(|block| self.block_falls_through(&block.statements.nodes)),
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
                        // Only treat call/new expressions as non-falling-through when
                        // they return never. Type assertions like `null as never` still
                        // complete normally at runtime.
                        if let Some(init_node) = self.ctx.arena.get(decl.initializer)
                            && (init_node.kind == syntax_kind_ext::CALL_EXPRESSION
                                || init_node.kind == syntax_kind_ext::NEW_EXPRESSION)
                        {
                            let init_type = self.get_type_of_node(decl.initializer);
                            if init_type.is_never() {
                                return false;
                            }
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
                .is_none_or(|catch_data| self.statement_falls_through(catch_data.block)),
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => self.loop_falls_through(node),
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_none_or(|labeled| self.statement_falls_through(labeled.statement)),
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
        let mut clause_indices = Vec::new();
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                has_default = true;
            }
            clause_indices.push(clause_idx);
        }

        // Without a default clause, unmatched discriminants skip the switch body,
        // so execution can always continue after the switch.
        if !has_default {
            return true;
        }

        // Analyze from bottom to top so empty/grouped clauses inherit the
        // fall-through behavior of the next clause in the chain.
        let mut falls_from_next = true;
        let mut any_entry_falls_through = false;

        for &clause_idx in clause_indices.iter().rev() {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_case_clause(clause_node) else {
                continue;
            };

            let clause_falls_through = if clause.statements.nodes.is_empty() {
                // Empty case labels fall through to the next clause.
                falls_from_next
            } else if clause
                .statements
                .nodes
                .iter()
                .any(|&stmt| self.contains_break_statement(stmt))
            {
                // A break can complete the switch normally, even if later clauses
                // would not fall through.
                true
            } else if self.block_falls_through(&clause.statements.nodes) {
                // Non-terminating clauses continue into the next clause.
                falls_from_next
            } else {
                // Clause exits function/control flow (e.g. return/throw).
                false
            };

            any_entry_falls_through |= clause_falls_through;
            falls_from_next = clause_falls_through;
        }

        any_entry_falls_through
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
    pub(crate) fn loop_falls_through(&mut self, node: &tsz_parser::parser::node::Node) -> bool {
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
            syntax_kind_ext::BLOCK => self.ctx.arena.get_block(node).is_some_and(|block| {
                block
                    .statements
                    .nodes
                    .iter()
                    .any(|&stmt| self.contains_break_statement(stmt))
            }),
            syntax_kind_ext::IF_STATEMENT => {
                self.ctx
                    .arena
                    .get_if_statement(node)
                    .is_some_and(|if_data| {
                        self.contains_break_statement(if_data.then_statement)
                            || (!if_data.else_statement.is_none()
                                && self.contains_break_statement(if_data.else_statement))
                    })
            }
            syntax_kind_ext::TRY_STATEMENT => {
                self.ctx.arena.get_try(node).is_some_and(|try_data| {
                    self.contains_break_statement(try_data.try_block)
                        || (!try_data.catch_clause.is_none()
                            && self.contains_break_statement(try_data.catch_clause))
                        || (!try_data.finally_block.is_none()
                            && self.contains_break_statement(try_data.finally_block))
                })
            }
            syntax_kind_ext::LABELED_STATEMENT => self
                .ctx
                .arena
                .get_labeled_statement(node)
                .is_some_and(|labeled| self.contains_break_statement(labeled.statement)),
            _ => false,
        }
    }

    /// Check if a statement is a `var` declaration without any initializers.
    /// `var t;` after a throw/return is hoisted and has no runtime effect,
    /// so TypeScript doesn't report TS7027 for it.
    fn is_var_without_initializer(
        &self,
        _stmt_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        use tsz_parser::parser::flags::node_flags;

        if node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_data) = self.ctx.arena.get_variable(node) else {
            return false;
        };
        // Check if it's `var` (not let/const) by examining declaration list flags
        // The flags are on the VariableDeclarationList child node
        for &decl_idx in &var_data.declarations.nodes {
            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                // Check the declaration node or its parent for let/const flags
                let flags = decl_node.flags as u32;
                if (flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    return false;
                }
                // Check if parent (VariableDeclarationList) has let/const flags
                if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                    && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                    && (parent_node.flags as u32 & (node_flags::LET | node_flags::CONST)) != 0
                {
                    return false;
                }
                // Check that declaration has no initializer
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    && !var_decl.initializer.is_none()
                {
                    return false;
                }
            }
        }
        true
    }
}
