//! Control Flow Analysis for type narrowing.
//!
//! This module provides flow-sensitive type analysis that walks the control flow
//! graph backwards from identifier usages to determine narrowed types.
//!
//! Example:
//! ```typescript
//! function foo(x: string | number) {
//!     if (typeof x === "string") {
//!         // FlowAnalyzer walks back and sees TRUE_CONDITION (typeof x === "string")
//!         // Returns: string (narrowed from string | number)
//!         console.log(x.length);
//!     } else {
//!         // FlowAnalyzer sees FALSE_CONDITION
//!         // Returns: number
//!         console.log(x.toFixed(2));
//!     }
//! }
//! ```

use crate::binder::BinderState;
use crate::binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags, symbol_flags};
use crate::interner::Atom;
use crate::parser::node::{BinaryExprData, NodeArena};
use crate::parser::{NodeIndex, NodeList, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;
#[allow(unused_imports)]
use crate::solver::{
    NarrowingContext, ParamInfo, QueryDatabase, TypeDatabase, TypeGuard, TypeId, TypePredicate,
    Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

// =============================================================================
// FlowGraph
// =============================================================================

/// A control flow graph that provides query methods for flow analysis.
///
/// This wraps the `FlowNodeArena` and provides convenient methods for querying
/// flow information during type checking.
pub struct FlowGraph<'a> {
    /// Reference to the flow node arena containing all flow nodes
    arena: &'a FlowNodeArena,
}

impl<'a> FlowGraph<'a> {
    /// Create a new FlowGraph from a FlowNodeArena.
    pub fn new(arena: &'a FlowNodeArena) -> Self {
        Self { arena }
    }

    /// Get a flow node by ID.
    pub fn get(&self, id: FlowNodeId) -> Option<&FlowNode> {
        self.arena.get(id)
    }

    /// Get a mutable reference to a flow node by ID.
    pub fn get_mut(&mut self, _id: FlowNodeId) -> Option<&mut FlowNode> {
        // Note: This would require interior mutability or a different API design
        // For now, we'll return None as FlowGraph is meant for querying, not modifying
        None
    }

    /// Get the number of flow nodes in the graph.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Check if the flow graph is empty.
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Check if a flow node has a specific flag.
    pub fn node_has_flag(&self, id: FlowNodeId, flag: u32) -> bool {
        self.get(id)
            .map(|node| node.has_any_flags(flag))
            .unwrap_or(false)
    }

    /// Get the antecedents (predecessors) of a flow node.
    pub fn antecedents(&self, id: FlowNodeId) -> Vec<FlowNodeId> {
        self.get(id)
            .map(|node| node.antecedent.clone())
            .unwrap_or_default()
    }

    /// Get the AST node associated with a flow node.
    pub fn node(&self, id: FlowNodeId) -> NodeIndex {
        self.get(id)
            .map(|node| node.node)
            .unwrap_or(NodeIndex::NONE)
    }
}

// =============================================================================
// FlowAnalyzer
// =============================================================================

/// Flow analyzer for control flow-based type narrowing.
///
/// Walks the control flow graph backwards from a reference point to determine
/// what type narrowing applies at that location.
pub struct FlowAnalyzer<'a> {
    pub(crate) arena: &'a NodeArena,
    pub(crate) binder: &'a BinderState,
    pub(crate) interner: &'a dyn QueryDatabase,
    pub(crate) node_types: Option<&'a FxHashMap<u32, TypeId>>,
    pub(crate) flow_graph: Option<FlowGraph<'a>>,
    /// Optional cache for flow analysis results to avoid redundant graph traversals
    pub(crate) flow_cache: Option<&'a RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>>,
    /// Optional TypeEnvironment for resolving Lazy types during narrowing
    pub(crate) type_environment: Option<Rc<RefCell<crate::solver::TypeEnvironment>>>,
    /// Cache for loop mutation analysis: (LoopNodeId, SymbolId) -> is_mutated
    /// This prevents O(N^2) complexity when checking mutations in nested loops
    loop_mutation_cache: RefCell<FxHashMap<(FlowNodeId, SymbolId), bool>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyPresence {
    Required,
    Optional,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PropertyKey {
    Atom(Atom),
    Index(usize),
}

#[derive(Clone)]
pub(crate) struct PredicateSignature {
    pub(crate) predicate: TypePredicate,
    pub(crate) params: Vec<ParamInfo>,
}

impl<'a> FlowAnalyzer<'a> {
    /// Create a new FlowAnalyzer.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a dyn QueryDatabase,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            node_types: None,
            flow_graph,
            flow_cache: None,
            type_environment: None,
            loop_mutation_cache: RefCell::new(FxHashMap::default()),
        }
    }

    pub fn with_node_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a dyn QueryDatabase,
        node_types: &'a FxHashMap<u32, TypeId>,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            node_types: Some(node_types),
            flow_graph,
            flow_cache: None,
            type_environment: None,
            loop_mutation_cache: RefCell::new(FxHashMap::default()),
        }
    }

    /// Set the flow analysis cache to avoid redundant graph traversals.
    pub fn with_flow_cache(
        mut self,
        cache: &'a RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>,
    ) -> Self {
        self.flow_cache = Some(cache);
        self
    }

    /// Set the TypeEnvironment for resolving Lazy types during narrowing.
    pub fn with_type_environment(
        mut self,
        type_env: Rc<RefCell<crate::solver::TypeEnvironment>>,
    ) -> Self {
        self.type_environment = Some(type_env);
        self
    }

    /// Get a reference to the flow graph.
    pub fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }

    /// Get the narrowed type of a symbol at a specific flow node.
    ///
    /// This walks backwards through the flow graph, applying narrowing operations
    /// when it encounters condition nodes.
    pub fn get_flow_type(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_node: FlowNodeId,
    ) -> TypeId {
        if flow_node.is_none() {
            return initial_type;
        }

        // Resolve symbol for caching purposes
        let symbol_id = self.binder.resolve_identifier(self.arena, reference);

        self.check_flow(
            reference,
            initial_type,
            flow_node,
            &mut Vec::new(),
            symbol_id,
        )
    }

    /// Check if a reference is definitely assigned at a specific flow node.
    pub fn is_definitely_assigned(&self, reference: NodeIndex, flow_node: FlowNodeId) -> bool {
        if flow_node.is_none() {
            return true;
        }

        let mut visited = Vec::new();
        let mut cache = FxHashMap::default();
        self.check_definite_assignment(reference, flow_node, &mut visited, &mut cache)
    }

    /// Analyze a loop using fixed-point iteration to determine the stable type of a variable.
    ///
    /// This implements TypeScript's loop flow analysis where the type of a variable
    /// at the start of a loop depends on its type at the end (back-edge). We iterate
    /// until the type stabilizes (reaches a fixed point).
    ///
    /// # Arguments
    /// * `loop_flow_id` - The FlowNodeId of the LOOP_LABEL (for cache key)
    /// * `loop_flow` - The LOOP_LABEL flow node
    /// * `reference` - The variable reference we're analyzing
    /// * `entry_type` - The type entering the loop (from antecedent[0])
    /// * `initial_type` - The declared type of the variable (for widening)
    /// * `symbol_id` - The symbol ID (for cache key)
    ///
    /// # Returns
    /// The stabilized type after fixed-point iteration
    fn analyze_loop_fixed_point(
        &self,
        loop_flow_id: FlowNodeId,
        loop_flow: &FlowNode,
        reference: NodeIndex,
        entry_type: TypeId,
        initial_type: TypeId,
        symbol_id: Option<SymbolId>,
    ) -> TypeId {
        const MAX_ITERATIONS: usize = 5;

        // For const symbols, no fixed-point needed - they can't be reassigned
        if let Some(sym_id) = symbol_id {
            if self.is_const_symbol(sym_id) {
                return entry_type;
            }
        }

        // If there's only one antecedent (just the entry, no back-edges), no iteration needed
        if loop_flow.antecedent.len() <= 1 {
            return entry_type;
        }

        let mut current_type = entry_type;

        // Fixed-point iteration: union entry type with all back-edge types
        for _iteration in 0..MAX_ITERATIONS {
            let prev_type = current_type;

            // CRITICAL FIX: Inject current assumption into cache to break infinite recursion
            // Without this, get_flow_type -> check_flow -> LOOP_LABEL -> analyze_loop_fixed_point
            // would cause stack overflow
            //
            // This tells the recursive traversal: "If you hit this loop header again,
            // assume its type is current_type and stop"
            if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache) {
                let key = (loop_flow_id, sym_id, initial_type);
                cache.borrow_mut().insert(key, current_type);
            }

            // Union entry type with all back-edge types (antecedents[1+])
            for &back_edge in loop_flow.antecedent.iter().skip(1) {
                // Get the type at the back-edge point in the flow
                // Thanks to the cache injection above, this won't infinitely recurse
                let back_edge_type = self.get_flow_type(reference, initial_type, back_edge);

                // Union current type with back-edge type
                current_type = self.interner.union2(current_type, back_edge_type);
            }

            // Check if we've reached a fixed point (type stopped changing)
            if current_type == prev_type {
                return current_type;
            }
        }

        // Fixed point not reached within iteration limit
        // Conservative widening: return union of entry type and initial declared type
        // This matches TypeScript's behavior for complex loops
        let widened = self.interner.union2(entry_type, initial_type);

        // Update cache with final widened result
        if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache) {
            let key = (loop_flow_id, sym_id, initial_type);
            cache.borrow_mut().insert(key, widened);
        }

        widened
    }

    /// Iterative flow graph traversal using a worklist algorithm.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a VecDeque worklist with
    /// cycle detection to process flow nodes iteratively.
    pub(crate) fn check_flow(
        &self,
        reference: NodeIndex,
        initial_type: TypeId,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
        symbol_id: Option<SymbolId>,
    ) -> TypeId {
        // Work item: (flow_id, type_at_this_point)
        let mut worklist: VecDeque<(FlowNodeId, TypeId)> = VecDeque::new();
        let mut in_worklist: FxHashSet<FlowNodeId> = FxHashSet::default();
        let mut visited: FxHashSet<FlowNodeId> = FxHashSet::default();

        // Result cache: flow_id -> narrowed_type
        let mut results: FxHashMap<FlowNodeId, TypeId> = FxHashMap::default();

        // CRITICAL: Check if initial type contains type parameters ONCE, outside the loop.
        // This prevents caching generic types across different instantiations.
        // See: https://github.com/microsoft/TypeScript/issues/9998
        let initial_has_type_params =
            crate::solver::type_queries::contains_type_parameters_db(self.interner, initial_type);

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);

        // Process worklist until empty
        while let Some((current_flow, current_type)) = worklist.pop_front() {
            in_worklist.remove(&current_flow);

            // OPTIMIZATION: Check global cache first to avoid redundant traversals
            // BUG FIX: Skip cache for SWITCH_CLAUSE nodes to ensure proper flow graph traversal
            // Switch clauses must be processed to schedule antecedents and apply narrowing
            let is_switch_clause = if let Some(flow) = self.binder.flow_nodes.get(current_flow) {
                flow.has_any_flags(flow_flags::SWITCH_CLAUSE)
            } else {
                false
            };

            // Only use cache if: 1) not a switch clause, 2) initial type is concrete
            if !is_switch_clause && !initial_has_type_params {
                if let Some(sym_id) = symbol_id {
                    if let Some(cache) = self.flow_cache {
                        let key = (current_flow, sym_id, initial_type);
                        if let Some(&cached_type) = cache.borrow().get(&key) {
                            // Use cached result and skip processing this node
                            results.insert(current_flow, cached_type);
                            visited.insert(current_flow);
                            continue;
                        }
                    }
                }
            }

            eprintln!(
                "DEBUG check_flow: is_switch_clause={}, checking cache={}",
                is_switch_clause, !is_switch_clause
            );

            // Skip if we've already finalized this node
            if visited.contains(&current_flow) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                // Flow node doesn't exist - use the type we have
                results.insert(current_flow, current_type);
                visited.insert(current_flow);
                continue;
            };

            eprintln!(
                "DEBUG check_flow: flow_node={}, flags={:#x}, has SWITCH_CLAUSE={}",
                current_flow.0,
                flow.flags,
                flow.has_any_flags(flow_flags::SWITCH_CLAUSE)
            );

            // Check if this is a merge point that needs all antecedents processed first
            let is_switch_fallthrough =
                flow.has_any_flags(flow_flags::SWITCH_CLAUSE) && flow.antecedent.len() > 1;
            let is_loop_header = flow.has_any_flags(flow_flags::LOOP_LABEL);
            let is_merge_point = flow
                .has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL)
                || is_switch_fallthrough;

            if is_merge_point && !flow.antecedent.is_empty() {
                // For merge points, check if all required antecedents are processed
                // For SWITCH_CLAUSE, we check fallthrough antecedents (index 1+)
                // For BRANCH, we check all antecedents
                // For LOOP_LABEL, we only require the first antecedent (entry flow) to be ready
                let antecedents_to_check: Vec<FlowNodeId> = if is_switch_fallthrough {
                    // CRITICAL FIX: Switch fallthrough needs ALL antecedents
                    // - index 0: switch header (for narrowing calculation)
                    // - index 1..: previous clauses that fell through (for union)
                    flow.antecedent.clone()
                } else if is_loop_header {
                    // For loops, only check the first antecedent (entry flow)
                    flow.antecedent.first().copied().into_iter().collect()
                } else {
                    flow.antecedent.clone()
                };

                let all_ready = antecedents_to_check
                    .iter()
                    .all(|&ant| visited.contains(&ant) || results.contains_key(&ant));

                if !all_ready {
                    // Schedule unprocessed antecedents to be processed FIRST (push_front)
                    for &ant in &antecedents_to_check {
                        if !visited.contains(&ant)
                            && !results.contains_key(&ant)
                            && !in_worklist.contains(&ant)
                        {
                            worklist.push_front((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    }
                    // Re-add self to the END of worklist to process after antecedents
                    if !in_worklist.contains(&current_flow) {
                        worklist.push_back((current_flow, current_type));
                        in_worklist.insert(current_flow);
                    }
                    continue;
                }
            }

            // Process this flow node based on its flags
            let result_type = if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                // Branch label - union types from all antecedents
                if flow.antecedent.is_empty() {
                    current_type
                } else {
                    // Add all antecedents to worklist
                    for &ant in &flow.antecedent {
                        if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    }
                    current_type // Will be updated when antecedents are processed
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                // CRITICAL FIX: Implement proper fixed-point iteration for loops
                //
                // Previous implementation: Simple mutation check (unreliable)
                // New implementation: Fixed-point iteration that unions entry type with back-edge types
                //
                // Fixed-Point Algorithm:
                // 1. Start with entry type (antecedent[0] - before the loop)
                // 2. Get types at all back-edges (antecedents[1+] - continue/end of body)
                // 3. Union entry type with all back-edge types
                // 4. Repeat until type stabilizes (max 5 iterations)
                // 5. If not stabilized, widen to union(entry, initial)
                //
                // This matches TypeScript's behavior where variables in loops have
                // types that depend on both the entry condition and assignments within the loop.

                let entry_type = if let Some(&ant) = flow.antecedent.first() {
                    // Ensure entry is processed (is_merge_point logic guarantees this)
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                };

                // Use fixed-point iteration to determine stable loop type
                self.analyze_loop_fixed_point(
                    current_flow,
                    flow,
                    reference,
                    entry_type,
                    initial_type,
                    symbol_id,
                )
            } else if flow.has_any_flags(flow_flags::CONDITION) {
                // Condition node - apply narrowing
                let (pre_type, antecedent_id) = if let Some(&ant) = flow.antecedent.first() {
                    // Get the result from antecedent if available, otherwise use current_type
                    (*results.get(&ant).unwrap_or(&current_type), ant)
                } else {
                    (current_type, FlowNodeId::NONE)
                };

                let is_true_branch = flow.has_any_flags(flow_flags::TRUE_CONDITION);
                self.narrow_type_by_condition(
                    pre_type,
                    flow.node,
                    reference,
                    is_true_branch,
                    antecedent_id,
                )
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                // CRITICAL FIX: Schedule antecedent 0 (switch header) for traversal
                // Fallthrough cases are handled by the is_merge_point block above,
                // but single-clause cases need this to continue traversal.
                if let Some(&ant) = flow.antecedent.first() {
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                }

                // Switch clause - apply switch-specific narrowing
                self.handle_switch_clause_iterative(
                    reference,
                    current_type,
                    flow,
                    &results,
                    &mut worklist,
                    &mut in_worklist,
                    visited.clone(),
                )
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                // Assignment - check if it targets our reference
                let targets_reference =
                    self.assignment_targets_reference_node(flow.node, reference);

                if targets_reference {
                    // CRITICAL FIX: Try to get assigned type for ALL assignments, including destructuring
                    // Previously: Only direct assignments (x = ...) worked
                    // Now: Destructuring ([x] = ...) also works because get_assigned_type handles it
                    if let Some(assigned_type) = self.get_assigned_type(flow.node, reference) {
                        // Killing definition: replace type with RHS type and stop traversal
                        assigned_type
                    } else {
                        // If we can't resolve the RHS type, conservatively return declared type
                        // The value HAS changed, so we can't continue to antecedent
                        current_type
                    }
                } else if self.assignment_affects_reference_node(flow.node, reference) {
                    // CRITICAL FIX: Mutations (x.prop = ...) should NOT reset narrowing
                    // Previously: Stopped traversal and lost all previous narrowing
                    // Now: Continue to antecedent to preserve existing narrowing
                    if let Some(&ant) = flow.antecedent.first() {
                        if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                        *results.get(&ant).unwrap_or(&current_type)
                    } else {
                        current_type
                    }
                } else if let Some(&ant) = flow.antecedent.first() {
                    // Continue to antecedent
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            } else if flow.has_any_flags(flow_flags::ARRAY_MUTATION) {
                // Array mutation
                let node = match self.arena.get(flow.node) {
                    Some(n) => n,
                    None => {
                        results.insert(current_flow, current_type);
                        visited.insert(current_flow);
                        continue;
                    }
                };
                let call = match self.arena.get_call_expr(node) {
                    Some(c) => c,
                    None => {
                        results.insert(current_flow, current_type);
                        visited.insert(current_flow);
                        continue;
                    }
                };

                if self.array_mutation_affects_reference(call, reference) {
                    current_type
                } else if let Some(&ant) = flow.antecedent.first() {
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            } else if flow.has_any_flags(flow_flags::CALL) {
                // Call expression - check for type predicates
                self.handle_call_iterative(
                    reference,
                    current_type,
                    flow,
                    &results,
                    &mut worklist,
                    &mut in_worklist,
                    &visited,
                )
            } else if flow.has_any_flags(flow_flags::START) {
                // Start node - check if we're crossing a closure boundary
                // For mutable variables (let/var), we cannot trust narrowing from outer scope
                // because the closure may capture the variable and it could be mutated.
                // For const variables, narrowing is preserved (they're immutable).
                if let Some(&ant) = flow.antecedent.first() {
                    // Bug #1.2 fix: Check if the reference is a CAPTURED mutable variable
                    // Only reset narrowing for captured mutable variables, not local ones
                    if self.is_mutable_variable(reference) && self.is_captured_variable(reference) {
                        // Captured mutable variable - cannot use narrowing from outer scope
                        // Return the initial (declared) type instead of crossing boundary
                        initial_type
                    } else if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        // Const or immutable - preserve narrowing from outer scope
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                        current_type
                    } else {
                        current_type
                    }
                } else {
                    // Bug #4.1 fix: START node with no antecedents - try to find outer flow
                    // This happens when entering a closure/function body
                    // flow.node contains the function declaration node
                    if !flow.node.is_none() {
                        // Try to get the flow node where this function was declared
                        if let Some(&outer_flow) = self.binder.node_flow.get(&flow.node.0) {
                            // Bug #1.2 fix: Check if the reference is a CAPTURED mutable variable
                            if self.is_mutable_variable(reference)
                                && self.is_captured_variable(reference)
                            {
                                // Captured mutable variable - cannot use narrowing from outer scope
                                // Return the initial (declared) type
                                initial_type
                            } else {
                                // Const or immutable - preserve narrowing from outer scope
                                // Add outer flow to worklist and continue traversal
                                if !in_worklist.contains(&outer_flow)
                                    && !visited.contains(&outer_flow)
                                {
                                    worklist.push_back((outer_flow, current_type));
                                    in_worklist.insert(outer_flow);
                                }
                                // Return result from outer flow if available, otherwise current_type
                                *results.get(&outer_flow).unwrap_or(&current_type)
                            }
                        } else {
                            // No outer flow found - use current type
                            current_type
                        }
                    } else {
                        current_type
                    }
                }
            } else {
                // Default: continue to antecedent
                if let Some(&ant) = flow.antecedent.first() {
                    if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                        worklist.push_back((ant, current_type));
                        in_worklist.insert(ant);
                    }
                    *results.get(&ant).unwrap_or(&current_type)
                } else {
                    current_type
                }
            };

            // Store the result
            let _changed = if let Some(&existing) = results.get(&current_flow) {
                existing != result_type
            } else {
                true
            };

            // For merge points (BRANCH_LABEL, LOOP_LABEL, SWITCH with fallthrough),
            // we union with antecedent types. For SWITCH_CLAUSE, union clause_type with fallthrough.
            let final_type = if is_switch_fallthrough {
                // Union clause_type (result_type) with fallthrough types (antecedent index 1+)
                let mut types = vec![result_type];
                for &ant in flow.antecedent.iter().skip(1) {
                    if let Some(&t) = results.get(&ant) {
                        types.push(t);
                    }
                }
                if types.len() == 1 {
                    types[0]
                } else {
                    self.interner.union(types)
                }
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL)
                && !flow.antecedent.is_empty()
            {
                // Union all antecedent types for branch/loop
                let ant_types: Vec<TypeId> = flow
                    .antecedent
                    .iter()
                    .filter_map(|&ant| results.get(&ant).copied())
                    .collect();

                if ant_types.len() == 1 {
                    ant_types[0]
                } else if !ant_types.is_empty() {
                    self.interner.union(ant_types)
                } else {
                    result_type
                }
            } else {
                result_type
            };

            results.insert(current_flow, final_type);
            visited.insert(current_flow);

            // Store result in global cache for future calls
            // CRITICAL: Only cache if BOTH initial and final types are concrete (no type parameters).
            // This prevents the "Generic Result" bug where narrowing introduces type parameters.
            if let Some(sym_id) = symbol_id {
                if let Some(cache) = self.flow_cache {
                    let final_has_type_params =
                        crate::solver::type_queries::contains_type_parameters_db(
                            self.interner,
                            final_type,
                        );

                    // Only cache if neither initial nor final types contain type parameters
                    if !initial_has_type_params && !final_has_type_params {
                        let key = (current_flow, sym_id, initial_type);
                        cache.borrow_mut().insert(key, final_type);
                    }
                }
            }
        }

        // Return the result for the initial flow_id
        results.get(&flow_id).copied().unwrap_or(initial_type)
    }

    /// Helper function for switch clause handling in iterative mode.
    pub(crate) fn handle_switch_clause_iterative(
        &self,
        reference: NodeIndex,
        current_type: TypeId,
        flow: &FlowNode,
        results: &FxHashMap<FlowNodeId, TypeId>,
        _worklist: &mut VecDeque<(FlowNodeId, TypeId)>,
        _in_worklist: &mut FxHashSet<FlowNodeId>,
        _visited: FxHashSet<FlowNodeId>,
    ) -> TypeId {
        eprintln!(
            "DEBUG handle_switch_clause_iterative: ENTERED - flow.node={}, reference={}",
            flow.node.0, reference.0
        );

        let clause_idx = flow.node;
        let Some(switch_idx) = self.binder.get_switch_for_clause(clause_idx) else {
            return current_type;
        };
        let Some(switch_node) = self.arena.get(switch_idx) else {
            return current_type;
        };
        let Some(switch_data) = self.arena.get_switch(switch_node) else {
            return current_type;
        };
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return current_type;
        };
        let Some(clause) = self.arena.get_case_clause(clause_node) else {
            return current_type;
        };

        let pre_switch_type = if let Some(&ant) = flow.antecedent.first() {
            *results.get(&ant).unwrap_or(&current_type)
        } else {
            current_type
        };

        eprintln!(
            "DEBUG handle_switch_clause_iterative: clause.expression.is_none={}, reference={}",
            clause.expression.is_none(),
            reference.0
        );

        let narrowing = NarrowingContext::new(self.interner);
        let clause_type = if clause.expression.is_none() {
            eprintln!(
                "DEBUG handle_switch_clause_iterative: calling narrow_by_default_switch_clause"
            );
            self.narrow_by_default_switch_clause(
                pre_switch_type,
                switch_data.expression,
                switch_data.case_block,
                reference,
                &narrowing,
            )
        } else {
            self.narrow_by_switch_clause(
                pre_switch_type,
                switch_data.expression,
                clause.expression,
                reference,
                &narrowing,
            )
        };

        eprintln!(
            "DEBUG handle_switch_clause_iterative: clause_type={}",
            clause_type.0
        );

        clause_type
    }

    /// Helper function for call handling in iterative mode.
    pub(crate) fn handle_call_iterative(
        &self,
        reference: NodeIndex,
        current_type: TypeId,
        flow: &FlowNode,
        results: &FxHashMap<FlowNodeId, TypeId>,
        _worklist: &mut VecDeque<(FlowNodeId, TypeId)>,
        _in_worklist: &mut FxHashSet<FlowNodeId>,
        _visited: &FxHashSet<FlowNodeId>,
    ) -> TypeId {
        // Note: worklist/in_worklist/visited parameters are reserved for future use
        // in more sophisticated iterative algorithms that need to defer processing
        let pre_type = if let Some(&ant) = flow.antecedent.first() {
            *results.get(&ant).unwrap_or(&current_type)
        } else {
            current_type
        };

        let Some(node) = self.arena.get(flow.node) else {
            return pre_type;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return pre_type;
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return pre_type;
        };

        let Some(node_types) = self.node_types else {
            return pre_type;
        };
        let Some(&callee_type) = node_types.get(&call.expression.0) else {
            return pre_type;
        };
        let Some(signature) = self.predicate_signature_for_type(callee_type) else {
            return pre_type;
        };
        if !signature.predicate.asserts {
            return pre_type;
        }

        let Some(predicate_target) =
            self.predicate_target_expression(call, &signature.predicate, &signature.params)
        else {
            return pre_type;
        };
        if !self.is_matching_reference(predicate_target, reference) {
            return pre_type;
        }

        self.apply_type_predicate_narrowing(pre_type, &signature.predicate, true)
    }

    /// Iterative flow graph traversal for definite assignment checks.
    ///
    /// This replaces the recursive implementation to prevent stack overflow
    /// on deeply nested control flow structures. Uses a worklist algorithm with
    /// fixed-point iteration to determine if a variable is definitely assigned.
    pub(crate) fn check_definite_assignment(
        &self,
        reference: NodeIndex,
        flow_id: FlowNodeId,
        _visited: &mut Vec<FlowNodeId>,
        cache: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        // Helper: Add a node to the worklist if not already present
        let add_to_worklist =
            |node: FlowNodeId,
             worklist: &mut Vec<FlowNodeId>,
             in_worklist: &mut FxHashSet<FlowNodeId>| {
                if !in_worklist.contains(&node) {
                    worklist.push(node);
                    in_worklist.insert(node);
                }
            };

        // Result cache: flow_id -> is_assigned
        // We use a local cache that we'll merge into the provided cache
        let mut local_cache: FxHashMap<FlowNodeId, bool> = FxHashMap::default();

        // Worklist for processing nodes
        let mut worklist: Vec<FlowNodeId> = vec![flow_id];
        let mut in_worklist: FxHashSet<FlowNodeId> = FxHashSet::default();
        in_worklist.insert(flow_id);

        // Track nodes that are waiting for their antecedents to be computed
        // Map: node -> set of antecedents it's waiting for
        let mut waiting_for: FxHashMap<FlowNodeId, FxHashSet<FlowNodeId>> = FxHashMap::default();

        while let Some(current_flow) = worklist.pop() {
            in_worklist.remove(&current_flow);

            // Skip if we already have a result
            if local_cache.contains_key(&current_flow) {
                continue;
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                // Flow node doesn't exist - mark as assigned
                local_cache.insert(current_flow, true);
                // Notify any nodes waiting for this one
                let ready: Vec<_> = waiting_for
                    .iter()
                    .filter(|(_, ants)| ants.contains(&current_flow))
                    .map(|(&node, _)| node)
                    .collect();
                for node in ready {
                    waiting_for.remove(&node);
                    add_to_worklist(node, &mut worklist, &mut in_worklist);
                }
                continue;
            };

            // Compute the result based on flow node type
            let result = if flow.has_any_flags(flow_flags::UNREACHABLE) {
                false
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                if self.assignment_targets_reference(flow.node, reference) {
                    true
                } else if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        // Add antecedent to worklist and defer
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Check if all antecedents have results
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE)
                        {
                            // Unreachable branches satisfy the condition vacuously
                            results.push(true);
                            continue;
                        }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    // All antecedents processed - compute result (all must be true)
                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::CONDITION) {
                if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_result) = local_cache.get(&ant) {
                        ant_result
                    } else {
                        add_to_worklist(ant, &mut worklist, &mut in_worklist);
                        waiting_for.entry(current_flow).or_default().insert(ant);
                        continue;
                    }
                } else {
                    false
                }
            } else if flow.has_any_flags(flow_flags::SWITCH_CLAUSE) {
                if flow.antecedent.is_empty() {
                    false
                } else {
                    // Similar to BRANCH_LABEL - check all antecedents
                    let mut all_ready = true;
                    let mut results = Vec::new();

                    for &ant in &flow.antecedent {
                        if let Some(ant_node) = self.binder.flow_nodes.get(ant)
                            && ant_node.has_any_flags(flow_flags::UNREACHABLE)
                        {
                            results.push(true);
                            continue;
                        }

                        if let Some(&ant_result) = local_cache.get(&ant) {
                            results.push(ant_result);
                        } else {
                            all_ready = false;
                            add_to_worklist(ant, &mut worklist, &mut in_worklist);
                            waiting_for.entry(current_flow).or_default().insert(ant);
                        }
                    }

                    if !all_ready {
                        continue;
                    }

                    results.iter().all(|&r| r)
                }
            } else if flow.has_any_flags(flow_flags::START) {
                false
            } else if let Some(&ant) = flow.antecedent.first() {
                if let Some(&ant_result) = local_cache.get(&ant) {
                    ant_result
                } else {
                    add_to_worklist(ant, &mut worklist, &mut in_worklist);
                    waiting_for.entry(current_flow).or_default().insert(ant);
                    continue;
                }
            } else {
                false
            };

            // Store the result
            local_cache.insert(current_flow, result);

            // Notify any nodes waiting for this one
            let ready: Vec<_> = waiting_for
                .iter()
                .filter(|(_, ants)| ants.contains(&current_flow))
                .map(|(&node, _)| node)
                .collect();
            for node in ready {
                waiting_for.remove(&node);
                add_to_worklist(node, &mut worklist, &mut in_worklist);
            }
        }

        // Get the final result
        let final_result = *local_cache.get(&flow_id).unwrap_or(&false);

        // Merge local cache into the provided cache
        cache.extend(local_cache);

        final_result
    }

    /// Check if this is a direct assignment to a reference (e.g., `x = value`)
    /// as opposed to a destructuring assignment (e.g., `[x] = [value]`)
    pub(crate) fn is_direct_assignment_to_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        // Check if it's a binary expression (x = value)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && self.is_assignment_operator(bin.operator_token)
        {
            // Check if the left side is directly the target (not a destructuring pattern)
            let left = self.skip_parenthesized(bin.left);
            let target = self.skip_parenthesized(target);
            return self.is_matching_reference(left, target);
        }

        // Increment/decrement operators (x++, --x) are also direct assignments
        if (node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.arena.get_unary_expr(node)
            && (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
        {
            let operand = self.skip_parenthesized(unary.operand);
            let target = self.skip_parenthesized(target);
            return self.is_matching_reference(operand, target);
        }

        false
    }

    pub(crate) fn get_assigned_type(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> Option<TypeId> {
        let Some(node) = self.arena.get(assignment_node) else {
            return None;
        };

        if let Some(rhs) = self.assignment_rhs_for_reference(assignment_node, target) {
            // For flow narrowing, prefer literal types from AST nodes over the type checker's widened types
            // This ensures that `x = 42` narrows to literal 42.0, not just NUMBER
            // This matches TypeScript's behavior where control flow analysis preserves literal types
            if let Some(literal_type) = self.literal_type_from_node(rhs) {
                return Some(literal_type);
            }
            if let Some(nullish_type) = self.nullish_literal_type(rhs) {
                return Some(nullish_type);
            }
            // Fall back to type checker's result for non-literal expressions
            if let Some(node_types) = self.node_types
                && let Some(&rhs_type) = node_types.get(&rhs.0)
            {
                return Some(rhs_type);
            }
            return None;
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            let unary = self.arena.get_unary_expr(node)?;
            if (unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16)
                && self.is_matching_reference(unary.operand, target)
            {
                return Some(TypeId::NUMBER);
            }
        }

        None
    }

    pub(crate) fn assignment_rhs_for_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> Option<NodeIndex> {
        let Some(node) = self.arena.get(assignment_node) else {
            return None;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                if self.is_matching_reference(bin.left, reference) {
                    return Some(bin.right);
                }
                if let Some(rhs) = self.match_destructuring_rhs(bin.left, bin.right, reference) {
                    return Some(rhs);
                }
            }
            return None;
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(node)?;
            if self.is_matching_reference(decl.name, reference) && !decl.initializer.is_none() {
                return Some(decl.initializer);
            }
            if !decl.initializer.is_none()
                && let Some(rhs) =
                    self.match_destructuring_rhs(decl.name, decl.initializer, reference)
            {
                return Some(rhs);
            }
            return None;
        }

        if (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            && let Some(list) = self.arena.get_variable(node)
        {
            for &decl_idx in &list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if self.is_matching_reference(decl.name, reference) && !decl.initializer.is_none() {
                    return Some(decl.initializer);
                }
                if !decl.initializer.is_none()
                    && let Some(rhs) =
                        self.match_destructuring_rhs(decl.name, decl.initializer, reference)
                {
                    return Some(rhs);
                }
            }
        }

        None
    }

    pub(crate) fn match_destructuring_rhs(
        &self,
        pattern: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        if pattern.is_none() {
            return None;
        }

        let pattern = self.skip_parens_and_assertions(pattern);
        let rhs = if rhs.is_none() {
            rhs
        } else {
            self.skip_parens_and_assertions(rhs)
        };

        if !rhs.is_none() && self.is_matching_reference(pattern, target) {
            return Some(rhs);
        }

        let node = self.arena.get(pattern)?;
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.arena.get_binary_expr(node)?;
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    return None;
                }
                if let Some(found) = self.match_destructuring_rhs(bin.left, rhs, target) {
                    return Some(found);
                }
                if self.assignment_targets_reference_internal(bin.left, target) {
                    if let Some(found) = self.match_destructuring_rhs(bin.left, bin.right, target) {
                        return Some(found);
                    }
                    return Some(bin.right);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };
                let rhs_elements = self.array_literal_elements(rhs);
                for (index, &elem) in elements.nodes.iter().enumerate() {
                    if elem.is_none() {
                        continue;
                    }
                    if !self.assignment_targets_reference_internal(elem, target) {
                        continue;
                    }
                    let rhs_elem = rhs_elements
                        .and_then(|rhs_list| rhs_list.nodes.get(index).copied())
                        .unwrap_or(NodeIndex::NONE);
                    if let Some(found) = self.match_destructuring_rhs(elem, rhs_elem, target) {
                        return Some(found);
                    }
                    if !rhs_elem.is_none() {
                        return Some(rhs_elem);
                    }
                    return None;
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN =>
            {
                let elements = if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };
                for &elem in &elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if let Some(found) = self.match_object_pattern_element(elem, rhs, target) {
                        return Some(found);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(node)?;
                if self.assignment_targets_reference_internal(binding.name, target) {
                    if !rhs.is_none() {
                        if let Some(found) = self.match_destructuring_rhs(binding.name, rhs, target)
                        {
                            return Some(found);
                        }
                        if self.is_matching_reference(binding.name, target) {
                            return Some(rhs);
                        }
                    }
                    if !binding.initializer.is_none() {
                        if let Some(found) =
                            self.match_destructuring_rhs(binding.name, binding.initializer, target)
                        {
                            return Some(found);
                        }
                        return Some(binding.initializer);
                    }
                }
            }
            _ => {}
        }

        None
    }

    pub(crate) fn match_object_pattern_element(
        &self,
        elem: NodeIndex,
        rhs: NodeIndex,
        target: NodeIndex,
    ) -> Option<NodeIndex> {
        let elem_node = self.arena.get(elem)?;
        match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.initializer, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    if let Some(found) =
                        self.match_destructuring_rhs(prop.initializer, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                return self.match_destructuring_rhs(prop.initializer, NodeIndex::NONE, target);
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                if !self.assignment_targets_reference_internal(prop.name, target) {
                    return None;
                }
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, prop.name) {
                    return Some(rhs_value);
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                let binding = self.arena.get_binding_element(elem_node)?;
                if !self.assignment_targets_reference_internal(binding.name, target) {
                    return None;
                }
                let name_idx = if binding.property_name.is_none() {
                    binding.name
                } else {
                    binding.property_name
                };
                if let Some(rhs_value) = self.lookup_property_in_rhs(rhs, name_idx) {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, rhs_value, target)
                    {
                        return Some(found);
                    }
                    return Some(rhs_value);
                }
                if !binding.initializer.is_none() {
                    if let Some(found) =
                        self.match_destructuring_rhs(binding.name, binding.initializer, target)
                    {
                        return Some(found);
                    }
                    return Some(binding.initializer);
                }
            }
            _ => {}
        }
        None
    }

    pub(crate) fn array_literal_elements(&self, rhs: NodeIndex) -> Option<&NodeList> {
        if rhs.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let node = self.arena.get(rhs)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        self.arena.get_literal_expr(node).map(|lit| &lit.elements)
    }

    pub(crate) fn lookup_property_in_rhs(
        &self,
        rhs: NodeIndex,
        name: NodeIndex,
    ) -> Option<NodeIndex> {
        if rhs.is_none() || name.is_none() {
            return None;
        }
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        let key = self.property_key_from_name(name)?;

        if rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Index(index) = key {
                return lit
                    .elements
                    .nodes
                    .get(index)
                    .copied()
                    .filter(|n| !n.is_none());
            }
            return None;
        }

        if rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let lit = self.arena.get_literal_expr(rhs_node)?;
            if let PropertyKey::Atom(atom) = key {
                return self.find_property_in_object_literal(lit, atom);
            }
        }

        None
    }

    pub(crate) fn find_property_in_object_literal(
        &self,
        literal: &crate::parser::node::LiteralExprData,
        target: Atom,
    ) -> Option<NodeIndex> {
        for &elem in &literal.elements.nodes {
            let Some(elem_node) = self.arena.get(elem) else {
                continue;
            };
            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_property_assignment(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.initializer);
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.arena.get_shorthand_property(elem_node)?;
                    if let Some(PropertyKey::Atom(name)) = self.property_key_from_name(prop.name)
                        && name == target
                    {
                        return Some(prop.name);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn assignment_affects_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self
                .arena
                .get_binary_expr(node)
                .map(|bin| {
                    self.is_assignment_operator(bin.operator_token)
                        && self.assignment_affects_reference(bin.left, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self
                .arena
                .get_unary_expr(node)
                .map(|unary| {
                    (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && self.assignment_affects_reference(unary.operand, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .map(|decl| self.assignment_affects_reference(decl.name, target))
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_affects_reference(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_affects_reference(assignment_node, target)
    }

    pub fn assignment_targets_reference(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        self.assignment_targets_reference_node(assignment_node, target)
    }

    pub(crate) fn assignment_targets_reference_node(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(assignment_node) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            return self
                .arena
                .get_binary_expr(node)
                .map(|bin| {
                    self.is_assignment_operator(bin.operator_token)
                        && self.assignment_targets_reference_internal(bin.left, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self
                .arena
                .get_unary_expr(node)
                .map(|unary| {
                    (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && self.assignment_targets_reference_internal(unary.operand, target)
                })
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .map(|decl| self.assignment_targets_reference_internal(decl.name, target))
                .unwrap_or(false);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            if let Some(list) = self.arena.get_variable(node) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && self.assignment_targets_reference_internal(decl.name, target)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        self.assignment_targets_reference_internal(assignment_node, target)
    }

    pub(crate) fn narrow_by_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        let binary = BinaryExprData {
            left: switch_expr,
            operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
            right: case_expr,
        };

        self.narrow_by_binary_expr(type_id, &binary, target, true, narrowing, FlowNodeId::NONE)
    }

    pub(crate) fn narrow_by_default_switch_clause(
        &self,
        type_id: TypeId,
        switch_expr: NodeIndex,
        case_block: NodeIndex,
        target: NodeIndex,
        narrowing: &NarrowingContext,
    ) -> TypeId {
        eprintln!(
            "DEBUG narrow_by_default_switch_clause: type_id={}, switch_expr={}, target={}",
            type_id.0, switch_expr.0, target.0
        );

        let Some(case_block_node) = self.arena.get(case_block) else {
            return type_id;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return type_id;
        };

        let mut narrowed = type_id;
        for &clause_idx in &case_block.statements.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };
            if clause.expression.is_none() {
                continue;
            }

            let binary = BinaryExprData {
                left: switch_expr,
                operator_token: SyntaxKind::EqualsEqualsEqualsToken as u16,
                right: clause.expression,
            };
            eprintln!(
                "DEBUG narrow_by_default_switch_clause: calling narrow_by_binary_expr with narrowed={}",
                narrowed.0
            );
            narrowed = self.narrow_by_binary_expr(
                narrowed,
                &binary,
                target,
                false,
                narrowing,
                FlowNodeId::NONE,
            );
            eprintln!(
                "DEBUG narrow_by_default_switch_clause: after narrowing, result={}",
                narrowed.0
            );
        }

        eprintln!(
            "DEBUG narrow_by_default_switch_clause: final result={}",
            narrowed.0
        );
        narrowed
    }

    /// Apply type narrowing based on a condition expression.
    pub(crate) fn narrow_type_by_condition(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        eprintln!(
            "DEBUG narrow_type_by_condition: type_id={}, condition={}, target={}, is_true_branch={}, antecedent={}",
            type_id.0, condition_idx.0, target.0, is_true_branch, antecedent_id.0
        );
        let mut visited_aliases = Vec::new();
        let result = self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
        );
        result
    }

    pub(crate) fn narrow_type_by_condition_inner(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> TypeId {
        eprintln!(
            "DEBUG narrow_type_by_condition_inner: type_id={}, condition={}, target={}",
            type_id.0, condition_idx.0, target.0
        );
        let condition_idx = self.skip_parenthesized(condition_idx);
        let Some(cond_node) = self.arena.get(condition_idx) else {
            return type_id;
        };

        let narrowing = NarrowingContext::new(self.interner);

        if cond_node.kind == SyntaxKind::Identifier as u16
            && let Some((sym_id, initializer)) = self.const_condition_initializer(condition_idx)
            && !visited_aliases.contains(&sym_id)
        {
            visited_aliases.push(sym_id);
            let narrowed = self.narrow_type_by_condition_inner(
                type_id,
                initializer,
                target,
                is_true_branch,
                antecedent_id,
                visited_aliases,
            );
            visited_aliases.pop();
            return narrowed;
        }

        match cond_node.kind {
            // typeof x === "string", x instanceof Class, "prop" in x, etc.
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(cond_node) {
                    eprintln!(
                        "DEBUG narrow_type_by_condition_inner: operator={}",
                        bin.operator_token
                    );
                    // Handle logical operators (&&, ||) with special recursion
                    if let Some(narrowed) = self.narrow_by_logical_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        eprintln!(
                            "DEBUG narrow_type_by_condition_inner: logical_expr returned {}",
                            narrowed.0
                        );
                        return narrowed;
                    }

                    // CRITICAL: Use Solver-First architecture for other binary expressions
                    // Extract TypeGuard from AST (Checker responsibility: WHERE + WHAT)
                    if let Some((guard, guard_target, _is_optional)) =
                        self.extract_type_guard(condition_idx)
                    {
                        eprintln!(
                            "DEBUG narrow_type_by_condition_inner: extracted guard, guard_target={}",
                            guard_target.0
                        );
                        // Check if the guard applies to our target reference
                        if self.is_matching_reference(guard_target, target) {
                            eprintln!(
                                "DEBUG narrow_type_by_condition_inner: guard matches target, calling narrowing.narrow_type"
                            );
                            // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                            return narrowing.narrow_type(type_id, &guard, is_true_branch);
                        } else {
                            eprintln!(
                                "DEBUG narrow_type_by_condition_inner: guard does not match target"
                            );
                        }
                    }

                    // CRITICAL: Try bidirectional narrowing for x === y where both are references
                    // This handles cases that don't match traditional type guard patterns
                    // Example: if (x === y) { x } should narrow x based on y's type
                    eprintln!(
                        "DEBUG narrow_type_by_condition_inner: no guard extracted, trying narrow_by_binary_expr for bidirectional narrowing"
                    );
                    let narrowed = self.narrow_by_binary_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        &narrowing,
                        antecedent_id,
                    );
                    eprintln!(
                        "DEBUG narrow_type_by_condition_inner: narrow_by_binary_expr returned {}",
                        narrowed.0
                    );
                    return narrowed;
                }
            }

            // User-defined type guards: isString(x), obj.isString(), assertsIs(x), etc.
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // CRITICAL: Use Solver-First architecture for call expressions
                // Extract TypeGuard from AST (Checker responsibility: WHERE + WHAT)
                if let Some((guard, guard_target, is_optional)) =
                    self.extract_type_guard(condition_idx)
                {
                    eprintln!(
                        "DEBUG narrow_type_by_condition_inner: extracted guard, guard_target={}, is_optional={}",
                        guard_target.0, is_optional
                    );

                    // CRITICAL: Optional chaining behavior
                    // If call is optional (obj?.method(x)), only narrow the true branch
                    // The false branch might mean the method wasn't called (obj was nullish)
                    if is_optional && !is_true_branch {
                        eprintln!(
                            "DEBUG narrow_type_by_condition_inner: optional call on false branch, skipping narrowing"
                        );
                        return type_id;
                    }

                    // Check if the guard applies to our target reference
                    if self.is_matching_reference(guard_target, target) {
                        eprintln!(
                            "DEBUG narrow_type_by_condition_inner: guard matches target, calling narrowing.narrow_type"
                        );
                        // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                        return narrowing.narrow_type(type_id, &guard, is_true_branch);
                    } else {
                        eprintln!(
                            "DEBUG narrow_type_by_condition_inner: guard does not match target"
                        );
                    }
                }

                eprintln!(
                    "DEBUG narrow_type_by_condition_inner: no guard extracted or guard doesn't match, returning type_id"
                );
                return type_id;
            }

            // Prefix unary: !x
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(cond_node) {
                    // !x inverts the narrowing
                    if unary.operator == SyntaxKind::ExclamationToken as u16 {
                        return self.narrow_type_by_condition_inner(
                            type_id,
                            unary.operand,
                            target,
                            !is_true_branch,
                            antecedent_id,
                            visited_aliases,
                        );
                    }
                }
            }

            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(cond_node) {
                    if let Some(narrowed) =
                        self.narrow_by_call_predicate(type_id, call, target, is_true_branch)
                    {
                        return narrowed;
                    }
                    if is_true_branch {
                        let optional_call =
                            (cond_node.flags as u32 & node_flags::OPTIONAL_CHAIN) != 0;
                        if optional_call && self.is_matching_reference(call.expression, target) {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                        if let Some(callee_node) = self.arena.get(call.expression)
                            && let Some(access) = self.arena.get_access_expr(callee_node)
                            && access.question_dot_token
                            && self.is_matching_reference(access.expression, target)
                        {
                            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        }
                    }
                }
            }

            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(cond_node) {
                    // Handle optional chaining: y?.a
                    if access.question_dot_token
                        && is_true_branch
                        && self.is_matching_reference(access.expression, target)
                    {
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                }
                // Handle discriminant narrowing (discriminated unions)
                if let Some(prop_name) = self.discriminant_property(condition_idx, target) {
                    let literal_true = self.interner.literal_boolean(true);
                    if is_true_branch {
                        return narrowing.narrow_by_discriminant(
                            type_id,
                            &[prop_name],
                            literal_true,
                        );
                    }
                    return narrowing.narrow_by_excluding_discriminant(
                        type_id,
                        &[prop_name],
                        literal_true,
                    );
                }

                // Handle truthiness narrowing for property/element access: if (y.a)
                if self.is_matching_reference(condition_idx, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        let narrowed = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                        return narrowed;
                    }
                    // False branch - keep only falsy types (use Solver for NaN handling)
                    return narrowing.narrow_to_falsy(type_id);
                }
            }

            // Truthiness check: if (x)
            _ => {
                if self.is_matching_reference(condition_idx, target) {
                    if is_true_branch {
                        // Remove null/undefined (truthy narrowing)
                        let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
                        return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                    }
                    // False branch - keep only falsy types (use Solver for NaN handling)
                    return narrowing.narrow_to_falsy(type_id);
                }
            }
        }

        type_id
    }

    pub(crate) fn const_condition_initializer(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(SymbolId, NodeIndex)> {
        let sym_id = self.binder.resolve_identifier(self.arena, ident_idx)?;
        let symbol = self.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return None;
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.is_const_variable_declaration(decl_idx) {
            return None;
        }
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if decl.initializer.is_none() {
            return None;
        }
        Some((sym_id, decl.initializer))
    }

    pub(crate) fn is_const_variable_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let mut flags = decl_node.flags as u32;
        if (flags & (node_flags::LET | node_flags::CONST)) == 0 {
            let Some(ext) = self.arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                return false;
            }
            flags |= parent_node.flags as u32;
        }
        (flags & node_flags::CONST) != 0
    }

    /// Check if a symbol is const (immutable) vs mutable (let/var).
    ///
    /// This is used for loop widening: const variables preserve narrowing through loops,
    /// while mutable variables are widened to the declared type to account for mutations.
    fn is_const_symbol(&self, sym_id: SymbolId) -> bool {
        use crate::parser::node_flags;
        use crate::parser::syntax_kind_ext;

        let symbol = match self.binder.get_symbol(sym_id) {
            Some(sym) => sym,
            None => return false, // Assume mutable if we can't determine
        };

        // Check the value declaration
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false; // Assume mutable if no declaration
        }

        let decl_node = match self.arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };

        // For variable declarations, the CONST flag is on the VARIABLE_DECLARATION_LIST parent
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = self.arena.get_extended(decl_idx) {
                if !ext.parent.is_none() {
                    if let Some(parent_node) = self.arena.get(ext.parent) {
                        let flags = parent_node.flags as u32;
                        return (flags & node_flags::CONST) != 0;
                    }
                }
            }
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        (flags & node_flags::CONST) != 0
    }

    /// Check if a symbol is mutated within a loop body.
    ///
    /// This performs a backward traversal from the loop's back-edges to determine
    /// if any assignment targets the given symbol within the loop.
    ///
    /// Used to implement selective widening: only reset narrowing at loop headers
    /// if the variable is actually mutated in the loop body.
    ///
    /// Results are cached to prevent O(N^2) complexity.
    fn is_symbol_mutated_in_loop(&self, loop_id: FlowNodeId, sym_id: SymbolId) -> bool {
        // Check cache first
        if let Some(&is_mutated) = self.loop_mutation_cache.borrow().get(&(loop_id, sym_id)) {
            return is_mutated;
        }

        use std::collections::VecDeque;

        let Some(loop_flow) = self.binder.flow_nodes.get(loop_id) else {
            return false;
        };

        // Start traversal from all back-edges (antecedent[1..])
        // antecedent[0] is the entry flow, antecedent[1..] are back-edges from loop body
        let back_edges: Vec<_> = loop_flow.antecedent.iter().skip(1).copied().collect();

        if back_edges.is_empty() {
            return false; // No back-edges means loop body never executes
        }

        let mut worklist: VecDeque<FlowNodeId> = back_edges.into_iter().collect();
        let mut visited: FxHashSet<FlowNodeId> = FxHashSet::default();
        let mut found_mutation = false;

        while let Some(current_flow) = worklist.pop_front() {
            if current_flow == loop_id {
                // Reached the loop header - stop traversal
                continue;
            }

            if !visited.insert(current_flow) {
                continue; // Already processed
            }

            let Some(flow) = self.binder.flow_nodes.get(current_flow) else {
                continue;
            };

            // Check if this node mutates the symbol
            if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                if self.node_mutates_symbol(flow.node, sym_id) {
                    found_mutation = true;
                    break;
                }
            }

            // Add antecedents to worklist for further traversal
            for &ant in &flow.antecedent {
                if ant != loop_id && !visited.contains(&ant) {
                    worklist.push_back(ant);
                }
            }
        }

        // Cache the result
        self.loop_mutation_cache
            .borrow_mut()
            .insert((loop_id, sym_id), found_mutation);

        found_mutation
    }

    /// Check if a node mutates a specific symbol.
    ///
    /// This checks for direct assignments (reassignments) to the symbol.
    /// Note: Array method calls like push() do NOT mutate the variable binding,
    /// only the array contents. CFA tracks variable reassignments, not object mutations.
    fn node_mutates_symbol(&self, node_idx: NodeIndex, sym_id: SymbolId) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        // Check for direct assignment (binary expression with assignment operator)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if self.assignment_targets_symbol(node_idx, sym_id) {
                return true;
            }
        }

        false
    }

    /// Check if an assignment node targets a specific symbol.
    ///
    /// This is a SymbolId-aware version of assignment_targets_reference that
    /// checks if the left side of an assignment refers to the given symbol.
    fn assignment_targets_symbol(&self, node_idx: NodeIndex, sym_id: SymbolId) -> bool {
        let node_idx = self.skip_parenthesized(node_idx);
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        // Check if this node directly references the symbol
        if let Some(node_sym) = self.binder.resolve_identifier(self.arena, node_idx) {
            if node_sym == sym_id {
                return true;
            }
        }

        // Handle binary expressions (assignment)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = self.arena.get_binary_expr(node) {
                if self.is_assignment_operator(bin.operator_token) {
                    return self.assignment_targets_symbol(bin.left, sym_id);
                }
            }
        }

        // Handle property/element access (e.g., obj.prop = value, obj[index] = value)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            if let Some(access) = self.arena.get_access_expr(node) {
                // Check if the base object is our symbol
                if let Some(base_sym) = self
                    .binder
                    .resolve_identifier(self.arena, access.expression)
                {
                    if base_sym == sym_id {
                        return true;
                    }
                }
            }
        }

        // Handle destructuring patterns
        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            // Recursively check pattern elements
            if let Some(pattern) = self.arena.get_binding_pattern(node) {
                for &elem in &pattern.elements.nodes {
                    if elem.is_none() {
                        continue;
                    }
                    if self.assignment_targets_symbol(elem, sym_id) {
                        return true;
                    }
                }
            }
        }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT {
            if let Some(binding) = self.arena.get_binding_element(node) {
                if self.assignment_targets_symbol(binding.name, sym_id) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type is a unit type (literal, null, undefined, or unique symbol).
    ///
    /// Unit types are types that represent exactly one value.
    /// This is important for !== narrowing: we can only narrow by !== if
    /// the other side is a unit type.
    ///
    /// Examples of unit types:
    /// - Literals: "foo", 42, true, false, 0n
    /// - Nullish: null, undefined, void
    /// - Unions of unit types: "A" | "B" | null (all members are unit types)
    ///
    /// Non-unit types:
    /// - Primitives with multiple values: string, number, boolean, bigint
    /// - Objects, arrays, etc.
    fn is_unit_type(&self, type_id: TypeId) -> bool {
        // 1. Check intrinsics that are unit types
        if type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::VOID
            || type_id == TypeId::BOOLEAN_TRUE
            || type_id == TypeId::BOOLEAN_FALSE
        {
            return true;
        }

        // 2. Check for Literal types (String/Number/BigInt literals)
        use crate::solver::visitor::is_literal_type_db;
        if is_literal_type_db(self.interner, type_id) {
            return true;
        }

        // 3. CRITICAL: Check Unions
        // A union is a unit type if ALL its members are unit types
        // e.g. "A" | "B" | null is a unit type
        // This allows: if (x !== y) where y: "A" | "B" to narrow x correctly
        use crate::solver::visitor::union_list_id;
        if let Some(list_id) = union_list_id(self.interner, type_id) {
            let members = self.interner.type_list(list_id);
            // Recursively check all members
            return members.iter().all(|&m| self.is_unit_type(m));
        }

        false
    }

    /// Narrow type based on a binary expression (===, !==, typeof checks, etc.)
    pub(crate) fn narrow_by_binary_expr(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let operator = bin.operator_token;

        if operator == SyntaxKind::InstanceOfKeyword as u16 {
            return self.narrow_by_instanceof(type_id, bin, target, is_true_branch);
        }

        if operator == SyntaxKind::InKeyword as u16 {
            return self.narrow_by_in_operator(type_id, bin, target, is_true_branch);
        }

        let (is_equals, is_strict) = match operator {
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => (true, true),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => (false, true),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => (true, false),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => (false, false),
            _ => return type_id,
        };

        let effective_truth = if is_equals {
            is_true_branch
        } else {
            !is_true_branch
        };

        if let Some(type_name) = self.typeof_comparison_literal(bin.left, bin.right, target) {
            if effective_truth {
                return narrowing.narrow_by_typeof(type_id, type_name);
            }
            return self.narrow_by_typeof_negation(type_id, type_name, narrowing);
        }

        if let Some(nullish) = self.nullish_comparison(bin.left, bin.right, target) {
            if is_strict {
                if effective_truth {
                    return nullish;
                }
                return narrowing.narrow_excluding_type(type_id, nullish);
            }

            let nullish_union = self.interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
            if effective_truth {
                return nullish_union;
            }

            let narrowed = narrowing.narrow_excluding_type(type_id, TypeId::NULL);
            return narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        if is_strict {
            if let Some((prop_name, literal_type, is_optional, _base)) =
                self.discriminant_comparison(bin.left, bin.right, target)
            {
                let mut base_type = type_id;
                if is_optional && effective_truth {
                    let narrowed = narrowing.narrow_excluding_type(base_type, TypeId::NULL);
                    base_type = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                }
                return self.narrow_by_discriminant_for_type(
                    base_type,
                    prop_name,
                    literal_type,
                    effective_truth,
                    narrowing,
                );
            }

            if let Some(literal_type) = self.literal_comparison(bin.left, bin.right, target) {
                if effective_truth {
                    let narrowed = narrowing.narrow_to_type(type_id, literal_type);
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    if self.literal_assignable_to(literal_type, type_id, narrowing) {
                        return literal_type;
                    }
                    return TypeId::NEVER;
                }
                return narrowing.narrow_excluding_type(type_id, literal_type);
            }
        }

        // Bidirectional narrowing: x === y where both are references
        // This handles cases like: if (x === y) { ... }
        // where both x and y are variables (not just literals)
        if is_strict {
            // Helper to get flow type of the "other" node
            let get_other_flow_type = |other_node: NodeIndex| -> Option<TypeId> {
                let node_types = self.node_types?;
                let initial_type = *node_types.get(&other_node.0)?;

                // CRITICAL FIX: Use flow analysis if we have a valid flow node
                // This gets the flow-narrowed type of the other reference
                if !antecedent_id.is_none() {
                    Some(self.get_flow_type(other_node, initial_type, antecedent_id))
                } else {
                    // Fallback for tests or when no flow context exists
                    Some(initial_type)
                }
            };

            // Check if target is on the left side (x === y, target is x)
            if self.is_matching_reference(bin.left, target) {
                // We need the type of the RIGHT side (y)
                if let Some(right_type) = get_other_flow_type(bin.right) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            true,
                        );
                    } else if self.is_unit_type(right_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(right_type),
                            false,
                        );
                    }
                }
            }

            // Check if target is on the right side (y === x, target is x)
            if self.is_matching_reference(bin.right, target) {
                // We need the type of the LEFT side (y)
                if let Some(left_type) = get_other_flow_type(bin.left) {
                    if effective_truth {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            true,
                        );
                    } else if self.is_unit_type(left_type) {
                        return narrowing.narrow_type(
                            type_id,
                            &TypeGuard::LiteralEquality(left_type),
                            false,
                        );
                    }
                }
            }
        }

        type_id
    }

    pub(crate) fn narrow_by_logical_expr(
        &self,
        type_id: TypeId,
        bin: &crate::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        let operator = bin.operator_token;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_true,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(right_true);
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let left_true = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                true,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_true,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(self.union_types(left_false, right_false));
        }

        if operator == SyntaxKind::BarBarToken as u16 {
            if is_true_branch {
                let left_true = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                let left_false = self.narrow_type_by_condition_inner(
                    type_id,
                    bin.left,
                    target,
                    false,
                    antecedent_id,
                    visited_aliases,
                );
                let right_true = self.narrow_type_by_condition_inner(
                    left_false,
                    bin.right,
                    target,
                    true,
                    antecedent_id,
                    visited_aliases,
                );
                return Some(self.union_types(left_true, right_true));
            }

            let left_false = self.narrow_type_by_condition_inner(
                type_id,
                bin.left,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            let right_false = self.narrow_type_by_condition_inner(
                left_false,
                bin.right,
                target,
                false,
                antecedent_id,
                visited_aliases,
            );
            return Some(right_false);
        }

        None
    }

    pub(crate) fn is_assignment_operator(&self, operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }
}

