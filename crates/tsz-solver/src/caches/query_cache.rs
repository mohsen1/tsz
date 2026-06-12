//! Cached query database implementation for the solver.
//!
//! `QueryCache` wraps a `TypeInterner` with memoization for evaluation,
//! relation, property, and element access queries. This is the concrete
//! database implementation used by the checker at runtime.

use crate::caches::db::{
    QueryDatabase, TypeApplicationEvalCache, TypeCompilerOptions, TypeDatabase,
    TypeDisplayProvenance, TypePredicateCache, TypeTupleLimitSignal,
};
use crate::caches::instantiation_cache::{InstantiationCache, InstantiationCacheKey};
use crate::caches::query_cache_statistics::{QueryCacheStatistics, RelationCacheStats};
use crate::caches::query_trace;
use crate::caches::subtype_reduction_cache::{SubtypeReductionCache, SubtypeReductionKey};
use crate::def::DefId;
use crate::evaluation::request::{EvaluationCacheKey, EvaluationRequest};
use crate::intern::TypeInterner;
use crate::objects::element_access::ElementAccessResult;
use crate::operations::property::PropertyAccessResult;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, RelationCacheValue,
    StringIntrinsicKind, SymbolRef, TemplateLiteralId, TemplateSpan, TupleElement, TupleListId,
    TypeApplication, TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
    Visibility,
};
use crate::visitor::is_error_type;
use dashmap::DashMap;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

// The trailing two `bool`s are `no_unchecked_indexed_access` and
// `exact_optional_property_types`. Evaluating a generic application can expand a
// homomorphic mapped type whose optional-modifier stripping depends on
// `exactOptionalPropertyTypes`, so both options are part of the cache identity
// (issue #10970).
type ApplicationEvalCacheKey = (DefId, smallvec::SmallVec<[TypeId; 4]>, bool, bool);
// Element access (indexed access) of an optional property includes `undefined`
// under both `exactOptionalPropertyTypes` settings (matching tsc), so the result
// does not depend on that option and it is intentionally not part of this key.
type ElementAccessTypeCacheKey = (TypeId, TypeId, Option<u32>, bool);
type PropertyAccessCacheKey = (TypeId, Atom, bool, bool);

const SUBTYPE_POLICY_TRACE_OP: &str = "is_subtype_of_with_policy";
const ASSIGNABILITY_POLICY_TRACE_OP: &str = "is_assignable_to_with_policy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachedPolicyRelation {
    Subtype,
    Assignability,
}

impl CachedPolicyRelation {
    const fn trace_op(self) -> &'static str {
        match self {
            Self::Subtype => SUBTYPE_POLICY_TRACE_OP,
            Self::Assignability => ASSIGNABILITY_POLICY_TRACE_OP,
        }
    }

    const fn relation_kind(self) -> RelationKind {
        match self {
            Self::Subtype => RelationKind::Subtype,
            Self::Assignability => RelationKind::Assignable,
        }
    }

    const fn cache_key(
        self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> RelationCacheKey {
        match self {
            Self::Subtype => RelationCacheKey::for_subtype(source, target, policy.cache_config()),
            Self::Assignability => {
                RelationCacheKey::for_assignability(source, target, policy.cache_config())
            }
        }
    }

    #[inline]
    const fn shared_slot(
        self,
        shared: &SharedQueryCache,
    ) -> &DashMap<RelationCacheKey, RelationCacheValue, FxBuildHasher> {
        match self {
            Self::Subtype => &shared.subtype_cache,
            Self::Assignability => &shared.assignability_cache,
        }
    }
}

/// Thread-safe shared query cache for cross-file type checking.
///
/// In multi-file projects (e.g., ts-toolbelt with 242 files), each file checker
/// gets its own `QueryCache` with `RefCell`-based local caches. Without sharing,
/// the same type evaluations, subtype checks, and assignability checks are
/// recomputed independently by every file checker.
///
/// `SharedQueryCache` uses `DashMap` for concurrent read/write access across
/// Rayon worker threads. Each per-file `QueryCache` checks its local cache first
/// (zero overhead), then falls back to the shared cache on miss. Results are
/// written to both local and shared caches.
///
/// Only the highest-impact caches are shared:
/// - `eval_cache`: type evaluation (conditional types, mapped types, etc.)
/// - `subtype_cache`: subtype relation results
/// - `assignability_cache`: assignability relation results
///
/// For the relation caches the sharing covers *both* the top-level
/// `is_cached_policy_relation` entry and the inner `QueryDatabase` entries
/// driven by the `SubtypeChecker`'s recursive descent. The latter is the
/// dominant cache traffic in deep mapped/conditional utility-type code, and
/// without it sibling per-file checkers re-derive the same subtree relation
/// in every file. Inner writes are gated by `cache_definitive!` in the
/// `SubtypeChecker`, so only lazy-resolution-stable results reach the
/// shared store (#10921).
///
/// `application_eval_cache` and `instantiation_cache` are intentionally NOT
/// shared cross-file: parallel file checking can observe incomplete lib-merge
/// state during the first evaluation of a generic type alias (e.g. `Promise<T>`,
/// `Awaited<T>`), producing a stale result that is then returned to sibling
/// files. Keeping those caches per-file eliminates the ordering-sensitive
/// correctness risk. See issue #9507. `TSZ_SHARE_INSTANTIATION_CACHES=1`
/// enables an experimental witness path for issue #13240; it is deliberately
/// opt-in until the ordering/staleness matrix proves it safe.
pub struct SharedQueryCache {
    eval_cache: DashMap<EvaluationCacheKey, TypeId, FxBuildHasher>,
    subtype_cache: DashMap<RelationCacheKey, RelationCacheValue, FxBuildHasher>,
    assignability_cache: DashMap<RelationCacheKey, RelationCacheValue, FxBuildHasher>,
    application_eval_cache: DashMap<ApplicationEvalCacheKey, TypeId, FxBuildHasher>,
    instantiation_cache: DashMap<InstantiationCacheKey, TypeId, FxBuildHasher>,
    share_instantiation_family: bool,
}

impl SharedQueryCache {
    pub fn new() -> Self {
        SharedQueryCache {
            eval_cache: DashMap::with_hasher(FxBuildHasher),
            subtype_cache: DashMap::with_hasher(FxBuildHasher),
            assignability_cache: DashMap::with_hasher(FxBuildHasher),
            application_eval_cache: DashMap::with_hasher(FxBuildHasher),
            instantiation_cache: DashMap::with_hasher(FxBuildHasher),
            share_instantiation_family: shared_instantiation_family_requested(),
        }
    }

    /// Number of entries across all shared caches.
    pub fn total_entries(&self) -> usize {
        self.eval_cache.len()
            + self.subtype_cache.len()
            + self.assignability_cache.len()
            + self.application_eval_cache.len()
            + self.instantiation_cache.len()
    }

    #[inline]
    fn shares_instantiation_family(&self) -> bool {
        self.share_instantiation_family
    }

