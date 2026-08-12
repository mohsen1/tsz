//! Cached query database implementation for the solver.
//!
//! `QueryCache` wraps a `TypeInterner` with memoization for evaluation,
//! relation, property, and element access queries. This is the concrete
//! database implementation used by the checker at runtime.

use crate::caches::application_eval_index::{
    self, ApplicationEvalDependencyIndex, ApplicationEvalDependencyIndexState,
};
use crate::caches::db::{
    IntersectionMergeCacheEntry, QueryDatabase, TypeBuiltinAccess, TypeCompilerOptions,
    TypeContainsByIdCache, TypeDatabase, TypeDisplayProvenance, TypeExtractParamsCache,
    TypePruneUnionCache, TypeRawIntersectionConstruction, TypeSubstitutionConstruction,
    TypeTupleLimitSignal, TypeWidenCache, UnionComplexityCheckpoint,
};
use crate::caches::eval_dependency_index::{self, EvalDependencyIndex, EvalDependencyIndexState};
use crate::caches::instantiation_cache::{InstantiationCache, InstantiationCacheKey};
use crate::caches::query_cache_statistics::{QueryCacheStatistics, RelationCacheStats};
use crate::caches::query_trace;
use crate::caches::shared_query_cache::ApplicationEvalCacheKey;
pub use crate::caches::shared_query_cache::SharedQueryCache;
use crate::caches::subtype_reduction_cache::{SubtypeReductionCache, SubtypeReductionKey};
use crate::def::DefId;
use crate::evaluation::cache_stability::EvaluationCacheLimitSnapshot;
use crate::evaluation::request::{EvaluationCacheKey, EvaluationRequest};
use crate::intern::TypeInterner;
use crate::objects::element_access::ElementAccessResult;
use crate::objects::{CollectPropertiesResultCache, PropertyCollectionResult};
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
};
use crate::visitor::is_error_type;
use dashmap::DashMap;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

mod js_signature_display;
mod predicate_cache;
mod resolver;

// Element access (indexed access) of an optional property includes `undefined`
// under both `exactOptionalPropertyTypes` settings (matching tsc), so the result
// does not depend on that option and it is intentionally not part of this key.
type ElementAccessTypeCacheKey = (TypeId, TypeId, Option<u32>, bool);
type PropertyAccessCacheKey = (TypeId, Atom, bool, bool, bool);
type ConditionalBranchVerdictCacheKey = (TypeId, TypeId, bool, bool);
type PermissiveFalseBranchCacheKey = (TypeId, TypeId, bool, bool);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationCacheProbe {
    Hit(bool),
    MissNotCached,
}

#[derive(Debug, Default)]
struct CacheCounter {
    hits: Cell<u64>,
    misses: Cell<u64>,
}

impl CacheCounter {
    const fn new() -> Self {
        Self {
            hits: Cell::new(0),
            misses: Cell::new(0),
        }
    }

    #[inline]
    fn record_hit(&self) {
        self.hits.set(self.hits.get() + 1);
    }

    #[inline]
    fn record_miss(&self) {
        self.misses.set(self.misses.get() + 1);
    }

    fn reset(&self) {
        self.hits.set(0);
        self.misses.set(0);
    }

    const fn hits(&self) -> u64 {
        self.hits.get()
    }

    const fn misses(&self) -> u64 {
        self.misses.get()
    }
}

#[derive(Debug, Default)]
struct SharedCacheCounter {
    local: CacheCounter,
    shared_hits: Cell<u64>,
    shared_misses: Cell<u64>,
    shared_inserts: Cell<u64>,
}

impl SharedCacheCounter {
    const fn new() -> Self {
        Self {
            local: CacheCounter::new(),
            shared_hits: Cell::new(0),
            shared_misses: Cell::new(0),
            shared_inserts: Cell::new(0),
        }
    }

    #[inline]
    fn record_hit(&self) {
        self.local.record_hit();
    }

    #[inline]
    fn record_miss(&self) {
        self.local.record_miss();
    }

    #[inline]
    fn record_shared_hit(&self) {
        self.shared_hits.set(self.shared_hits.get() + 1);
    }

    #[inline]
    fn record_shared_miss(&self) {
        self.shared_misses.set(self.shared_misses.get() + 1);
    }

    #[inline]
    fn record_shared_insert(&self) {
        self.shared_inserts.set(self.shared_inserts.get() + 1);
    }

    fn reset(&self) {
        self.local.reset();
        self.shared_hits.set(0);
        self.shared_misses.set(0);
        self.shared_inserts.set(0);
    }

    const fn hits(&self) -> u64 {
        self.local.hits()
    }

    const fn misses(&self) -> u64 {
        self.local.misses()
    }

    const fn shared_hits(&self) -> u64 {
        self.shared_hits.get()
    }

    const fn shared_misses(&self) -> u64 {
        self.shared_misses.get()
    }