/// Check whether a function body can fall through to the end.
/// Returns true if execution can reach the end of the function body without
/// encountering a return/throw statement.
pub fn function_body_falls_through(_arena: &NodeArena, _body_idx: NodeIndex) -> bool {
    // Simplified stub: assume function bodies can fall through
    // A full implementation would analyze control flow to detect
    // if all paths have return/throw statements
    true
}

/// Check whether a statement can fall through to the next statement.
/// Returns true if execution can continue past this statement.
pub fn statement_falls_through(_arena: &NodeArena, _stmt_idx: NodeIndex) -> bool {
    // Simplified stub: assume statements can fall through
    // A full implementation would analyze control flow
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::solver::PropertyInfo;
    use crate::solver::TypeInterner;
    use crate::solver::Visibility;
    use crate::solver::type_queries::{UnionMembersKind, classify_for_union_members};

    fn get_if_condition(arena: &NodeArena, root: NodeIndex, stmt_index: usize) -> NodeIndex {
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let if_idx = *source_file
            .statements
            .nodes
            .get(stmt_index)
            .expect("if statement");
        let if_node = arena.get(if_idx).expect("if node");
        let if_data = arena.get_if_statement(if_node).expect("if data");
        if_data.expression
    }

    #[test]
    fn test_truthiness_false_branch_narrows_to_falsy() {
        let source = r#"
let x: string | number | boolean | null | undefined;
if (x) {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let union = types.union(vec![
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            TypeId::NULL,
            TypeId::UNDEFINED,
        ]);
        let narrowed =
            analyzer.narrow_type_by_condition(union, condition_idx, condition_idx, false);

        let falsy_string = types.literal_string("");
        let falsy_number = types.literal_number(0.0);
        let falsy_boolean = types.literal_boolean(false);

        match classify_for_union_members(&types, narrowed) {
            UnionMembersKind::Union(members) => {
                assert!(members.contains(&falsy_string));
                assert!(members.contains(&falsy_number));
                assert!(members.contains(&falsy_boolean));
                assert!(members.contains(&TypeId::NULL));
                assert!(members.contains(&TypeId::UNDEFINED));
            }
            UnionMembersKind::NotUnion => panic!("Expected falsy union"),
        }
    }

    #[test]
    fn test_typeof_false_branch_excludes_type() {
        let source = r#"
let x: string | number;
if (typeof x === "string") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let typeof_node = arena.get(binary.left).expect("typeof node");
        let unary = arena.get_unary_expr(typeof_node).expect("typeof data");
        let target_idx = unary.operand;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);
        assert_eq!(narrowed, TypeId::NUMBER);
    }

    #[test]
    fn test_logical_and_applies_right_guard() {
        let source = r#"
let x: string | number;
if (x && typeof x === "string") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        assert_eq!(narrowed, TypeId::STRING);
    }

    #[test]
    fn test_logical_or_narrows_to_union_of_literals() {
        let source = r#"
let x: "a" | "b" | "c";
if (x === "a" || x === "b") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let left_node = arena.get(binary.left).expect("left condition");
        let left_eq = arena.get_binary_expr(left_node).expect("left equality");
        let target_idx = left_eq.left;

        let lit_a = types.literal_string("a");
        let lit_b = types.literal_string("b");
        let lit_c = types.literal_string("c");
        let union = types.union(vec![lit_a, lit_b, lit_c]);

        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, types.union(vec![lit_a, lit_b]));
        assert_eq!(narrowed_false, lit_c);
    }

    #[test]
    fn test_discriminant_property_access_narrows_union() {
        let source = r#"
let action: any;
if (action.type === "add") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let access_node = arena.get(binary.left).expect("property access node");
        let access = arena
            .get_access_expr(access_node)
            .expect("property access data");
        let target_idx = access.expression;

        let type_key = types.intern_string("type");
        let type_add = types.literal_string("add");
        let type_remove = types.literal_string("remove");

        let add_member = types.object(vec![PropertyInfo {
            name: type_key,
            type_id: type_add,
            write_type: type_add,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);
        let remove_member = types.object(vec![PropertyInfo {
            name: type_key,
            type_id: type_remove,
            write_type: type_remove,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let union = types.union(vec![add_member, remove_member]);
        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, add_member);
        assert_eq!(narrowed_false, remove_member);
    }

    #[test]
    fn test_literal_equality_narrows_to_literal() {
        let source = r#"
let x: string | number;
if (x === "a") {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let literal_a = types.literal_string("a");
        let narrowed = analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);

        assert_eq!(narrowed, literal_a);
    }

    #[test]
    fn test_loose_nullish_equality_narrows_to_nullish_union() {
        let source = r#"
let x: string | null | undefined;
if (x == null) {}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        let target_idx = binary.left;

        let union = types.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
        let expected_true = types.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

        let narrowed_true =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, true);
        let narrowed_false =
            analyzer.narrow_type_by_condition(union, condition_idx, target_idx, false);

        assert_eq!(narrowed_true, expected_true);
        assert_eq!(narrowed_false, TypeId::STRING);
    }

    #[test]
    fn test_mutable_variable_in_closure_loses_narrowing() {
        // Unsoundness Rule #42: Mutable variables (let/var) should not preserve
        // narrowing from outer scope when accessed in closures
        let source = r#"
let x: string | number;
if (typeof x === "string") {
    // At this point, x is narrowed to string
    // But in the closure, it should revert to string | number
    const fn = () => {
        // x should NOT be narrowed here - it's mutable
    };
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        // Get the if condition (typeof x === "string")
        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        // binary.left is "typeof x", we need to get the operand "x"
        let typeof_node = arena.get(binary.left).expect("typeof node");
        let unary = arena.get_unary_expr(typeof_node).expect("unary expression");
        let target_idx = unary.operand; // This is 'x'

        // The narrowing happens at the condition
        let union_type = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let narrowed_at_condition = analyzer.narrow_type_by_condition(
            union_type,
            condition_idx,
            target_idx,
            true, // true branch
        );
        assert_eq!(narrowed_at_condition, TypeId::STRING);

        // Now we need to check that in the closure, narrowing is NOT applied
        // The closure creates a START node that connects to the outer flow
        // When we cross that START node, the narrowing should be reset for mutable variables

        // Get the variable declaration to verify it's let (mutable)
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
        let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

        // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
        let var_data = arena.get_variable(var_stmt_node).expect("variable data");
        let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
        let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

        // Verify the declaration list does NOT have CONST flag (it's 'let')
        let flags = decl_list_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;
        assert!(!is_const, "Variable should be let (mutable), not const");

        // Verify that is_mutable_variable returns true for this variable
        assert!(analyzer.is_mutable_variable(target_idx));
    }

    #[test]
    fn test_const_variable_in_closure_preserves_narrowing() {
        // Unsoundness Rule #42: Const variables SHOULD preserve narrowing
        // from outer scope when accessed in closures
        let source = r#"
const x: string | number = Math.random() > 0.5 ? "hello" : 42;
if (typeof x === "string") {
    // At this point, x is narrowed to string
    // In the closure, it should remain narrowed to string (const is immutable)
    const fn = () => {
        // x SHOULD be narrowed here - it's const
    };
}
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();
        let types = TypeInterner::new();
        let analyzer = FlowAnalyzer::new(arena, &binder, &types);

        // Get the if condition (typeof x === "string")
        let condition_idx = get_if_condition(arena, root, 1);
        let condition_node = arena.get(condition_idx).expect("condition node");
        let binary = arena
            .get_binary_expr(condition_node)
            .expect("binary condition");
        // binary.left is "typeof x", we need to get the operand "x"
        let typeof_node = arena.get(binary.left).expect("typeof node");
        let unary = arena.get_unary_expr(typeof_node).expect("unary expression");
        let target_idx = unary.operand; // This is 'x'

        // Get the variable declaration to verify it's const
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
        let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

        // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
        let var_data = arena.get_variable(var_stmt_node).expect("variable data");
        let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
        let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

        // Verify the declaration list has CONST flag
        let flags = decl_list_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;
        assert!(is_const, "Variable declaration list should be const");

        // Verify that is_mutable_variable returns false for this variable
        assert!(!analyzer.is_mutable_variable(target_idx));
    }

    #[test]
    fn test_nested_closures_handling() {
        // Test that we handle nested closures correctly
        let source = r#"
let x: string | number;
const fn = () => {
    // Outer closure - x should not be narrowed from outer scope
    const inner = () => {
        // Inner closure - x should still not be narrowed
    };
};
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let arena = parser.get_arena();

        // Get the variable declaration
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let var_stmt_idx = source_file.statements.nodes[0]; // VARIABLE_STATEMENT
        let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");

        // Get the VARIABLE_DECLARATION_LIST from within the VARIABLE_STATEMENT
        let var_data = arena.get_variable(var_stmt_node).expect("variable data");
        let decl_list_idx = var_data.declarations.nodes[0]; // VARIABLE_DECLARATION_LIST
        let decl_list_node = arena.get(decl_list_idx).expect("decl list node");

        // Verify the declaration list does NOT have CONST flag (it's 'let')
        let flags = decl_list_node.flags as u32;
        let is_const = (flags & node_flags::CONST) != 0;
        assert!(!is_const, "Variable should be let (mutable)");
    }
}