    /// Estimate the resident heap bytes of the shared cache maps.
    ///
    /// Entry-count-based estimate (`DashMap` does not expose bucket
    /// capacity) for residency accounting (#13249 step 1); called once at
    /// perf-counter snapshot time, never on a checking hot path.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // DashMap per-entry overhead: bucket slot + hash + shard padding.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;
        let mut size = std::mem::size_of::<Self>();
        size += self.eval_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<EvaluationCacheKey>()
                + std::mem::size_of::<TypeId>());
        size += (self.subtype_cache.len() + self.assignability_cache.len())
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<RelationCacheKey>()
                + std::mem::size_of::<bool>());
        size += self.application_eval_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<ApplicationEvalCacheKey>()
                + std::mem::size_of::<TypeId>());
        size += self.instantiation_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<InstantiationCacheKey>()
                + std::mem::size_of::<TypeId>());
        size
    }
}

impl Default for SharedQueryCache {
    fn default() -> Self {
        Self::new()
    }
}

fn shared_instantiation_family_requested() -> bool {
    std::env::var_os("TSZ_SHARE_INSTANTIATION_CACHES").is_some_and(|value| value != "0")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationCacheProbe {
    Hit(bool),
    MissNotCached,
}

/// Query database wrapper with basic caching.
///
/// Uses `RefCell`/`Cell` instead of `RwLock`/`Atomic*` because `QueryCache`
/// borrows `&'a TypeInterner` and is inherently single-threaded. `RefCell::borrow()`
/// is a simple integer check vs `RwLock::read()`'s atomic CAS, saving overhead on
/// every subtype check, property lookup, and evaluation cache hit.
pub struct QueryCache<'a> {
    interner: &'a TypeInterner,
    eval_cache: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    /// Substitution-independent evaluation cache (see the `closed_eval` module
    /// in `evaluate`). Keyed by `(TypeId, no_unchecked_indexed_access)`.
    closed_eval_cache: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    application_eval_cache: RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    element_access_cache: RefCell<FxHashMap<ElementAccessTypeCacheKey, TypeId>>,
    object_spread_properties_cache: RefCell<FxHashMap<TypeId, Vec<PropertyInfo>>>,
    subtype_cache: RefCell<FxHashMap<RelationCacheKey, RelationCacheValue>>,
    /// Separate cache for assignability to prevent loose results from poisoning subtype checks.
    assignability_cache: RefCell<FxHashMap<RelationCacheKey, RelationCacheValue>>,
    property_cache: RefCell<FxHashMap<PropertyAccessCacheKey, PropertyAccessResult>>,
    /// Computed variance masks for generic `DefIds`.
    variance_cache: RefCell<FxHashMap<DefId, Arc<[Variance]>>>,
    /// Canonical `TypeId` for structurally identical types.
    canonical_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Cache for intersection-to-merged-object results.
    /// Avoids expensive `collect_properties` calls for the same intersection target
    /// across multiple `SubtypeChecker` instances (common in constraint checking).
    /// `Some(type_id)` = successfully merged, `None` = not eligible for merging.
    intersection_merge_cache: RefCell<FxHashMap<TypeId, Option<TypeId>>>,
    /// Cross-call cache for `instantiate_type` results, keyed by
    /// `(TypeId, CanonicalSubst, mode_bits, Option<this_type>)`.
    ///
    /// Active when solver production paths call the cache-aware instantiation
    /// entry points with `Some(&dyn QueryDatabase)`. Plain `TypeDatabase`
    /// callers keep using trait defaults and do not populate this cache.
    instantiation_cache: InstantiationCache,
    /// Cross-call cache for `remove_subtypes_for_bct` results, keyed by
    /// `(SortedTypeIds, mode_bits)`. Mirrors `subtypeReductionCache` in tsc
    /// (`TypeScript/src/compiler/checker.ts:18128-18132`). Closes the O(N²)
    /// hot loop in `compute_best_common_type` for repeated BCT call sites
    /// that share the same input list (e.g., the `BCT candidates=200` bench
    /// fixture exercises four such sites).
    subtype_reduction_cache: SubtypeReductionCache,
    application_eval_cache_hits: Cell<u64>,
    application_eval_cache_misses: Cell<u64>,
    subtype_cache_hits: Cell<u64>,
    subtype_cache_misses: Cell<u64>,
    assignability_cache_hits: Cell<u64>,
    assignability_cache_misses: Cell<u64>,
    intersection_merge_cache_hits: Cell<u64>,
    intersection_merge_cache_misses: Cell<u64>,
    instantiation_cache_hits: Cell<u64>,
    instantiation_cache_misses: Cell<u64>,
    subtype_reduction_cache_hits: Cell<u64>,
    subtype_reduction_cache_misses: Cell<u64>,
    no_unchecked_indexed_access: Cell<bool>,
    exact_optional_property_types: Cell<bool>,
    /// Optional shared cross-file cache for multi-file project checking.
    /// When present, local cache misses fall through to the shared `DashMap` cache,
    /// and local cache inserts are also written to the shared cache.
    shared: Option<&'a SharedQueryCache>,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        Self::with_optional_shared(interner, None)
    }

    /// Create a `QueryCache` backed by a shared cross-file cache.
    ///
    /// Local `RefCell`-based caches provide zero-overhead single-threaded access.
    /// On local miss, the shared `DashMap` cache is consulted. Results are written
    /// to both local and shared caches for cross-file benefit.
    pub fn new_with_shared(interner: &'a TypeInterner, shared: &'a SharedQueryCache) -> Self {
        Self::with_optional_shared(interner, Some(shared))
    }

    fn with_optional_shared(
        interner: &'a TypeInterner,
        shared: Option<&'a SharedQueryCache>,
    ) -> Self {
        QueryCache {
            interner,
            eval_cache: RefCell::new(FxHashMap::default()),
            closed_eval_cache: RefCell::new(FxHashMap::default()),
            application_eval_cache: RefCell::new(FxHashMap::default()),
            element_access_cache: RefCell::new(FxHashMap::default()),
            object_spread_properties_cache: RefCell::new(FxHashMap::default()),
            subtype_cache: RefCell::new(FxHashMap::default()),
            assignability_cache: RefCell::new(FxHashMap::default()),
            property_cache: RefCell::new(FxHashMap::default()),
            variance_cache: RefCell::new(FxHashMap::default()),
            canonical_cache: RefCell::new(FxHashMap::default()),
            intersection_merge_cache: RefCell::new(FxHashMap::default()),
            instantiation_cache: InstantiationCache::new(),
            subtype_reduction_cache: SubtypeReductionCache::new(),
            application_eval_cache_hits: Cell::new(0),
            application_eval_cache_misses: Cell::new(0),
            subtype_cache_hits: Cell::new(0),
            subtype_cache_misses: Cell::new(0),
            assignability_cache_hits: Cell::new(0),
            assignability_cache_misses: Cell::new(0),
            intersection_merge_cache_hits: Cell::new(0),
            intersection_merge_cache_misses: Cell::new(0),
            instantiation_cache_hits: Cell::new(0),
            instantiation_cache_misses: Cell::new(0),
            subtype_reduction_cache_hits: Cell::new(0),
            subtype_reduction_cache_misses: Cell::new(0),
            no_unchecked_indexed_access: Cell::new(interner.no_unchecked_indexed_access()),
            exact_optional_property_types: Cell::new(interner.exact_optional_property_types()),
            shared,
        }
    }

    pub fn clear(&self) {
        self.eval_cache.borrow_mut().clear();
        self.closed_eval_cache.borrow_mut().clear();
        self.element_access_cache.borrow_mut().clear();
        self.application_eval_cache.borrow_mut().clear();
        self.object_spread_properties_cache.borrow_mut().clear();
        self.subtype_cache.borrow_mut().clear();
        self.assignability_cache.borrow_mut().clear();
        self.property_cache.borrow_mut().clear();
        self.variance_cache.borrow_mut().clear();
        self.canonical_cache.borrow_mut().clear();
        self.intersection_merge_cache.borrow_mut().clear();
        self.instantiation_cache.clear();
        self.subtype_reduction_cache.clear();
        self.reset_relation_cache_stats();
    }

    pub fn relation_cache_stats(&self) -> RelationCacheStats {
        let subtype_entries = self.subtype_cache.borrow().len();
        let assignability_entries = self.assignability_cache.borrow().len();
        RelationCacheStats {
            subtype_hits: self.subtype_cache_hits.get(),
            subtype_misses: self.subtype_cache_misses.get(),
            subtype_entries,
            assignability_hits: self.assignability_cache_hits.get(),
            assignability_misses: self.assignability_cache_misses.get(),
            assignability_entries,
        }
    }

    /// Snapshot all cache sizes and hit/miss counters.
    ///
    /// Suitable for periodic logging or `--extendedDiagnostics`.
    pub fn statistics(&self) -> QueryCacheStatistics {
        QueryCacheStatistics {
            eval_cache_entries: self.eval_cache.borrow().len(),
            closed_eval_cache_entries: self.closed_eval_cache.borrow().len(),
            application_eval_cache_entries: self.application_eval_cache.borrow().len(),
            application_eval_cache_hits: self.application_eval_cache_hits.get(),
            application_eval_cache_misses: self.application_eval_cache_misses.get(),
            element_access_cache_entries: self.element_access_cache.borrow().len(),
            object_spread_cache_entries: self.object_spread_properties_cache.borrow().len(),
            property_cache_entries: self.property_cache.borrow().len(),
            variance_cache_entries: self.variance_cache.borrow().len(),
            canonical_cache_entries: self.canonical_cache.borrow().len(),
            intersection_merge_cache_entries: self.intersection_merge_cache.borrow().len(),
            intersection_merge_cache_hits: self.intersection_merge_cache_hits.get(),
            intersection_merge_cache_misses: self.intersection_merge_cache_misses.get(),
            instantiation_cache_entries: self.instantiation_cache.len(),
            instantiation_cache_hits: self.instantiation_cache_hits.get(),
            instantiation_cache_misses: self.instantiation_cache_misses.get(),
            subtype_reduction_cache_entries: self.subtype_reduction_cache.len(),
            subtype_reduction_cache_hits: self.subtype_reduction_cache_hits.get(),
            subtype_reduction_cache_misses: self.subtype_reduction_cache_misses.get(),
            relation: self.relation_cache_stats(),
        }
    }

    pub fn reset_relation_cache_stats(&self) {
        self.application_eval_cache_hits.set(0);
        self.application_eval_cache_misses.set(0);
        self.subtype_cache_hits.set(0);
        self.subtype_cache_misses.set(0);
        self.assignability_cache_hits.set(0);
        self.assignability_cache_misses.set(0);
        self.intersection_merge_cache_hits.set(0);
        self.intersection_merge_cache_misses.set(0);
        self.instantiation_cache_hits.set(0);
        self.instantiation_cache_misses.set(0);
        self.subtype_reduction_cache_hits.set(0);
        self.subtype_reduction_cache_misses.set(0);
    }

    pub fn probe_subtype_cache(&self, key: RelationCacheKey) -> RelationCacheProbe {
        match self.lookup_subtype_cache(key) {
            Some(result) => RelationCacheProbe::Hit(result),
            None => RelationCacheProbe::MissNotCached,
        }
    }

    fn relation_fast_path(&self, source: TypeId, target: TypeId) -> Option<bool> {
        // Fast identity/top/bottom paths avoid cache key construction,
        // RefCell borrowing, and relation engine allocation entirely.
        if source == target
            || target == TypeId::UNKNOWN
            || source == TypeId::NEVER
            || source == TypeId::ERROR
            || target == TypeId::ERROR
            || is_error_type(self.as_type_database(), source)
            || is_error_type(self.as_type_database(), target)
        {
            return Some(true);
        }
        if target == TypeId::NEVER {
            return Some(false);
        }
        // `any` is related to everything except `never`, which is already
        // handled above. This preserves the previous query-cache shortcut.
        if source == TypeId::ANY || target == TypeId::ANY {
            return Some(true);
        }
        None
    }

    #[inline]
    const fn relation_local_cache(
        &self,
        relation: CachedPolicyRelation,
    ) -> &RefCell<FxHashMap<RelationCacheKey, RelationCacheValue>> {
        match relation {
            CachedPolicyRelation::Subtype => &self.subtype_cache,
            CachedPolicyRelation::Assignability => &self.assignability_cache,
        }
    }

    #[inline]
    const fn relation_cache_hit_counter(&self, relation: CachedPolicyRelation) -> &Cell<u64> {
        match relation {
            CachedPolicyRelation::Subtype => &self.subtype_cache_hits,
            CachedPolicyRelation::Assignability => &self.assignability_cache_hits,
        }
    }

    #[inline]
    const fn relation_cache_miss_counter(&self, relation: CachedPolicyRelation) -> &Cell<u64> {
        match relation {
            CachedPolicyRelation::Subtype => &self.subtype_cache_misses,
            CachedPolicyRelation::Assignability => &self.assignability_cache_misses,
        }
    }

    /// Look up the full cached relation entry (definitive or budget-conditional).
    ///
    /// Any found entry counts as a hit; the caller decides whether a
    /// `LimitTrue` entry's fuel band makes it usable for the current query.
    fn lookup_policy_relation_cache_value(
        &self,
        relation: CachedPolicyRelation,
        key: RelationCacheKey,
    ) -> Option<RelationCacheValue> {
        if let Some(result) = self
            .relation_local_cache(relation)
            .borrow()
            .get(&key)
            .copied()
        {
            let hits = self.relation_cache_hit_counter(relation);
            hits.set(hits.get() + 1);
            return Some(result);
        }

        if let Some(shared) = self.shared
            && let Some(result) = relation.shared_slot(shared).get(&key).map(|r| *r)
        {
            self.relation_local_cache(relation)
                .borrow_mut()
                .insert(key, result);
            let hits = self.relation_cache_hit_counter(relation);
            hits.set(hits.get() + 1);
            return Some(result);
        }

        let misses = self.relation_cache_miss_counter(relation);
        misses.set(misses.get() + 1);
        None
    }

    /// Boolean view of the relation cache: surfaces only definitive entries.
    fn lookup_policy_relation_cache(
        &self,
        relation: CachedPolicyRelation,
        key: RelationCacheKey,
    ) -> Option<bool> {
        self.lookup_policy_relation_cache_value(relation, key)
            .and_then(RelationCacheValue::as_definitive)
    }

    fn insert_policy_relation_cache(
        &self,
        relation: CachedPolicyRelation,
        key: RelationCacheKey,
        result: bool,
    ) {
        let value = RelationCacheValue::from_bool(result);
        self.relation_local_cache(relation)
            .borrow_mut()
            .insert(key, value);
        if let Some(shared) = self.shared {
            relation.shared_slot(shared).insert(key, value);
        }
    }

    /// Whether a `LimitTrue` entry's fuel band covers the current query's
    /// remaining global subtype fuel budget (and the policy is enabled).
    fn limit_true_usable(fuel_band: u32) -> bool {
        crate::limits::limit_result_cache_enabled()
            && crate::relations::subtype::cache::remaining_global_subtype_fuel() <= fuel_band
    }

    fn is_cached_policy_relation(
        &self,
        relation: CachedPolicyRelation,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        if let Some(result) = self.relation_fast_path(source, target) {
            return result;
        }

        let trace_enabled = query_trace::enabled();
        let trace_op = relation.trace_op();
        let cache_config = policy.cache_config();
        let trace_query_id = trace_enabled.then(|| {
            let query_id = query_trace::next_query_id();
            query_trace::relation_start(query_id, trace_op, source, target, cache_config);
            query_id
        });
        let key = relation.cache_key(source, target, policy);

        match self.lookup_policy_relation_cache_value(relation, key) {
            Some(RelationCacheValue::True) => {
                if let Some(query_id) = trace_query_id {
                    query_trace::relation_end(query_id, trace_op, true, true);
                }
                return true;
            }
            Some(RelationCacheValue::False) => {
                if let Some(query_id) = trace_query_id {
                    query_trace::relation_end(query_id, trace_op, false, true);
                }
                return false;
            }
            Some(RelationCacheValue::LimitTrue { fuel_band })
                if Self::limit_true_usable(fuel_band) =>
            {
                tsz_common::perf_counters::record_relation_limit_cache_hit();
                if let Some(query_id) = trace_query_id {
                    query_trace::relation_end(query_id, trace_op, true, true);
                }
                return true;
            }
            // A larger budget is available: recompute honestly below and
            // let the definitive insert overwrite the limit entry.
            Some(RelationCacheValue::LimitTrue { .. }) | None => {}
        }

        let result = query_relation(
            self.as_type_database(),
            source,
            target,
            relation.relation_kind(),
            policy,
            RelationContext::default(),
        )
        .related;

        self.insert_policy_relation_cache(relation, key, result);
        if let Some(query_id) = trace_query_id {
            query_trace::relation_end(query_id, trace_op, result, false);
        }
        result
    }

    fn check_property_cache(&self, key: PropertyAccessCacheKey) -> Option<PropertyAccessResult> {
        self.property_cache.borrow().get(&key).copied()
    }

    fn insert_property_cache(&self, key: PropertyAccessCacheKey, result: PropertyAccessResult) {
        self.property_cache.borrow_mut().insert(key, result);
    }

    fn check_element_access_cache(&self, key: ElementAccessTypeCacheKey) -> Option<TypeId> {
        self.element_access_cache.borrow().get(&key).copied()
    }

    fn insert_element_access_cache(&self, key: ElementAccessTypeCacheKey, result: TypeId) {
        self.element_access_cache.borrow_mut().insert(key, result);
    }

    fn check_application_eval_cache(&self, key: ApplicationEvalCacheKey) -> Option<TypeId> {
        if let Some(result) = self.application_eval_cache.borrow().get(&key).copied() {
            self.application_eval_cache_hits
                .set(self.application_eval_cache_hits.get() + 1);
            return Some(result);
        }
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
            && let Some(result) = shared.application_eval_cache.get(&key).map(|entry| *entry)
        {
            self.application_eval_cache.borrow_mut().insert(key, result);
            self.application_eval_cache_hits
                .set(self.application_eval_cache_hits.get() + 1);
            return Some(result);
        }
        self.application_eval_cache_misses
            .set(self.application_eval_cache_misses.get() + 1);
        None
    }

    fn insert_application_eval_cache(&self, key: ApplicationEvalCacheKey, result: TypeId) {
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
        {
            shared.application_eval_cache.insert(key.clone(), result);
        }
        self.application_eval_cache.borrow_mut().insert(key, result);
    }

    fn check_object_spread_properties_cache(&self, key: TypeId) -> Option<Vec<PropertyInfo>> {
        self.object_spread_properties_cache
            .borrow()
            .get(&key)
            .cloned()
    }

    fn insert_object_spread_properties_cache(&self, key: TypeId, value: Vec<PropertyInfo>) {
        self.object_spread_properties_cache
            .borrow_mut()
            .insert(key, value);
    }

    fn collect_object_spread_properties_inner(
        &self,
        spread_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> Vec<PropertyInfo> {
        let normalized =
            self.evaluate_type_with_options(spread_type, self.no_unchecked_indexed_access());

        if !visited.insert(normalized) {
            return Vec::new();
        }

        if normalized != spread_type {
            return self.collect_object_spread_properties_inner(normalized, visited);
        }

        let Some(key) = self.interner.lookup(normalized) else {
            return Vec::new();
        };

        let props = match key {
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                self.interner.object_shape(shape_id).properties.to_vec()
            }
            TypeData::Callable(shape_id) => {
                self.interner.callable_shape(shape_id).properties.to_vec()
            }
            TypeData::Intersection(members_id) => {
                let members = self.interner.type_list(members_id);
                let mut merged: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

                for &member in members.iter() {
                    for prop in self.collect_object_spread_properties_inner(member, visited) {
                        merged.insert(prop.name, prop);
                    }
                }

                merged.into_values().collect()
            }
            TypeData::Union(members_id) => {
                let members = self.interner.type_list(members_id);
                // Collect properties from non-nullish union members.
                // Nullish members (null, undefined, void) spread to {} and
                // contribute no properties. Properties that don't appear in
                // every non-nullish member become optional.
                let has_nullish = members.iter().any(|m| m.is_nullable());
                let non_nullish_count = members.iter().filter(|m| !m.is_nullable()).count();

                if non_nullish_count == 0 {
                    return Vec::new();
                }

                // Collect properties per member
                let mut all_props: Vec<Vec<PropertyInfo>> = Vec::with_capacity(non_nullish_count);
                for &member in members.iter().filter(|m| !m.is_nullable()) {
                    all_props.push(self.collect_object_spread_properties_inner(member, visited));
                }

                // Merge: a property appears in the result if it exists in at
                // least one member. Its type is the union of types across
                // members where it appears. It is optional if it doesn't
                // appear in all non-nullish members or if any nullish member
                // exists (since the spread could be null/undefined → {}).
                let mut merged: FxHashMap<Atom, (TypeId, bool, usize)> = FxHashMap::default();
                for member_props in &all_props {
                    for prop in member_props {
                        let entry =
                            merged
                                .entry(prop.name)
                                .or_insert((prop.type_id, prop.optional, 0));
                        if entry.0 != prop.type_id {
                            entry.0 = self.interner.union2(entry.0, prop.type_id);
                        }
                        entry.1 = entry.1 && prop.optional;
                        entry.2 += 1;
                    }
                }

                merged
                    .into_iter()
                    .map(|(name, (type_id, was_optional, count))| {
                        let optional = was_optional || has_nullish || count < non_nullish_count;
                        PropertyInfo {
                            name,
                            type_id,
                            optional,
                            readonly: false,
                            write_type: type_id,
                            is_class_prototype: false,
                            is_method: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: 0,
                            is_string_named: false,
                            is_symbol_named: false,
                            single_quoted_name: false,
                        }
                    })
                    .collect()
            }
            TypeData::TypeParameter(info) => {
                // For type parameters with constraints (e.g. `T extends { x: number }`),
                // collect properties from the constraint. Required properties in the
                // constraint are guaranteed to exist on any value of type T.
                if let Some(constraint) = info.constraint {
                    return self.collect_object_spread_properties_inner(constraint, visited);
                }
                Vec::new()
            }
            _ => Vec::new(),
        };

        // Spread removes readonly modifiers from properties (TypeScript spec).
        // `{ ...readonlyObj }` produces a mutable copy.
        // Also reset write_type to match type_id so the property is fully writable.
        // Class prototype members (methods/accessors) are excluded from spread results
        // because they live on the prototype, not as own enumerable properties.
        // This matches tsc's isSpreadPrototypeProperty() behavior.
        props
            .into_iter()
            .filter(|p| {
                !p.is_class_prototype
                    && p.visibility == Visibility::Public
                    && !self
                        .resolve_atom_ref(p.name)
                        .starts_with("__private_brand_")
            })
            .map(|mut p| {
                p.readonly = false;
                p.write_type = p.type_id;
                p
            })
            .collect()
    }
}

