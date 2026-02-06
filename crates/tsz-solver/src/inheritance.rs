//! Inheritance Graph Solver
//!
//! Manages the nominal inheritance relationships between classes and interfaces.
//! Provides O(1) subtype checks via lazy transitive closure and handles
//! Method Resolution Order (MRO) for member lookup.

use fixedbitset::FixedBitSet;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use tsz_binder::SymbolId;

/// Represents a node in the inheritance graph.
#[derive(Debug, Clone)]
struct ClassNode {
    /// Direct parents (extends and implements)
    parents: Vec<SymbolId>,
    /// Children (for invalidation/reverse lookup)
    children: Vec<SymbolId>,
    /// Cached transitive closure (all ancestors)
    /// If None, it needs to be computed.
    ancestors_bitset: Option<FixedBitSet>,
    /// Cached Method Resolution Order (linearized ancestors)
    mro: Option<Vec<SymbolId>>,
}

impl Default for ClassNode {
    fn default() -> Self {
        Self {
            parents: Vec::new(),
            children: Vec::new(),
            ancestors_bitset: None,
            mro: None,
        }
    }
}

pub struct InheritanceGraph {
    /// Map from SymbolId to graph node data
    nodes: RefCell<FxHashMap<SymbolId, ClassNode>>,
    /// Maximum SymbolId seen so far (for BitSet sizing)
    max_symbol_id: RefCell<usize>,
}

impl Default for InheritanceGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl InheritanceGraph {
    pub fn new() -> Self {
        Self {
            nodes: RefCell::new(FxHashMap::default()),
            max_symbol_id: RefCell::new(0),
        }
    }

    /// Register a class or interface and its direct parents.
    ///
    /// # Arguments
    /// * `child` - The SymbolId of the class/interface being defined
    /// * `parents` - List of SymbolIds this type extends or implements
    pub fn add_inheritance(&self, child: SymbolId, parents: &[SymbolId]) {
        let mut nodes = self.nodes.borrow_mut();
        let mut max_id = self.max_symbol_id.borrow_mut();

        // Update max ID for bitset sizing
        *max_id = (*max_id).max(child.0 as usize);
        for &p in parents {
            *max_id = (*max_id).max(p.0 as usize);
        }

        // Register child
        let child_node = nodes.entry(child).or_default();

        // Check if edges actually changed to avoid invalidating cache unnecessarily
        if child_node.parents == parents {
            return;
        }

        child_node.parents = parents.to_vec();

        // Invalidate caches
        child_node.ancestors_bitset = None;
        child_node.mro = None;

        // Register reverse edges (for future invalidation logic)
        for &parent in parents {
            let parent_node = nodes.entry(parent).or_default();
            if !parent_node.children.contains(&child) {
                parent_node.children.push(child);
            }
        }
    }

    /// Checks if `child` is a subtype of `ancestor` nominally.
    ///
    /// This is an O(1) operation after the first lazy computation.
    /// Returns `true` if `child` extends or implements `ancestor` (transitively).
    pub fn is_derived_from(&self, child: SymbolId, ancestor: SymbolId) -> bool {
        if child == ancestor {
            return true;
        }

        // Fast path: check if nodes exist
        let nodes = self.nodes.borrow();
        if !nodes.contains_key(&child) || !nodes.contains_key(&ancestor) {
            return false;
        }
        drop(nodes); // Release borrow for compute

        self.ensure_transitive_closure(child);

        let nodes = self.nodes.borrow();
        if let Some(node) = nodes.get(&child) {
            if let Some(bits) = &node.ancestors_bitset {
                return bits.contains(ancestor.0 as usize);
            }
        }

        false
    }

    /// Gets the Method Resolution Order (MRO) for a symbol.
    ///
    /// Returns a list of SymbolIds in the order they should be searched for members.
    /// Implements a depth-first, left-to-right traversal (standard for TS/JS).
    pub fn get_resolution_order(&self, symbol_id: SymbolId) -> Vec<SymbolId> {
        self.ensure_mro(symbol_id);

        let nodes = self.nodes.borrow();
        if let Some(node) = nodes.get(&symbol_id) {
            if let Some(mro) = &node.mro {
                return mro.clone();
            }
        }

        vec![symbol_id] // Fallback: just the symbol itself
    }

