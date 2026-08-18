//! File-session shared caches wired as one bundle into every context-backed
//! `FlowAnalyzer`, split out of `context/mod.rs`.

use super::SymbolFlowMemoCaches;
use super::aliases::FlowAnalysisCacheMap;
use super::caches::CowCache;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::TypeId;

/// File-session shared caches wired as one bundle into every context-backed
/// `FlowAnalyzer`.
///
/// Mirrors `SymbolFlowMemoCaches`: `CheckerContext` owns one non-optional
/// bundle and `FlowAnalyzer::from_ctx` receives it whole, so partial cache
/// wiring at a `FlowAnalyzer` construction site is unrepresentable.
#[derive(Debug)]
pub struct FlowSharedCaches {
    /// Cache for control flow analysis results.
    /// Key: (`FlowNodeId`, `SymbolId`, `InitialTypeId`) -> `NarrowedTypeId`
    /// Prevents re-traversing the flow graph for the same symbol/flow combination.
    /// Fixes performance regression on binaryArithmeticControlFlowGraphNotTooLarge.ts
    /// where each operand in a + b + c was triggering fresh graph traversals.
    pub flow_analysis_cache: RefCell<CowCache<FlowAnalysisCacheMap>>,

    /// Interner that gives property/element reference *paths* (`a.b`) a
    /// session-stable synthetic cache symbol, so `flow_analysis_cache` is shared
    /// across occurrences of the same path instead of keyed per syntactic node
    /// (avoids O(N²) re-walks of the flow graph). Append-only and rebuildable;
    /// its structural-keyed cache entries are dropped on incremental save.
    pub flow_reference_keys: RefCell<FxHashMap<Vec<u32>, u32>>,

    /// Reusable buffers for flow analysis to avoid frequent heap allocations in `check_flow`.
    pub flow_worklist: RefCell<VecDeque<(tsz_binder::FlowNodeId, TypeId)>>,
    pub flow_in_worklist: RefCell<FxHashSet<tsz_binder::FlowNodeId>>,
    pub flow_visited: RefCell<FxHashSet<tsz_binder::FlowNodeId>>,
    pub flow_results: RefCell<FxHashMap<tsz_binder::FlowNodeId, TypeId>>,

    /// Shared cache for narrowing operations (type resolution, property lookup).
    /// Reused across flow analysis passes to prevent O(N^2) behavior in CFA chains.
    pub narrowing_cache: tsz_solver::narrowing::NarrowingCache,

    /// Cache for switch-reference relevance checks.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_switch_reference_cache: RefCell<FxHashMap<(u32, u32), bool>>,

    /// Cache numeric atom conversions during flow analysis.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,

    /// Cache the literal `TypeId` of a switch-case clause expression.
    /// Key: clause-expression `NodeIndex.0` -> its literal `TypeId` (or `None`
    /// when the expression is not a recognized literal). `literal_type_from_node`
    /// is a pure function of the immutable post-bind AST node, so entries are
    /// stable for the whole file check and need no invalidation within a pass.
    ///
    /// Reused across `FlowAnalyzer` instances within a single file check. Without
    /// this, `narrow_by_switch_case_clause` re-derives (and re-interns) every
    /// predecessor case label on every case, giving O(N^2) string interns for an
    /// N-arm literal switch (each case body's flow re-walk scans all earlier
    /// clauses). The cache collapses the per-switch literal materialization to
    /// O(N) total: each clause expression is interned once, then read as an O(1)
    /// hash hit.
    pub flow_switch_case_literal_cache: RefCell<FxHashMap<u32, Option<TypeId>>>,

    /// Cache whether a switch's case block consists entirely of pairwise
    /// distinct recognized literal labels (no `default`, no duplicate label).
    /// Key: case-block `NodeIndex.0` -> the per-switch boolean.
    ///
    /// `narrow_by_switch_case_clause` checks, for each clause, whether *every*
    /// earlier clause is a distinct literal so it can skip building the
    /// excluded-literal set. That predecessor scan is O(K) per clause, so an
    /// N-arm literal `switch` (e.g. a discriminated-union dispatch) re-scans
    /// O(N^2) clauses total even when each label lookup is an O(1) cache hit
    /// (the literal materialization itself is already memoized by
    /// `flow_switch_case_literal_cache`). The all-clauses-distinct property is a
    /// pure function of the immutable post-bind case block: when it holds, the
    /// per-clause predecessor check is *always* satisfied, so the whole switch's
    /// per-clause scans collapse to one O(N) pass computed once and read as an
    /// O(1) hit thereafter. Behavior is unchanged: a `false` (or absent) entry
    /// falls through to the existing per-clause logic.
    ///
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_switch_all_distinct_literals_cache: RefCell<FxHashMap<u32, bool>>,

    /// Shared reference-equivalence cache used by flow narrowing.
    /// Key: (`node_a`, `node_b`) -> whether they reference the same symbol/property chain.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_reference_match_cache: RefCell<FxHashMap<(u32, u32), bool>>,

    /// Symbol-stable flow memo tables reused across `FlowAnalyzer` instances
    /// within a single file check.
    pub symbol_flow_memo: SymbolFlowMemoCaches,

    /// Instantiated type predicates from generic call resolutions.
    /// Keyed by call expression node index. Used by flow narrowing to get
    /// predicates with inferred type arguments applied (e.g., `T` -> `string`).
    pub call_type_predicates: crate::control_flow::CallPredicateMap,

    /// Import-alias callee symbols the flow syntactic call fallback could not
    /// resolve because the alias target's type has not been computed yet
    /// (e.g. an imported generic function called inside a contextually typed
    /// closure whose flow runs before the import is ever typed — unavoidable
    /// in import cycles, where no file check order can type the provider
    /// first). Recorded by the read-only flow analyzer; drained by
    /// `check_flow_usage`, which forces `get_type_of_symbol` on each alias and
    /// re-runs flow narrowing once so the fallback can see the computed type.
    pub unresolved_import_callees: RefCell<FxHashSet<SymbolId>>,
}

impl FlowSharedCaches {
    pub fn new() -> Self {
        Self {
            flow_analysis_cache: RefCell::new(CowCache::new(FxHashMap::with_capacity_and_hasher(
                128,
                Default::default(),
            ))),
            flow_reference_keys: RefCell::new(FxHashMap::default()),
            flow_worklist: RefCell::new(VecDeque::with_capacity(32)),
            flow_in_worklist: RefCell::new(FxHashSet::default()),
            flow_visited: RefCell::new(FxHashSet::default()),
            flow_results: RefCell::new(FxHashMap::with_capacity_and_hasher(64, Default::default())),
            narrowing_cache: tsz_solver::narrowing::NarrowingCache::new(),
            flow_switch_reference_cache: RefCell::new(FxHashMap::default()),
            flow_numeric_atom_cache: RefCell::new(FxHashMap::default()),
            flow_switch_case_literal_cache: RefCell::new(FxHashMap::default()),
            flow_switch_all_distinct_literals_cache: RefCell::new(FxHashMap::default()),
            flow_reference_match_cache: RefCell::new(FxHashMap::default()),
            symbol_flow_memo: SymbolFlowMemoCaches::default(),
            call_type_predicates: crate::control_flow::CallPredicateMap::default(),
            unresolved_import_callees: RefCell::new(FxHashSet::default()),
        }
    }
}

impl Default for FlowSharedCaches {
    fn default() -> Self {
        Self::new()
    }
}