impl TypePredicateCache for QueryCache<'_> {
    fn contains_this_type_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_this_type_cached(type_id)
    }

    fn set_contains_this_type_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_this_type_cache(type_id, result);
    }

    fn contains_infer_types_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_infer_types_cached(type_id)
    }

    fn set_contains_infer_types_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_infer_types_cache(type_id, result);
    }

    fn contains_type_query_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_type_query_cached(type_id)
    }

    fn set_contains_type_query_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_contains_type_query_cache(type_id, result);
    }

    fn contains_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_type_params_cached(type_id)
    }

    fn set_contains_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_type_params_cache(type_id, result);
    }

    fn contains_lazy_or_recursive_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_lazy_or_recursive_cached(type_id)
    }

    fn set_contains_lazy_or_recursive_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_lazy_or_recursive_cache(type_id, result);
    }

    fn contains_unresolved_application_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner
            .contains_unresolved_application_cached(type_id)
    }

    fn set_contains_unresolved_application_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_unresolved_application_cache(type_id, result);
    }

    fn contains_resolver_dependent_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_resolver_dependent_cached(type_id)
    }

    fn set_contains_resolver_dependent_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_resolver_dependent_cache(type_id, result);
    }

    fn contains_conditional_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_conditional_cached(type_id)
    }

    fn set_contains_conditional_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_conditional_cache(type_id, result);
    }

    fn contains_param_or_infer_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_param_or_infer_root_cached(type_id)
    }

    fn set_contains_param_or_infer_root_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_param_or_infer_root_cache(type_id, result);
    }

    fn contains_generic_params_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_generic_params_root_cached(type_id)
    }

    fn set_contains_generic_params_root_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_generic_params_root_cache(type_id, result);
    }

    fn eval_contains_infer_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.eval_contains_infer_cached(type_id)
    }

    fn set_eval_contains_infer_cache(&self, type_id: TypeId, result: bool) {
        self.interner.set_eval_contains_infer_cache(type_id, result);
    }

    fn contains_file_relative_cached(&self, type_id: TypeId) -> Option<bool> {
        self.interner.contains_file_relative_cached(type_id)
    }

    fn set_contains_file_relative_cache(&self, type_id: TypeId, result: bool) {
        self.interner
            .set_contains_file_relative_cache(type_id, result);
    }
}