    const fn shared_inserts(&self) -> u64 {
        self.shared_inserts.get()
    }
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
    eval_dependency_index: EvalDependencyIndex,
    /// Top-level `eval_cache` keys whose result passed the depth-agnostic gate
    /// but not the stricter `is_stable_for_run_wide_cache` gate — i.e. a
    /// registration-window artifact (`UnresolvedDef`), valid only within the
    /// window that produced it. The bounded checker pool reuses one
    /// `QueryCache` across a partition's files, each a distinct window, so
    /// these are evicted at every file boundary by
    /// `evict_registration_window_eval_entries`; clean entries stay, preserving
    /// the pool's cross-file amortization (#16553).
    registration_window_eval_keys: RefCell<rustc_hash::FxHashSet<EvaluationCacheKey>>,
    /// Substitution-independent evaluation cache (see the `closed_eval` module
    /// in `evaluate`). Keyed by
    /// `(TypeId, no_unchecked_indexed_access, exact_optional_property_types)`.
    closed_eval_cache: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    closed_eval_dependency_index: EvalDependencyIndex,
    /// Persistent conditional-branch subtype verdicts (issues #8356 / #13097).
    /// Keyed by `(check, extends, no_unchecked_indexed_access,
    /// exact_optional_property_types)`; stores only definitive, limit-free
    /// verdicts so it survives the per-evaluator `conditional_subtype_cache`
    /// that is dropped on every evaluator construction. Shares this cache's
    /// `clear()`/file lifecycle envelope.
    conditional_branch_verdict_cache: RefCell<FxHashMap<ConditionalBranchVerdictCacheKey, bool>>,
    /// Persistent results for tsc's permissive-instantiation false-branch gate.
    /// Writes are guarded by the conditional-branch verdict cache for the
    /// instantiated permissive pair.
    permissive_false_branch_cache: RefCell<FxHashMap<PermissiveFalseBranchCacheKey, bool>>,
    application_eval_cache: RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    application_eval_dependency_index: ApplicationEvalDependencyIndex,
    element_access_cache: RefCell<FxHashMap<ElementAccessTypeCacheKey, TypeId>>,
    object_spread_properties_cache: RefCell<FxHashMap<TypeId, Vec<PropertyInfo>>>,
    /// Memo for completed context-free `collect_properties_cached` results,
    /// scoped by `resolver_generation` so a result is never reused after lazy
    /// `DefId` resolution can change it. Generations advance monotonically, so
    /// the memo retains only the most-recent generations per `TypeId` and evicts
    /// superseded ones rather than letting the flat `(TypeId, generation)` map
    /// grow unbounded (issue #14347). Shares this cache's `clear()`/lifecycle
    /// envelope (same as `object_spread_properties_cache`).
    collect_properties_result_cache: RefCell<collect_properties_memo::CollectPropertiesMemo>,
    collect_properties_cache_stats: CacheCounter,
    subtype_cache: RefCell<FxHashMap<RelationCacheKey, RelationCacheValue>>,
    /// Separate cache for assignability to prevent loose results from poisoning subtype checks.
    assignability_cache: RefCell<FxHashMap<RelationCacheKey, RelationCacheValue>>,
    property_cache: RefCell<FxHashMap<PropertyAccessCacheKey, PropertyAccessResult>>,
    /// Computed variance masks for generic `DefIds`.
    variance_cache: RefCell<FxHashMap<DefId, Arc<[Variance]>>>,
    /// Canonical `TypeId` for structurally identical types — stable entries
    /// only, used as the canonicalizer cross-probe interior memo.
    canonical_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Dirty canonical results (registration-window or guard-truncated artifacts),
    /// memoized so repeat probes stay O(1). Kept separate from
    /// `canonical_cache` because reusing one must taint the consumer (see
    /// `Canonicalizer::shared_artifacts`) — merging the tiers would launder
    /// artifacts into stable entries for every type containing them.
    canonical_artifact_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Cache for intersection-to-merged-object results.
    /// Avoids expensive `collect_properties` calls for the same intersection target
    /// across multiple `SubtypeChecker` instances (common in constraint checking).
    /// Scoped by resolver generation because lazy member resolution can change
    /// both successful merges and negative eligibility decisions.
    /// `Some(type_id)` = successfully merged, `None` = not eligible for merging.
    intersection_merge_cache: RefCell<intersection_merge_memo::IntersectionMergeMemo>,
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
    application_eval_cache_stats: SharedCacheCounter,
    subtype_cache_stats: CacheCounter,
    assignability_cache_stats: CacheCounter,
    intersection_merge_cache_stats: CacheCounter,
    instantiation_cache_stats: SharedCacheCounter,
    subtype_reduction_cache_stats: CacheCounter,
    no_unchecked_indexed_access: Cell<bool>,
    exact_optional_property_types: Cell<bool>,
    strict_null_checks: Cell<bool>,
    /// Optional shared cross-file cache for multi-file project checking.
    /// When present, local cache misses fall through to the shared `DashMap` cache,
    /// and local cache inserts are also written to the shared cache.
    shared: Option<&'a SharedQueryCache>,
    /// Optional shared `DefinitionStore` for cross-arena declaration identity
    /// (issue #14344).
    ///
    /// The `QueryCache` is the `&dyn QueryDatabase` resolver used by generic-call
    /// inference (`InferenceContext::with_query_db`). Without the store its
    /// `DefId`-keyed `TypeResolver` methods (`def_to_symbol_id`, `get_def_kind`,
    /// `get_def_name`, `canonical_def_id`) return trait defaults, so the
    /// `shared_application_base_def_id` cross-arena base unification that depends
    /// on them stays dead at the `infer_applications` base-differ site. When
    /// wired (the production project path), those methods resolve through this
    /// store. Read-only; never mutated through the `QueryCache`.
    definition_store: Option<&'a crate::def::DefinitionStore>,
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
        // Measurement-only (issue #13097): a fresh `QueryCache` marks a new
        // file scope for the cross-evaluator memo-loss audit, mirroring the
        // per-file lifetime of the caches whose loss it measures.
        crate::evaluation::memo_audit::begin_file_scope();
        QueryCache {
            interner,
            eval_cache: RefCell::new(FxHashMap::default()),
            eval_dependency_index: RefCell::new(EvalDependencyIndexState::default()),
            registration_window_eval_keys: RefCell::new(rustc_hash::FxHashSet::default()),
            closed_eval_cache: RefCell::new(FxHashMap::default()),
            closed_eval_dependency_index: RefCell::new(EvalDependencyIndexState::default()),
            conditional_branch_verdict_cache: RefCell::new(FxHashMap::default()),
            permissive_false_branch_cache: RefCell::new(FxHashMap::default()),
            application_eval_cache: RefCell::new(FxHashMap::default()),
            application_eval_dependency_index: RefCell::new(
                ApplicationEvalDependencyIndexState::default(),
            ),
            element_access_cache: RefCell::new(FxHashMap::default()),
            object_spread_properties_cache: RefCell::new(FxHashMap::default()),
            collect_properties_result_cache: RefCell::new(
                collect_properties_memo::CollectPropertiesMemo::default(),
            ),
            collect_properties_cache_stats: CacheCounter::new(),
            subtype_cache: RefCell::new(FxHashMap::default()),
            assignability_cache: RefCell::new(FxHashMap::default()),
            property_cache: RefCell::new(FxHashMap::default()),
            variance_cache: RefCell::new(FxHashMap::default()),
            canonical_cache: RefCell::new(FxHashMap::default()),
            canonical_artifact_cache: RefCell::new(FxHashMap::default()),
            intersection_merge_cache: RefCell::new(
                intersection_merge_memo::IntersectionMergeMemo::default(),
            ),
            instantiation_cache: InstantiationCache::new(),
            subtype_reduction_cache: SubtypeReductionCache::new(),
            application_eval_cache_stats: SharedCacheCounter::new(),
            subtype_cache_stats: CacheCounter::new(),
            assignability_cache_stats: CacheCounter::new(),
            intersection_merge_cache_stats: CacheCounter::new(),
            instantiation_cache_stats: SharedCacheCounter::new(),
            subtype_reduction_cache_stats: CacheCounter::new(),
            no_unchecked_indexed_access: Cell::new(interner.no_unchecked_indexed_access()),
            exact_optional_property_types: Cell::new(interner.exact_optional_property_types()),
            strict_null_checks: Cell::new(interner.strict_null_checks()),
            shared,
            definition_store: None,
        }
    }

    /// Attach a shared `DefinitionStore` so this `QueryCache`'s `DefId`-keyed
    /// `TypeResolver` methods resolve, enabling cross-arena base unification in
    /// generic-call inference (issue #14344). Read-only; the store is never
    /// mutated through here.
    #[must_use]
    pub const fn with_definition_store(mut self, store: &'a crate::def::DefinitionStore) -> Self {
        self.definition_store = Some(store);
        self
    }

    pub(crate) const fn has_definition_store(&self) -> bool {
        self.definition_store.is_some()
    }

    /// The shared `DefinitionStore` attached to this cache, when present.
    ///
    /// Read-only access for the store-only re-reduce resolver shim
    /// (`store_resolver_backed_evaluator`, issue #14344 / #14345). The store is
    /// program-global and never mutated through the `QueryCache`.
    pub(crate) const fn definition_store(&self) -> Option<&'a crate::def::DefinitionStore> {
        self.definition_store
    }

    pub fn clear(&self) {
        self.eval_cache.borrow_mut().clear();
        self.eval_dependency_index.borrow_mut().clear();
        self.registration_window_eval_keys.borrow_mut().clear();
        self.closed_eval_cache.borrow_mut().clear();
        self.closed_eval_dependency_index.borrow_mut().clear();
        self.conditional_branch_verdict_cache.borrow_mut().clear();
        self.permissive_false_branch_cache.borrow_mut().clear();
        self.element_access_cache.borrow_mut().clear();
        self.application_eval_cache.borrow_mut().clear();
        self.application_eval_dependency_index.borrow_mut().clear();
        self.object_spread_properties_cache.borrow_mut().clear();
        self.collect_properties_result_cache.borrow_mut().clear();
        self.subtype_cache.borrow_mut().clear();
        self.assignability_cache.borrow_mut().clear();
        self.property_cache.borrow_mut().clear();
        self.variance_cache.borrow_mut().clear();
        self.canonical_cache.borrow_mut().clear();
        self.canonical_artifact_cache.borrow_mut().clear();
        self.intersection_merge_cache.borrow_mut().clear();
        self.instantiation_cache.clear();
        self.subtype_reduction_cache.clear();
        self.reset_relation_cache_stats();
    }

    pub fn relation_cache_stats(&self) -> RelationCacheStats {
        let subtype_entries = self.subtype_cache.borrow().len();
        let assignability_entries = self.assignability_cache.borrow().len();
        RelationCacheStats {
            subtype_hits: self.subtype_cache_stats.hits(),
            subtype_misses: self.subtype_cache_stats.misses(),
            subtype_entries,
            assignability_hits: self.assignability_cache_stats.hits(),
            assignability_misses: self.assignability_cache_stats.misses(),
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
            conditional_branch_verdict_cache_entries: self
                .conditional_branch_verdict_cache
                .borrow()
                .len(),
            permissive_false_branch_cache_entries: self
                .permissive_false_branch_cache
                .borrow()
                .len(),
            application_eval_cache_entries: self.application_eval_cache.borrow().len(),
            application_eval_cache_hits: self.application_eval_cache_stats.hits(),
            application_eval_cache_misses: self.application_eval_cache_stats.misses(),
            application_eval_cache_shared_hits: self.application_eval_cache_stats.shared_hits(),
            application_eval_cache_shared_misses: self.application_eval_cache_stats.shared_misses(),
            application_eval_cache_shared_inserts: self
                .application_eval_cache_stats
                .shared_inserts(),
            element_access_cache_entries: self.element_access_cache.borrow().len(),
            object_spread_cache_entries: self.object_spread_properties_cache.borrow().len(),
            property_cache_entries: self.property_cache.borrow().len(),
            variance_cache_entries: self.variance_cache.borrow().len(),
            canonical_cache_entries: self.canonical_cache.borrow().len(),
            intersection_merge_cache_entries: self
                .intersection_merge_cache
                .borrow()
                .total_entries(),
            intersection_merge_cache_hits: self.intersection_merge_cache_stats.hits(),
            intersection_merge_cache_misses: self.intersection_merge_cache_stats.misses(),
            instantiation_cache_entries: self.instantiation_cache.len(),
            instantiation_cache_hits: self.instantiation_cache_stats.hits(),
            instantiation_cache_misses: self.instantiation_cache_stats.misses(),
            instantiation_cache_shared_hits: self.instantiation_cache_stats.shared_hits(),
            instantiation_cache_shared_misses: self.instantiation_cache_stats.shared_misses(),
            instantiation_cache_shared_inserts: self.instantiation_cache_stats.shared_inserts(),
            subtype_reduction_cache_entries: self.subtype_reduction_cache.len(),
            subtype_reduction_cache_hits: self.subtype_reduction_cache_stats.hits(),
            subtype_reduction_cache_misses: self.subtype_reduction_cache_stats.misses(),
            relation: self.relation_cache_stats(),
        }
    }

    pub fn reset_relation_cache_stats(&self) {
        self.application_eval_cache_stats.reset();
        self.subtype_cache_stats.reset();
        self.assignability_cache_stats.reset();
        self.intersection_merge_cache_stats.reset();
        self.collect_properties_cache_stats.reset();
        self.instantiation_cache_stats.reset();
        self.subtype_reduction_cache_stats.reset();
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
    const fn relation_cache_counter(&self, relation: CachedPolicyRelation) -> &CacheCounter {
        match relation {
            CachedPolicyRelation::Subtype => &self.subtype_cache_stats,
            CachedPolicyRelation::Assignability => &self.assignability_cache_stats,
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
            self.relation_cache_counter(relation).record_hit();
            return Some(result);
        }

        if let Some(shared) = self.shared
            && let Some(result) = relation.shared_slot(shared).get(&key).map(|r| *r)
        {
            self.relation_local_cache(relation)
                .borrow_mut()
                .insert(key, result);
            self.relation_cache_counter(relation).record_hit();
            return Some(result);
        }

        self.relation_cache_counter(relation).record_miss();
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

        let relation_result = query_relation(
            self.as_type_database(),
            source,
            target,
            relation.relation_kind(),
            policy,
            RelationContext::default(),
        );
        let result = relation_result.related;

        // Keep request-local relation answers out of this outer boolean cache.
        // The typed stability signal includes local depth/iteration overflow,
        // global fuel and shared solver-frame limits, truncated evaluation,
        // and unresolved semantic references; not all of those appear in the
        // diagnostic termination channel.
        if relation_result.is_cacheable() {
            self.insert_policy_relation_cache(relation, key, result);
        }
        if let Some(query_id) = trace_query_id {
            query_trace::relation_end(query_id, trace_op, result, false);
        }
        result
    }
}