    /// Finds the Least Upper Bound (common ancestor) of two symbols.
    ///
    /// Returns the most specific symbol that both A and B inherit from.
    /// In cases of multiple inheritance (interfaces), this might return one of several valid candidates.
    pub fn find_common_ancestor(&self, a: SymbolId, b: SymbolId) -> Option<SymbolId> {
        if self.is_derived_from(a, b) {
            return Some(b);
        }
        if self.is_derived_from(b, a) {
            return Some(a);
        }

        self.ensure_transitive_closure(a);
        self.ensure_transitive_closure(b);

        let nodes = self.nodes.borrow();
        let node_a = nodes.get(&a)?;
        let node_b = nodes.get(&b)?;

        let bits_a = node_a.ancestors_bitset.as_ref()?;
        let bits_b = node_b.ancestors_bitset.as_ref()?;

        // Intersection of ancestors
        let mut common = bits_a.clone();
        common.intersect_with(bits_b);

        // We want the "lowest" (most specific) ancestor.
        // In a topological sort, this is usually the one with the longest path or
        // appearing earliest in MRO.
        // Simplified approach: Iterate A's MRO and return the first one present in B's ancestors.

        drop(nodes); // Release for MRO check
        let mro_a = self.get_resolution_order(a);

        for ancestor in mro_a {
            if self.is_derived_from(b, ancestor) {
                return Some(ancestor);
            }
        }

        None
    }

    /// Detects if adding an edge would create a cycle.
    pub fn detects_cycle(&self, child: SymbolId, parent: SymbolId) -> bool {
        // If parent is already derived from child, adding child->parent creates a cycle
        self.is_derived_from(parent, child)
    }

    /// Get the direct parents of a symbol (for cycle detection).
    pub fn get_parents(&self, symbol_id: SymbolId) -> Vec<SymbolId> {
        let nodes = self.nodes.borrow();
        if let Some(node) = nodes.get(&symbol_id) {
            node.parents.clone()
        } else {
            Vec::new()
        }
    }

    // =========================================================================
    // Internal Lazy Computation Methods
    // =========================================================================

    /// Lazily computes the transitive closure (ancestor bitset) for a node.
    fn ensure_transitive_closure(&self, symbol_id: SymbolId) {
        let mut nodes = self.nodes.borrow_mut();

        // If already computed, return
        if let Some(node) = nodes.get(&symbol_id) {
            if node.ancestors_bitset.is_some() {
                return;
            }
        } else {
            return; // Node doesn't exist
        }

        // Stack for DFS
        let max_len = *self.max_symbol_id.borrow() + 1;

        // Cycle detection set for this traversal
        let mut path = FxHashSet::default();

        self.compute_closure_recursive(symbol_id, &mut nodes, &mut path, max_len);
    }

    fn compute_closure_recursive(
        &self,
        current: SymbolId,
        nodes: &mut FxHashMap<SymbolId, ClassNode>,
        path: &mut FxHashSet<SymbolId>,
        bitset_len: usize,
    ) {
        if path.contains(&current) {
            // Cycle detected, stop recursion here.
            // In a real compiler, we might emit a diagnostic here,
            // but the solver just wants to avoid infinite loops.
            return;
        }

        // If already computed, we are good
        if let Some(node) = nodes.get(&current) {
            if node.ancestors_bitset.is_some() {
                return;
            }
        }

        path.insert(current);

        // Clone parents to avoid borrowing issues during recursion
        let parents = if let Some(node) = nodes.get(&current) {
            node.parents.clone()
        } else {
            Vec::new()
        };

        let mut my_bits = FixedBitSet::with_capacity(bitset_len);

        for parent in parents {
            // Ensure parent is computed
            self.compute_closure_recursive(parent, nodes, path, bitset_len);

            // Add parent itself
            my_bits.insert(parent.0 as usize);

            // Add parent's ancestors
            if let Some(parent_node) = nodes.get(&parent) {
                if let Some(parent_bits) = &parent_node.ancestors_bitset {
                    my_bits.union_with(parent_bits);
                }
            }
        }

        // Save result
        if let Some(node) = nodes.get_mut(&current) {
            node.ancestors_bitset = Some(my_bits);
        }

        path.remove(&current);
    }