impl TypeTupleLimitSignal for QueryCache<'_> {
    fn take_tuple_too_large(&self) -> bool {
        self.interner.take_tuple_too_large()
    }

    fn mark_tuple_too_large(&self) {
        self.interner.set_tuple_too_large();
    }
}

impl TypeDisplayProvenance for QueryCache<'_> {
    fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.interner.store_display_properties(type_id, props);
    }

    fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.interner.get_display_properties(type_id)
    }

    fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        self.interner.store_display_alias(evaluated, application);
    }

    fn store_display_alias_preferring_application(&self, evaluated: TypeId, application: TypeId) {
        self.interner
            .store_display_alias_preferring_application(evaluated, application);
    }

    fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.get_display_alias(type_id)
    }

    fn record_application_eval_origin(&self, evaluated: TypeId, application: TypeId) {
        self.interner
            .record_application_eval_origin(evaluated, application);
    }

    fn get_application_eval_origin(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.get_application_eval_origin(type_id)
    }

    fn mark_conditional_alias_base(&self, base: TypeId) {
        self.interner.mark_conditional_alias_base(base);
    }

    fn is_conditional_alias_base(&self, base: TypeId) -> bool {
        self.interner.is_conditional_alias_base(base)
    }

    fn store_union_origin(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        self.interner
            .store_union_origin(union_type_id, origin_members);
    }

    fn replace_union_origin_for_display(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        self.interner
            .replace_union_origin_for_display(union_type_id, origin_members);
    }

    fn get_union_origin(&self, type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        self.interner.get_union_origin(type_id)
    }

    fn take_union_too_complex(&self) -> bool {
        self.interner.take_union_too_complex()
    }

    fn is_union_too_complex(&self) -> bool {
        self.interner.is_union_too_complex()
    }

    fn mark_union_too_complex(&self) {
        self.interner.set_union_too_complex();
    }
}

