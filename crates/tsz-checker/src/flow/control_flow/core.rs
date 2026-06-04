use crate::query_boundaries::common::QueryDatabase;

use crate::query_boundaries::flow as flow_boundary;

use crate::query_boundaries::flow_analysis as query;

use crate::query_boundaries::flow_analysis::{tuple_elements_for_type, union_members_for_type};

use crate::query_boundaries::state::checking::find_property_in_object_by_str;

use rustc_hash::{FxHashMap, FxHashSet};

use std::cell::RefCell;

use std::collections::VecDeque;

use tsz_binder::BinderState;

use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags};

use tsz_common::interner::Atom;

use tsz_parser::parser::node::{CallExprData, NodeArena};

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::computation::TypeEnvironment;

use tsz_solver::narrowing::{GuardSense, NarrowingCache, NarrowingContext};

use tsz_solver::{ParamInfo, TupleElement, TypeId, TypePredicate};

type FlowCache = FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>;

type ReferenceMatchCache = RefCell<FxHashMap<(u32, u32), bool>>;

type ReferenceSymbolCache = RefCell<FxHashMap<u32, Option<SymbolId>>>;

#[must_use]
pub(crate) fn flow_cache_entries(cache: &FlowCache) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn flow_cache_estimated_size_bytes(cache: &FlowCache) -> usize {
    cache.capacity()
        * (std::mem::size_of::<(FlowNodeId, SymbolId, TypeId)>()
            + std::mem::size_of::<TypeId>()
            + 8)
}

#[must_use]
pub(crate) fn reference_match_cache_entries(cache: &ReferenceMatchCache) -> usize {
    cache.borrow().len()
}

#[must_use]
pub(crate) fn reference_match_cache_estimated_size_bytes(cache: &ReferenceMatchCache) -> usize {
    let cache = cache.borrow();
    cache.capacity() * (std::mem::size_of::<(u32, u32)>() + std::mem::size_of::<bool>() + 8)
}

#[must_use]
pub(crate) fn reference_symbol_cache_entries(cache: &ReferenceSymbolCache) -> usize {
    cache.borrow().len()
}

#[must_use]
pub(crate) fn reference_symbol_cache_estimated_size_bytes(cache: &ReferenceSymbolCache) -> usize {
    let cache = cache.borrow();
    cache.capacity() * (std::mem::size_of::<u32>() + std::mem::size_of::<Option<SymbolId>>() + 8)
}

#[must_use]
pub(crate) fn switch_reference_cache_entries(cache: &ReferenceMatchCache) -> usize {
    reference_match_cache_entries(cache)
}

#[must_use]
pub(crate) fn switch_reference_cache_estimated_size_bytes(cache: &ReferenceMatchCache) -> usize {
    reference_match_cache_estimated_size_bytes(cache)
}

#[must_use]
pub(crate) fn numeric_atom_cache_entries(cache: &RefCell<FxHashMap<u64, Atom>>) -> usize {
    cache.borrow().len()
}

#[must_use]
pub(crate) fn numeric_atom_cache_estimated_size_bytes(
    cache: &RefCell<FxHashMap<u64, Atom>>,
) -> usize {
    let cache = cache.borrow();
    cache.capacity() * (std::mem::size_of::<u64>() + std::mem::size_of::<Atom>() + 8)
}

#[must_use]
pub(crate) fn shared_numeric_atom_cache_entries(
    cache: Option<&RefCell<FxHashMap<u64, Atom>>>,
) -> usize {
    cache.map(numeric_atom_cache_entries).unwrap_or(0)
}

#[must_use]
pub(crate) const fn shared_numeric_atom_cache_estimated_size_bytes(
    _cache: Option<&RefCell<FxHashMap<u64, Atom>>>,
) -> usize {
    0
}

/// Instantiated type predicates from generic call resolutions, keyed by call node index.
#[derive(Debug, Default)]
pub struct CallPredicateMap {
    predicates: FxHashMap<u32, (TypePredicate, Vec<ParamInfo>)>,
    invalid_assertion_calls: FxHashSet<u32>,
}