impl TypeTupleLimitSignal for QueryCache<'_> {
    fn take_tuple_too_large(&self) -> bool {
        self.interner.take_tuple_too_large()
    }

    fn mark_tuple_too_large(&self) {
        self.interner.set_tuple_too_large();
    }

    fn is_tuple_too_large(&self) -> bool {
        self.interner.is_tuple_too_large()
    }

    fn is_poisoned(&self) -> bool {
        self.interner.is_poisoned()
    }
}

impl TypeDisplayProvenance for QueryCache<'_> {
    fn display_provenance_generation(&self) -> u64 {
        self.interner.display_provenance_generation()
    }

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

    fn transfer_rewritten_application_display_alias(&self, evaluated: TypeId, application: TypeId) {
        self.interner
            .transfer_rewritten_application_display_alias(evaluated, application);
    }

    fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.get_display_alias(type_id)
    }

    fn store_merged_intersection_origin(&self, merged: TypeId, intersection: TypeId) {
        self.interner
            .store_merged_intersection_origin(merged, intersection);
    }

    fn get_merged_intersection_origin(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.get_merged_intersection_origin(type_id)
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

    fn mark_global_this_surface_display(&self, type_id: TypeId) {
        self.interner.mark_global_this_surface_display(type_id);
    }

    fn is_global_this_surface_display(&self, type_id: TypeId) -> bool {
        self.interner.is_global_this_surface_display(type_id)
    }

    fn mark_literal_object_annotation(&self, type_id: TypeId) {
        self.interner.mark_literal_object_annotation(type_id);
    }

    fn is_literal_object_annotation(&self, type_id: TypeId) -> bool {
        self.interner.is_literal_object_annotation(type_id)
    }

    fn mark_union_literal_member(&self, union_type_id: TypeId, member_type_id: TypeId) {
        self.interner
            .mark_union_literal_member(union_type_id, member_type_id);
    }

    fn is_union_literal_member(&self, union_type_id: TypeId, member_type_id: TypeId) -> bool {
        self.interner
            .is_union_literal_member(union_type_id, member_type_id)
    }

    fn store_union_origin(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        self.interner
            .store_union_origin(union_type_id, origin_members);
    }

    fn store_rewritten_union_origin(
        &self,
        union_type_id: TypeId,
        origin_members: Vec<TypeId>,
        is_fallback: bool,
    ) {
        self.interner
            .store_rewritten_union_origin(union_type_id, origin_members, is_fallback);
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

    fn union_complexity_checkpoint(&self) -> UnionComplexityCheckpoint {
        self.interner.union_complexity_checkpoint()
    }

    fn union_complexity_changed_since(&self, checkpoint: UnionComplexityCheckpoint) -> bool {
        self.interner.union_complexity_changed_since(checkpoint)
    }

    fn take_union_too_complex_since(&self, checkpoint: UnionComplexityCheckpoint) -> bool {
        self.interner.take_union_too_complex_since(checkpoint)
    }

    fn discard_union_too_complex_since(&self, checkpoint: UnionComplexityCheckpoint) {
        self.interner.discard_union_too_complex_since(checkpoint);
    }

    fn mark_union_too_complex(&self) {
        self.interner.set_union_too_complex();
    }
}

impl TypeRawIntersectionConstruction for QueryCache<'_> {
    fn intersect_types_raw_for_replay(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.intersect_types_raw_for_replay(members)
    }
}

