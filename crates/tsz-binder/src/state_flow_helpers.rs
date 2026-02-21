//! Flow graph construction helpers.
//!
//! This module provides factory methods for creating flow graph nodes
//! used in control flow analysis (branch labels, conditions, assignments, etc.).

use crate::state::BinderState;
use crate::{FlowNodeId, flow_flags};
use tsz_parser::NodeIndex;

impl BinderState {
    // =========================================================================
    // Flow graph construction helpers
    // =========================================================================

    /// Create a branch label flow node for merging control flow paths.
    pub(crate) fn create_branch_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::BRANCH_LABEL)
    }

    /// Create a loop label flow node for back-edges.
    pub(crate) fn create_loop_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::LOOP_LABEL)
    }

    /// Create a flow condition node for tracking type narrowing.
    pub(crate) fn create_flow_condition(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        condition: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flags);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.antecedent.push(antecedent);
            node.node = condition;
        }
        id
    }

    /// Create a flow node for a switch clause with optional fallthrough.
    pub(crate) fn create_switch_clause_flow(
        &mut self,
        pre_switch: FlowNodeId,
        fallthrough: FlowNodeId,
        clause: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::SWITCH_CLAUSE);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = clause;
        }
        self.add_antecedent(id, pre_switch);
        self.add_antecedent(id, fallthrough);
        id
    }

    /// Create a flow node for an assignment.
    pub(crate) fn create_flow_assignment(&mut self, assignment: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ASSIGNMENT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = assignment;
            if self.current_flow.is_some() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for a call expression.
    pub(crate) fn create_flow_call(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::CALL);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if self.current_flow.is_some() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for array mutation (e.g. push/splice).
    pub(crate) fn create_flow_array_mutation(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ARRAY_MUTATION);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if self.current_flow.is_some() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for await expression (async suspension point).
    pub(crate) fn create_flow_await_point(&mut self, await_expr: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::AWAIT_POINT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = await_expr;
            if self.current_flow.is_some() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for yield expression (generator suspension point).
    pub(crate) fn create_flow_yield_point(&mut self, yield_expr: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::YIELD_POINT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = yield_expr;
            if self.current_flow.is_some() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Add an antecedent to a flow node (for merging branches).
    pub(crate) fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if antecedent.is_none() || antecedent == self.unreachable_flow {
            return;
        }
        if let Some(node) = self.flow_nodes.get_mut(label)
            && !node.antecedent.contains(&antecedent)
        {
            node.antecedent.push(antecedent);
        }
    }
}
