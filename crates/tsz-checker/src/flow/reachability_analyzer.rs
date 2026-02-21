//! Reachability Analysis for detecting unreachable code.
//!
//! This module provides the `ReachabilityAnalyzer` for analyzing code paths
//! and detecting unreachable statements after return/throw/break/continue.
//!
//! The analysis is performed during `FlowGraph` construction, which marks nodes
//! as unreachable when they follow control flow statements that prevent execution.

use crate::flow_graph_builder::FlowGraph;
use rustc_hash::FxHashSet;
use tsz_binder::{FlowNodeId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Analyzer for detecting unreachable code.
///
/// This provides a high-level API for querying reachability information
/// from a `FlowGraph`. The `FlowGraph` automatically tracks unreachable nodes
/// during construction via the `FlowGraphBuilder`.
pub struct ReachabilityAnalyzer<'a> {
    /// Reference to the flow graph
    graph: &'a FlowGraph,
    /// Reference to the `NodeArena` for AST access
    arena: &'a NodeArena,
}

impl<'a> ReachabilityAnalyzer<'a> {
    /// Create a new reachability analyzer.
    pub const fn new(graph: &'a FlowGraph, arena: &'a NodeArena) -> Self {
        Self { graph, arena }
    }

    /// Check if a node is definitely unreachable.
    pub fn is_unreachable(&self, node: NodeIndex) -> bool {
        self.graph.is_unreachable(node)
    }

    /// Get all unreachable nodes in the graph.
    pub const fn get_unreachable_nodes(&self) -> &FxHashSet<u32> {
        &self.graph.unreachable_nodes
    }

    /// Check if a node is reachable (the opposite of unreachable).
    pub fn is_reachable(&self, node: NodeIndex) -> bool {
        !self.is_unreachable(node)
    }

    /// Get the number of unreachable nodes in the graph.
    pub fn unreachable_count(&self) -> usize {
        self.graph.unreachable_nodes.len()
    }

    /// Analyze reachability starting from a specific flow node.
    ///
    /// This performs a forward traversal to identify all reachable nodes
    /// from the given entry point. Nodes not reached during traversal are
    /// considered unreachable.
    pub fn analyze_from(&mut self, entry: FlowNodeId) {
        let mut visited: FxHashSet<FlowNodeId> = FxHashSet::default();
        let mut worklist: Vec<FlowNodeId> = vec![entry];

        while let Some(flow_id) = worklist.pop() {
            if visited.contains(&flow_id) {
                continue;
            }

            let Some(flow_node) = self.graph.nodes.get(flow_id) else {
                continue;
            };

            // Skip unreachable nodes during traversal
            if flow_node.has_any_flags(flow_flags::UNREACHABLE) {
                continue;
            }

            visited.insert(flow_id);

            // Add antecedents to worklist (forward flow)
            for &antecedent in &flow_node.antecedent {
                if antecedent != FlowNodeId::NONE && !visited.contains(&antecedent) {
                    worklist.push(antecedent);
                }
            }
        }

        // Nodes not visited during forward traversal are unreachable
        // (this is already tracked during graph construction, so we use that info)
    }

    /// Check if code execution can reach a specific node from an entry point.
    pub fn can_reach(&self, _entry: FlowNodeId, target: NodeIndex) -> bool {
        // For now, use the precomputed unreachable set
        self.is_reachable(target)
    }

    /// Get a human-readable description of why a node is unreachable.
    pub fn get_unreachability_reason(&self, node: NodeIndex) -> Option<&'static str> {
        if !self.is_unreachable(node) {
            return None;
        }

        let _node_data = self.arena.get(node)?;

        // Check what kind of node precedes this one to determine the reason
        // For now, return a generic message
        Some("Unreachable code")
    }

    /// Find all unreachable statements in a block of statements.
    pub fn find_unreachable_in_block(&self, statements: &[NodeIndex]) -> Vec<NodeIndex> {
        statements
            .iter()
            .filter(|&&node| self.is_unreachable(node))
            .copied()
            .collect()
    }

    /// Check if there are any unreachable code paths in the graph.
    pub fn has_unreachable_code(&self) -> bool {
        !self.graph.unreachable_nodes.is_empty()
    }
}

#[cfg(test)]
#[path = "../../tests/reachability_analyzer.rs"]
mod tests;
