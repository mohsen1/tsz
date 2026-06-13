use super::flow_dp::{ChainReachabilityMemo, resolve_chain_reachability};
use crate::query_boundaries::common::QueryDatabase;
use crate::query_boundaries::common::{NarrowingCache, NarrowingContext, TypeEnvironment};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::flow_analysis as query;
use crate::query_boundaries::flow_analysis::union_members_for_type;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use tsz_binder::BinderState;
use tsz_binder::{FlowNode, FlowNodeArena, FlowNodeId, SymbolId, flow_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::node::{CallExprData, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{ParamInfo, TupleElement, TypeId, TypePredicate};

type FlowCache = crate::context::CowCache<FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>>;
type ReferenceMatchCache = RefCell<FxHashMap<(u32, u32), bool>>;
type ReferenceSymbolCache = RefCell<FxHashMap<u32, Option<SymbolId>>>;
/// Session-scoped interner mapping a structural reference *path*
/// (`[base_symbol_id, prop_atom, ...]`) to a sequential id. See
/// [`FlowAnalyzer::flow_reference_path_symbol`].
pub(crate) type FlowReferenceKeyInterner = RefCell<FxHashMap<Vec<u32>, u32>>;

// Flow-cache symbol space partition. The `SymbolId` slot of a `FlowCache` key
// must distinguish three disjoint kinds of reference so distinct references can
// never alias:
// - real binder symbols keep bit 31 clear;
// - structural reference *paths* (`a.b`) set bit 31, clear bit 30, and carry an
//   interned id in the low 30 bits (occurrence-independent, interned per run);
// - the per-syntactic-node fallback (`f().x`) sets bits 31 and 30 and carries
//   the node index in the low 30 bits (program-stable across runs).
/// Bit 31: any synthetic (non-binder) flow-cache symbol.
const FLOW_CACHE_SYNTHETIC_BIT: u32 = 0x8000_0000;
/// Bit 30: per-node fallback (vs. structural path) within the synthetic space.
const FLOW_CACHE_PER_NODE_BIT: u32 = 0x4000_0000;
/// Low 30 bits carrying the interned id or node index.
const FLOW_CACHE_PAYLOAD_MASK: u32 = 0x3FFF_FFFF;

/// Synthetic cache symbol for an interned structural reference-path `id`.
/// `id` must be `< FLOW_CACHE_PER_NODE_BIT` (enforced by the caller).
pub(crate) const fn structural_flow_cache_symbol(id: u32) -> SymbolId {
    SymbolId(FLOW_CACHE_SYNTHETIC_BIT | id)
}

/// Largest exclusive bound for a structural-path id before its bit would
/// collide with the per-node fallback space.
pub(crate) const FLOW_CACHE_STRUCTURAL_ID_LIMIT: u32 = FLOW_CACHE_PER_NODE_BIT;

// Reserved *base* components for `this` / `super` reference paths, which carry
// no binder symbol. These occupy position 0 of a structural key
// (`[base, prop_atom_0, ...]`). Real bases push a binder `SymbolId` whose bit 31
// is clear (`is_real_binder_symbol`), so reserving values with bit 31 set keeps
// `this.x` / `super.x` paths disjoint from every `symbol#k.x` path at position 0
// and from each other. They are key payload, not cache symbols, and never reach
// `structural_flow_cache_symbol`.
/// Structural-key base component for a `this` receiver.
pub(crate) const FLOW_CACHE_THIS_BASE_KEY: u32 = FLOW_CACHE_SYNTHETIC_BIT;
/// Structural-key base component for a `super` receiver.
pub(crate) const FLOW_CACHE_SUPER_BASE_KEY: u32 = FLOW_CACHE_SYNTHETIC_BIT | 1;

/// Per-syntactic-node fallback cache symbol for a reference `node`.
pub(crate) const fn per_node_flow_cache_symbol(node: NodeIndex) -> SymbolId {
    SymbolId(
        FLOW_CACHE_SYNTHETIC_BIT | FLOW_CACHE_PER_NODE_BIT | (node.0 & FLOW_CACHE_PAYLOAD_MASK),
    )
}

/// True when `symbol` is a real binder symbol (bit 31 clear), i.e. not in the
/// synthetic flow-cache space.
pub(crate) const fn is_real_binder_symbol(symbol: SymbolId) -> bool {
    symbol.0 & FLOW_CACHE_SYNTHETIC_BIT == 0
}

/// True when a flow-cache entry keyed by `symbol` is stable across runs and so
/// safe to persist on incremental save. Real binder symbols and per-node keys
/// qualify; structural-path keys are interned per run and must be dropped.
pub(crate) const fn is_session_stable_flow_cache_symbol(symbol: SymbolId) -> bool {
    symbol.0 & (FLOW_CACHE_SYNTHETIC_BIT | FLOW_CACHE_PER_NODE_BIT) != FLOW_CACHE_SYNTHETIC_BIT
}

mod flow_query;
mod flow_traversal;

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

// Guard against pathological requeue loops in flow traversal.
// The BFS worklist re-queues CONDITION/NARROWING nodes after scheduling their
// antecedents. For a linear flow graph with N nodes and branch conditions, the
// worklist can visit O(N²) total nodes because each condition node defers to
// antecedents and re-enqueues itself. Measured: 149 flow nodes → ~8500 steps
// (≈57×N). The minimum floor of 10_000 ensures small-to-medium files (up to
// ~170 flow nodes) complete their flow analysis correctly. The scale of 12
// and max of 40_000 keep large files bounded.
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

/// Find a symbol's representative identifier node, preferring a usage site over
/// a declaration identifier (binding/variable/parameter), because usage nodes
/// carry richer flow facts (e.g. switch discriminants).
///
/// The scan walks the whole `node_symbols` table, so it is memoized per
/// `SymbolId` when a cache is supplied: correlated destructured-narrowing queries
/// the same sibling symbols once per reference, and without the cache each query
/// re-scans the entire symbol table (`O(references · node_symbols)`).
pub(crate) fn symbol_first_identifier_ref(
    arena: &NodeArena,
    binder: &BinderState,
    cache: Option<&RefCell<FxHashMap<SymbolId, Option<NodeIndex>>>>,
    sym: SymbolId,
) -> Option<NodeIndex> {
    if let Some(cache) = cache
        && let Some(&cached) = cache.borrow().get(&sym)
    {
        return cached;
    }

    let result = compute_symbol_first_identifier_ref(arena, binder, sym);

    if let Some(cache) = cache {
        cache.borrow_mut().insert(sym, result);
    }

    result
}

fn compute_symbol_first_identifier_ref(
    arena: &NodeArena,
    binder: &BinderState,
    sym: SymbolId,
) -> Option<NodeIndex> {
    let mut declaration_ident = None;
    for (&node_id, &node_sym) in binder.node_symbols.iter() {
        if node_sym != sym {
            continue;
        }
        let idx = NodeIndex(node_id);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            continue;
        }

        let is_declaration_ident = arena
            .get_extended(idx)
            .and_then(|ext| arena.get(ext.parent))
            .is_some_and(|parent| {
                parent.kind == syntax_kind_ext::BINDING_ELEMENT
                    || parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                    || parent.kind == syntax_kind_ext::PARAMETER
            });

        if !is_declaration_ident {
            return Some(idx);
        }
        declaration_ident = Some(idx);
    }
    declaration_ident
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
#[path = "core_tests.rs"]
mod tests;

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

type AliasBaseAssignmentKey = (u32, u32);
type AliasBaseAssignmentCache = RefCell<FxHashMap<AliasBaseAssignmentKey, bool>>;
type AliasPathAssignmentKey = (u32, u32, u32);
type AliasPathAssignmentCache = RefCell<FxHashMap<AliasPathAssignmentKey, bool>>;

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
    /// Optional `TypeEnvironment` for resolving Lazy types during narrowing
    pub(crate) type_environment: Option<&'a RefCell<TypeEnvironment>>,
    /// Context-owned shared flow cache bundle. `Some` iff the analyzer was
    /// wired from a `CheckerContext` (see `FlowAnalyzer::from_ctx`). The
    /// bundle is wired whole, so partial cache wiring at a construction site
    /// is unrepresentable; isolated analyzers (unit tests) run uncached.
    pub(crate) shared: Option<&'a crate::context::FlowSharedCaches>,
    /// Cache for switch-reference relevance checks.
    /// Key: (`switch_expr_node`, `reference_node`) -> whether switch can narrow reference.
    switch_reference_cache: RefCell<FxHashMap<(u32, u32), bool>>,
    /// Cache for `is_matching_reference` results.
    /// Key: (`node_a`, `node_b`) -> whether references match (same symbol/property chain).
    /// This avoids O(N²) repeated comparisons during flow analysis with many variables.
    pub(crate) reference_match_cache: ReferenceMatchCache,
    /// Cache for `reference_symbol` lookups.
    /// Key: `node` -> resolved symbol (or `None` when not resolvable as a symbol).
    pub(crate) reference_symbol_cache: ReferenceSymbolCache,
    /// Cache numeric atom conversions during a single flow walk.
    /// Key: normalized f64 bits (with +0 normalized separately from -0).
    pub(crate) numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,
    pub(crate) destructured_bindings:
        Option<&'a FxHashMap<SymbolId, crate::context::DestructuredBindingInfo>>,
    pub(crate) concrete_this_type: Option<TypeId>,
    /// Current nesting depth of re-entrant flow-type queries. Narrowing one
    /// reference can require the flow type of *another* reference (e.g. an
    /// aliased condition or optional-chain guard), so `get_flow_type` →
    /// `check_flow` → `get_flow_type` re-enters. `flow_step_budget` bounds the
    /// work *within* a single traversal but not this nesting, so deeply nested
    /// narrowing (large modules such as the `effect` canary) would otherwise
    /// recurse until the native stack overflows. Mirrors tsc's `flowDepth`
    /// guard in `getFlowTypeOfReference`.
    pub(crate) flow_query_depth: Cell<u32>,
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
    /// Deduplicate flow merge members using identity only.
    ///
    /// Flow merges must NOT use structural assignability to eliminate types.
    /// Structural subtype reduction collapses distinct class types that share
    /// the same interface (e.g. `Derived1 | Derived2` → `Derived1` when
    /// Derived2 has all of Derived1's members), which loses narrowing
    /// information needed by subsequent control flow analysis.
    ///
    /// The solver's `union()` handles any appropriate subtype reduction
    /// when constructing the actual union type.
    fn simplify_flow_merge_types(&self, types: Vec<TypeId>) -> Vec<TypeId> {
        let mut seen = FxHashSet::with_capacity_and_hasher(types.len(), Default::default());
        let mut simplified = Vec::with_capacity(types.len());
        for ty in types {
            if seen.insert(ty) {
                simplified.push(ty);
            }
        }
        if simplified.contains(&TypeId::UNKNOWN) {
            return vec![TypeId::UNKNOWN];
        }
        simplified
    }

    fn reference_is_evolving_array_symbol(&self, reference: NodeIndex) -> bool {
        let Some(sym_id) = self.reference_symbol(reference) else {
            return false;
        };
        if self.is_control_flow_typed_any_symbol(sym_id) {
            return true;
        }

        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        let value_decl = symbol.value_declaration;
        let Some(mut decl_node) = self.arena.get(value_decl) else {
            return false;
        };
        if decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.arena.get_extended(value_decl)
            && ext.parent.is_some()
            && let Some(parent_node) = self.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
        {
            decl_node = parent_node;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.type_annotation.is_some() || decl.initializer.is_none() {
            return false;
        }
        self.arena.get(decl.initializer).is_some_and(|node| {
            node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && self
                    .arena
                    .get_literal_expr(node)
                    .is_some_and(|lit| lit.elements.nodes.is_empty())
        })
    }

    fn array_mutation_evolved_type(
        &self,
        current_type: TypeId,
        call: &CallExprData,
        reference: NodeIndex,
    ) -> (TypeId, bool) {
        if !self.reference_is_evolving_array_symbol(reference) {
            return (current_type, true);
        }

        let Some(callee_node) = self.arena.get(call.expression) else {
            return (current_type, true);
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return (current_type, true);
        };
        let Some(name_node) = self.arena.get(access.name_or_argument) else {
            return (current_type, true);
        };
        let method_name = if let Some(ident) = self.arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = self.arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return (current_type, true);
            }
        } else {
            return (current_type, true);
        };
        if method_name != "push" && method_name != "unshift" {
            return (current_type, true);
        }

        let Some(args) = &call.arguments else {
            return (current_type, true);
        };
        let Some(current_element) = query::get_array_element_type(self.interner, current_type)
        else {
            return (current_type, true);
        };

        let mut element_types = Vec::new();
        if current_element != TypeId::ANY && current_element != TypeId::NEVER {
            element_types.push(current_element);
        }
        for &arg in &args.nodes {
            if !arg.is_some() {
                continue;
            }
            let Some(arg_type) = self
                .node_types
                .and_then(|node_types| node_types.get(&arg.0).copied())
                .or_else(|| self.literal_type_from_node(arg))
            else {
                return (current_type, false);
            };
            if arg_type == TypeId::ERROR {
                return (current_type, false);
            }
            element_types.push(query::widen_literal_to_primitive(self.interner, arg_type));
        }
        if element_types.is_empty() {
            return (current_type, true);
        }

        let element_type = self.simplify_flow_merge_types(element_types);
        let element_type = if element_type.len() == 1 {
            element_type[0]
        } else {
            query::union_types(self.interner, element_type)
        };
        (query::array_type(self.interner, element_type), true)
    }

    /// Returns true when two types represent the same union member set.
    ///
    /// Used by switch-clause fallthrough merging to preserve the original
    /// pre-switch type identity (including alias/display metadata) when the
    /// merged type expands back to that same semantic union.
    fn same_union_member_set(&self, left: TypeId, right: TypeId) -> bool {
        fn normalized_union_members(db: &dyn QueryDatabase, ty: TypeId) -> Vec<TypeId> {
            if let Some(members) = union_members_for_type(db, ty) {
                let mut normalized: Vec<TypeId> = members.to_vec();
                normalized.sort_unstable_by_key(|member| member.0);
                normalized.dedup();
                normalized
            } else {
                vec![ty]
            }
        }

        normalized_union_members(self.interner, left)
            == normalized_union_members(self.interner, right)
    }

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
            checker_context: None,
            node_types: None,
            flow_graph,
            type_environment: None,
            shared: None,
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            reference_match_cache: RefCell::new(FxHashMap::default()),
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            destructured_bindings: None,
            concrete_this_type: None,
            flow_query_depth: Cell::new(0),
        }
    }

    pub fn with_node_types(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        interner: &'a dyn QueryDatabase,
        node_types: &'a crate::context::NodeTypeCache,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        Self {
            arena,
            binder,
            interner,
            checker_context: None,
            node_types: Some(node_types),
            flow_graph,
            type_environment: None,
            shared: None,
            switch_reference_cache: RefCell::new(FxHashMap::default()),
            reference_match_cache: RefCell::new(FxHashMap::default()),
            reference_symbol_cache: RefCell::new(FxHashMap::default()),
            numeric_atom_cache: RefCell::new(FxHashMap::default()),
            destructured_bindings: None,
            concrete_this_type: None,
            flow_query_depth: Cell::new(0),
        }
    }

    /// Wire the context-owned shared flow cache bundle.
    ///
    /// The bundle is all-or-nothing: every shared cache the analyzer can use
    /// comes from this one struct, so a construction site cannot wire a
    /// partial subset.
    pub const fn with_shared_caches(
        mut self,
        shared: &'a crate::context::FlowSharedCaches,
    ) -> Self {
        self.shared = Some(shared);
        self
    }

    /// Build a fully wired analyzer from a `CheckerContext`.
    ///
    /// This is the single production construction path: the context-owned
    /// `FlowSharedCaches` bundle, the type environment, the checker context,
    /// destructured-binding info, and the enclosing-class `this` type are all
    /// wired here, so individual launch sites cannot drift.
    pub fn from_ctx(ctx: &'a crate::context::CheckerContext<'a>) -> Self {
        let analyzer = Self::with_node_types(ctx.arena, ctx.binder, ctx.types, &ctx.node_types)
            .with_shared_caches(&ctx.flow_shared)
            .with_type_environment(&ctx.type_environment)
            .with_checker_context(ctx)
            .with_destructured_bindings(&ctx.destructured_bindings);

        if let Some(class_info) = &ctx.enclosing_class
            && let Some(instance_this_type) = class_info.cached_instance_this_type
        {
            return analyzer.with_concrete_this_type(instance_this_type);
        }

        analyzer
    }

    /// Shared flow-analysis cache (graph-walk results), when wired.
    #[inline]
    pub(crate) fn flow_cache(&self) -> Option<&'a RefCell<FlowCache>> {
        self.shared.map(|s| &s.flow_analysis_cache)
    }

    /// Shared reference-path key interner, when wired.
    ///
    /// Without it, references that do not resolve to a single symbol (e.g.
    /// `a.b`) fall back to a per-syntactic-node synthetic cache symbol, so each
    /// occurrence re-walks the whole flow graph (O(N²) over N occurrences). With
    /// it, every occurrence of the same path shares cache entries (O(N)).
    #[inline]
    pub(crate) fn shared_flow_reference_keys(&self) -> Option<&'a FlowReferenceKeyInterner> {
        self.shared.map(|s| &s.flow_reference_keys)
    }

    /// Shared switch-reference relevance cache, when wired.
    #[inline]
    pub(crate) fn shared_switch_reference_cache(&self) -> Option<&'a ReferenceMatchCache> {
        self.shared.map(|s| &s.flow_switch_reference_cache)
    }

    /// Shared `is_matching_reference` cache, when wired.
    #[inline]
    pub(crate) fn shared_reference_match_cache(&self) -> Option<&'a ReferenceMatchCache> {
        self.shared.map(|s| &s.flow_reference_match_cache)
    }

    /// Shared alias-base-assignment memo, when wired.
    /// Key: (`target_reference_node`, `alias_decl_pos`) -> whether any
    /// containing-function assignment after the alias declaration targets the
    /// reference or its base.
    #[inline]
    pub(crate) fn shared_alias_base_assignment_cache(
        &self,
    ) -> Option<&'a AliasBaseAssignmentCache> {
        self.shared
            .map(|s| &s.symbol_flow_memo.alias_base_assignment)
    }

    /// Shared numeric atom cache, when wired.
    #[inline]
    pub(crate) fn shared_numeric_atom_cache(&self) -> Option<&'a RefCell<FxHashMap<u64, Atom>>> {
        self.shared.map(|s| &s.flow_numeric_atom_cache)
    }

    /// Shared alias path-assignment memo, when wired.
    /// Key: (`alias_symbol`, `target_reference_node`, `antecedent_flow`) ->
    /// whether the backward path from the antecedent to the alias declaration
    /// contains an assignment to the target reference or its base.
    #[inline]
    pub(crate) fn shared_alias_path_assignment_cache(
        &self,
    ) -> Option<&'a AliasPathAssignmentCache> {
        self.shared
            .map(|s| &s.symbol_flow_memo.alias_path_assignment)
    }

    /// Shared narrowing cache, when wired.
    #[inline]
    pub(crate) fn narrowing_cache(&self) -> Option<&'a NarrowingCache> {
        self.shared.map(|s| &s.narrowing_cache)
    }

    /// Instantiated call type predicates from generic call resolutions, when wired.
    #[inline]
    pub(crate) fn call_type_predicates(&self) -> Option<&'a CallPredicateMap> {
        self.shared.map(|s| &s.call_type_predicates)
    }

    /// Reusable flow worklist buffer, when wired.
    #[inline]
    pub(crate) fn flow_worklist(&self) -> Option<&'a RefCell<VecDeque<(FlowNodeId, TypeId)>>> {
        self.shared.map(|s| &s.flow_worklist)
    }

    /// Reusable flow in-worklist set, when wired.
    #[inline]
    pub(crate) fn flow_in_worklist(&self) -> Option<&'a RefCell<FxHashSet<FlowNodeId>>> {
        self.shared.map(|s| &s.flow_in_worklist)
    }

    /// Reusable flow visited set, when wired.
    #[inline]
    pub(crate) fn flow_visited(&self) -> Option<&'a RefCell<FxHashSet<FlowNodeId>>> {
        self.shared.map(|s| &s.flow_visited)
    }

    /// Reusable flow results map, when wired.
    #[inline]
    pub(crate) fn flow_results(&self) -> Option<&'a RefCell<FxHashMap<FlowNodeId, TypeId>>> {
        self.shared.map(|s| &s.flow_results)
    }

    /// Shared last-assignment-position memo for "effectively const" detection.
    /// Key: `SymbolId` -> last assignment byte position (0 = never reassigned).
    #[inline]
    pub(crate) fn shared_symbol_last_assignment_pos(
        &self,
    ) -> Option<&'a RefCell<FxHashMap<tsz_binder::SymbolId, u32>>> {
        self.shared.map(|s| &s.symbol_flow_memo.last_assignment_pos)
    }

    /// Shared nested-closure-assignment memo for "effectively const" detection.
    /// Key: `SymbolId` -> whether any reassignment lives in a nested closure.
    /// The predicate is symbol-stable, so memoizing it avoids a per-reference
    /// full flow-node scan in `is_effectively_const_for_narrowing`.
    #[inline]
    pub(crate) fn shared_symbol_nested_closure_assignment(
        &self,
    ) -> Option<&'a RefCell<FxHashMap<tsz_binder::SymbolId, bool>>> {
        self.shared
            .map(|s| &s.symbol_flow_memo.nested_closure_assignment)
    }

    /// Shared representative-identifier memo for destructured-binding narrowing.
    /// Key: `SymbolId` -> preferred identifier node (usage over declaration), if any.
    /// Memoizes the `node_symbols` scan in correlated destructured-binding narrowing.
    #[inline]
    pub(crate) fn shared_symbol_first_identifier_ref(
        &self,
    ) -> Option<&'a RefCell<FxHashMap<tsz_binder::SymbolId, Option<NodeIndex>>>> {
        self.shared
            .map(|s| &s.symbol_flow_memo.first_identifier_ref)
    }

    pub const fn with_destructured_bindings(
        mut self,
        bindings: &'a FxHashMap<SymbolId, crate::context::DestructuredBindingInfo>,
    ) -> Self {
        self.destructured_bindings = Some(bindings);
        self
    }

    pub const fn with_concrete_this_type(mut self, concrete_this_type: TypeId) -> Self {
        self.concrete_this_type = Some(concrete_this_type);
        self
    }

    /// Check if a type contains type parameters, using the shared narrowing cache
    /// when available to avoid per-call `FxHashMap` allocation.
    fn contains_type_parameters_cached(&self, type_id: TypeId) -> bool {
        if let Some(cache) = self.narrowing_cache() {
            let cached = cache
                .contains_type_parameters_cache
                .borrow()
                .get(&type_id)
                .copied();
            if let Some(result) = cached {
                return result;
            }
            let result = query::contains_type_parameters(self.interner, type_id);
            cache
                .contains_type_parameters_cache
                .borrow_mut()
                .insert(type_id, result);
            result
        } else {
            query::contains_type_parameters(self.interner, type_id)
        }
    }

    /// Create a `NarrowingContext`, sharing the pre-allocated cache when available.
    /// This avoids 7 `FxHashMap` allocations per narrowing operation on the hot path.
    pub(super) fn make_narrowing_context(&self) -> NarrowingContext<'_> {
        if let Some(cache) = self.narrowing_cache() {
            NarrowingContext::with_cache(self.interner, cache)
        } else {
            NarrowingContext::new(self.interner)
        }
    }

    fn flow_assignability_related(&self, source: TypeId, target: TypeId) -> bool {
        let env = self.type_environment.map(std::cell::RefCell::borrow);
        query::flow_assignability_outcome(
            self.interner,
            env.as_deref(),
            self.concrete_this_type,
            source,
            target,
            false,
        )
        .related
    }

    /// Set the `TypeEnvironment` for resolving Lazy types during narrowing.
    pub const fn with_type_environment(mut self, type_env: &'a RefCell<TypeEnvironment>) -> Self {
        self.type_environment = Some(type_env);
        self
    }

    /// Set the owning checker context for stable `DefId` fallback resolution.
    pub const fn with_checker_context(
        mut self,
        ctx: &'a crate::context::CheckerContext<'a>,
    ) -> Self {
        self.checker_context = Some(ctx);
        self
    }

    /// Check if the switch expression is the literal `true` keyword.
    /// `switch(true)` is a pattern where each case clause acts as an independent
    /// type guard condition, not a comparison against the switch expression.
    pub(crate) fn is_switch_true(&self, switch_expr: NodeIndex) -> bool {
        self.arena
            .get(switch_expr)
            .is_some_and(|node| node.kind == SyntaxKind::TrueKeyword as u16)
    }

    /// Returns `true` when `flow_id`'s antecedent chain (including itself)
    /// contains a `SWITCH_CLAUSE` flow node.
    ///
    /// Used by the `check_flow` worklist to skip flow-cache reads/writes for
    /// `unknown`-typed references whose chain passes through switch clauses.
    /// Memoized in the per-`check_flow` [`ChainReachabilityMemo`] scratch so
    /// the worklist no longer allocates a fresh `VecDeque` + `FxHashSet` per
    /// iteration (#13083). The verdict is a pure property of the flow graph,
    /// which is immutable for the duration of one `check_flow` traversal, so
    /// memoized answers stay valid for the scratch lifetime. The previous
    /// one-shot BFS gave up after 32 nodes and reported `false`; the memoized
    /// form is exact, so very deep switch chains now conservatively skip the
    /// flow cache (recompute) instead of consulting it.
    fn flow_chain_contains_switch_clause_with_memo(
        &self,
        flow_id: FlowNodeId,
        memo: &mut ChainReachabilityMemo,
    ) -> bool {
        resolve_chain_reachability(
            flow_id,
            memo,
            |node| {
                let Some(flow) = self.binder.flow_nodes.get(node) else {
                    return smallvec::SmallVec::new();
                };
                flow.antecedent
                    .iter()
                    .copied()
                    .filter(|antecedent| !antecedent.is_none())
                    .collect()
            },
            |node| {
                self.binder
                    .flow_nodes
                    .get(node)
                    .is_some_and(|flow| flow.has_any_flags(flow_flags::SWITCH_CLAUSE))
            },
        )
    }

    #[inline]
    fn switch_can_affect_reference(&self, switch_expr: NodeIndex, reference: NodeIndex) -> bool {
        // switch(true) can narrow any reference — each case expression is an
        // independent condition (like an if-else chain).
        if self.is_switch_true(switch_expr) {
            return true;
        }

        let key = (switch_expr.0, reference.0);
        if let Some(shared) = self.shared_switch_reference_cache()
            && let Some(&cached) = shared.borrow().get(&key)
        {
            return cached;
        }
        if let Some(&cached) = self.switch_reference_cache.borrow().get(&key) {
            return cached;
        }

        let affects = self.is_matching_reference(switch_expr, reference)
            || self
                .relative_discriminant_path(switch_expr, reference)
                .is_some_and(|(path, _)| !path.is_empty())
            // switch (typeof x) narrows x through typeof comparison
            || self.is_typeof_target(switch_expr, reference)
            || self.is_optional_chain_containing_target(switch_expr, reference)
            // switch (alias) where alias is a const alias for reference.prop
            // (e.g. `const kind = obj.kind; switch(kind)`) or a destructuring alias
            // (e.g. `const { kind } = obj; switch(kind)`) — the aliased discriminant
            // path is resolved by narrow_by_switch_case_clause → narrow_by_binary_expr
            // → discriminant_comparison → aliased_discriminant once we allow entry.
            || self.is_aliased_discriminant_switch_expr(switch_expr, reference);

        if let Some(shared) = self.shared_switch_reference_cache() {
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
            if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache()) {
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
                if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache())
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
        if let (Some(sym_id), Some(cache)) = (symbol_id, self.flow_cache()) {
            let key = (loop_flow_id, sym_id, initial_type);
            cache.borrow_mut().insert(key, widened);
        }

        widened
    }

    /// Internal sentinel for "unreachable never" — returned by `handle_call_iterative`
    /// when a call returns `never`. This is distinct from `TypeId::NEVER` which represents
    /// legitimate narrowing to the empty type (e.g., exhaustive checks). This sentinel is
    /// used only within `check_flow` and never escapes to the rest of the system.
    ///
    /// Matches tsc's `unreachableNeverType` vs `neverType` distinction:
    /// - At `BRANCH_LABEL` merge points, `UNREACHABLE_NEVER` branches are filtered out
    /// - At the final return, `UNREACHABLE_NEVER` is mapped back to `initial_type`
    ///   (declared type), matching tsc's `getFlowTypeOfReference` behavior
    const UNREACHABLE_NEVER: TypeId = TypeId(98);

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

        // Handle fallthrough from previous case clauses.
        // When there's fallthrough, union the pre_switch type with fallthrough types
        // to get the base type for narrowing.
        let base_type = if flow.antecedent.len() > 1 {
            let mut types = vec![pre_switch_type];
            for &ant in flow.antecedent.iter().skip(1) {
                if let Some(&t) = results.get(&ant) {
                    types.push(t);
                }
            }
            let types = self.simplify_flow_merge_types(types);
            let merged = if types.len() == 1 {
                types[0]
            } else {
                query::union_types(self.interner, types)
            };
            // Preserve pre-switch identity (alias/display metadata) when the
            // fallthrough merge expands back to the same semantic union.
            // This keeps diagnostics stable for cases like `switch(true)`
            // fallthrough where `MyType` should remain `MyType` instead of
            // widening to a freshly-constructed `A | B | C` union.
            if self.same_union_member_set(merged, pre_switch_type) {
                pre_switch_type
            } else {
                merged
            }
        } else {
            pre_switch_type
        };

        // Fast path: if this switch cannot narrow the reference at all, avoid
        // per-clause narrowing setup/work (narrowing context creation, expression checks).
        if !self.switch_can_affect_reference(switch_data.expression, reference) {
            return base_type;
        }

        // Create narrowing context and wire up TypeEnvironment if available
        let env_borrow;
        let mut narrowing = self.make_narrowing_context();

        if let Some(env) = &self.type_environment {
            env_borrow = env.borrow();
            narrowing = narrowing.with_resolver(&*env_borrow);
        }

        // For implicit default, apply default clause narrowing (exclude all case types)
        if is_implicit_default {
            return self.narrow_by_default_switch_clause(
                base_type,
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
                base_type,
                switch_data.expression,
                switch_data.case_block,
                reference,
                &narrowing,
            )
        } else if self.is_switch_true(switch_data.expression) {
            // For switch(true), dispatch to a case requires prior cases to be false
            // and the current case condition to be true.
            self.narrow_by_switch_true_case_clause(
                base_type,
                switch_data.case_block,
                clause_idx,
                clause.expression,
                reference,
            )
        } else {
            self.narrow_by_switch_case_clause(
                base_type,
                switch_data.expression,
                clause_idx,
                clause.expression,
                reference,
                &narrowing,
                flow.antecedent.len() > 1,
            )
        }
    }

    fn antecedent_requires_defer(
        &self,
        antecedent: FlowNodeId,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
    ) -> bool {
        let Some(ant_flow) = self.binder.flow_nodes.get(antecedent) else {
            return false;
        };
        let ant_flags = ant_flow.flags;
        let ant_is_targeting_assignment = (ant_flags & flow_flags::ASSIGNMENT) != 0
            && ant_flow.node.is_some()
            && (symbol_id
                .zip(self.reference_symbol(ant_flow.node))
                .is_some_and(|(target, assignment)| target == assignment)
                || self.assignment_targets_reference_node(ant_flow.node, reference));

        (ant_flags & flow_flags::CONDITION) != 0
            || (ant_flags & flow_flags::CALL) != 0
            || (ant_flags & flow_flags::LOOP_LABEL) != 0
            || (ant_flags & flow_flags::BRANCH_LABEL) != 0
            || ant_is_targeting_assignment
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
        if self
            .call_type_predicates()
            .is_some_and(|calls| calls.is_invalid_assertion_call(flow.node.0))
        {
            return pre_type;
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return pre_type;
        };

        let Some(node_types) = self.node_types else {
            return pre_type;
        };

        // A never-returning call makes this branch dead; keep it distinct from
        // legitimate narrowing to `never` (for example exhaustive type checks).
        if let Some(&call_return_type) = node_types.get(&flow.node.0) {
            if call_return_type == TypeId::NEVER {
                return Self::UNREACHABLE_NEVER;
            }
            // Stale early `any` call caches still need the callee/binder fallback
            // for explicit `never` returns.
            if call_return_type == TypeId::ANY {
                if let Some(&callee_type) = node_types.get(&call.expression.0)
                    && callee_type != TypeId::ANY
                    && callee_type != TypeId::ERROR
                    && query::function_return_type(self.interner, callee_type)
                        == Some(TypeId::NEVER)
                {
                    return Self::UNREACHABLE_NEVER;
                }
                // When both caches are stale, use binder declarations directly.
                if self.callee_declaration_returns_never(call.expression) {
                    return Self::UNREACHABLE_NEVER;
                }
            }
        }
        let Some(&callee_type) = node_types.get(&call.expression.0) else {
            return pre_type;
        };

        // Cache holds solver-instantiated predicates (generic T → concrete type arg).
        // Raw callee type carries the uninstantiated signature; cache must win when present.
        let (assertion_predicate, assertion_params) = if let Some(predicates) =
            self.call_type_predicates()
            && let Some((pred, params)) = predicates.get(&flow.node.0)
            && pred.asserts
        {
            (*pred, params.clone())
        } else {
            let Some(sig) = self.predicate_signature_for_type(callee_type) else {
                return pre_type;
            };
            if !sig.predicate.asserts {
                return pre_type;
            }
            (sig.predicate, sig.params)
        };

        let Some(predicate_target) =
            self.predicate_target_expression(call, &assertion_predicate, &assertion_params)
        else {
            return pre_type;
        };

        // For generic assertion functions like `assertEqual<T>(value: any, type: T): asserts value is T`,
        // the predicate's type_id may still be an unresolved type parameter T. Resolve it by
        // matching against the call's actual argument types.
        let resolved_predicate = self.resolve_generic_predicate(
            &assertion_predicate,
            &assertion_params,
            call,
            callee_type,
            node_types,
        );

        if self.is_matching_reference(predicate_target, reference) {
            return self.apply_type_predicate_narrowing(pre_type, &resolved_predicate, true);
        }

        // Check if predicate_target is a negated expression (!innerCall)
        // This handles cases like assert(!typeGuard(x)) where we need to narrow
        // based on the inner call's predicate but with negative sense.
        if resolved_predicate.type_id.is_some()
            && let Some(pred_node) = self.arena.get(predicate_target)
            && pred_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr(pred_node)
            && unary.operator == SyntaxKind::ExclamationToken as u16
        {
            // The predicate target is !innerCall
            // Check if the inner call's argument matches our reference
            if let Some((_guard, guard_target, _is_optional)) =
                self.extract_type_guard(unary.operand)
                && self.is_matching_reference(guard_target, reference)
            {
                // Apply the guard with NEGATIVE sense (because of the !)
                return self.apply_type_predicate_narrowing(
                    pre_type,
                    &resolved_predicate,
                    false, // false = negative sense
                );
            }
        }

        // Optional-chain intermediate transport for assertion predicates:
        // `assertNonNull(o?.foo)` and similar predicates prove that the chain
        // reached its tail value, so prefix references (`o`, `o.foo` intermediates)
        // must be non-nullish after the assertion.
        //
        // IMPORTANT: do not return early here. We still need to run discriminant
        // and condition-based assertion narrowing on top of this transport.
        let mut narrowed_pre_type = pre_type;
        let mut applied_optional_chain_transport = false;
        if self.contains_optional_chain(predicate_target)
            && self.is_optional_chain_prefix(predicate_target, reference)
        {
            narrowed_pre_type =
                flow_boundary::narrow_optional_chain(self.interner.as_type_database(), pre_type);
            applied_optional_chain_transport = true;
        }

        // Discriminant narrowing: if the predicate target is a property access on the
        // reference (e.g., assertEqual(animal.type, 'cat') narrows animal from Cat|Dog to Cat),
        // extract the property path and narrow the parent object by discriminant.
        if let Some(predicate_type) = resolved_predicate.type_id
            && query::is_unit_type(self.interner, predicate_type)
            && let Some((property_path, _is_optional, base)) =
                self.discriminant_property_info(predicate_target, reference)
            && self.is_matching_reference(base, reference)
        {
            let env_borrow;
            let mut narrowing = self.make_narrowing_context();

            if let Some(env) = &self.type_environment {
                env_borrow = env.borrow();
                narrowing = narrowing.with_resolver(&*env_borrow);
            }
            return query::narrow_by_discriminant_in_context(
                &narrowing,
                narrowed_pre_type,
                &property_path,
                predicate_type,
            );
        }

        // Condition-based assertion narrowing: for `assert(condition)` where the predicate
        // has no type (just `asserts value`), the argument expression acts as a narrowing
        // condition. After the assertion, the condition is known true, so we narrow the
        // reference using the condition expression, just like an if-statement.
        // e.g., assert(typeof x === "string") narrows x to string.
        if resolved_predicate.type_id.is_none() {
            let antecedent_id = flow.antecedent.first().copied().unwrap_or(FlowNodeId::NONE);

            // Check if the predicate target is a negated expression (!predicate(x))
            // If so, we need to extract the inner type guard and apply it with negated sense.
            if let Some(pred_node) = self.arena.get(predicate_target)
                && pred_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr(pred_node)
                && unary.operator == SyntaxKind::ExclamationToken as u16
            {
                // The argument is !typeGuardCall, so typeGuardCall is false.
                // Extract the type guard from the inner call and apply with negative sense.
                if let Some((guard, guard_target, _is_optional)) =
                    self.extract_type_guard(unary.operand)
                    && self.is_matching_reference(guard_target, reference)
                {
                    let env_borrow;
                    let narrowing = if let Some(env) = &self.type_environment {
                        env_borrow = env.borrow();
                        self.make_narrowing_context().with_resolver(&*env_borrow)
                    } else {
                        self.make_narrowing_context()
                    };
                    // Apply the guard with negative sense because of the `!`.
                    let narrowed = query::narrow_with_guard_in_context(
                        &narrowing,
                        narrowed_pre_type,
                        &guard,
                        false,
                    );
                    if narrowed != narrowed_pre_type {
                        return narrowed;
                    }
                }
            }

            return self.narrow_type_by_condition(
                narrowed_pre_type,
                predicate_target,
                reference,
                true,
                antecedent_id,
            );
        }

        if applied_optional_chain_transport {
            narrowed_pre_type
        } else {
            pre_type
        }
    }

    /// Check if a callee expression's declaration has an explicit `never` return
    /// type annotation, using only binder symbol tables (no type computation).
    ///
    /// This is used as a fallback when the `node_types` cache contains a stale
    /// `any` for the call expression (common for `this.method()` during early
    /// type environment building when `this` isn't fully resolved yet).
    fn callee_declaration_returns_never(&self, callee_idx: NodeIndex) -> bool {
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return false;
        };

        match callee_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                // Direct call: `bail()`
                if let Some(&sym_id) = self.binder.node_symbols.get(&callee_idx.0) {
                    return self.symbol_declaration_returns_never(sym_id);
                }
                if let Some(sym_id) = self.binder.resolve_identifier(self.arena, callee_idx) {
                    return self.symbol_declaration_returns_never(sym_id);
                }
                false
            }
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Property access: `this.bail()` or `obj.bail()`
                let Some(access) = self.arena.get_access_expr(callee_node) else {
                    return false;
                };

                // Try binder node_symbols for the property name
                if let Some(&sym_id) = self.binder.node_symbols.get(&access.name_or_argument.0)
                    && self.symbol_declaration_returns_never(sym_id)
                {
                    return true;
                }

                // For `this.method()`, look up via enclosing class member table
                let Some(expr_node) = self.arena.get(access.expression) else {
                    return false;
                };
                if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
                    let Some(name_node) = self.arena.get(access.name_or_argument) else {
                        return false;
                    };
                    let Some(ident) = self.arena.get_identifier(name_node) else {
                        return false;
                    };
                    let property_name = &ident.escaped_text;

                    // Walk up to find the enclosing class declaration
                    if let Some(class_sym) = self.find_enclosing_class_symbol(callee_idx)
                        && let Some(class_symbol) = self.binder.get_symbol(class_sym)
                        && let Some(ref members) = class_symbol.members
                        && let Some(member_sym_id) = members.get(property_name)
                    {
                        return self.symbol_declaration_returns_never(member_sym_id);
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Check if a symbol's value declaration has an explicit `never` return type.
    fn symbol_declaration_returns_never(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_idx) = symbol.primary_declaration() else {
            return false;
        };
        self.declaration_has_never_return_type(decl_idx)
    }

    /// Check if a function/method declaration has an explicit `: never` return type annotation.
    /// Handles both direct `NeverKeyword` and `TypeReference` wrapping it.
    fn declaration_has_never_return_type(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };

        // Get the type_annotation from either a function or method declaration
        let type_annotation = if let Some(func) = self.arena.get_function(decl_node) {
            func.type_annotation
        } else if let Some(method) = self.arena.get_method_decl(decl_node) {
            method.type_annotation
        } else {
            return false;
        };

        self.type_node_is_never(type_annotation)
    }

    /// Check if a type node represents the `never` type.
    /// Handles both direct `NeverKeyword` and `TypeReference` wrapping a `never` identifier.
    fn type_node_is_never(&self, type_idx: NodeIndex) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };

        if type_node.kind == SyntaxKind::NeverKeyword as u16 {
            return true;
        }

        // `never` may be parsed as a TypeReference with type_name being a NeverKeyword
        // or an Identifier with text "never"
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.arena.get_type_ref(type_node)
            && let Some(name_node) = self.arena.get(type_ref.type_name)
        {
            if name_node.kind == SyntaxKind::NeverKeyword as u16 {
                return true;
            }
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return ident.escaped_text == "never";
            }
        }

        false
    }

    /// Find the enclosing class symbol for a node by walking up the AST parents.
    fn find_enclosing_class_symbol(&self, start: NodeIndex) -> Option<tsz_binder::SymbolId> {
        let mut current = start;
        for _ in 0..50 {
            let ext = self.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                return self.binder.get_node_symbol(current);
            }
        }
        None
    }
}
