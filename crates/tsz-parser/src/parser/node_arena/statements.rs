//! `NodeArena` constructors for control-flow statement nodes (blocks,
//! conditionals, loops, switch, try/catch, labeled, jump, with).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    BlockData, CaseClauseData, CatchClauseData, ExprStatementData, ForInOfData, IfStatementData,
    JumpData, LabeledData, LoopData, NodeArenaInner, ReturnData, SwitchData, TryData, WithData,
};

impl NodeArenaInner {
    /// Add a block node
    pub fn add_block(&mut self, kind: u16, pos: u32, end: u32, data: BlockData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.statements, parent);
        push_data_node!(self, parent, kind, pos, end, blocks, data)
    }

    /// Add an if statement node
    pub fn add_if_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IfStatementData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.then_statement, parent);
        self.set_parent(data.else_statement, parent);
        push_data_node!(self, parent, kind, pos, end, if_statements, data)
    }

    /// Add a loop node (for/while/do)
    pub fn add_loop(&mut self, kind: u16, pos: u32, end: u32, data: LoopData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.initializer, parent);
        self.set_parent(data.condition, parent);
        self.set_parent(data.incrementor, parent);
        self.set_parent(data.statement, parent);
        push_data_node!(self, parent, kind, pos, end, loops, data)
    }

    /// Add a for-in/for-of statement node
    pub fn add_for_in_of(&mut self, kind: u16, pos: u32, end: u32, data: ForInOfData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.initializer, parent);
        self.set_parent(data.expression, parent);
        self.set_parent(data.statement, parent);
        push_data_node!(self, parent, kind, pos, end, for_in_of, data)
    }

    /// Add a return/throw statement node
    pub fn add_return(&mut self, kind: u16, pos: u32, end: u32, data: ReturnData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, return_data, data)
    }

    /// Add an expression statement node
    pub fn add_expr_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprStatementData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, expr_statements, data)
    }

    /// Add a switch statement node
    pub fn add_switch(&mut self, kind: u16, pos: u32, end: u32, data: SwitchData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.case_block, parent);
        push_data_node!(self, parent, kind, pos, end, switch_data, data)
    }

    /// Add a case/default clause node
    pub fn add_case_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CaseClauseData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent_list(&data.statements, parent);
        push_data_node!(self, parent, kind, pos, end, case_clauses, data)
    }

    /// Add a try statement node
    pub fn add_try(&mut self, kind: u16, pos: u32, end: u32, data: TryData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.try_block, parent);
        self.set_parent(data.catch_clause, parent);
        self.set_parent(data.finally_block, parent);
        push_data_node!(self, parent, kind, pos, end, try_data, data)
    }

    /// Add a catch clause node
    pub fn add_catch_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CatchClauseData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.variable_declaration, parent);
        self.set_parent(data.block, parent);
        push_data_node!(self, parent, kind, pos, end, catch_clauses, data)
    }

    /// Add a labeled statement node
    pub fn add_labeled(&mut self, kind: u16, pos: u32, end: u32, data: LabeledData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.label, parent);
        self.set_parent(data.statement, parent);
        push_data_node!(self, parent, kind, pos, end, labeled_data, data)
    }

    /// Add a break/continue statement node
    pub fn add_jump(&mut self, kind: u16, pos: u32, end: u32, data: JumpData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.label, parent);
        push_data_node!(self, parent, kind, pos, end, jump_data, data)
    }

    /// Add a with statement node
    pub fn add_with(&mut self, kind: u16, pos: u32, end: u32, data: WithData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.statement, parent);
        push_data_node!(self, parent, kind, pos, end, with_data, data)
    }
}
