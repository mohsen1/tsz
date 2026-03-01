use crate::query_boundaries::flow_analysis as query;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use tsz_binder::BinderState;
use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{NarrowingContext, ParamInfo, QueryDatabase, TypeId, TypePredicate};

type FlowCache = FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>;
type ReferenceMatchCache = RefCell<FxHashMap<(u32, u32), bool>>;
type ReferenceSymbolCache = RefCell<FxHashMap<u32, Option<SymbolId>>>;
/// Instantiated type predicates from generic call resolutions, keyed by call node index.
pub(crate) type CallPredicateMap = FxHashMap<u32, (TypePredicate, Vec<ParamInfo>)>;

// Guard against pathological requeue loops in flow traversal.
const FLOW_STEP_BUDGET_MIN: usize = 1_024;
const FLOW_STEP_BUDGET_SCALE: usize = 12;
const FLOW_STEP_BUDGET_MAX: usize = 40_000;

const fn flow_step_budget(flow_node_count: usize) -> usize {
    let scaled = flow_node_count.saturating_mul(FLOW_STEP_BUDGET_SCALE);
    if scaled < FLOW_STEP_BUDGET_MIN {
        FLOW_STEP_BUDGET_MIN
    } else if scaled > FLOW_STEP_BUDGET_MAX {
        FLOW_STEP_BUDGET_MAX
    } else {
        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FLOW_STEP_BUDGET_MAX, FLOW_STEP_BUDGET_MIN, FLOW_STEP_BUDGET_SCALE, flow_step_budget,
    };

    #[test]
    fn flow_step_budget_has_minimum_floor() {
        assert_eq!(flow_step_budget(0), FLOW_STEP_BUDGET_MIN);
        assert_eq!(flow_step_budget(1), FLOW_STEP_BUDGET_MIN);
    }

    #[test]
    fn flow_step_budget_scales_with_graph_size() {
        let nodes = FLOW_STEP_BUDGET_MIN / FLOW_STEP_BUDGET_SCALE + 10;
        assert_eq!(flow_step_budget(nodes), nodes * FLOW_STEP_BUDGET_SCALE);
    }

    #[test]
    fn flow_step_budget_has_upper_cap() {
        assert_eq!(flow_step_budget(usize::MAX), FLOW_STEP_BUDGET_MAX);
    }

    #[test]
    fn flow_step_budget_caps_large_graphs() {
        let nodes = FLOW_STEP_BUDGET_MAX;
        assert_eq!(flow_step_budget(nodes), FLOW_STEP_BUDGET_MAX);
    }

    #[test]
    fn flow_step_budget_caps_large_contention_graphs_earlier() {
        // Keep pathological full-suite flow walks bounded under worker contention.
        assert_eq!(flow_step_budget(8_000), FLOW_STEP_BUDGET_MAX);
    }
}

// =============================================================================
// FlowGraph
// =============================================================================