    /// Lazily computes the MRO for a node.
    fn ensure_mro(&self, symbol_id: SymbolId) {
        let mut nodes = self.nodes.borrow_mut();

        if let Some(node) = nodes.get(&symbol_id) {
            if node.mro.is_some() {
                return;
            }
        } else {
            return;
        }

        // Standard Depth-First Left-to-Right traversal for TypeScript
        // (Note: Python uses C3, but TS is simpler)
        let mut mro = Vec::new();
        let mut visited = FxHashSet::default();
        let mut queue = VecDeque::new();

        queue.push_back(symbol_id);

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }

            mro.push(current);

            if let Some(node) = nodes.get(&current) {
                // Add parents to queue
                // For class extends A implements B, C -> A, B, C
                for parent in &node.parents {
                    queue.push_back(*parent);
                }
            }
        }

        if let Some(node) = nodes.get_mut(&symbol_id) {
            node.mro = Some(mro);
        }
    }

    /// Clear all cached data (useful for testing or rebuilding)
    pub fn clear(&self) {
        self.nodes.borrow_mut().clear();
        *self.max_symbol_id.borrow_mut() = 0;
    }

    /// Get the number of nodes in the graph
    pub fn len(&self) -> usize {
        self.nodes.borrow().len()
    }

    /// Check if the graph is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_inheritance() {
        let graph = InheritanceGraph::new();
        let parent = SymbolId(1);
        let child = SymbolId(2);

        graph.add_inheritance(child, &[parent]);

        assert!(graph.is_derived_from(child, parent));
        assert!(!graph.is_derived_from(parent, child));
    }

    #[test]
    fn test_transitive_inheritance() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);

        // A -> B -> C
        graph.add_inheritance(b, &[a]);
        graph.add_inheritance(c, &[b]);

        assert!(graph.is_derived_from(c, a)); // Transitive
        assert!(graph.is_derived_from(c, b));
        assert!(!graph.is_derived_from(a, c));
    }

    #[test]
    fn test_diamond_inheritance() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);
        let d = SymbolId(4);

        // Diamond: A is top, B and C extend A, D extends both B and C
        graph.add_inheritance(b, &[a]);
        graph.add_inheritance(c, &[a]);
        graph.add_inheritance(d, &[b, c]);

        assert!(graph.is_derived_from(d, a)); // D derives from A through both paths
        assert!(graph.is_derived_from(d, b));
        assert!(graph.is_derived_from(d, c));
    }

    #[test]
    fn test_cycle_detection() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);

        // A -> B
        graph.add_inheritance(b, &[a]);

        // Check if adding C -> A would create cycle (A -> B -> C -> A)
        // This should not create a cycle yet
        assert!(!graph.detects_cycle(c, a));

        // Now add the cycle-creating edge
        graph.add_inheritance(a, &[c]);

        // Now parent is derived from child, so detects_cycle should return true
        assert!(graph.detects_cycle(a, b));
    }

    #[test]
    fn test_multiple_inheritance() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);

        // C extends both A and B
        graph.add_inheritance(c, &[a, b]);

        assert!(graph.is_derived_from(c, a));
        assert!(graph.is_derived_from(c, b));

        let mro = graph.get_resolution_order(c);
        // MRO should be C, then A, then B (or C, B, A depending on order)
        assert_eq!(mro[0], c);
    }

    #[test]
    fn test_common_ancestor() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);
        let d = SymbolId(4);

        // Diamond: A at top, B and C extend A, D extends both
        graph.add_inheritance(b, &[a]);
        graph.add_inheritance(c, &[a]);
        graph.add_inheritance(d, &[b, c]);

        // Common ancestor of B and C should be A
        assert_eq!(graph.find_common_ancestor(b, c), Some(a));

        // Common ancestor of D and B should be B
        assert_eq!(graph.find_common_ancestor(d, b), Some(b));
    }

    #[test]
    fn test_no_common_ancestor() {
        let graph = InheritanceGraph::new();
        let a = SymbolId(1);
        let b = SymbolId(2);
        let c = SymbolId(3);
        let d = SymbolId(4);

        // Two separate chains: A->B and C->D
        graph.add_inheritance(b, &[a]);
        graph.add_inheritance(d, &[c]);

        // No common ancestor
        assert_eq!(graph.find_common_ancestor(b, d), None);
    }
}
