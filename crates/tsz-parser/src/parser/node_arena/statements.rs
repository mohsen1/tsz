//! `NodeArena` constructors for control-flow statement nodes (blocks,
//! conditionals, loops, switch, try/catch, labeled, jump, with).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    BlockData, CaseClauseData, CatchClauseData, ExprStatementData, ExtendedNodeInfo, ForInOfData,
    IfStatementData, JumpData, LabeledData, LoopData, Node, NodeArena, ReturnData, SwitchData,
    TryData, WithData,
};

impl NodeArena {
    /// Add a block node
    pub fn add_block(&mut self, kind: u16, pos: u32, end: u32, data: BlockData) -> NodeIndex {
        let statements = data.statements.clone();

        let data_index = self.len_u32(self.blocks.len());
        self.blocks.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&statements, parent);

        parent
    }

    /// Add an if statement node
    pub fn add_if_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IfStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let then_statement = data.then_statement;
        let else_statement = data.else_statement;

        let data_index = self.len_u32(self.if_statements.len());
        self.if_statements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(then_statement, parent);
        self.set_parent(else_statement, parent);

        parent
    }

    /// Add a loop node (for/while/do)
    pub fn add_loop(&mut self, kind: u16, pos: u32, end: u32, data: LoopData) -> NodeIndex {
        let initializer = data.initializer;
        let condition = data.condition;
        let incrementor = data.incrementor;
        let statement = data.statement;
        let data_index = self.len_u32(self.loops.len());
        self.loops.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(condition, parent);
        self.set_parent(incrementor, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a for-in/for-of statement node
    pub fn add_for_in_of(&mut self, kind: u16, pos: u32, end: u32, data: ForInOfData) -> NodeIndex {
        let initializer = data.initializer;
        let expression = data.expression;
        let statement = data.statement;
        let data_index = self.len_u32(self.for_in_of.len());
        self.for_in_of.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(initializer, parent);
        self.set_parent(expression, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a return/throw statement node
    pub fn add_return(&mut self, kind: u16, pos: u32, end: u32, data: ReturnData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.return_data.len());
        self.return_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);

        parent
    }

    /// Add an expression statement node
    pub fn add_expr_statement(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprStatementData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.expr_statements.len());
        self.expr_statements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a switch statement node
    pub fn add_switch(&mut self, kind: u16, pos: u32, end: u32, data: SwitchData) -> NodeIndex {
        let expression = data.expression;
        let case_block = data.case_block;
        let data_index = self.len_u32(self.switch_data.len());
        self.switch_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(case_block, parent);
        parent
    }

    /// Add a case/default clause node
    pub fn add_case_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CaseClauseData,
    ) -> NodeIndex {
        let expression = data.expression;
        let statements = data.statements.clone();
        let data_index = self.len_u32(self.case_clauses.len());
        self.case_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_list(&statements, parent);
        parent
    }

    /// Add a try statement node
    pub fn add_try(&mut self, kind: u16, pos: u32, end: u32, data: TryData) -> NodeIndex {
        let try_block = data.try_block;
        let catch_clause = data.catch_clause;
        let finally_block = data.finally_block;
        let data_index = self.len_u32(self.try_data.len());
        self.try_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(try_block, parent);
        self.set_parent(catch_clause, parent);
        self.set_parent(finally_block, parent);
        parent
    }

    /// Add a catch clause node
    pub fn add_catch_clause(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CatchClauseData,
    ) -> NodeIndex {
        let variable_declaration = data.variable_declaration;
        let block = data.block;
        let data_index = self.len_u32(self.catch_clauses.len());
        self.catch_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(variable_declaration, parent);
        self.set_parent(block, parent);

        parent
    }

    /// Add a labeled statement node
    pub fn add_labeled(&mut self, kind: u16, pos: u32, end: u32, data: LabeledData) -> NodeIndex {
        let label = data.label;
        let statement = data.statement;
        let data_index = self.len_u32(self.labeled_data.len());
        self.labeled_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(label, parent);
        self.set_parent(statement, parent);
        parent
    }

    /// Add a break/continue statement node
    pub fn add_jump(&mut self, kind: u16, pos: u32, end: u32, data: JumpData) -> NodeIndex {
        let label = data.label;
        let data_index = self.len_u32(self.jump_data.len());
        self.jump_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(label, parent);
        parent
    }

    /// Add a with statement node
    pub fn add_with(&mut self, kind: u16, pos: u32, end: u32, data: WithData) -> NodeIndex {
        let expression = data.expression;
        let statement = data.statement;
        let data_index = self.len_u32(self.with_data.len());
        self.with_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(statement, parent);
        parent
    }
}
