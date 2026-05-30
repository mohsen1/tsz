//! Control-flow statement conversion: `throw`, `try`, loops, `switch`,
//! `break`, `continue`, `labeled`.
//!
//! Extracted from `class_es5_ast_to_ir.rs` so the central AST→IR conversion
//! file stays under the §19 2000-line cap. Behavior is unchanged.

use super::{
    AstToIr, IRCatchClause, IRNode, IRSwitchCase, catch_binding_identifier_text,
    get_identifier_text,
};
use tsz_parser::parser::NodeIndex;

impl<'a> AstToIr<'a> {
    pub(super) fn convert_throw_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // Throw uses ReturnData (same as return statement)
        if let Some(return_data) = self.arena.get_return_statement(node) {
            IRNode::ThrowStatement(Box::new(self.convert_expression(return_data.expression)))
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_try_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(try_data) = self.arena.get_try(node) {
            let try_block = Box::new(self.convert_statement(try_data.try_block));

            let catch_clause = if try_data.catch_clause.is_none() {
                None
            } else if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                && let Some(catch) = self.arena.get_catch_clause(catch_node)
            {
                let param = if catch.variable_declaration.is_none() {
                    None
                } else {
                    catch_binding_identifier_text(self.arena, catch.variable_declaration)
                };
                let catch_block = self.arena.get(catch.block);
                let body = if let Some(block_node) = catch_block
                    && let Some(block) = self.arena.get_block(block_node)
                {
                    block
                        .statements
                        .nodes
                        .iter()
                        .map(|&s| self.convert_statement(s))
                        .collect()
                } else {
                    vec![]
                };
                Some(IRCatchClause {
                    param: param.map(Into::into),
                    body,
                    single_line: false,
                })
            } else {
                None
            };

            let finally_block = if try_data.finally_block.is_none() {
                None
            } else {
                Some(Box::new(self.convert_statement(try_data.finally_block)))
            };

            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_for_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // For uses LoopData (same as while/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            let initializer = if loop_data.initializer.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.initializer)))
            };
            let condition = if loop_data.condition.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.condition)))
            };
            let incrementor = if loop_data.incrementor.is_none() {
                None
            } else {
                Some(Box::new(self.convert_expression(loop_data.incrementor)))
            };
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // While uses LoopData (same as for/do-while)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::WhileStatement {
                condition: Box::new(self.convert_expression(loop_data.condition)),
                body: Box::new(self.convert_statement(loop_data.statement)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_do_while_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // DoWhile uses LoopData (same as while/for loops)
        if let Some(loop_data) = self.arena.get_loop(node) {
            IRNode::DoWhileStatement {
                body: Box::new(self.convert_statement(loop_data.statement)),
                condition: Box::new(self.convert_expression(loop_data.condition)),
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    pub(super) fn convert_switch_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(switch_data) = self.arena.get_switch(node) {
            // Case block uses BlockData where statements contains the case clauses
            let cases = if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                && let Some(block_data) = self.arena.get_block(case_block_node)
            {
                block_data
                    .statements
                    .nodes
                    .iter()
                    .map(|&c| self.convert_switch_case(c))
                    .collect()
            } else {
                vec![]
            };
            IRNode::SwitchStatement {
                expression: Box::new(self.convert_expression(switch_data.expression)),
                cases,
            }
        } else {
            IRNode::ASTRef(idx)
        }
    }

    fn convert_switch_case(&self, idx: NodeIndex) -> IRSwitchCase {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        // get_case_clause works for both CASE_CLAUSE and DEFAULT_CLAUSE
        // For DEFAULT_CLAUSE, expression is NONE
        if let Some(case_clause) = self.arena.get_case_clause(node) {
            let test = if case_clause.expression.is_none() {
                None // Default clause
            } else {
                Some(self.convert_expression(case_clause.expression))
            };
            IRSwitchCase {
                test,
                statements: case_clause
                    .statements
                    .nodes
                    .iter()
                    .map(|&s| self.convert_statement(s))
                    .collect(),
                inline: false,
            }
        } else {
            IRSwitchCase {
                test: None,
                statements: vec![],
                inline: false,
            }
        }
    }

    pub(super) fn convert_break_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::BreakStatement(label.map(Into::into))
        } else {
            IRNode::BreakStatement(None)
        }
    }

    pub(super) fn convert_continue_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(jump_data) = self.arena.get_jump_data(node) {
            let label = if jump_data.label.is_none() {
                None
            } else {
                get_identifier_text(self.arena, jump_data.label)
            };
            IRNode::ContinueStatement(label.map(Into::into))
        } else {
            IRNode::ContinueStatement(None)
        }
    }

    pub(super) fn convert_labeled_statement(&self, idx: NodeIndex) -> IRNode {
        let node = self
            .arena
            .get(idx)
            .expect("NodeIndex must be valid in arena");
        if let Some(labeled) = self.arena.get_labeled_statement(node)
            && let Some(label) = get_identifier_text(self.arena, labeled.label)
        {
            if self.emit_await_as_yield && label == "await" {
                return IRNode::Sequence(vec![
                    IRNode::expr_stmt(IRNode::Raw("yield ".into())),
                    self.convert_statement(labeled.statement),
                ]);
            }
            return IRNode::LabeledStatement {
                label: label.into(),
                statement: Box::new(self.convert_statement(labeled.statement)),
            };
        }
        IRNode::ASTRef(idx)
    }
}