/// A control flow graph that provides query methods for flow analysis.
///
/// This wraps the `FlowNodeArena` and provides convenient methods for querying
/// flow information during type checking.
#[derive(Debug)]
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
    /// Cache for `reference_symbol` lookups.
    /// Key: `node` -> resolved symbol (or `None` when not resolvable as a symbol).
    pub(crate) reference_symbol_cache: ReferenceSymbolCache,
    /// Optional shared reference-match cache from the checker context.
    /// When provided, this lets multiple `FlowAnalyzer` instances reuse reference
    /// equivalence results within the same file check.
    pub(crate) shared_reference_match_cache: Option<&'a ReferenceMatchCache>,
    /// Cache numeric atom conversions during a single flow walk.
    /// Key: normalized f64 bits (with +0 normalized separately from -0).
    pub(crate) numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,
    /// Optional shared numeric atom cache.
    pub(crate) shared_numeric_atom_cache: Option<&'a RefCell<FxHashMap<u64, Atom>>>,
    /// Optional shared narrowing cache.
    pub(crate) narrowing_cache: Option<&'a tsz_solver::NarrowingCache>,
    /// Instantiated type predicates from generic call resolutions.
    /// Keyed by call expression node index.
    pub(crate) call_type_predicates: Option<&'a CallPredicateMap>,
    /// Reusable buffers for flow analysis.
    pub(crate) flow_worklist: Option<&'a RefCell<VecDeque<(FlowNodeId, TypeId)>>>,
    pub(crate) flow_in_worklist: Option<&'a RefCell<FxHashSet<FlowNodeId>>>,
    pub(crate) flow_visited: Option<&'a RefCell<FxHashSet<FlowNodeId>>>,
    pub(crate) flow_results: Option<&'a RefCell<FxHashMap<FlowNodeId, TypeId>>>,
    /// Shared cache for last assignment position per symbol.
    /// Key: `SymbolId` -> last assignment byte position (0 = never reassigned).
    pub(crate) shared_symbol_last_assignment_pos:
        Option<&'a RefCell<FxHashMap<tsz_binder::SymbolId, u32>>>,
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
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            shared_numeric_atom_cache: None,
            narrowing_cache: None,
            call_type_predicates: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
            shared_symbol_last_assignment_pos: None,
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
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            shared_reference_match_cache: None,
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            shared_numeric_atom_cache: None,
            narrowing_cache: None,
            call_type_predicates: None,
            flow_worklist: None,
            flow_in_worklist: None,
            flow_visited: None,
            flow_results: None,
            shared_symbol_last_assignment_pos: None,
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

    /// Set instantiated call type predicates from generic call resolutions.
    pub const fn with_call_type_predicates(mut self, predicates: &'a CallPredicateMap) -> Self {
        self.call_type_predicates = Some(predicates);
        self
    }

    /// Set a shared numeric atom cache.
    pub const fn with_numeric_atom_cache(
        mut self,
        cache: &'a RefCell<FxHashMap<u64, Atom>>,
    ) -> Self {
        self.shared_numeric_atom_cache = Some(cache);
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

    /// Set a shared last-assignment-position cache for "effectively const" detection.
    pub const fn with_symbol_last_assignment_pos(
        mut self,
        cache: &'a RefCell<FxHashMap<tsz_binder::SymbolId, u32>>,
    ) -> Self {
        self.shared_symbol_last_assignment_pos = Some(cache);
        self
    }

    pub(crate) fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        if let Some(env) = &self.type_environment {
            return query::is_assignable_with_env(
                self.interner,
                &env.borrow(),
                source,
                target,
                false,
            );
        }
        query::is_assignable(self.interner, source, target)
    }

    pub(crate) fn is_assignable_to_strict_null(&self, source: TypeId, target: TypeId) -> bool {
        if let Some(env) = &self.type_environment {
            return query::is_assignable_with_env(
                self.interner,
                &env.borrow(),
                source,
                target,
                true,
            );
        }
        query::is_assignable_strict_null(self.interner, source, target)
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

        // Resolve symbol for caching purposes.
        // Fallback to reference_symbol for non-identifier references (e.g. some
        // qualified/member references) so repeated flow queries can share cache
        // entries instead of using per-node synthetic symbols.
        let symbol_id = self
            .binder
            .resolve_identifier(self.arena, reference)
            .or_else(|| self.reference_symbol(reference));

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
                current_type =
                    query::union_types(self.interner, vec![current_type, back_edge_type]);
            }

            // Check if we've reached a fixed point (type stopped changing)
            if current_type == prev_type {
                // Update cache with the final converged type for all intermediate keys.
                // During iteration, we inject `(loop, sym, entry_type) -> entry_type` which
                // is a pessimistic guess. Once the fixed point is reached, we must update
                // the cache so subsequent queries with initial_type=entry_type get the
                // correct converged result, not the stale intermediate.
                if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache)
                    && entry_type != current_type
                {
                    let entry_key = (loop_flow_id, sym_id, entry_type);
                    cache.borrow_mut().insert(entry_key, current_type);
                }
                return current_type;
            }
        }

        // Fixed point not reached within iteration limit
        // Conservative widening: return union of entry type and initial declared type
        // This matches TypeScript's behavior for complex loops
        let widened = query::union_types(self.interner, vec![entry_type, initial_type]);

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

        let worklist = worklist_borrow
            .as_deref_mut()
            .unwrap_or(&mut local_worklist);
        let in_worklist = in_worklist_borrow
            .as_deref_mut()
            .unwrap_or(&mut local_in_worklist);
        let visited = visited_borrow.as_deref_mut().unwrap_or(&mut local_visited);
        let results = results_borrow.as_deref_mut().unwrap_or(&mut local_results);

        // Clear buffers for reuse
        worklist.clear();
        in_worklist.clear();
        visited.clear();
        results.clear();

        // CRITICAL: Check if initial type contains type parameters ONCE, outside the loop.
        // This prevents caching generic types across different instantiations.
        // See: https://github.com/microsoft/TypeScript/issues/9998
        let initial_has_type_params = query::contains_type_parameters(self.interner, initial_type);

        // Use a synthetic cache symbol for references that don't resolve to a symbol
        // (for example complex/property references). This enables cache reuse while
        // keeping symbol-backed keys disjoint.
        let cache_symbol = symbol_id.unwrap_or(SymbolId(reference.0.wrapping_add(1) | 0x8000_0000));

        // Initialize worklist with the entry point
        worklist.push_back((flow_id, initial_type));
        in_worklist.insert(flow_id);
        let step_budget = flow_step_budget(self.binder.flow_nodes.len());
        let mut steps = 0usize;

        // Process worklist until empty
        while let Some((current_flow, current_type)) = worklist.pop_front() {
            steps += 1;
            if steps > step_budget {
                // Bail out conservatively to avoid unbounded traversal in pathological CFGs.
                return results.get(&flow_id).copied().unwrap_or(initial_type);
            }
            in_worklist.remove(&current_flow);

            // Check global cache first to avoid redundant traversals.
            // Skip cache for SWITCH_CLAUSE nodes — they must be processed to
            // schedule antecedents and apply narrowing.
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
                && let Some(cache) = self.flow_cache
            {
                let key = (current_flow, cache_symbol, initial_type);
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
                // Some flow graphs can contain self-antecedent edges on merge nodes.
                // Treat self-edges as already satisfied to avoid requeueing the same
                // node forever before it can be finalized.
                let mut all_ready = true;
                let mut check_antecedent_ready = |ant: FlowNodeId| {
                    if ant != current_flow && !visited.contains(&ant) && !results.contains_key(&ant)
                    {
                        all_ready = false;
                    }
                };
                if is_loop_header {
                    if let Some(&ant) = flow.antecedent.first() {
                        check_antecedent_ready(ant);
                    }
                } else {
                    // BRANCH/SWITCH/CALL merge points check all antecedents.
                    for &ant in &flow.antecedent {
                        check_antecedent_ready(ant);
                    }
                }

                if !all_ready {
                    // Schedule unprocessed antecedents to be processed FIRST (push_front).
                    let mut schedule_antecedent = |ant: FlowNodeId| {
                        if ant == current_flow {
                            return;
                        }
                        if !visited.contains(&ant)
                            && !results.contains_key(&ant)
                            && !in_worklist.contains(&ant)
                        {
                            worklist.push_front((ant, current_type));
                            in_worklist.insert(ant);
                        }
                    };
                    if is_loop_header {
                        if let Some(&ant) = flow.antecedent.first() {
                            schedule_antecedent(ant);
                        }
                    } else {
                        for &ant in &flow.antecedent {
                            schedule_antecedent(ant);
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
                            // For logical assignments (??=, ||=, &&=), the binder creates
                            // a two-branch flow graph: one branch for the short-circuit
                            // (original value, with condition narrowing) and one branch for
                            // the assignment (RHS value). On the assignment branch, the
                            // variable holds exactly the RHS value — skip narrow_assignment
                            // which uses mutual-subtype filtering and can fail when the RHS
                            // type is structurally different from declared union members
                            // (e.g., arrow with different return type).
                            if self.is_logical_assignment(flow.node) {
                                assigned_type
                            } else {
                                // Killing definition: replace type with RHS type and stop traversal.
                                // Use the DECLARED type for narrowing (matching tsc's getAssignmentReducedType),
                                // not initial_type which may be an already-narrowed type from loop analysis.
                                // This is critical for loops like `let code: 0|1 = 0; while(true) { code = code === 1 ? 0 : 1; }`
                                // where initial_type is `0` (narrowed) but declared type is `0|1`.
                                let declared_type = symbol_id
                                    .and_then(|sid| self.binder.get_symbol(sid))
                                    .filter(|sym| sym.value_declaration.is_some())
                                    .and_then(|sym| {
                                        self.node_types.and_then(|nt| {
                                            nt.get(&sym.value_declaration.0).copied()
                                        })
                                    });
                                let narrowing_base = declared_type.unwrap_or(initial_type);
                                self.narrow_assignment(narrowing_base, assigned_type)
                            }
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
                    // CRITICAL: If the antecedent hasn't been processed yet, we must defer to
                    // avoid losing narrowing. Without this, the worklist may process this
                    // ASSIGNMENT before its antecedent chain is resolved, using the un-narrowed
                    // type. This applies to CONDITION nodes (which directly narrow), CALL nodes
                    // (assertion functions), BRANCH_LABEL (merges), and also ASSIGNMENT chains
                    // that may themselves lead to conditions (e.g. `let v1 = x; let v2 = x;`
                    // inside an `if (x instanceof C)` block).
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
                                            | flow_flags::BRANCH_LABEL
                                            | flow_flags::ASSIGNMENT,
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
                // Start node - check if we're crossing a closure boundary.
                //
                // For "effectively mutable" captured variables (let/var that are
                // actually reassigned), we cannot trust narrowing from outer scope
                // because the closure may execute after the variable is mutated.
                //
                // For "effectively const" variables (const, or parameters/let/var
                // that are never reassigned), narrowing is preserved. This implements
                // tsc's "implicit const parameter" feature.
                let outer_flow_id = flow.antecedent.first().copied().or_else(|| {
                    // START with no antecedents - try to find outer flow via node_flow map
                    if flow.node.is_some() {
                        self.binder.node_flow.get(&flow.node.0).copied()
                    } else {
                        None
                    }
                });

                if let Some(outer_flow) = outer_flow_id {
                    if self.is_captured_variable(reference)
                        && !self.is_effectively_const_for_narrowing(reference)
                    {
                        // Captured mutable variable that IS reassigned -
                        // cannot use narrowing from outer scope
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
                    query::union_types(self.interner, types)
                }
            } else if flow.has_any_flags(flow_flags::LOOP_LABEL) {
                // LOOP_LABEL: use result_type directly from analyze_loop_fixed_point.
                // The fixed-point iteration already computes the correct union of entry
                // type and back-edge types. Re-unioning antecedent results here would
                // give the wrong answer because back-edge results are computed inside
                // analyze_loop_fixed_point's internal get_flow_type calls (which have
                // their own check_flow invocations with separate `results` maps) and
                // are NOT present in our local `results` map.
                result_type
            } else if flow.has_any_flags(flow_flags::BRANCH_LABEL) && !flow.antecedent.is_empty() {
                // Union all antecedent types for branch merge points
                let ant_types: Vec<TypeId> = flow
                    .antecedent
                    .iter()
                    .filter_map(|&ant| results.get(&ant).copied())
                    .collect();

                if ant_types.len() == 1 {
                    ant_types[0]
                } else if !ant_types.is_empty() {
                    query::union_types(self.interner, ant_types)
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
            if let Some(cache) = self.flow_cache {
                let final_has_type_params =
                    query::contains_type_parameters(self.interner, final_type);

                // Only cache if neither initial nor final types contain type parameters
                if !initial_has_type_params && !final_has_type_params {
                    let key = (current_flow, cache_symbol, initial_type);
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
}
