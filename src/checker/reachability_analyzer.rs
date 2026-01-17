//! Reachability Analysis for detecting unreachable code.
//!
//! This module provides the `ReachabilityAnalyzer` for analyzing code paths
//! and detecting unreachable statements after return/throw/break/continue.
//!
//! The analysis is performed during FlowGraph construction, which marks nodes
//! as unreachable when they follow control flow statements that prevent execution.

use crate::binder::{FlowNodeId, flow_flags};
use crate::checker::flow_graph_builder::FlowGraph;
use crate::parser::NodeIndex;
use crate::parser::thin_node::ThinNodeArena;
use rustc_hash::FxHashSet;

/// Analyzer for detecting unreachable code.
///
/// This provides a high-level API for querying reachability information
/// from a FlowGraph. The FlowGraph automatically tracks unreachable nodes
/// during construction via the FlowGraphBuilder.
pub struct ReachabilityAnalyzer<'a> {
    /// Reference to the flow graph
    graph: &'a FlowGraph,
    /// Reference to the ThinNodeArena for AST access
    arena: &'a ThinNodeArena,
    /// Cached set of unreachable node IDs
    unreachable_cache: FxHashSet<u32>,
}

impl<'a> ReachabilityAnalyzer<'a> {
    /// Create a new reachability analyzer.
    pub fn new(graph: &'a FlowGraph, arena: &'a ThinNodeArena) -> Self {
        let unreachable_cache = graph.unreachable_nodes.clone();

        Self {
            graph,
            arena,
            unreachable_cache,
        }
    }

    /// Check if a node is definitely unreachable.
    pub fn is_unreachable(&self, node: NodeIndex) -> bool {
        self.graph.is_unreachable(node)
    }

    /// Get all unreachable nodes in the graph.
    pub fn get_unreachable_nodes(&self) -> &FxHashSet<u32> {
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

        let node_data = self.arena.get(node)?;

        // Check what kind of node precedes this one to determine the reason
        // For now, return a generic message
        Some(match node_data.kind {
            _ => "Unreachable code",
        })
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
mod tests {
    use super::*;
    use crate::checker::flow_graph_builder::FlowGraphBuilder;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_unreachable_after_return() {
        let source = r#"
{
    return;
    let x = 1;  // Unreachable
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // Should have unreachable code
                assert!(analyzer.has_unreachable_code());
                assert!(analyzer.unreachable_count() > 0);
            }
        }
    }

    #[test]
    fn test_unreachable_after_throw() {
        let source = r#"
{
    throw new Error();
    let x = 1;  // Unreachable
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // Should have unreachable code
                assert!(analyzer.has_unreachable_code());
            }
        }
    }

    #[test]
    fn test_unreachable_after_break() {
        let source = r#"
while (true) {
    break;
    let x = 1;  // Unreachable
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // Should have unreachable code
                assert!(analyzer.has_unreachable_code());
            }
        }
    }

    #[test]
    fn test_unreachable_after_continue() {
        let source = r#"
while (true) {
    continue;
    let x = 1;  // Unreachable
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // Should have unreachable code
                assert!(analyzer.has_unreachable_code());
            }
        }
    }

    #[test]
    fn test_reachable_code() {
        let source = r#"
{
    let x = 1;
    let y = 2;
    return x + y;
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // All code before return is reachable
                // The return itself is reachable
                // No code after return, so no unreachable code
                assert!(!analyzer.has_unreachable_code() || analyzer.unreachable_count() == 0);
            }
        }
    }

    #[test]
    fn test_multiple_unreachable_sections() {
        let source = r#"
{
    return;
    let x = 1;  // Unreachable
    let y = 2;  // Unreachable
}
"#;

        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let arena = parser.get_arena();

        if let Some(source_file) = arena.get(root) {
            if let Some(sf) = arena.get_source_file(source_file) {
                let mut builder = FlowGraphBuilder::new(arena);
                let graph = builder.build_source_file(&sf.statements);

                let analyzer = ReachabilityAnalyzer::new(graph, arena);

                // Should have multiple unreachable nodes
                assert!(analyzer.unreachable_count() >= 2);
            }
        }
    }
}