impl TypeCompilerOptions for QueryCache<'_> {
    fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access.get()
    }

    fn exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types.get()
    }
}

impl TypeApplicationEvalCache for QueryCache<'_> {
    fn lookup_application_eval_cache(
        &self,
        def_id: DefId,
        args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.check_application_eval_cache((
            def_id,
            smallvec::SmallVec::from_slice(args),
            no_unchecked_indexed_access,
            self.exact_optional_property_types(),
        ))
    }

    fn insert_application_eval_cache(
        &self,
        def_id: DefId,
        args: &[TypeId],
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        QueryCache::insert_application_eval_cache(
            self,
            (
                def_id,
                smallvec::SmallVec::from_slice(args),
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ),
            result,
        );
    }

    fn invalidate_application_eval_cache_for_def(&self, def_id: DefId) {
        let mut cache = self.application_eval_cache.borrow_mut();
        if !cache.is_empty() {
            cache.retain(|(key_def, key_args, _, _), &mut result| {
                *key_def != def_id
                    && !key_args.iter().any(|&arg| {
                        crate::visitors::visitor::contains_lazy_def_id(self.interner, arg, def_id)
                    })
                    && !crate::visitors::visitor::contains_lazy_def_id(
                        self.interner,
                        result,
                        def_id,
                    )
            });
        }
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
            && !shared.application_eval_cache.is_empty()
        {
            shared
                .application_eval_cache
                .retain(|(key_def, key_args, _, _), result| {
                    *key_def != def_id
                        && !key_args.iter().any(|&arg| {
                            crate::visitors::visitor::contains_lazy_def_id(
                                self.interner,
                                arg,
                                def_id,
                            )
                        })
                        && !crate::visitors::visitor::contains_lazy_def_id(
                            self.interner,
                            *result,
                            def_id,
                        )
                });
        }
    }

    fn lookup_closed_eval_cache(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.closed_eval_cache
            .borrow()
            .get(&EvaluationCacheKey::new(
                type_id,
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ))
            .copied()
    }

    fn insert_closed_eval_cache(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        self.closed_eval_cache.borrow_mut().insert(
            EvaluationCacheKey::new(
                type_id,
                no_unchecked_indexed_access,
                self.exact_optional_property_types(),
            ),
            result,
        );
    }
}