impl CallPredicateMap {
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &(TypePredicate, Vec<ParamInfo>))> {
        self.predicates.iter()
    }

    pub(crate) fn get(&self, call_idx: &u32) -> Option<&(TypePredicate, Vec<ParamInfo>)> {
        self.predicates.get(call_idx)
    }

    pub(crate) fn insert(
        &mut self,
        call_idx: u32,
        predicate: (TypePredicate, Vec<ParamInfo>),
    ) -> Option<(TypePredicate, Vec<ParamInfo>)> {
        self.invalid_assertion_calls.remove(&call_idx);
        self.predicates.insert(call_idx, predicate)
    }

    pub(crate) fn mark_invalid_assertion_call(&mut self, call_idx: u32) {
        self.predicates.remove(&call_idx);
        self.invalid_assertion_calls.insert(call_idx);
    }

    pub(crate) fn is_invalid_assertion_call(&self, call_idx: u32) -> bool {
        self.invalid_assertion_calls.contains(&call_idx)
    }
}

const FLOW_STEP_BUDGET_MIN: usize = 10_000;

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

/// Re-enqueue `current_flow` after an antecedent whose narrowing result is needed.
/// Caller must still `continue`; the helper only manages shared buffers.
fn defer_to_antecedent(
    worklist: &mut VecDeque<(FlowNodeId, TypeId)>,
    in_worklist: &mut FxHashSet<FlowNodeId>,
    ant: FlowNodeId,
    current_flow: FlowNodeId,
    current_type: TypeId,
) {
    if !in_worklist.contains(&ant) {
        worklist.push_front((ant, current_type));
        in_worklist.insert(ant);
    }
    if !in_worklist.contains(&current_flow) {
        worklist.push_back((current_flow, current_type));
        in_worklist.insert(current_flow);
    }
}

