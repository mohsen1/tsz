//! Flow graph construction helpers.
//!
//! This module provides factory methods for creating flow graph nodes
//! used in control flow analysis (branch labels, conditions, assignments, etc.).

use super::BinderState;
use crate::{FlowNodeId, flow_flags};
use std::sync::Arc;
use tsz_parser::NodeIndex;

impl BinderState {
    // =========================================================================
    // Flow graph construction helpers
    // =========================================================================

    /// Create a branch label flow node for merging control flow paths.
    pub(crate) fn create_branch_label(&mut self) -> FlowNodeId {
        Arc::make_mut(&mut self.flow_nodes).alloc(flow_flags::BRANCH_LABEL)
    }

    /// Create a loop label flow node for back-edges.
    pub(crate) fn create_loop_label(&mut self) -> FlowNodeId {
        Arc::make_mut(&mut self.flow_nodes).alloc(flow_flags::LOOP_LABEL)
    }

    /// Create a flow condition node for tracking type narrowing.
    pub(crate) fn create_flow_condition(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        condition: NodeIndex,
    ) -> FlowNodeId {
        let flow_nodes = Arc::make_mut(&mut self.flow_nodes);
        let id = flow_nodes.alloc(flags);
        if let Some(node) = flow_nodes.get_mut(id) {
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
        let id = {
            let flow_nodes = Arc::make_mut(&mut self.flow_nodes);
            let id = flow_nodes.alloc(flow_flags::SWITCH_CLAUSE);
            if let Some(node) = flow_nodes.get_mut(id) {
                node.node = clause;
            }
            id
        };
        self.add_antecedent(id, pre_switch);
        self.add_antecedent(id, fallthrough);
        id
    }

    /// Shared template for flow nodes that record an AST node and chain the
    /// current flow as their antecedent. The five `create_flow_*` wrappers
    /// below differ only in the `flow_flags` constant they pass here.
    fn create_flow_node_with_node(&mut self, flags: u32, node_idx: NodeIndex) -> FlowNodeId {
        let current_flow = self.current_flow;
        let flow_nodes = Arc::make_mut(&mut self.flow_nodes);
        let id = flow_nodes.alloc(flags);
        if let Some(node) = flow_nodes.get_mut(id) {
            node.node = node_idx;
            if current_flow.is_some() {
                node.antecedent.push(current_flow);
            }
        }
        id
    }

    /// Create a flow node for an assignment.
    pub(crate) fn create_flow_assignment(&mut self, assignment: NodeIndex) -> FlowNodeId {
        self.create_flow_node_with_node(flow_flags::ASSIGNMENT, assignment)
    }

    /// Create a flow node for a call expression.
    pub(crate) fn create_flow_call(&mut self, call: NodeIndex) -> FlowNodeId {
        self.create_flow_node_with_node(flow_flags::CALL, call)
    }

    /// Create a flow node for array mutation (e.g. push/splice).
    pub(crate) fn create_flow_array_mutation(&mut self, call: NodeIndex) -> FlowNodeId {
        self.create_flow_node_with_node(flow_flags::ARRAY_MUTATION, call)
    }

    /// Create a flow node for await expression (async suspension point).
    pub(crate) fn create_flow_await_point(&mut self, await_expr: NodeIndex) -> FlowNodeId {
        self.create_flow_node_with_node(flow_flags::AWAIT_POINT, await_expr)
    }

    /// Create a flow node for yield expression (generator suspension point).
    pub(crate) fn create_flow_yield_point(&mut self, yield_expr: NodeIndex) -> FlowNodeId {
        self.create_flow_node_with_node(flow_flags::YIELD_POINT, yield_expr)
    }

    /// Add an antecedent to a flow node (for merging branches).
    pub(crate) fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if antecedent.is_none() || antecedent == self.unreachable_flow {
            return;
        }
        if let Some(node) = Arc::make_mut(&mut self.flow_nodes).get_mut(label)
            && !node.antecedent.contains(&antecedent)
        {
            node.antecedent.push(antecedent);
        }
    }

    /// Finalize a branch/merge label as `current_flow`, the way tsc's
    /// `finishFlowLabel` does: a label with no (reachable) antecedents collapses
    /// to the unreachable flow, a label with exactly one collapses to that
    /// antecedent, and a label with several stays itself. `add_antecedent` skips
    /// unreachable antecedents, so e.g. an `if` whose both arms `return` leaves an
    /// antecedent-less merge label; assigning it directly to `current_flow` would
    /// treat that unreachable merge point as reachable and drop the narrowing of
    /// subsequent statements (a false negated-discriminant / definite-assignment
    /// result). Use this instead of a bare `self.current_flow = label`.
    pub(crate) fn finish_flow_label(&mut self, label: FlowNodeId) {
        let unreachable = self.unreachable_flow;
        let resolved = match self.flow_nodes.get(label) {
            Some(node) => match node.antecedent.len() {
                0 => unreachable,
                1 => node.antecedent[0],
                _ => label,
            },
            None => unreachable,
        };
        self.current_flow = resolved;
    }
}