impl TypeCompilerOptions for QueryCache<'_> {
    fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access.get()
    }

    fn exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types.get()
    }

    fn strict_null_checks(&self) -> bool {
        self.strict_null_checks.get()
    }
}

impl TypeWidenCache for QueryCache<'_> {
    fn widen_type_memo(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.widen_type_memo(type_id)
    }

    fn set_widen_type_memo(&self, type_id: TypeId, result: TypeId) {
        self.interner.set_widen_type_memo(type_id, result);
    }
}

impl TypeSubstitutionConstruction for QueryCache<'_> {
    fn substitution(&self, base_type: TypeId, constraint: TypeId) -> TypeId {
        self.interner.substitution(base_type, constraint)
    }
}

impl TypeExtractParamsCache for QueryCache<'_> {
    fn extract_type_params_memo(&self, type_id: TypeId) -> Option<Arc<[TypeParamInfo]>> {
        self.interner.extract_type_params_memo(type_id)
    }

    fn set_extract_type_params_memo(&self, type_id: TypeId, params: Arc<[TypeParamInfo]>) {
        self.interner.set_extract_type_params_memo(type_id, params);
    }

    fn contravariant_infer_names_memo(&self, type_id: TypeId) -> Option<Arc<[Atom]>> {
        self.interner.contravariant_infer_names_memo(type_id)
    }

    fn set_contravariant_infer_names_memo(&self, type_id: TypeId, names: Arc<[Atom]>) {
        self.interner
            .set_contravariant_infer_names_memo(type_id, names);
    }
}

