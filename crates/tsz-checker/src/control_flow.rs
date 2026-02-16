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

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use tsz_binder::BinderState;
use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::{BinaryExprData, NodeArena};
use tsz_parser::parser::{NodeIndex, NodeList, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, ParamInfo, QueryDatabase, TypeGuard, TypeId, TypePredicate};

type FlowCache = FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>;

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
    /// Create a new `FlowGraph` from a `FlowNodeArena`.
    pub const fn new(arena: &'a FlowNodeArena) -> Self {
        Self { arena }
    }

    /// Get a flow node by ID.
    pub fn get(&self, id: FlowNodeId) -> Option<&FlowNode> {
        self.arena.get(id)
    }

    /// Get the number of flow nodes in the graph.
    pub const fn len(&self) -> usize {
        self.arena.len()
    }

    /// Check if the flow graph is empty.
    pub const fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Check if a flow node has a specific flag.
    pub fn node_has_flag(&self, id: FlowNodeId, flag: u32) -> bool {
        self.get(id).is_some_and(|node| node.has_any_flags(flag))
    }

    /// Get the antecedents (predecessors) of a flow node.
    pub fn antecedents(&self, id: FlowNodeId) -> Vec<FlowNodeId> {
        self.get(id)
            .map(|node| node.antecedent.clone())
            .unwrap_or_default()
    }

    /// Get the AST node associated with a flow node.
    pub fn node(&self, id: FlowNodeId) -> NodeIndex {
        self.get(id).map_or(NodeIndex::NONE, |node| node.node)
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
    pub(crate) flow_cache: Option<&'a RefCell<FlowCache>>,
    /// Optional `TypeEnvironment` for resolving Lazy types during narrowing
    pub(crate) type_environment: Option<Rc<RefCell<tsz_solver::TypeEnvironment>>>,
    /// Cache for switch-reference relevance checks.
    /// Key: (`switch_expr_node`, `reference_node`) -> whether switch can narrow reference.
    switch_reference_cache: RefCell<FxHashMap<(u32, u32), bool>>,
    /// Cache for `is_matching_reference` results.
    /// Key: (`node_a`, `node_b`) -> whether references match (same symbol/property chain).
    /// This avoids O(N²) repeated comparisons during flow analysis with many variables.
    pub(crate) reference_match_cache: RefCell<FxHashMap<(u32, u32), bool>>,
    /// Cache numeric atom conversions during a single flow walk.
    /// Key: normalized f64 bits (with +0 normalized separately from -0).
    pub(crate) numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,
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
    /// Create a new `FlowAnalyzer`.
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
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            reference_match_cache: RefCell::new(FxHashMap::default()),
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
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
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            reference_match_cache: RefCell::new(FxHashMap::default()),
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
        }
    }

    /// Set the flow analysis cache to avoid redundant graph traversals.
    pub const fn with_flow_cache(
        mut self,
        cache: &'a RefCell<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>,
    ) -> Self {
        self.flow_cache = Some(cache);
        self
    }

    /// Set the `TypeEnvironment` for resolving Lazy types during narrowing.
    pub fn with_type_environment(
        mut self,
        type_env: Rc<RefCell<tsz_solver::TypeEnvironment>>,
    ) -> Self {
        self.type_environment = Some(type_env);
        self
    }

    #[inline]
    fn switch_can_affect_reference(&self, switch_expr: NodeIndex, reference: NodeIndex) -> bool {
        let key = (switch_expr.0, reference.0);
        if let Some(&cached) = self.switch_reference_cache.borrow().get(&key) {
            return cached;
        }

        let affects = self.is_matching_reference(switch_expr, reference)
            || self
                .discriminant_property_info(switch_expr, reference)
                .is_some_and(|(_, _, base)| self.is_matching_reference(base, reference))
            // switch (typeof x) narrows x through typeof comparison
            || self.is_typeof_target(switch_expr, reference);

        self.switch_reference_cache
            .borrow_mut()
            .insert(key, affects);
        affects
    }

    /// Get a reference to the flow graph.
    pub const fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
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
    /// * `loop_flow_id` - The `FlowNodeId` of the `LOOP_LABEL` (for cache key)
    /// * `loop_flow` - The `LOOP_LABEL` flow node
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
        if let Some(sym_id) = symbol_id
            && self.is_const_symbol(sym_id)
        {
            return entry_type;
        }

        // Without a symbol_id we cannot inject cache entries to break the
        // get_flow_type → check_flow → LOOP_LABEL → analyze_loop_fixed_point
        // recursion cycle.  This happens for property-access references
        // (e.g. `fns.length`) whose base symbol is tracked separately.
        // Returning the entry type is safe because property access expressions
        // are never reassigned inside loops.
        if symbol_id.is_none() {
            return entry_type;
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
                current_type = self.interner.union(vec![current_type, back_edge_type]);
            }

            // Check if we've reached a fixed point (type stopped changing)
            if current_type == prev_type {
                return current_type;
            }
        }

        // Fixed point not reached within iteration limit
        // Conservative widening: return union of entry type and initial declared type
        // This matches TypeScript's behavior for complex loops
        let widened = self.interner.union(vec![entry_type, initial_type]);

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
    /// on deeply nested control flow structures. Uses a `VecDeque` worklist with
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
            tsz_solver::type_queries::contains_type_parameters_db(self.interner, initial_type);

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);

        // Process worklist until empty
        while let Some((current_flow, current_type)) = worklist.pop_front() {
            in_worklist.remove(&current_flow);

            // OPTIMIZATION: Check global cache first to avoid redundant traversals
            // BUG FIX: Skip cache for SWITCH_CLAUSE nodes to ensure proper flow graph traversal
            // Switch clauses must be processed to schedule antecedents and apply narrowing
            let (is_switch_clause, is_loop_label_node) =
                if let Some(flow) = self.binder.flow_nodes.get(current_flow) {
                    (
                        flow.has_any_flags(flow_flags::SWITCH_CLAUSE),
                        flow.has_any_flags(flow_flags::LOOP_LABEL),
                    )
                } else {
                    (false, false)
                };

            // Use cache if: 1) not a switch clause, AND
            // 2) either initial type is concrete OR this is a loop label.
            // Loop labels MUST always check cache because analyze_loop_fixed_point
            // injects entries as a recursion guard — skipping the check causes
            // stack overflow when types contain type parameters.
            if !is_switch_clause
                && (!initial_has_type_params || is_loop_label_node)
                && let Some(sym_id) = symbol_id
                && let Some(cache) = self.flow_cache
            {
                let key = (current_flow, sym_id, initial_type);
                if let Some(&cached_type) = cache.borrow().get(&key) {
                    // Use cached result and skip processing this node
                    results.insert(current_flow, cached_type);
                    visited.insert(current_flow);
                    continue;
                }
            }

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

            // Check if this is a merge point that needs all antecedents processed first
            let is_switch_fallthrough =
                flow.has_any_flags(flow_flags::SWITCH_CLAUSE) && flow.antecedent.len() > 1;
            let is_loop_header = flow.has_any_flags(flow_flags::LOOP_LABEL);
            let is_call = flow.has_any_flags(flow_flags::CALL);
            // Note: ARRAY_MUTATION merge point check is handled below since we need to check
            // if the mutation actually affects the reference we're analyzing
            let is_merge_point = flow
                .has_any_flags(flow_flags::BRANCH_LABEL | flow_flags::LOOP_LABEL)
                || is_switch_fallthrough
                || is_call; // CRITICAL: CALL nodes need antecedent for assertion functions

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
                // CRITICAL: For else-if chains, the antecedent is a CONDITION node
                // from the outer if's false branch. We must wait for it to be computed
                // so we narrow from the already-narrowed type, not the original type.
                let (pre_type, antecedent_id) = if let Some(&ant) = flow.antecedent.first() {
                    if let Some(&ant_type) = results.get(&ant) {
                        // Antecedent already computed — use its narrowed type
                        (ant_type, ant)
                    } else if !visited.contains(&ant) {
                        // Antecedent not yet computed — defer if it's a CONDITION
                        // (else-if chain) or CALL (which carries assertion narrowing
                        // or passes through the narrowed type from its own antecedent).
                        // This ensures nested type guards chain correctly:
                        //   if (hasLegs(x)) { if (hasWings(x)) { x.legs; } }
                        let ant_flags = self
                            .binder
                            .flow_nodes
                            .get(ant)
                            .map(|f| f.flags)
                            .unwrap_or(0);
                        let ant_needs_defer = (ant_flags & flow_flags::CONDITION) != 0
                            || (ant_flags & flow_flags::CALL) != 0;
                        if ant_needs_defer {
                            if !in_worklist.contains(&ant) {
                                worklist.push_front((ant, current_type));
                                in_worklist.insert(ant);
                            }
                            if !in_worklist.contains(&current_flow) {
                                worklist.push_back((current_flow, current_type));
                                in_worklist.insert(current_flow);
                            }
                            continue;
                        }
                        (current_type, ant)
                    } else {
                        // Antecedent visited but no result — use current_type
                        (current_type, ant)
                    }
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
                if let Some(&ant) = flow.antecedent.first()
                    && !in_worklist.contains(&ant)
                    && !visited.contains(&ant)
                {
                    worklist.push_back((ant, current_type));
                    in_worklist.insert(ant);
                }

                // Switch clause - apply switch-specific narrowing
                self.handle_switch_clause_iterative(reference, current_type, flow, &results)
            } else if flow.has_any_flags(flow_flags::ASSIGNMENT) {
                // OPTIMIZATION: Quick symbol-based filtering before expensive AST comparison.
                // If we have a resolved symbol and the assignment's target has a different symbol,
                // we can skip this assignment entirely. This turns O(N²) into O(N) for cases like
                // many independent variable assignments.
                let targets_reference = if let Some(target_sym) = symbol_id {
                    // Get the assignment target's symbol (O(1) lookup)
                    let assignment_sym = self.reference_symbol(flow.node);
                    if assignment_sym.is_some() && assignment_sym != Some(target_sym) {
                        // Symbols differ - this assignment cannot target our reference
                        false
                    } else {
                        // Same symbol or couldn't determine - do full check
                        self.assignment_targets_reference_node(flow.node, reference)
                    }
                } else {
                    // No symbol ID - must do full check
                    self.assignment_targets_reference_node(flow.node, reference)
                };

                if targets_reference {
                    // CRITICAL FIX: Skip "killing definition" narrowing for ANY and ERROR types only
                    // These types should preserve their identity across assignments to match tsc behavior
                    //
                    // IMPORTANT: unknown is NOT included here because it SHOULD be narrowed by assignments
                    // Example: let x: unknown; x = 123; should narrow x to number
                    //
                    // any absorbs assignments (stays any)
                    // error persists to prevent cascading errors
                    if initial_type != TypeId::ANY && initial_type != TypeId::ERROR {
                        // Check if this is a destructuring assignment (widens literals to primitives)
                        let is_destructuring = self.is_destructuring_assignment(flow.node);

                        // CRITICAL FIX: Try to get assigned type for ALL assignments, including destructuring
                        // Previously: Only direct assignments (x = ...) worked
                        // Now: Destructuring ([x] = ...) also works because get_assigned_type handles it
                        if let Some(assigned_type) =
                            self.get_assigned_type(flow.node, reference, is_destructuring)
                        {
                            // Killing definition: replace type with RHS type and stop traversal
                            assigned_type
                        } else {
                            // If we can't resolve the RHS type, conservatively return declared type
                            // The value HAS changed, so we can't continue to antecedent
                            current_type
                        }
                    } else {
                        // For any/error types: Don't apply narrowing - continue to antecedent
                        // This allows condition narrowing (typeof guards) to still work
                        if let Some(&ant) = flow.antecedent.first() {
                            if !in_worklist.contains(&ant) && !visited.contains(&ant) {
                                worklist.push_back((ant, current_type));
                                in_worklist.insert(ant);
                            }
                            *results.get(&ant).unwrap_or(&current_type)
                        } else {
                            current_type
                        }
                    }
                } else if self.assignment_affects_reference_node(flow.node, reference) {
                    // Two sub-cases of "affects reference":
                    // 1. Base reassignment (obj = ... affects obj.prop): clears narrowing
                    // 2. Property mutation (obj.prop.x = ... affects obj.prop): preserves narrowing
                    //
                    // Check if the assignment targets a BASE of the reference. If so,
                    // the reference value may have changed entirely and narrowing is invalid.
                    let is_base_reassignment =
                        self.assignment_targets_base_of_reference(flow.node, reference);

                    if is_base_reassignment {
                        // Base was reassigned — narrowing is invalidated.
                        // Return initial (declared) type.
                        if let Some(&ant) = flow.antecedent.first()
                            && !in_worklist.contains(&ant)
                            && !visited.contains(&ant)
                        {
                            worklist.push_back((ant, current_type));
                            in_worklist.insert(ant);
                        }
                        current_type
                    } else {
                        // Property mutation — preserve narrowing from antecedent.
                        // Must defer when antecedent carries narrowing (CONDITION/CALL)
                        // and hasn't been computed yet, otherwise we lose typeof narrowing.
                        if let Some(&ant) = flow.antecedent.first() {
                            if let Some(&ant_type) = results.get(&ant) {
                                ant_type
                            } else if !visited.contains(&ant) {
                                let ant_needs_defer =
                                    self.binder.flow_nodes.get(ant).is_some_and(|f| {
                                        f.has_any_flags(flow_flags::CONDITION | flow_flags::CALL)
                                    });
                                if ant_needs_defer {
                                    if !in_worklist.contains(&ant) {
                                        worklist.push_front((ant, current_type));
                                        in_worklist.insert(ant);
                                    }
                                    if !in_worklist.contains(&current_flow) {
                                        worklist.push_back((current_flow, current_type));
                                        in_worklist.insert(current_flow);
                                    }
                                    continue;
                                }
                                if !in_worklist.contains(&ant) {
                                    worklist.push_back((ant, current_type));
                                    in_worklist.insert(ant);
                                }
                                *results.get(&ant).unwrap_or(&current_type)
                            } else {
                                current_type
                            }
                        } else {
                            current_type
                        }
                    }
                } else {
                    // This assignment doesn't affect our reference — pass through to antecedent.
                    // CRITICAL: If the antecedent carries narrowing info and hasn't been processed
                    // yet, we must defer to avoid losing narrowing. Without this, the worklist
                    // may process this ASSIGNMENT before its antecedent chain is resolved, using
                    // the un-narrowed type. This applies to CONDITION nodes (which directly
                    // narrow) and CALL nodes (which are merge points whose own antecedents may
                    // carry narrowing from conditions).
                    if let Some(&ant) = flow.antecedent.first() {
                        if let Some(&ant_type) = results.get(&ant) {
                            // Antecedent already computed — use its result
                            ant_type
                        } else if !visited.contains(&ant) {
                            let ant_needs_defer =
                                self.binder.flow_nodes.get(ant).is_some_and(|f| {
                                    f.has_any_flags(flow_flags::CONDITION | flow_flags::CALL)
                                });
                            if ant_needs_defer {
                                // Defer: process antecedent first, then re-process self
                                if !in_worklist.contains(&ant) {
                                    worklist.push_front((ant, current_type));
                                    in_worklist.insert(ant);
                                }
                                if !in_worklist.contains(&current_flow) {
                                    worklist.push_back((current_flow, current_type));
                                    in_worklist.insert(current_flow);
                                }
                                continue;
                            }
                            // Non-narrowing antecedent: schedule it but use current_type
                            if !in_worklist.contains(&ant) {
                                worklist.push_back((ant, current_type));
                                in_worklist.insert(ant);
                            }
                            *results.get(&ant).unwrap_or(&current_type)
                        } else {
                            current_type
                        }
                    } else {
                        current_type
                    }
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

                // Check if this array mutation affects our reference
                let affects_ref = self.array_mutation_affects_reference(call, reference);

                // For affected references, ARRAY_MUTATION acts as a merge point to preserve narrowing
                let needs_antecedent = affects_ref && !flow.antecedent.is_empty();

                if needs_antecedent {
                    // Check if antecedent is ready (similar to merge point logic)
                    if let Some(&ant) = flow.antecedent.first() {
                        if !visited.contains(&ant) && !results.contains_key(&ant) {
                            // Antecedent not ready - schedule it and defer self
                            if !in_worklist.contains(&ant) {
                                worklist.push_front((ant, current_type));
                                in_worklist.insert(ant);
                            }
                            if !in_worklist.contains(&current_flow) {
                                worklist.push_back((current_flow, current_type));
                                in_worklist.insert(current_flow);
                            }
                            continue;
                        }
                        // Antecedent is ready - get its result
                        *results.get(&ant).unwrap_or(&current_type)
                    } else {
                        current_type
                    }
                } else if affects_ref {
                    // For local variables, TypeScript preserves narrowing across method calls
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
                self.handle_call_iterative(reference, current_type, flow, &results)
            } else if flow.has_any_flags(flow_flags::START) {
                // Start node - check if we're crossing a closure boundary
                // For mutable variables (let/var), we cannot trust narrowing from outer scope
                // because the closure may capture the variable and it could be mutated.
                // For const variables, narrowing is preserved (they're immutable).
                let outer_flow_id = flow.antecedent.first().copied().or_else(|| {
                    // START with no antecedents - try to find outer flow via node_flow map
                    if !flow.node.is_none() {
                        self.binder.node_flow.get(&flow.node.0).copied()
                    } else {
                        None
                    }
                });

                if let Some(outer_flow) = outer_flow_id {
                    if self.is_mutable_variable(reference) && self.is_captured_variable(reference) {
                        // Captured mutable variable - cannot use narrowing from outer scope
                        initial_type
                    } else {
                        // Const or local variable - preserve narrowing from outer scope.
                        // Recursively resolve the outer flow to get the narrowed type.
                        // This is needed because the iterative worklist processes START
                        // before its outer antecedent, so the result wouldn't propagate back.
                        self.check_flow(reference, initial_type, outer_flow, _visited, symbol_id)
                    }
                } else {
                    current_type
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
            if let Some(sym_id) = symbol_id
                && let Some(cache) = self.flow_cache
            {
                let final_has_type_params = tsz_solver::type_queries::contains_type_parameters_db(
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
    ) -> TypeId {
        let clause_idx = flow.node;

        // Check if this is an implicit default (node is the case_block itself)
        // This happens when a switch has no default clause - we use the case_block
        // as a marker to represent the implicit "no match" path
        let is_implicit_default = if let Some(node) = self.arena.get(clause_idx) {
            node.kind == syntax_kind_ext::BLOCK
        } else {
            false
        };

        // For implicit default, the parent is the switch statement (not tracked in switch_clause_to_switch)
        let switch_idx = if is_implicit_default {
            // Get parent of case_block, which should be the switch statement
            self.arena.get_extended(clause_idx).and_then(|ext| {
                // The parent of the case_block is the switch statement
                if ext.parent.is_none() {
                    None
                } else {
                    Some(ext.parent)
                }
            })
        } else {
            // Normal case/default clause - use the binder's mapping
            self.binder.get_switch_for_clause(clause_idx)
        };

        let Some(switch_idx) = switch_idx else {
            return current_type;
        };
        let Some(switch_node) = self.arena.get(switch_idx) else {
            return current_type;
        };
        let Some(switch_data) = self.arena.get_switch(switch_node) else {
            return current_type;
        };

        let pre_switch_type = if let Some(&ant) = flow.antecedent.first() {
            *results.get(&ant).unwrap_or(&current_type)
        } else {
            current_type
        };

        // Fast path: if this switch cannot narrow the reference at all, avoid
        // per-clause narrowing setup/work (narrowing context creation, expression checks).
        if !self.switch_can_affect_reference(switch_data.expression, reference) {
            return pre_switch_type;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

        // For implicit default, apply default clause narrowing (exclude all case types)
        if is_implicit_default {
            return self.narrow_by_default_switch_clause(
                pre_switch_type,
                switch_data.expression,
                switch_data.case_block,
                reference,
                &narrowing,
            );
        }

        // Normal case/default clause handling
        let Some(clause_node) = self.arena.get(clause_idx) else {
            return current_type;
        };
        let Some(clause) = self.arena.get_case_clause(clause_node) else {
            return current_type;
        };

        if clause.expression.is_none() {
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
        }
    }

    /// Helper function for call handling in iterative mode.
    pub(crate) fn handle_call_iterative(
        &self,
        reference: NodeIndex,
        current_type: TypeId,
        flow: &FlowNode,
        results: &FxHashMap<FlowNodeId, TypeId>,
    ) -> TypeId {
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

        // For generic assertion functions like `assertEqual<T>(value: any, type: T): asserts value is T`,
        // the predicate's type_id is the unresolved type parameter T. Resolve it by matching against
        // the call's actual argument types.
        let resolved_predicate = self.resolve_generic_predicate(
            &signature.predicate,
            &signature.params,
            call,
            callee_type,
            node_types,
        );

        if self.is_matching_reference(predicate_target, reference) {
            return self.apply_type_predicate_narrowing(pre_type, &resolved_predicate, true);
        }

        // Discriminant narrowing: if the predicate target is a property access on the
        // reference (e.g., assertEqual(animal.type, 'cat') narrows animal from Cat|Dog to Cat),
        // extract the property path and narrow the parent object by discriminant.
        if let Some(predicate_type) = resolved_predicate.type_id
            && let Some((property_path, _is_optional, base)) =
                self.discriminant_property_info(predicate_target, reference)
            && self.is_matching_reference(base, reference)
        {
            let env_borrow;
            let narrowing = if let Some(env) = &self.type_environment {
                env_borrow = env.borrow();
                NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
            } else {
                NarrowingContext::new(self.interner)
            };
            return narrowing.narrow_by_discriminant(pre_type, &property_path, predicate_type);
        }

        // Condition-based assertion narrowing: for `assert(condition)` where the predicate
        // has no type (just `asserts value`), the argument expression acts as a narrowing
        // condition. After the assertion, the condition is known true, so we narrow the
        // reference using the condition expression, just like an if-statement.
        // e.g., assert(typeof x === "string") narrows x to string.
        if resolved_predicate.type_id.is_none() {
            let antecedent_id = flow.antecedent.first().copied().unwrap_or(FlowNodeId::NONE);
            return self.narrow_type_by_condition(
                pre_type,
                predicate_target,
                reference,
                true,
                antecedent_id,
            );
        }

        pre_type
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
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL | flow_flags::CONDITION) {
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

    /// Widen literal types to their primitive types for array destructuring.
    ///
    /// In array destructuring contexts, literals are widened to their base primitives:
    /// - `1` -> `number`
    /// - `"hello"` -> `string`
    /// - `true` -> `boolean`
    ///
    /// This matches TypeScript's behavior where `[x] = [1]` narrows `x` to `number`, not literal `1`.
    fn widen_to_primitive(&self, type_id: TypeId) -> TypeId {
        tsz_solver::type_queries::widen_literal_to_primitive(self.interner, type_id)
    }

    /// Check if an assignment node is a mutable variable declaration (let/var) without a type annotation.
    /// Used to determine when literal types should be widened to their base types.
    fn is_mutable_var_decl_without_annotation(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        // Handle VARIABLE_DECLARATION directly
        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = self.arena.get_variable_declaration(node_data) else {
                return false;
            };
            // If there's a type annotation, don't widen - the user specified the type
            if !decl.type_annotation.is_none() {
                return false;
            }
            // Check if the parent declaration list is let/var (not const)
            if let Some(ext) = self.arena.get_extended(node)
                && !ext.parent.is_none()
                && let Some(parent_node) = self.arena.get(ext.parent)
            {
                let flags = parent_node.flags as u32;
                return (flags & node_flags::CONST) == 0;
            }
            return false;
        }

        // Handle VARIABLE_DECLARATION_LIST or VARIABLE_STATEMENT: check flags on the list
        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_data.kind == syntax_kind_ext::VARIABLE_STATEMENT
        {
            let flags = node_data.flags as u32;
            if (flags & node_flags::CONST) != 0 {
                return false;
            }
            // Check individual declarations for type annotations
            if let Some(list) = self.arena.get_variable(node_data) {
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && decl.type_annotation.is_none()
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if an assignment flow node is a variable declaration with a type annotation.
    ///
    /// When a variable has an explicit type annotation, the flow analysis should
    /// use the declared type (not the initializer's structural type) for non-literal
    /// assignments. This prevents the initializer's type from overriding the declared
    /// type in the flow graph.
    fn is_var_decl_with_type_annotation(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        if node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.arena.get_variable_declaration(node_data)
        {
            return !decl.type_annotation.is_none();
        }

        // Handle VARIABLE_DECLARATION_LIST or VARIABLE_STATEMENT
        if (node_data.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_data.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            && let Some(list) = self.arena.get_variable(node_data)
        {
            for &decl_idx in &list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    && !decl.type_annotation.is_none()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if an assignment node represents a destructuring assignment.
    /// Destructuring assignments widen literals to primitives, unlike direct assignments.
    fn is_destructuring_assignment(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        match node_data.kind {
            syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(bin) = self.arena.get_binary_expr(node_data) else {
                    return false;
                };
                // Check if left side is a binding pattern OR array/object literal (for destructuring)
                let left_is_binding = self.is_binding_pattern(bin.left);
                let left_is_literal = self.contains_destructuring_pattern(bin.left);
                left_is_binding || left_is_literal
            }
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let Some(decl) = self.arena.get_variable_declaration(node_data) else {
                    return false;
                };
                // Check if name is a binding pattern (destructuring in variable declaration)
                self.is_binding_pattern(decl.name)
            }
            _ => false,
        }
    }

    /// Check if a node is a binding pattern (array or object destructuring pattern)
    fn is_binding_pattern(&self, node: NodeIndex) -> bool {
        if node.is_none() {
            return false;
        }
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        matches!(
            node_data.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        )
    }

    /// Check if a node contains a destructuring pattern (array/object literal with binding elements).
    /// This handles cases like `[x] = [1]` where the left side is an array literal containing binding patterns.
    ///
    /// Note: In TypeScript, if an array or object literal appears on the left side of an assignment,
    /// it's ALWAYS a destructuring pattern, regardless of what elements it contains.
    fn contains_destructuring_pattern(&self, node: NodeIndex) -> bool {
        if node.is_none() {
            return false;
        }
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };

        // If this is an array or object literal, it's a destructuring pattern when on the left side of an assignment
        matches!(
            node_data.kind,
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        )
    }

    pub(crate) fn get_assigned_type(
        &self,
        assignment_node: NodeIndex,
        target: NodeIndex,
        widen_literals_for_destructuring: bool,
    ) -> Option<TypeId> {
        let node = self.arena.get(assignment_node)?;

        // CRITICAL FIX: Handle compound assignments (+=, -=, *=, etc.)
        // Compound assignments compute the result of a binary operation and assign it back.
        // Example: x += 1 where x: string | number should narrow x to number after assignment.
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let bin = self.arena.get_binary_expr(node)?;
            // Check if this is an assignment to our target reference
            if self.is_matching_reference(bin.left, target) {
                // Check if this is a compound assignment operator (not simple =)
                if bin.operator_token != SyntaxKind::EqualsToken as u16
                    && self.is_compound_assignment_operator(bin.operator_token)
                {
                    use tsz_solver::{BinaryOpEvaluator, BinaryOpResult};

                    // When node_types is not available, use heuristics for flow narrowing
                    if self.node_types.is_none() {
                        // For operators that ONLY produce number, kill narrowing
                        return match bin.operator_token {
                            k if k == SyntaxKind::MinusEqualsToken as u16
                                || k == SyntaxKind::AsteriskEqualsToken as u16
                                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                                || k == SyntaxKind::SlashEqualsToken as u16
                                || k == SyntaxKind::PercentEqualsToken as u16
                                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                                    as u16
                                || k == SyntaxKind::AmpersandEqualsToken as u16
                                || k == SyntaxKind::BarEqualsToken as u16
                                || k == SyntaxKind::CaretEqualsToken as u16 =>
                            {
                                Some(TypeId::NUMBER)
                            }
                            // For +=: check if RHS is a number literal (common case: x += 1)
                            k if k == SyntaxKind::PlusEqualsToken as u16 => {
                                // If RHS is a numeric literal, we can safely infer NUMBER
                                if let Some(literal_type) = self.literal_type_from_node(bin.right)
                                    && self.is_number_type(literal_type)
                                {
                                    return Some(TypeId::NUMBER);
                                }
                                // Otherwise, preserve narrowing (could be string concatenation)
                                None
                            }
                            // ??= could be any type - preserve narrowing without type info
                            k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => None,
                            // For logical assignments, preserve narrowing (don't kill it)
                            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                                || k == SyntaxKind::BarBarEqualsToken as u16 =>
                            {
                                None
                            }
                            _ => None,
                        };
                    }

                    // Get LHS type (current narrowed type of the variable)
                    let left_type = if let Some(node_types) = self.node_types
                        && let Some(&lhs_type) = node_types.get(&bin.left.0)
                    {
                        lhs_type
                    } else {
                        // Fall back - shouldn't happen due to the check above
                        return None;
                    };

                    // Get RHS type
                    let right_type = if let Some(node_types) = self.node_types
                        && let Some(&rhs_type) = node_types.get(&bin.right.0)
                    {
                        rhs_type
                    } else {
                        // Fall back - shouldn't happen due to the check above
                        return None;
                    };

                    // Map compound assignment operator to binary operator
                    let op_str = self.map_compound_operator_to_binary(bin.operator_token)?;

                    // Evaluate the binary operation to get result type
                    let evaluator = BinaryOpEvaluator::new(self.interner);
                    return match evaluator.evaluate(left_type, right_type, op_str) {
                        BinaryOpResult::Success(result) => Some(result),
                        // For type errors, return ANY to prevent cascading errors
                        BinaryOpResult::TypeError { .. } => Some(TypeId::ANY),
                    };
                }
            }
        }

        if let Some(rhs) = self.assignment_rhs_for_reference(assignment_node, target) {
            // For flow narrowing, prefer literal types from AST nodes over the type checker's widened types
            // This ensures that `x = 42` narrows to literal 42.0, not just NUMBER
            // This matches TypeScript's behavior where control flow analysis preserves literal types
            if let Some(literal_type) = self.literal_type_from_node(rhs) {
                // For destructuring contexts, widen literals to primitives to match TypeScript
                // Example: [x] = [1] widens to number, ({ x } = { x: 1 }) widens to number
                // Also handles default values: [x = 2] = [] widens to number
                if widen_literals_for_destructuring {
                    return Some(self.widen_to_primitive(literal_type));
                }
                // For mutable variable declarations (let/var) without type annotations,
                // widen literal types to their base types to match TypeScript behavior.
                // Example: let x = "hi" -> string (not "hi"), let x = 42 -> number (not 42)
                if self.is_mutable_var_decl_without_annotation(assignment_node) {
                    return Some(self.widen_to_primitive(literal_type));
                }
                return Some(literal_type);
            }
            if let Some(nullish_type) = self.nullish_literal_type(rhs) {
                return Some(nullish_type);
            }
            // Fall back to type checker's result for non-literal expressions
            //
            // FIX: For variable declarations with type annotations where the RHS is a
            // structural literal (object/array), the declared type should be preserved —
            // don't let the initializer's structural type override the annotated type.
            // Literal and nullish initializers (handled above) still narrow correctly,
            // but object/array literals produce structural types that lose optional
            // properties and interface identity.
            //
            // Example: `var obj4: I<number,string> = { one: 1 };`
            //   - Declared type: I<number, string> (includes two?: string)
            //   - Initializer type: { one: number } (missing the optional property)
            //   - Without this fix, flow uses { one: number } instead of I<number, string>
            //
            // We only apply this for object/array literals. Type assertions like
            // `{} as any` and other expressions should still use the node_types result.
            if self.is_var_decl_with_type_annotation(assignment_node)
                && let Some(rhs_node) = self.arena.get(rhs)
                && (rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    || rhs_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
            {
                return None;
            }
            if let Some(node_types) = self.node_types
                && let Some(&rhs_type) = node_types.get(&rhs.0)
            {
                // Only apply assignment-based "killing definition" narrowing when
                // the write itself is compatible. For invalid assignments, TypeScript
                // reports the assignment error but keeps subsequent reads at the
                // variable's declared type.
                if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(bin) = self.arena.get_binary_expr(node)
                    && bin.operator_token == SyntaxKind::EqualsToken as u16
                    && self.is_matching_reference(bin.left, target)
                {
                    let declared_target_type = self
                        .binder
                        .resolve_identifier(self.arena, bin.left)
                        .and_then(|sym| self.binder.get_symbol(sym))
                        .map(|sym| sym.value_declaration)
                        .filter(|decl| !decl.is_none())
                        .and_then(|decl| node_types.get(&decl.0).copied())
                        .or_else(|| node_types.get(&bin.left.0).copied());

                    if let Some(lhs_type) = declared_target_type
                        && !self.interner.is_assignable_to(rhs_type, lhs_type)
                    {
                        return None;
                    }
                }
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

        // For for-of / for-in initializer assignments, the binder creates a
        // flow ASSIGNMENT pointing to the initializer node (e.g. the identifier
        // `x` in `for (x of arr)`). Walk up to find the parent for-of/for-in
        // statement and get the iterated expression's element type.
        if self.is_matching_reference(assignment_node, target)
            && let Some(ext) = self.arena.get_extended(assignment_node)
            && !ext.parent.is_none()
            && let Some(parent_node) = self.arena.get(ext.parent)
            && (parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                || parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT)
            && let Some(for_data) = self.arena.get_for_in_of(parent_node)
            && let Some(node_types) = self.node_types
            && let Some(&expr_type) = node_types.get(&for_data.expression.0)
        {
            if parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT {
                return Some(TypeId::STRING);
            }
            // for-of: extract element type from the array/iterable expression type
            if let Some(elem) =
                tsz_solver::type_queries::get_array_element_type(self.interner, expr_type)
            {
                return Some(elem);
            }
        }

        None
    }

    /// Check if an operator token is a compound assignment operator.
    /// Returns true for +=, -=, *=, /=, etc., but not for simple =.
    const fn is_compound_assignment_operator(&self, operator_token: u16) -> bool {
        matches!(
            operator_token,
            k if k == SyntaxKind::PlusEqualsToken as u16
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
                || k == SyntaxKind::CaretEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
        )
    }

    /// Map a compound assignment operator to its corresponding binary operator.
    /// Returns None if the operator is not a recognized compound assignment.
    const fn map_compound_operator_to_binary(&self, operator_token: u16) -> Option<&'static str> {
        match operator_token {
            k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
            k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
            k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("**"),
            k if k == SyntaxKind::SlashEqualsToken as u16 => Some("/"),
            k if k == SyntaxKind::PercentEqualsToken as u16 => Some("%"),
            k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => Some("<<"),
            k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => Some(">>"),
            k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => {
                Some(">>>")
            }
            k if k == SyntaxKind::AmpersandEqualsToken as u16 => Some("&"),
            k if k == SyntaxKind::BarEqualsToken as u16 => Some("|"),
            k if k == SyntaxKind::CaretEqualsToken as u16 => Some("^"),
            k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => Some("&&"),
            k if k == SyntaxKind::BarBarEqualsToken as u16 => Some("||"),
            k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => Some("??"),
            _ => None,
        }
    }

    /// Check if a type is a number type (NUMBER or number literal).
    /// Used to infer result types for compound assignments when type checker results aren't available.
    fn is_number_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::NUMBER
            || tsz_solver::type_queries::is_number_literal(self.interner, type_id)
    }

    pub(crate) fn assignment_rhs_for_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> Option<NodeIndex> {
        let node = self.arena.get(assignment_node)?;

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

        // Handle default values: when RHS is empty, check for default value in pattern element
        if rhs.is_none()
            && self.assignment_targets_reference_internal(pattern, target)
            && let Some(binding) = self.arena.get_binding_element_at(pattern)
            && !binding.initializer.is_none()
        {
            return Some(binding.initializer);
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
                // FIX: Array destructuring should return the matching RHS element
                // After [x] = [1] where x: string | number, TypeScript produces `number` (widened primitive)
                // We return the element node here; get_assigned_type handles the widening
                let elements = if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    self.arena.get_literal_expr(node).map(|lit| &lit.elements)?
                } else {
                    self.arena
                        .get_binding_pattern(node)
                        .map(|pat| &pat.elements)?
                };

                // Get elements from the RHS array literal
                let rhs_elements = self.array_literal_elements(rhs);

                for (i, &elem) in elements.nodes.iter().enumerate() {
                    if elem.is_none() {
                        continue;
                    }

                    // Check if this specific element (or its children) targets our reference
                    if self.assignment_targets_reference_internal(elem, target) {
                        let rhs_elem = rhs_elements
                            .and_then(|re| re.nodes.get(i).copied())
                            .unwrap_or(NodeIndex::NONE);

                        // Recurse to handle nested destructuring: [[x]] = [[1]]
                        return self.match_destructuring_rhs(elem, rhs_elem, target);
                    }
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
        let key = self
            .property_key_from_name(name)
            .or_else(|| self.property_key_from_name_with_rhs_effects(name, rhs))?;

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

    fn property_key_from_name_with_rhs_effects(
        &self,
        name: NodeIndex,
        rhs: NodeIndex,
    ) -> Option<PropertyKey> {
        let name = self.skip_parens_and_assertions(name);
        let name_node = self.arena.get(name)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(name_node)?;
        let key_expr = self.skip_parens_and_assertions(computed.expression);

        if let Some(key) = self.property_key_from_assignment_like_expr(key_expr) {
            return Some(key);
        }

        let key_node = self.arena.get(key_expr)?;
        if key_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        self.property_key_from_rhs_assignment_to_reference(rhs, key_expr)
    }

    fn property_key_from_assignment_like_expr(&self, expr: NodeIndex) -> Option<PropertyKey> {
        let expr = self.skip_parens_and_assertions(expr);
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }

        if let Some(value) = self.literal_number_from_node_or_type(bin.right)
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(PropertyKey::Index(value as usize));
        }
        self.literal_atom_from_node_or_type(bin.right)
            .map(PropertyKey::Atom)
    }

    fn property_key_from_rhs_assignment_to_reference(
        &self,
        rhs: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let rhs = self.skip_parens_and_assertions(rhs);
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let rhs_elements = self.arena.get_literal_expr(rhs_node)?;
        let mut inferred = None;
        for &elem in &rhs_elements.elements.nodes {
            if elem.is_none() {
                continue;
            }
            if let Some(key) = self.property_key_from_assignment_to_reference(elem, reference) {
                inferred = Some(key);
            }
        }
        inferred
    }

    fn property_key_from_assignment_to_reference(
        &self,
        expr: NodeIndex,
        reference: NodeIndex,
    ) -> Option<PropertyKey> {
        let expr = self.skip_parens_and_assertions(expr);
        let node = self.arena.get(expr)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let bin = self.arena.get_binary_expr(node)?;
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        if !self.is_matching_reference(bin.left, reference) {
            return None;
        }
        if let Some(value) = self.literal_number_from_node_or_type(bin.right)
            && value.fract() == 0.0
            && value >= 0.0
        {
            return Some(PropertyKey::Index(value as usize));
        }
        self.literal_atom_from_node_or_type(bin.right)
            .map(PropertyKey::Atom)
    }

    pub(crate) fn find_property_in_object_literal(
        &self,
        literal: &tsz_parser::parser::node::LiteralExprData,
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
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                self.is_assignment_operator(bin.operator_token)
                    && self.assignment_affects_reference(bin.left, target)
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_affects_reference(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_affects_reference(decl.name, target));
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
            return self.arena.get_binary_expr(node).is_some_and(|bin| {
                let is_op = self.is_assignment_operator(bin.operator_token);
                let targets = self.assignment_targets_reference_internal(bin.left, target);
                is_op && targets
            });
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
        {
            return self.arena.get_unary_expr(node).is_some_and(|unary| {
                (unary.operator == SyntaxKind::PlusPlusToken as u16
                    || unary.operator == SyntaxKind::MinusMinusToken as u16)
                    && self.assignment_targets_reference_internal(unary.operand, target)
            });
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            return self
                .arena
                .get_variable_declaration(node)
                .is_some_and(|decl| self.assignment_targets_reference_internal(decl.name, target));
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

    /// Check if the assignment node reassigns a BASE of the reference.
    ///
    /// For example, if `reference` is `obj.prop` and the assignment is `obj = { prop: 1 }`,
    /// this returns true because `obj` (a base of `obj.prop`) is being reassigned.
    ///
    /// But if `reference` is `config['works']` and the assignment is `config.works.prop = 'test'`,
    /// this returns false because the LHS is deeper than the reference, not a base of it.
    pub(crate) fn assignment_targets_base_of_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
    ) -> bool {
        // Walk up the bases of the reference and check if the assignment targets any of them
        let mut current = self.reference_base(reference);
        while let Some(base) = current {
            if self.assignment_targets_reference_node(assignment_node, base) {
                return true;
            }
            current = self.reference_base(base);
        }
        false
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
        let Some(case_block_node) = self.arena.get(case_block) else {
            return type_id;
        };
        let Some(case_block) = self.arena.get_block(case_block_node) else {
            return type_id;
        };

        // Fast path: if this switch does not reference the target (directly or via discriminant
        // property access like switch(x.kind) when narrowing x), it cannot affect target's type.
        let target_is_switch_expr = self.is_matching_reference(switch_expr, target);
        if !target_is_switch_expr {
            let switch_targets_base = self
                .discriminant_property_info(switch_expr, target)
                .is_some_and(|(_, _, base)| self.is_matching_reference(base, target));
            if !switch_targets_base {
                return type_id;
            }
        }

        // Excluding finitely many case literals from broad primitive domains does not narrow.
        // Example: number minus {0, 1, 2, ...} is still number.
        if target_is_switch_expr
            && matches!(
                type_id,
                TypeId::NUMBER | TypeId::STRING | TypeId::BIGINT | TypeId::SYMBOL | TypeId::OBJECT
            )
        {
            return type_id;
        }

        // OPTIMIZATION: For direct switches on the target (switch(x) {...}),
        // collect all case types first and exclude them in a single O(N) pass.
        // This avoids O(N²) behavior when there are many case clauses.
        if target_is_switch_expr {
            // Collect all case expression types
            let mut excluded_types: Vec<TypeId> = Vec::new();
            for &clause_idx in &case_block.statements.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(clause) = self.arena.get_case_clause(clause_node) else {
                    continue;
                };
                if clause.expression.is_none() {
                    continue; // Skip default clause
                }

                // Try to get the type of the case expression
                // First try literal extraction (fast path for constants)
                if let Some(lit_type) = self.literal_type_from_node(clause.expression) {
                    excluded_types.push(lit_type);
                } else if let Some(node_types) = self.node_types {
                    // Fall back to computed node types
                    if let Some(&expr_type) = node_types.get(&clause.expression.0) {
                        excluded_types.push(expr_type);
                    }
                }
            }

            if !excluded_types.is_empty() {
                // Use batched narrowing for O(N) instead of O(N²)
                return narrowing.narrow_excluding_types(type_id, &excluded_types);
            }
        }

        // Fall back to sequential narrowing for complex cases
        // (e.g., switch(x.kind) where we need property-based narrowing)
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
            narrowed = self.narrow_by_binary_expr(
                narrowed,
                &binary,
                target,
                false,
                narrowing,
                FlowNodeId::NONE,
            );
        }

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
        let mut visited_aliases = Vec::new();

        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
        )
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
        let condition_idx = self.skip_parenthesized(condition_idx);
        let Some(cond_node) = self.arena.get(condition_idx) else {
            return type_id;
        };

        // Create narrowing context and wire up TypeEnvironment if available
        // This enables proper resolution of Lazy types (type aliases) during narrowing
        let env_borrow;
        let narrowing = if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            NarrowingContext::new(self.interner).with_resolver(&*env_borrow)
        } else {
            NarrowingContext::new(self.interner)
        };

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
                    // Handle logical operators (&&, ||) with special recursion
                    if let Some(narrowed) = self.narrow_by_logical_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        antecedent_id,
                        visited_aliases,
                    ) {
                        return narrowed;
                    }

                    // CRITICAL: Use Solver-First architecture for other binary expressions
                    // Extract TypeGuard from AST (Checker responsibility: WHERE + WHAT)
                    if let Some((guard, guard_target, _is_optional)) =
                        self.extract_type_guard(condition_idx)
                    {
                        // Check if the guard applies to our target reference
                        if self.is_matching_reference(guard_target, target) {
                            // CRITICAL FIX: Don't apply discriminant guards to property/element access results
                            // Discriminant guards (like `obj.kind === "a"`) should only narrow the base object (`obj`),
                            // not property access results (like `obj.value`).
                            //
                            // Example:
                            //   type U = { kind: "a"; value: string } | { kind: "b"; value: number };
                            //   let obj: U = { kind: "a", value: "ok" };
                            //   if (obj.kind === "a") {
                            //     obj.value.toUpperCase(); // obj.value should be narrowed to string via obj
                            //   }
                            //
                            // The discriminant guard narrows `obj` to { kind: "a"; value: string }, and then
                            // accessing `obj.value` gives us `string`. We should NOT try to narrow `obj.value`
                            // directly by the discriminant (which would fail since `string | number` has no `kind` property).
                            let is_discriminant_guard =
                                matches!(guard, TypeGuard::Discriminant { .. });
                            let is_property_access = self.is_property_or_element_access(target);

                            if is_discriminant_guard && is_property_access {
                                // Skip narrowing - the discriminant guard applies to the base, not the property
                                // The property type will be computed from the already-narrowed base object
                                return type_id;
                            }

                            // CRITICAL: Invert sense for inequality operators (!== and !=)
                            // This applies to ALL guards, not just typeof
                            // For `x !== "string"` or `x.kind !== "circle"`, the true branch should EXCLUDE
                            let effective_sense = if bin.operator_token
                                == SyntaxKind::ExclamationEqualsEqualsToken as u16
                                || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
                            {
                                !is_true_branch
                            } else {
                                is_true_branch
                            };
                            // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                            return narrowing.narrow_type(type_id, &guard, effective_sense);
                        }
                    }

                    // CRITICAL: Try bidirectional narrowing for x === y where both are references
                    // This handles cases that don't match traditional type guard patterns
                    // Example: if (x === y) { x } should narrow x based on y's type
                    let narrowed = self.narrow_by_binary_expr(
                        type_id,
                        bin,
                        target,
                        is_true_branch,
                        &narrowing,
                        antecedent_id,
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
                    // CRITICAL: Optional chaining behavior
                    // If call is optional (obj?.method(x)), only narrow the true branch
                    // The false branch might mean the method wasn't called (obj was nullish)
                    if is_optional && !is_true_branch {
                        return type_id;
                    }

                    // Check if the guard applies to our target reference
                    if self.is_matching_reference(guard_target, target) {
                        use tracing::trace;
                        trace!(
                            ?guard,
                            ?type_id,
                            ?is_true_branch,
                            "Applying guard from call expression"
                        );
                        // Delegate to Solver for the calculation (Solver responsibility: RESULT)
                        let result = narrowing.narrow_type(type_id, &guard, is_true_branch);
                        trace!(?result, "Guard application result");
                        return result;
                    }
                }

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
                // For `if (x.flag)` where x is a discriminated union like
                // `{flag: true; data: string} | {flag: false; data: number}`,
                // narrow x by discriminant `flag === true`.
                // BUT: if the result is `never`, the type isn't actually a
                // discriminated union — fall through to truthiness narrowing.
                if let Some(property_path) = self.discriminant_property(condition_idx, target) {
                    let literal_true = self.interner.literal_boolean(true);
                    let narrowed = if is_true_branch {
                        narrowing.narrow_by_discriminant(type_id, &property_path, literal_true)
                    } else {
                        narrowing.narrow_by_excluding_discriminant(
                            type_id,
                            &property_path,
                            literal_true,
                        )
                    };
                    if narrowed != TypeId::NEVER {
                        return narrowed;
                    }
                    // Fall through: not a real discriminated union, try truthiness
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
            // Use Solver-First architecture: delegate to TypeGuard::Truthy
            _ => {
                if self.is_matching_reference(condition_idx, target) {
                    return narrowing.narrow_type(type_id, &TypeGuard::Truthy, is_true_branch);
                }
            }
        }

        type_id
    }

    /// Check if a node is a property access or element access expression.
    ///
    /// This is used to prevent discriminant guards from being applied to property
    /// access results. Discriminant guards (like `obj.kind === "a"`) should only
    /// narrow the base object (`obj`), not property access results (like `obj.value`).
    fn is_property_or_element_access(&self, node: NodeIndex) -> bool {
        let node = self.skip_parenthesized_non_recursive(node);
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || node_data.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    /// Skip parentheses (non-recursive to avoid issues with circular references).
    fn skip_parenthesized_non_recursive(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            // Limit iterations to prevent infinite loops
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return idx;
                };
                idx = paren.expression;
            } else {
                return idx;
            }
        }
        idx
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
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

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
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(ext) = self.arena.get_extended(decl_idx)
            && !ext.parent.is_none()
            && let Some(parent_node) = self.arena.get(ext.parent)
        {
            let flags = parent_node.flags as u32;
            return (flags & node_flags::CONST) != 0;
        }

        // For other node types, check the node's own flags
        let flags = decl_node.flags as u32;
        (flags & node_flags::CONST) != 0
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
        use tsz_solver::visitor::is_literal_type_db;
        if is_literal_type_db(self.interner, type_id) {
            return true;
        }

        // 3. CRITICAL: Check Unions
        // A union is a unit type if ALL its members are unit types
        // e.g. "A" | "B" | null is a unit type
        // This allows: if (x !== y) where y: "A" | "B" to narrow x correctly
        use tsz_solver::visitor::union_list_id;
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
        bin: &tsz_parser::parser::node::BinaryExprData,
        target: NodeIndex,
        is_true_branch: bool,
        narrowing: &NarrowingContext,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let operator = bin.operator_token;

        // Unwrap assignment expressions: if (flag = (x instanceof Foo)) should narrow based on RHS
        // The assignment itself doesn't provide narrowing, but its RHS might
        if operator == SyntaxKind::EqualsToken as u16 {
            if self.arena.get(bin.right).is_some() {
                // Recursively narrow based on the RHS expression
                let mut visited = Vec::new();
                return self.narrow_type_by_condition_inner(
                    type_id,
                    bin.right,
                    target,
                    is_true_branch,
                    antecedent_id,
                    &mut visited,
                );
            }
            return type_id;
        }

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
            if let Some((property_path, literal_type, is_optional, base)) =
                self.discriminant_comparison(bin.left, bin.right, target)
            {
                // CRITICAL FIX: Don't apply discriminant guards to property/element access results
                // Discriminant guards (like `obj.kind === "a"`) should only narrow the base object (`obj`),
                // not property access results (like `obj.value`).
                let is_property_access = self.is_property_or_element_access(target);

                // CRITICAL FIX: Don't apply discriminant narrowing to let-bound variables
                // in ALIASED discriminant scenarios.
                // For aliased discriminants (narrowing `data` based on `success.flag`),
                // only const-bound variables can be safely narrowed.
                // But for DIRECT discriminants (narrowing `x` based on `x.kind`),
                // we should narrow even let-bound variables because the check is on the same object.
                //
                // Example of unsafe aliased discriminant:
                //   let { data, success } = getResult();
                //   if (success) { data.method(); }  // ERROR - data is let-bound, success could change
                //
                // Example of safe direct discriminant:
                //   let x: { kind: "a" } | { kind: "b" };
                //   if (x.kind === "a") { x; }  // OK - checking x's own property
                let is_aliased_discriminant = !self.is_matching_reference(base, target);
                let is_mutable = self.is_mutable_variable(target);

                if !(is_property_access || is_aliased_discriminant && is_mutable) {
                    let mut base_type = type_id;
                    if is_optional && effective_truth {
                        let narrowed = narrowing.narrow_excluding_type(base_type, TypeId::NULL);
                        base_type = narrowing.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
                    }
                    return self.narrow_by_discriminant_for_type(
                        base_type,
                        &property_path,
                        literal_type,
                        effective_truth,
                        narrowing,
                    );
                }
                // For property access targets or aliased let-bound variables, skip discriminant narrowing
                // The property type will be computed from the already-narrowed base object
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
        bin: &tsz_parser::parser::node::BinaryExprData,
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

    pub(crate) const fn is_assignment_operator(&self, operator: u16) -> bool {
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

#[cfg(test)]
#[path = "../tests/control_flow.rs"]
mod tests;
