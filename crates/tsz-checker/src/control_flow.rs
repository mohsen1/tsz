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
use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, ParamInfo, QueryDatabase, TypeId, TypePredicate};

type FlowCache = FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>;
type ReferenceMatchCache = RefCell<FxHashMap<(u32, u32), bool>>;

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
    /// Optional shared switch-reference cache.
    pub(crate) shared_switch_reference_cache: Option<&'a ReferenceMatchCache>,
    /// Cache for `is_matching_reference` results.
    /// Key: (`node_a`, `node_b`) -> whether references match (same symbol/property chain).
    /// This avoids O(N²) repeated comparisons during flow analysis with many variables.
    pub(crate) reference_match_cache: ReferenceMatchCache,
    /// Optional shared reference-match cache from the checker context.
    /// When provided, this lets multiple `FlowAnalyzer` instances reuse reference
    /// equivalence results within the same file check.
    pub(crate) shared_reference_match_cache: Option<&'a ReferenceMatchCache>,
    /// Cache numeric atom conversions during a single flow walk.
    /// Key: normalized f64 bits (with +0 normalized separately from -0).
    pub(crate) numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,
    /// Optional shared narrowing cache.
    pub(crate) narrowing_cache: Option<&'a tsz_solver::NarrowingCache>,
    /// Reusable buffers for flow analysis.
    pub(crate) flow_worklist: Option<&'a RefCell<VecDeque<(FlowNodeId, TypeId)>>>,
    pub(crate) flow_in_worklist: Option<&'a RefCell<FxHashSet<FlowNodeId>>>,
    pub(crate) flow_visited: Option<&'a RefCell<FxHashSet<FlowNodeId>>>,
    pub(crate) flow_results: Option<&'a RefCell<FxHashMap<FlowNodeId, TypeId>>>,
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
            shared_switch_reference_cache: None,
            reference_match_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            narrowing_cache: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
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
            shared_switch_reference_cache: None,
            reference_match_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            narrowing_cache: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
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

    /// Set a shared reference-match cache used by `is_matching_reference`.
    pub const fn with_reference_match_cache(mut self, cache: &'a ReferenceMatchCache) -> Self {
        self.shared_reference_match_cache = Some(cache);
        self
    }

    /// Set a shared switch-reference cache.
    pub const fn with_switch_reference_cache(mut self, cache: &'a ReferenceMatchCache) -> Self {
        self.shared_switch_reference_cache = Some(cache);
        self
    }

    /// Set a shared narrowing cache.
    pub const fn with_narrowing_cache(mut self, cache: &'a tsz_solver::NarrowingCache) -> Self {
        self.narrowing_cache = Some(cache);
        self
    }

    /// Set reusable flow buffers.
    pub const fn with_flow_buffers(
        mut self,
        worklist: &'a RefCell<VecDeque<(FlowNodeId, TypeId)>>,
        in_worklist: &'a RefCell<FxHashSet<FlowNodeId>>,
        visited: &'a RefCell<FxHashSet<FlowNodeId>>,
        results: &'a RefCell<FxHashMap<FlowNodeId, TypeId>>,
    ) -> Self {
        self.flow_worklist = Some(worklist);
        self.flow_in_worklist = Some(in_worklist);
        self.flow_visited = Some(visited);
        self.flow_results = Some(results);
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

    /// Check if the switch expression is the literal `true` keyword.
    /// `switch(true)` is a pattern where each case clause acts as an independent
    /// type guard condition, not a comparison against the switch expression.
    fn is_switch_true(&self, switch_expr: NodeIndex) -> bool {
        self.arena
            .get(switch_expr)
            .is_some_and(|node| node.kind == SyntaxKind::TrueKeyword as u16)
    }

    #[inline]
    fn switch_can_affect_reference(&self, switch_expr: NodeIndex, reference: NodeIndex) -> bool {
        // switch(true) can narrow any reference — each case expression is an
        // independent condition (like an if-else chain).
        if self.is_switch_true(switch_expr) {
            return true;
        }

        let key = (switch_expr.0, reference.0);
        if let Some(shared) = self.shared_switch_reference_cache
            && let Some(&cached) = shared.borrow().get(&key)
        {
            return cached;
        }
        if let Some(&cached) = self.switch_reference_cache.borrow().get(&key) {
            return cached;
        }

        let affects = self.is_matching_reference(switch_expr, reference)
            || self
                .discriminant_property_info(switch_expr, reference)
                .is_some_and(|(_, _, base)| self.is_matching_reference(base, reference))
            // switch (typeof x) narrows x through typeof comparison
            || self.is_typeof_target(switch_expr, reference);

        if let Some(shared) = self.shared_switch_reference_cache {
            shared.borrow_mut().insert(key, affects);
        }
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
            //
            // We inject under TWO keys: one with initial_type (for the outer check_flow's
            // cache lookup) and one with current_type (for the inner back-edge traversal
            // which uses current_type as its initial_type).
            if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache) {
                let key = (loop_flow_id, sym_id, initial_type);
                cache.borrow_mut().insert(key, current_type);
                if current_type != initial_type {
                    let inner_key = (loop_flow_id, sym_id, current_type);
                    cache.borrow_mut().insert(inner_key, current_type);
                }
            }

            // Union entry type with all back-edge types (antecedents[1+])
            for &back_edge in loop_flow.antecedent.iter().skip(1) {
                // Use current_type (the current loop assumption) as the initial type
                // for back-edge traversal instead of the declared type. This ensures
                // narrowing inside the loop body uses the loop's computed type, not
                // the full declared type. E.g., if declared type is string|number|boolean
                // but the loop only assigns string and number, narrowing typeof !== "number"
                // should give string (not string|boolean).
                let back_edge_type = self.get_flow_type(reference, current_type, back_edge);

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
        // Reusable buffers to avoid heap allocations in hot path.
        // Use try_borrow_mut to handle re-entrancy safely (e.g. during bidirectional narrowing).
        let mut local_worklist = VecDeque::new();
        let mut local_in_worklist = FxHashSet::default();
        let mut local_visited = FxHashSet::default();
        let mut local_results = FxHashMap::default();

        // Borrow shared buffers if available and NOT already borrowed, otherwise fallback to local ones
        let mut worklist_borrow = self.flow_worklist.and_then(|b| b.try_borrow_mut().ok());
        let mut in_worklist_borrow = self.flow_in_worklist.and_then(|b| b.try_borrow_mut().ok());
        let mut visited_borrow = self.flow_visited.and_then(|b| b.try_borrow_mut().ok());
        let mut results_borrow = self.flow_results.and_then(|b| b.try_borrow_mut().ok());

        let worklist = worklist_borrow.as_deref_mut()
            .unwrap_or(&mut local_worklist);
        let in_worklist = in_worklist_borrow.as_deref_mut()
            .unwrap_or(&mut local_in_worklist);
        let visited = visited_borrow.as_deref_mut()
            .unwrap_or(&mut local_visited);
        let results = results_borrow.as_deref_mut()
            .unwrap_or(&mut local_results);

        // Clear buffers for reuse
        worklist.clear();
        in_worklist.clear();
        visited.clear();
        results.clear();

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
                        // Antecedent not yet computed — defer if it could carry
                        // narrowing info we need:
                        //   CONDITION: else-if chains (nested type guards)
                        //   CALL: assertion functions
                        //   LOOP_LABEL: loop fixed-point analysis (incomplete types)
                        //   BRANCH_LABEL: merges after if-return that carry narrowed types
                        //   ASSIGNMENT: may chain through from narrowing antecedents
                        let ant_flags = self
                            .binder
                            .flow_nodes
                            .get(ant)
                            .map(|f| f.flags)
                            .unwrap_or(0);
                        let ant_needs_defer = (ant_flags & flow_flags::CONDITION) != 0
                            || (ant_flags & flow_flags::CALL) != 0
                            || (ant_flags & flow_flags::LOOP_LABEL) != 0
                            || (ant_flags & flow_flags::BRANCH_LABEL) != 0;
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
                self.handle_switch_clause_iterative(reference, current_type, flow, results)
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
                                    f.has_any_flags(
                                        flow_flags::CONDITION
                                            | flow_flags::CALL
                                            | flow_flags::BRANCH_LABEL,
                                    )
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
                self.handle_call_iterative(reference, current_type, flow, results)
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
            node.kind == syntax_kind_ext::CASE_BLOCK
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
        let mut narrowing = if let Some(cache) = self.narrowing_cache {
            NarrowingContext::with_cache(self.interner, cache)
        } else {
            NarrowingContext::new(self.interner)
        };

        if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            narrowing = narrowing.with_resolver(&*env_borrow);
        }

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
        } else if self.is_switch_true(switch_data.expression) {
            // For switch(true), each case expression is an independent condition.
            // Treat `case expr:` as `if (expr)` rather than `if (true === expr)`.
            self.narrow_type_by_condition(
                pre_switch_type,
                clause.expression,
                reference,
                true,
                FlowNodeId::NONE,
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
            let mut narrowing = if let Some(cache) = self.narrowing_cache {
                NarrowingContext::with_cache(self.interner, cache)
            } else {
                NarrowingContext::new(self.interner)
            };

            if let Some(env) = &self.type_environment {
                env_borrow = env.borrow();
                narrowing = narrowing.with_resolver(&*env_borrow);
            }
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

    /// Check if an assignment node is a mutable variable declaration (let/var) without a type annotation.
    /// Used to determine when literal types should be widened to their base types.
    pub(crate) fn is_mutable_var_decl_without_annotation(&self, node: NodeIndex) -> bool {
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
    pub(crate) fn is_var_decl_with_type_annotation(&self, node: NodeIndex) -> bool {
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

    /// Get the declared annotation type for a variable declaration node, if available.
    ///
    /// Returns `Some(type_id)` when `assignment_node` is a `VARIABLE_DECLARATION` with a
    /// type annotation whose type has already been computed and cached in `node_types`.
    /// Returns `None` otherwise (no annotation, wrong node kind, or not cached yet).
    ///
    /// The type annotation node index is used as the cache key (not the declaration node),
    /// matching how `get_type_from_type_node` caches in `node_types`.
    pub(crate) fn annotation_type_from_var_decl_node(
        &self,
        assignment_node: NodeIndex,
    ) -> Option<TypeId> {
        let decl_data = self.arena.get(assignment_node)?;
        if decl_data.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_data)?;
        if var_decl.type_annotation.is_none() {
            return None;
        }
        let node_types = self.node_types?;
        node_types.get(&var_decl.type_annotation.0).copied()
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
}