impl TypeContainsByIdCache for QueryCache<'_> {
    fn contains_type_by_id_memo(&self, root: TypeId, target: TypeId) -> Option<bool> {
        self.interner.contains_type_by_id_memo(root, target)
    }

    fn set_contains_type_by_id_memo(&self, root: TypeId, target: TypeId, result: bool) {
        self.interner
            .set_contains_type_by_id_memo(root, target, result);
    }
}

impl TypePruneUnionCache for QueryCache<'_> {
    fn prune_union_members_memo(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.prune_union_members_memo(type_id)
    }

    fn set_prune_union_members_memo(&self, type_id: TypeId, result: TypeId) {
        self.interner.set_prune_union_members_memo(type_id, result);
    }
}

impl TypeBuiltinAccess for QueryCache<'_> {
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
}

impl TypeDatabase for QueryCache<'_> {
    fn type_database_identity(&self) -> usize {
        self.interner.type_database_identity()
    }

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

impl CollectPropertiesResultCache for QueryCache<'_> {
    fn collect_properties_result_cached(
        &self,
        type_id: TypeId,
        resolver_generation: u64,
    ) -> Option<PropertyCollectionResult> {
        let result = self
            .collect_properties_result_cache
            .borrow()
            .get(type_id, resolver_generation);
        if result.is_some() {
            self.collect_properties_cache_stats.record_hit();
        } else {
            self.collect_properties_cache_stats.record_miss();
        }
        result
    }

    fn set_collect_properties_result_cache(
        &self,
        type_id: TypeId,
        resolver_generation: u64,
        result: PropertyCollectionResult,
    ) {
        self.collect_properties_result_cache.borrow_mut().insert(
            type_id,
            resolver_generation,
            result,
        );
    }
}