fn resolve_tuple_binding_type(
    db: &dyn QueryDatabase,
    elems: &[TupleElement],
    element_index: usize,
    is_rest: bool,
) -> Option<TypeId> {
    if is_rest {
        let rest_elem = elems
            .iter()
            .skip(element_index)
            .find(|e| e.rest)
            .or_else(|| elems.get(element_index))?;
        Some(db.factory().array(rest_elem.type_id))
    } else {
        elems.get(element_index).map(|e| e.type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FLOW_STEP_BUDGET_MAX, FLOW_STEP_BUDGET_MIN, FLOW_STEP_BUDGET_SCALE, flow_step_budget,
    };
    use super::{
        FlowCache, ReferenceMatchCache, ReferenceSymbolCache, flow_cache_entries,
        flow_cache_estimated_size_bytes, numeric_atom_cache_entries,
        numeric_atom_cache_estimated_size_bytes, reference_match_cache_entries,
        reference_match_cache_estimated_size_bytes, reference_symbol_cache_entries,
        reference_symbol_cache_estimated_size_bytes, shared_numeric_atom_cache_entries,
        shared_numeric_atom_cache_estimated_size_bytes, switch_reference_cache_entries,
        switch_reference_cache_estimated_size_bytes,
    };
    use rustc_hash::FxHashMap;
    use std::cell::RefCell;
    use tsz_binder::{FlowNodeId, SymbolId};
    use tsz_common::interner::Atom;
    use tsz_solver::TypeId;

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

    #[test]
    fn flow_cache_statistics_report_entries_and_size() {
        let mut cache = FlowCache::default();
        assert_eq!(flow_cache_entries(&cache), 0);
        assert_eq!(flow_cache_estimated_size_bytes(&cache), 0);

        cache.insert((FlowNodeId(1), SymbolId(2), TypeId(3)), TypeId(4));
        cache.insert((FlowNodeId(5), SymbolId(6), TypeId(7)), TypeId(8));

        assert_eq!(flow_cache_entries(&cache), 2);
        assert!(
            flow_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(FlowNodeId, SymbolId, TypeId)>()
                    + std::mem::size_of::<TypeId>())
        );
    }

    #[test]
    fn reference_match_cache_statistics_report_entries_and_size() {
        let cache = ReferenceMatchCache::default();
        assert_eq!(reference_match_cache_entries(&cache), 0);
        assert_eq!(reference_match_cache_estimated_size_bytes(&cache), 0);

        cache.borrow_mut().insert((1, 2), true);
        cache.borrow_mut().insert((3, 4), false);

        assert_eq!(reference_match_cache_entries(&cache), 2);
        assert!(
            reference_match_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(u32, u32)>() + std::mem::size_of::<bool>())
        );
    }

    #[test]
    fn reference_symbol_cache_statistics_report_entries_and_size() {
        let cache = ReferenceSymbolCache::default();
        assert_eq!(reference_symbol_cache_entries(&cache), 0);
        assert_eq!(reference_symbol_cache_estimated_size_bytes(&cache), 0);

        cache.borrow_mut().insert(1, Some(SymbolId(2)));
        cache.borrow_mut().insert(3, None);

        assert_eq!(reference_symbol_cache_entries(&cache), 2);
        assert!(
            reference_symbol_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<u32>() + std::mem::size_of::<Option<SymbolId>>())
        );
    }

    #[test]
    fn switch_reference_cache_statistics_report_entries_and_size() {
        let cache = ReferenceMatchCache::default();
        assert_eq!(switch_reference_cache_entries(&cache), 0);
        assert_eq!(switch_reference_cache_estimated_size_bytes(&cache), 0);

        cache.borrow_mut().insert((1, 2), true);
        cache.borrow_mut().insert((3, 4), false);

        assert_eq!(switch_reference_cache_entries(&cache), 2);
        assert!(
            switch_reference_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(u32, u32)>() + std::mem::size_of::<bool>())
        );
    }

    #[test]
    fn numeric_atom_cache_statistics_report_entries_and_size() {
        let cache = RefCell::new(FxHashMap::default());
        assert_eq!(numeric_atom_cache_entries(&cache), 0);
        assert_eq!(numeric_atom_cache_estimated_size_bytes(&cache), 0);

        cache.borrow_mut().insert(1, Atom(2));
        cache.borrow_mut().insert(3, Atom(4));

        assert_eq!(numeric_atom_cache_entries(&cache), 2);
        assert!(
            numeric_atom_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<u64>() + std::mem::size_of::<Atom>())
        );
    }

    #[test]
    fn shared_numeric_atom_cache_statistics_report_borrowed_entries_and_zero_owned_size() {
        let cache = RefCell::new(FxHashMap::default());
        assert_eq!(shared_numeric_atom_cache_entries(Some(&cache)), 0);
        assert_eq!(
            shared_numeric_atom_cache_estimated_size_bytes(Some(&cache)),
            0
        );
        assert_eq!(shared_numeric_atom_cache_entries(None), 0);
        assert_eq!(shared_numeric_atom_cache_estimated_size_bytes(None), 0);

        cache.borrow_mut().insert(1, Atom(2));

        assert_eq!(shared_numeric_atom_cache_entries(Some(&cache)), 1);
        assert_eq!(
            shared_numeric_atom_cache_estimated_size_bytes(Some(&cache)),
            0
        );
    }
}

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

/// Flow analyzer for control flow-based type narrowing.
///
/// Walks the control flow graph backwards from a reference point to determine
/// what type narrowing applies at that location.
pub struct FlowAnalyzer<'a> {
    pub(crate) arena: &'a NodeArena,
    pub(crate) binder: &'a BinderState,
    pub(crate) interner: &'a dyn QueryDatabase,
    /// Optional checker context for creating real `DefId`-backed lazy refs
    /// when the flow snapshot has not seen a symbol yet.
    pub(crate) checker_context: Option<&'a crate::context::CheckerContext<'a>>,
    pub(crate) node_types: Option<&'a crate::context::NodeTypeCache>,
    pub(crate) flow_graph: Option<FlowGraph<'a>>,
    /// Optional cache for flow analysis results to avoid redundant graph traversals
    pub(crate) flow_cache: Option<&'a RefCell<FlowCache>>,
    /// Optional `TypeEnvironment` for resolving Lazy types during narrowing
    pub(crate) type_environment: Option<&'a RefCell<TypeEnvironment>>,
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
    pub(crate) narrowing_cache: Option<&'a NarrowingCache>,
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
    pub(crate) destructured_bindings:
        Option<&'a FxHashMap<SymbolId, crate::context::DestructuredBindingInfo>>,
    pub(crate) concrete_this_type: Option<TypeId>,
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

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
