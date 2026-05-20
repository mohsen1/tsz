//! Reachability Analysis for detecting unreachable code.
//!
//! This module provides the `ReachabilityAnalyzer` for analyzing code paths
//! and detecting unreachable statements after return/throw/break/continue.
//!
//! The analysis is performed during `FlowGraph` construction, which marks nodes
//! as unreachable when they follow control flow statements that prevent execution.

use crate::flow_graph_builder::FlowGraph;
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
    _arena: &'a NodeArena,
}

impl<'a> ReachabilityAnalyzer<'a> {
    /// Create a new reachability analyzer.
    pub const fn new(graph: &'a FlowGraph, arena: &'a NodeArena) -> Self {
        Self {
            graph,
            _arena: arena,
        }
    }

    /// Get the number of unreachable nodes in the graph.
    pub fn unreachable_count(&self) -> usize {
        self.graph.unreachable_nodes.len()
    }

    /// Check if there are any unreachable code paths in the graph.
    pub fn has_unreachable_code(&self) -> bool {
        !self.graph.unreachable_nodes.is_empty()
    }
}

#[cfg(test)]
#[path = "../../tests/reachability_analyzer.rs"]
mod tests;