// `TypeInterner` is the other `QueryDatabase` implementor; it has no query
// cache, so it keeps the no-op defaults (no collect-properties memoization).
impl CollectPropertiesResultCache for TypeInterner {}

impl QueryDatabase for QueryCache<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn definition_store_for_inference(&self) -> Option<&crate::def::DefinitionStore> {
        self.definition_store()
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
        if let Some(result) = self.lookup_eval_cache_layers(key) {
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
            self.insert_eval_cache_entry(key, type_id);
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

        let limit_snapshot = EvaluationCacheLimitSnapshot::capture(self.interner);
        let mut evaluator = self.query_backed_evaluator();
        let evaluation_memo_result = evaluator.evaluate_request_memo_result(request);
        let result = evaluation_memo_result.into_type_id();

        // PERF: Persist intermediate evaluation results from this session into
        // the long-lived eval_cache. During recursive mapped type expansion
        // (e.g., DeepPartial<T>), the evaluator computes many sub-results
        // that would otherwise be recomputed in subsequent top-level evaluate
        // calls. Only persist entries where the result differs from the input
        // (identity mappings are free to recompute) and skip intrinsics.
        //
        // CORRECTNESS GATE: a limit-truncated result must NOT be persisted
        // here. The persistent `eval_cache` key is the zero-identity
        // `(TypeId, options)` form of `EvaluationCacheKey`; it does not capture
        // the ambient stack depth at which a bail occurred, so a depth-bailed
        // intermediate (e.g. a recursive array alias
        // `RecArray<T> = Array<T | RecArray<T>>` evaluated while the
        // def-depth was already high, collapsing to `error`) would otherwise
        // be cached and then read back at top level where it should have
        // converged. That poisons later type-checking (an `error` element
        // silently satisfies assignability) in a declaration/cache-order-
        // dependent way — the exact non-determinism that makes
        // recursive-utility fixtures flip with surrounding code.
        //
        // The discrimination is per-entry (issue #13241, extending the
        // PR #12902 application-eval epoch split): the top-level result uses
        // the named `EvaluationMemoResult` stability verdict (#14346), which
        // combines the typed `EvaluationResult` verdict with the legacy
        // run-sticky `recursion_limit_hit` guard for taint classes not solely
        // modeled by the typed channel yet. Its subtree IS the whole run, while
        // drained intermediates are filtered through the evaluator's per-node
        // `tainted` set, so the clean intermediates of a run whose *unrelated
        // sibling* subtree bailed are still persisted instead of being
        // recomputed from scratch on every later query.
        // A union-complexity overflow is not routed through the evaluator's
        // limit epoch, so it conservatively suppresses all writes, as before.
        //
        // #16553/#16587: the per-node `tainted` set only catches a node whose
        // *own* evaluation window moved `limit_epoch` (a depth/recursion bail
        // local to that node). It does NOT catch a node whose own window was
        // clean but which was evaluated *after* an unrelated sibling already
        // set the evaluator-wide `unresolved_def_seen` flag — the epoch simply
        // never moves again once that fires, so every later node looks clean
        // by the epoch-delta test alone even though the whole run is no longer
        // a registration-window-independent function of its key. An
        // intermediate is the sharper hazard here (sharper than the top-level
        // entry, which stays gated by the looser `top_level_clean` below to
        // preserve same-file circular-default recovery, #16587's reverted
        // regression): it publishes under a key a *different*, unrelated
        // top-level query in a *different* file can read back with no idea it
        // was computed mid-registration-window. `is_stable_for_run_wide_cache`
        // checks the raw `EvaluationRequestStability` directly (mirroring
        // `closed_eval::commit_closed_eval_writes`'s own run-wide gate)
        // instead of going through the looser `EvaluationMemoStability`, so it
        // refuses `UnresolvedDef` here even though `top_level_clean` (below)
        // deliberately still tolerates it.
        let union_complexity_stable =
            limit_snapshot.union_complexity_stayed_stable_after(self.interner);
        let top_level_clean = evaluation_memo_result.is_stable_for_depth_agnostic_cache();
        let intermediates_clean = evaluation_memo_result.is_stable_for_run_wide_cache();
        if union_complexity_stable
            && (top_level_clean || crate::limits::limit_result_cache_enabled())
        {
            if top_level_clean {
                self.insert_eval_cache_entry(key, result);
                if !intermediates_clean {
                    // `top_level_clean` (depth-agnostic) admitted this entry, but
                    // the stricter run-wide gate refused it: its value is a
                    // function of the current registration window, not of its key
                    // alone (an `UnresolvedDef` taint). It is reusable *within*
                    // this window — a same-file circular type-parameter default's
                    // recovery relies on exactly that (#16587) — but the bounded
                    // checker pool reuses one `QueryCache` across the files of a
                    // partition, each a distinct window, so it must be evicted at
                    // the next file boundary rather than shadow the next file's
                    // authoritative answer (#16553).
                    self.registration_window_eval_keys.borrow_mut().insert(key);
                }
                // The shared cache is the run-wide, cross-window cache other
                // files/threads read without ever entering this window; per
                // `is_stable_for_run_wide_cache`'s own contract it must never
                // receive a registration-window artifact. Gate its top-level
                // write on run-wide stability, matching the intermediate drain
                // below (previously it inherited only the looser
                // `top_level_clean`, which tolerates `UnresolvedDef`).
                if intermediates_clean && let Some(shared) = self.shared {
                    shared.insert_eval_cache(self.interner, self.definition_store(), key, result);
                }
            }
            for (intermediate_id, intermediate_result) in evaluator.drain_stable_cache() {
                if intermediates_clean
                    && intermediate_id != intermediate_result
                    && !intermediate_id.is_intrinsic()
                {
                    let ikey = request.with_type_id(intermediate_id).cache_key();
                    self.insert_eval_cache_entry_if_absent(ikey, intermediate_result);
                    if let Some(shared) = self.shared {
                        shared.insert_eval_cache_if_absent(
                            self.interner,
                            self.definition_store(),
                            ikey,
                            intermediate_result,
                        );
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
        if let Some(mut evaluator) = self.store_backed_rereduce_evaluator() {
            return evaluator.evaluate_conditional(cond);
        }
        self.query_backed_evaluator().evaluate_conditional(cond)
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        // #14344 / #14345 OPTION B (default-OFF, byte-parity): when both
        // `TSZ_INST_RESOLVER_REREDUCE=1` and `TSZ_OPTIONB_STORE_RESOLVER=1` are
        // set and a shared `DefinitionStore` is attached, run the index access
        // through the arena-invariant store-only resolver shim so a cross-arena
        // `Lazy(URItoKindN)` base materializes to its empty-Object snapshot here
        // (`visit_lazy` → `resolve_lazy` via the store) and the published
        // home-symbol redirect can re-index the populated home body. The shim
        // resolves NONE of the per-arena maps, and the evaluator adopts the
        // limited-resolver discipline (never writes the resolver-independent
        // `application_eval_cache`, never persists into the program-global eval
        // memo), so the cross-call cache keys stay resolver-independent. With
        // either flag OFF (or no store) this keeps the literal
        // `NoopResolver`-backed `query_backed_evaluator` path.
        if crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled()
            && crate::instantiation::instantiate::flags::optionb_store_resolver_enabled()
            && let Some(store) = self.definition_store()
        {
            let shim = crate::caches::query_cache_evaluation::StoreOnlyResolver::new(store);
            let mut evaluator = self.store_resolver_backed_evaluator(&shim);
            evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
            return evaluator.evaluate_index_access(object_type, index_type);
        }
        if let Some(mut evaluator) = self.store_backed_rereduce_evaluator() {
            evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
            return evaluator.evaluate_index_access(object_type, index_type);
        }
        let mut evaluator = self.query_backed_evaluator();
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.evaluate_index_access(object_type, index_type)
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        if let Some(mut evaluator) = self.store_backed_rereduce_evaluator() {
            return evaluator.evaluate_keyof(operand);
        }
        self.query_backed_evaluator().evaluate_keyof(operand)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        if let Some(mut evaluator) = self.store_backed_rereduce_evaluator() {
            return evaluator.evaluate_mapped(mapped);
        }
        self.query_backed_evaluator().evaluate_mapped(mapped)
    }

    /// Look up a cross-call `instantiate_type` result.
    ///
    /// Hit/miss counters mirror the subtype counters and feed
    /// `QueryCacheStatistics`.
    fn lookup_instantiation_cache(&self, key: &InstantiationCacheKey) -> Option<TypeId> {
        match self.instantiation_cache.lookup(key) {
            Some(result) => {
                self.instantiation_cache_stats.record_hit();
                Some(result)
            }
            None => {
                if let Some(shared) = self.shared
                    && shared.shares_instantiation_family()
                {
                    if let Some(result) = shared.instantiation_cache.get(key).map(|entry| *entry) {
                        self.instantiation_cache.insert(key.clone(), result);
                        self.instantiation_cache_stats.record_hit();
                        self.instantiation_cache_stats.record_shared_hit();
                        tsz_common::perf_counters::record_shared_instantiation_cache_hit();
                        return Some(result);
                    }
                    self.instantiation_cache_stats.record_shared_miss();
                    tsz_common::perf_counters::record_shared_instantiation_cache_miss();
                } else {
                    tsz_common::perf_counters::record_shared_instantiation_cache_bypass();
                }
                self.instantiation_cache_stats.record_miss();
                None
            }
        }
    }

    /// Store an `instantiate_type` result in the cross-call cache.
    fn insert_instantiation_cache_with_project_stability(
        &self,
        key: InstantiationCacheKey,
        result: TypeId,
        stable_for_project_cache: bool,
    ) {
        if let Some(shared) = self.shared
            && shared.shares_instantiation_family()
            && stable_for_project_cache
        {
            shared.instantiation_cache.insert(key.clone(), result);
            self.instantiation_cache_stats.record_shared_insert();
            tsz_common::perf_counters::record_shared_instantiation_cache_insert();
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
                self.subtype_reduction_cache_stats.record_hit();
                Some(result)
            }
            None => {
                self.subtype_reduction_cache_stats.record_miss();
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

    fn lookup_intersection_merge(
        &self,
        intersection_id: TypeId,
        resolver_generation: u64,
    ) -> Option<IntersectionMergeCacheEntry> {
        let result = self
            .intersection_merge_cache
            .borrow()
            .get(intersection_id, resolver_generation);
        if result.is_some() {
            self.intersection_merge_cache_stats.record_hit();
        } else {
            self.intersection_merge_cache_stats.record_miss();
        }
        result
    }

    fn insert_intersection_merge(
        &self,
        intersection_id: TypeId,
        resolver_generation: u64,
        result: Option<TypeId>,
    ) {
        self.intersection_merge_cache.borrow_mut().insert(
            intersection_id,
            resolver_generation,
            result,
        );
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
        self.resolve_property_access_atom(object_type, self.interner.intern_string(prop_name))
    }

    fn resolve_property_access_atom(
        &self,
        object_type: TypeId,
        prop_atom: Atom,
    ) -> crate::operations::property::PropertyAccessResult {
        self.property_access_atom_with_options(
            object_type,
            prop_atom,
            self.no_unchecked_indexed_access(),
        )
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult {
        self.property_access_atom_with_options(
            object_type,
            self.interner.intern_string(prop_name),
            no_unchecked_indexed_access,
        )
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

        let mut traversal = spread::ObjectSpreadTraversalState::default();
        let result = self.collect_object_spread_properties_inner(spread_type, &mut traversal);
        if traversal.is_cacheable() {
            self.insert_object_spread_properties_cache(spread_type, result.clone());
        }
        result
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access.set(enabled);
        self.interner.set_no_unchecked_indexed_access(enabled);
    }

    fn set_exact_optional_property_types(&self, enabled: bool) {
        self.exact_optional_property_types.set(enabled);
        self.interner.set_exact_optional_property_types(enabled);
    }

    fn set_strict_null_checks(&self, enabled: bool) {
        self.strict_null_checks.set(enabled);
        self.interner.set_strict_null_checks(enabled);
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
        // Check the stable tier first, then the artifact tier.
        if let Some(canonical) = self
            .canonical_cache
            .borrow()
            .get(&type_id)
            .copied()
            .or_else(|| {
                self.canonical_artifact_cache
                    .borrow()
                    .get(&type_id)
                    .copied()
            })
        {
            return canonical;
        }

        // Compute canonical form using a fresh Canonicalizer
        // CRITICAL: Always start with empty stacks for absolute De Bruijn indices
        // This ensures the cached TypeId represents the absolute structural form
        //
        // The canonicalizer shares interior and root results through the two
        // tiers (see `Canonicalizer::shared_cache` / `shared_artifacts` for
        // the invariants, #13508): stable results land in `canonical_cache`,
        // dirty (registration-window or guard-truncated) results land in
        // `canonical_artifact_cache`, whose reuse taints the consumer so an
        // artifact can never be laundered into the stable tier through an
        // enclosing type.
        use crate::canonicalize::Canonicalizer;
        let mut canon = Canonicalizer::new(self.as_type_database(), self)
            .with_shared_cache(&self.canonical_cache)
            .with_shared_artifacts(&self.canonical_artifact_cache);
        canon.canonicalize(type_id)
    }
}

#[cfg(test)]
#[path = "../../tests/db_tests.rs"]
mod tests;

#[path = "query_cache_collect_properties_memo.rs"]
mod collect_properties_memo;
#[path = "query_cache_intersection_merge_memo.rs"]
mod intersection_merge_memo;

#[path = "query_cache_application_eval.rs"]
mod application_eval;

#[path = "query_cache_size.rs"]
mod size;

#[path = "query_cache_property.rs"]
mod property;

#[path = "query_cache_spread.rs"]
mod spread;

#[path = "query_cache_registration_window.rs"]
mod registration_window;