impl TypeDatabase for QueryCache<'_> {
    fn intern(&self, key: TypeData) -> TypeId {
        self.interner.intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        self.interner.lookup(id)
    }

    fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        self.interner.lookup_alloc_order(id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        self.interner.intern_string(s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        self.interner.resolve_atom(atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.interner.resolve_atom_ref(atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.interner.type_list(id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.interner.tuple_list(id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.interner.template_list(id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.interner.object_shape(id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        self.interner.object_property_index(shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.interner.function_shape(id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.interner.callable_shape(id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.interner.conditional_type(id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.interner.mapped_type(id)
    }

    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        self.interner.get_conditional(id)
    }

    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        self.interner.get_mapped(id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.interner.type_application(id)
    }

    fn shared_def_variance(&self, def_id: DefId) -> Option<(Arc<[Variance]>, Arc<[DefId]>)> {
        self.interner.shared_def_variance(def_id)
    }

    fn insert_shared_def_variance(&self, def_id: DefId, mask: Arc<[Variance]>, gaps: Arc<[DefId]>) {
        self.interner.insert_shared_def_variance(def_id, mask, gaps);
    }

    fn literal_string(&self, value: &str) -> TypeId {
        self.interner.literal_string(value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        self.interner.literal_number(value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        self.interner.literal_boolean(value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        self.interner.literal_bigint(value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.interner.literal_bigint_with_sign(negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union(members)
    }

    fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        self.interner.union_from_slice(members)
    }

    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union_literal_reduce(members)
    }

    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        self.interner.union_from_sorted_vec(flat)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.union2(left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.interner.union3(first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.intersection(members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.intersection2(left, right)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.intersect_types_raw2(left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        self.interner.array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.interner.tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.interner.object(properties)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        self.interner.object_with_flags(properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        self.interner
            .object_with_flags_and_symbol(properties, flags, symbol)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.interner.object_with_index(shape)
    }

    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.interner.object_type_from_shape(shape_id)
    }

    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.interner.object_with_index_type_from_shape(shape_id)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        self.interner.function(shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        self.interner.callable(shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.interner.template_literal(spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.interner.conditional(conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        self.interner.mapped(mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.interner.reference(symbol)
    }

    fn lazy(&self, def_id: DefId) -> TypeId {
        self.interner.lazy(def_id)
    }

    fn bound_parameter(&self, index: u32) -> TypeId {
        self.interner.bound_parameter(index)
    }

    fn recursive(&self, depth: u32) -> TypeId {
        self.interner.recursive(depth)
    }

    fn type_param(&self, info: TypeParamInfo) -> TypeId {
        self.interner.type_param(info)
    }

    fn unresolved_type_name(&self, name: Atom) -> TypeId {
        self.interner.unresolved_type_name(name)
    }

    fn type_query(&self, symbol: SymbolRef) -> TypeId {
        self.interner.type_query(symbol)
    }

    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        self.interner.enum_type(def_id, structural_type)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.interner.application(base, args)
    }

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.interner.literal_string_atom(atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union_preserve_members(members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.interner.readonly_type(inner)
    }

    fn keyof(&self, inner: TypeId) -> TypeId {
        self.interner.keyof(inner)
    }

    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.interner.index_access(object_type, index_type)
    }

    fn this_type(&self) -> TypeId {
        self.interner.this_type()
    }

    fn no_infer(&self, inner: TypeId) -> TypeId {
        self.interner.no_infer(inner)
    }

    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        self.interner.unique_symbol(symbol)
    }

    fn infer(&self, info: TypeParamInfo) -> TypeId {
        self.interner.infer(info)
    }

    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        self.interner.string_intrinsic(kind, type_arg)
    }

    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId> {
        // Delegate to the interner
        self.interner.get_class_base_type(symbol_id)
    }

    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        self.interner.is_identity_comparable_type(type_id)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.interner.get_array_base_type_params()
    }

    fn get_array_display_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_display_base_type()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_readonly_array_base_type()
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.interner.get_boxed_type(kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        self.interner.is_boxed_def_id(def_id, kind)
    }

    fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        self.interner.is_this_type_marker_def_id(def_id)
    }

    fn consume_evaluation_fuel(&self, amount: u32) -> bool {
        self.interner.consume_evaluation_fuel(amount)
    }

    fn is_evaluation_fuel_exhausted(&self) -> bool {
        self.interner.is_evaluation_fuel_exhausted()
    }

    fn reset_evaluation_fuel(&self) {
        self.interner.reset_evaluation_fuel();
    }
}

/// Implement `TypeResolver` for `QueryCache` with noop resolution.
///
/// `QueryCache` doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or `DefIds`. Only `resolve_ref`
/// (required) is explicitly implemented; all other resolution methods
/// inherit the trait's default `None`/`false` behavior. The three boxed/array
/// methods delegate to the underlying interner.
impl TypeResolver for QueryCache<'_> {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.interner.get_boxed_type(kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.interner.get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_readonly_array_base_type()
    }
}

impl QueryDatabase for QueryCache<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn fresh_type_param(&self, info: TypeParamInfo) -> TypeId {
        self.interner.fresh_type_param(info)
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.interner.set_array_base_type(type_id, type_params);
    }

    fn register_array_display_base_type(&self, type_id: TypeId) {
        self.interner.set_array_display_base_type(type_id);
    }

    fn register_readonly_array_base_type(&self, type_id: TypeId) {
        self.interner.set_readonly_array_base_type(type_id);
    }

    fn register_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        self.interner.set_boxed_type(kind, type_id);
    }

    fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        self.interner.register_boxed_def_id(kind, def_id);
    }

    fn register_this_type_def_id(&self, def_id: DefId) {
        self.interner.register_this_type_def_id(def_id);
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_options(type_id, self.no_unchecked_indexed_access())
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        // Fast path: intrinsic types never need evaluation
        if type_id.is_intrinsic() {
            return type_id;
        }

        let request = EvaluationRequest::new(type_id)
            .with_no_unchecked_indexed_access(no_unchecked_indexed_access)
            .with_exact_optional_property_types(self.exact_optional_property_types());
        let key = request.cache_key();
        let cached = self.eval_cache.borrow().get(&key).copied();

        if let Some(result) = cached {
            return result;
        }

        // L2: Check shared cross-file cache before doing expensive evaluation.
        if let Some(shared) = self.shared
            && let Some(result) = shared.eval_cache.get(&key).map(|r| *r)
        {
            self.eval_cache.borrow_mut().insert(key, result);
            return result;
        }

        // Fast path: leaf types that never change during evaluation.
        // Skip TypeEvaluator creation for types where visit_type_key returns type_id unchanged.
        if let Some(
            TypeData::Literal(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::TypeParameter(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Infer(_)
            | TypeData::Enum(_, _)
            | TypeData::BoundParameter(_)
            | TypeData::Recursive(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::ReadonlyType(_)
            | TypeData::Error,
        ) = self.interner.lookup(type_id)
        {
            self.eval_cache.borrow_mut().insert(key, type_id);
            return type_id;
        }

        let trace_enabled = query_trace::enabled();
        let trace_query_id = trace_enabled.then(|| {
            let query_id = query_trace::next_query_id();
            query_trace::unary_start(
                query_id,
                "evaluate_type_with_options",
                type_id,
                no_unchecked_indexed_access,
            );
            query_id
        });

        let union_too_complex_before = self.interner.is_union_too_complex();
        let mut evaluator = self.query_backed_evaluator();
        let result = evaluator.evaluate_request_result(request).into_type_id();

        // PERF: Persist intermediate evaluation results from this session into
        // the long-lived eval_cache. During recursive mapped type expansion
        // (e.g., DeepPartial<T>), the evaluator computes many sub-results
        // that would otherwise be recomputed in subsequent top-level evaluate
        // calls. Only persist entries where the result differs from the input
        // (identity mappings are free to recompute) and skip intrinsics.
        //
        // CORRECTNESS GATE: a limit-truncated result must NOT be persisted
        // here. The `eval_cache` key is `(TypeId, options)` — it does not
        // capture the ambient stack depth at which a bail occurred — so a
        // depth-bailed intermediate (e.g. a recursive array alias
        // `RecArray<T> = Array<T | RecArray<T>>` evaluated while the
        // def-depth was already high, collapsing to `error`) would otherwise
        // be cached and then read back at top level where it should have
        // converged. That poisons later type-checking (an `error` element
        // silently satisfies assignability) in a declaration/cache-order-
        // dependent way — the exact non-determinism that makes
        // recursive-utility fixtures flip with surrounding code.
        //
        // The discrimination is per-entry (issue #13241, extending the
        // PR #12902 application-eval epoch split): the top-level result is
        // gated on the run-sticky `recursion_limit_hit` (its subtree IS the
        // whole run), while drained intermediates are filtered through the
        // evaluator's per-node `tainted` set, so the clean intermediates of a
        // run whose *unrelated sibling* subtree bailed are still persisted
        // instead of being recomputed from scratch on every later query.
        // A union-complexity overflow is not routed through the evaluator's
        // limit epoch, so it conservatively suppresses all writes, as before.
        let newly_union_too_complex =
            self.interner.is_union_too_complex() && !union_too_complex_before;
        let top_level_clean = !evaluator.recursion_limit_hit();
        if !newly_union_too_complex
            && (top_level_clean || crate::limits::limit_result_cache_enabled())
        {
            let tainted = evaluator.take_tainted();
            let mut cache = self.eval_cache.borrow_mut();
            if top_level_clean {
                cache.insert(key, result);
                // Also write to shared cache for cross-file benefit.
                if let Some(shared) = self.shared {
                    shared.eval_cache.insert(key, result);
                }
            }
            for (intermediate_id, intermediate_result) in evaluator.drain_cache() {
                if intermediate_id != intermediate_result
                    && !intermediate_id.is_intrinsic()
                    && !tainted.contains(&intermediate_id)
                {
                    let ikey = request.with_type_id(intermediate_id).cache_key();
                    cache.entry(ikey).or_insert(intermediate_result);
                    if let Some(shared) = self.shared {
                        shared.eval_cache.entry(ikey).or_insert(intermediate_result);
                    }
                }
            }
        }

        if let Some(query_id) = trace_query_id {
            query_trace::unary_end(query_id, "evaluate_type_with_options", result, false);
        }
        result
    }

    /// Cache-aware override of the `QueryDatabase` default, which would build a
    /// `query_db = None` evaluator and bypass the cross-call instantiation cache.
    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        self.query_backed_evaluator().evaluate_conditional(cond)
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        let mut evaluator = self.query_backed_evaluator();
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.evaluate_index_access(object_type, index_type)
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        self.query_backed_evaluator().evaluate_keyof(operand)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        self.query_backed_evaluator().evaluate_mapped(mapped)
    }

    /// Look up a cross-call `instantiate_type` result.
    ///
    /// Hit/miss counters mirror the subtype counters and feed
    /// `QueryCacheStatistics`.
    fn lookup_instantiation_cache(&self, key: &InstantiationCacheKey) -> Option<TypeId> {
        match self.instantiation_cache.lookup(key) {
            Some(result) => {
                self.instantiation_cache_hits
                    .set(self.instantiation_cache_hits.get() + 1);
                Some(result)
            }
            None => {
                if let Some(shared) = self.shared
                    && shared.shares_instantiation_family()
                    && let Some(result) = shared.instantiation_cache.get(key).map(|entry| *entry)
                {
                    self.instantiation_cache.insert(key.clone(), result);
                    self.instantiation_cache_hits
                        .set(self.instantiation_cache_hits.get() + 1);
                    return Some(result);
                }
                self.instantiation_cache_misses
                    .set(self.instantiation_cache_misses.get() + 1);
                None
            }
        }
    }

    /// Store an `instantiate_type` result in the cross-call cache.
    fn insert_instantiation_cache(&self, key: InstantiationCacheKey, result: TypeId) {
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
        {
            shared.instantiation_cache.insert(key.clone(), result);
        }
        self.instantiation_cache.insert(key, result);
    }

    /// Look up a cached `remove_subtypes_for_bct` result. Hit/miss counters
    /// mirror the instantiation-cache counters and feed
    /// `QueryCacheStatistics`.
    fn lookup_subtype_reduction_cache(
        &self,
        key: &SubtypeReductionKey,
    ) -> Option<std::sync::Arc<[TypeId]>> {
        match self.subtype_reduction_cache.lookup(key) {
            Some(result) => {
                self.subtype_reduction_cache_hits
                    .set(self.subtype_reduction_cache_hits.get() + 1);
                Some(result)
            }
            None => {
                self.subtype_reduction_cache_misses
                    .set(self.subtype_reduction_cache_misses.get() + 1);
                None
            }
        }
    }

    /// Store a `remove_subtypes_for_bct` result in the cross-call cache.
    fn insert_subtype_reduction_cache(
        &self,
        key: SubtypeReductionKey,
        result: std::sync::Arc<[TypeId]>,
    ) {
        self.subtype_reduction_cache.insert(key, result);
    }

    fn is_subtype_of_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        self.is_cached_policy_relation(CachedPolicyRelation::Subtype, source, target, policy)
    }

    fn is_assignable_to_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        self.is_cached_policy_relation(CachedPolicyRelation::Assignability, source, target, policy)
    }

    /// Convenience wrapper for `is_subtype_of` with default flags.
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    /// Convenience wrapper for `is_assignable_to` with default flags.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    fn lookup_subtype_cache(&self, key: RelationCacheKey) -> Option<bool> {
        self.lookup_policy_relation_cache(CachedPolicyRelation::Subtype, key)
    }

    fn insert_subtype_cache(&self, key: RelationCacheKey, result: bool) {
        self.insert_policy_relation_cache(CachedPolicyRelation::Subtype, key, result);
    }

    fn lookup_subtype_cache_value(&self, key: RelationCacheKey) -> Option<RelationCacheValue> {
        self.lookup_policy_relation_cache_value(CachedPolicyRelation::Subtype, key)
    }

    /// Promote a coinductively validated maybe-key to definitive `true`.
    ///
    /// Never overwrites an existing definitive entry (a sibling checker may
    /// hold an honest `false`); upgrades an existing `LimitTrue` to definitive.
    fn promote_subtype_cache_true(&self, key: RelationCacheKey) {
        {
            let mut local = self.subtype_cache.borrow_mut();
            match local.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    if matches!(entry.get(), RelationCacheValue::LimitTrue { .. }) {
                        entry.insert(RelationCacheValue::True);
                    }
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(RelationCacheValue::True);
                }
            }
        }
        if let Some(shared) = self.shared {
            match shared.subtype_cache.entry(key) {
                dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                    if matches!(entry.get(), RelationCacheValue::LimitTrue { .. }) {
                        *entry.get_mut() = RelationCacheValue::True;
                    }
                }
                dashmap::mapref::entry::Entry::Vacant(slot) => {
                    slot.insert(RelationCacheValue::True);
                }
            }
        }
    }

    /// Record an assumed-related fuel-limit verdict valid up to `fuel_band`.
    ///
    /// Never overwrites a definitive entry; merges with an existing
    /// `LimitTrue` by keeping the larger band (the stronger statement).
    fn insert_subtype_limit_true(&self, key: RelationCacheKey, fuel_band: u32) {
        let merge = |existing: &mut RelationCacheValue| {
            if let RelationCacheValue::LimitTrue {
                fuel_band: existing_band,
            } = existing
            {
                *existing_band = (*existing_band).max(fuel_band);
            }
        };
        {
            let mut local = self.subtype_cache.borrow_mut();
            match local.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut entry) => merge(entry.get_mut()),
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(RelationCacheValue::LimitTrue { fuel_band });
                }
            }
        }
        if let Some(shared) = self.shared {
            match shared.subtype_cache.entry(key) {
                dashmap::mapref::entry::Entry::Occupied(mut entry) => merge(entry.get_mut()),
                dashmap::mapref::entry::Entry::Vacant(slot) => {
                    slot.insert(RelationCacheValue::LimitTrue { fuel_band });
                }
            }
        }
    }

    fn lookup_assignability_cache(&self, key: RelationCacheKey) -> Option<bool> {
        self.lookup_policy_relation_cache(CachedPolicyRelation::Assignability, key)
    }

    fn insert_assignability_cache(&self, key: RelationCacheKey, result: bool) {
        self.insert_policy_relation_cache(CachedPolicyRelation::Assignability, key, result);
    }

    fn lookup_intersection_merge(&self, intersection_id: TypeId) -> Option<Option<TypeId>> {
        let result = self
            .intersection_merge_cache
            .borrow()
            .get(&intersection_id)
            .copied();
        if result.is_some() {
            self.intersection_merge_cache_hits
                .set(self.intersection_merge_cache_hits.get() + 1);
        } else {
            self.intersection_merge_cache_misses
                .set(self.intersection_merge_cache_misses.get() + 1);
        }
        result
    }

    fn insert_intersection_merge(&self, intersection_id: TypeId, result: Option<TypeId>) {
        self.intersection_merge_cache
            .borrow_mut()
            .insert(intersection_id, result);
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        // Delegate to the interner - caching could be added later if needed
        self.interner.get_index_signatures(type_id)
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        // Delegate to the interner
        self.interner.is_nullish_type(type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        crate::narrowing::remove_nullish_query(self, type_id)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations::property::PropertyAccessResult {
        self.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.no_unchecked_indexed_access(),
        )
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult {
        // QueryCache doesn't have full TypeResolver capability, so use PropertyAccessEvaluator
        // with the current QueryDatabase.
        let prop_atom = self.interner.intern_string(prop_name);
        let exact_optional_property_types =
            crate::caches::db::TypeCompilerOptions::exact_optional_property_types(self);
        let key = (
            object_type,
            prop_atom,
            no_unchecked_indexed_access,
            exact_optional_property_types,
        );
        if let Some(result) = self.check_property_cache(key) {
            return result;
        }

        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.set_exact_optional_property_types(exact_optional_property_types);
        let result = evaluator.resolve_property_access(object_type, prop_name);
        self.insert_property_cache(key, result);
        result
    }

    fn resolve_any_index_access(
        &self,
        object_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<crate::operations::property::PropertyAccessResult> {
        let exact_optional_property_types =
            crate::caches::db::TypeCompilerOptions::exact_optional_property_types(self);
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.set_exact_optional_property_types(exact_optional_property_types);
        evaluator.resolve_any_index_access(object_type)
    }

    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        let key = (
            object_type,
            index_type,
            literal_index.map(|idx| idx as u32),
            self.no_unchecked_indexed_access(),
        );
        if let Some(result) = self.check_element_access_cache(key) {
            return result;
        }

        let result = match self.resolve_element_access(object_type, index_type, literal_index) {
            ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        };

        self.insert_element_access_cache(key, result);
        result
    }

    fn collect_object_spread_properties(&self, spread_type: TypeId) -> Vec<PropertyInfo> {
        if let Some(cached) = self.check_object_spread_properties_cache(spread_type) {
            return cached;
        }

        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        let result = self.collect_object_spread_properties_inner(spread_type, &mut visited);
        self.insert_object_spread_properties_cache(spread_type, result.clone());
        result
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access.set(enabled);
    }

    fn set_exact_optional_property_types(&self, enabled: bool) {
        self.exact_optional_property_types.set(enabled);
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        // Session cache first (shared with the resolver-aware cached boundary).
        if let Some(cached) = self.variance_cache.borrow().get(&def_id) {
            return Some(Arc::clone(cached));
        }
        // Compute via the type's body. `self` is both db and resolver here.
        let params = self.get_lazy_type_params(def_id)?;
        if params.is_empty() {
            return None;
        }
        let body = self.resolve_lazy(def_id, self.as_type_database())?;
        let result: Arc<[Variance]> = params
            .iter()
            .map(|param| crate::relations::variance::compute_variance(self, body, param.name))
            .collect();
        self.variance_cache
            .borrow_mut()
            .insert(def_id, Arc::clone(&result));
        Some(result)
    }

    fn get_cached_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        self.variance_cache.borrow().get(&def_id).map(Arc::clone)
    }

    fn insert_type_param_variance(&self, def_id: DefId, variance: Arc<[Variance]>) {
        self.variance_cache.borrow_mut().insert(def_id, variance);
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // Check cache first
        let cached = self.canonical_cache.borrow().get(&type_id).copied();

        if let Some(canonical) = cached {
            return canonical;
        }

        // Compute canonical form using a fresh Canonicalizer
        // CRITICAL: Always start with empty stacks for absolute De Bruijn indices
        // This ensures the cached TypeId represents the absolute structural form
        use crate::canonicalize::Canonicalizer;
        let mut canon = Canonicalizer::new(self.as_type_database(), self);
        let canonical = canon.canonicalize(type_id);

        // Cache the result
        self.canonical_cache.borrow_mut().insert(type_id, canonical);

        canonical
    }
}

#[cfg(test)]
#[path = "../../tests/db_tests.rs"]
mod tests;

// `estimated_size_bytes` lives in a child module to keep this shard under
// the 2000-line file-size cap; child modules retain private-field access.
#[path = "query_cache_size.rs"]
mod size;
