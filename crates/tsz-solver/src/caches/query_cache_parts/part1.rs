use crate::caches::db::{
    QueryDatabase, TypeApplicationEvalCache, TypeCompilerOptions, TypeDatabase,
    TypeDisplayProvenance, TypePredicateCache, TypeTupleLimitSignal,
};

use crate::caches::instantiation_cache::{InstantiationCache, InstantiationCacheKey};

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
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, StringIntrinsicKind, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo, Variance, Visibility,
};

use crate::visitor::is_error_type;

use dashmap::DashMap;

use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use std::cell::{Cell, RefCell};

use std::sync::Arc;

use tsz_binder::SymbolId;

use tsz_common::interner::Atom;

type ApplicationEvalCacheKey = (DefId, smallvec::SmallVec<[TypeId; 4]>, bool);

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
    ) -> &DashMap<RelationCacheKey, bool, FxBuildHasher> {
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
/// correctness risk. See issue #9507.
pub struct SharedQueryCache {
    eval_cache: DashMap<EvaluationCacheKey, TypeId, FxBuildHasher>,
    subtype_cache: DashMap<RelationCacheKey, bool, FxBuildHasher>,
    assignability_cache: DashMap<RelationCacheKey, bool, FxBuildHasher>,
}

impl SharedQueryCache {
    pub fn new() -> Self {
        SharedQueryCache {
            eval_cache: DashMap::with_hasher(FxBuildHasher),
            subtype_cache: DashMap::with_hasher(FxBuildHasher),
            assignability_cache: DashMap::with_hasher(FxBuildHasher),
        }
    }

    /// Number of entries across all shared caches.
    pub fn total_entries(&self) -> usize {
        self.eval_cache.len() + self.subtype_cache.len() + self.assignability_cache.len()
    }
}

impl Default for SharedQueryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationCacheProbe {
    Hit(bool),
    MissNotCached,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationCacheStats {
    pub subtype_hits: u64,
    pub subtype_misses: u64,
    pub subtype_entries: usize,
    pub assignability_hits: u64,
    pub assignability_misses: u64,
    pub assignability_entries: usize,
}

/// Snapshot of all `QueryCache` sizes for observability.
///
/// Captures entry counts for every memoization cache and relation hit/miss
/// counters. Intended for `--extendedDiagnostics` and performance monitoring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueryCacheStatistics {
    /// Number of memoized `evaluate_type` results.
    pub eval_cache_entries: usize,
    /// Number of memoized application evaluation results.
    pub application_eval_cache_entries: usize,
    /// Number of times the application eval cache returned a hit.
    pub application_eval_cache_hits: u64,
    /// Number of times the application eval cache was probed and missed.
    pub application_eval_cache_misses: u64,
    /// Number of memoized element access results.
    pub element_access_cache_entries: usize,
    /// Number of memoized object spread property lists.
    pub object_spread_cache_entries: usize,
    /// Number of memoized property access results.
    pub property_cache_entries: usize,
    /// Number of memoized variance computations.
    pub variance_cache_entries: usize,
    /// Number of memoized canonical type mappings.
    pub canonical_cache_entries: usize,
    /// Number of memoized intersection-to-merged-object results.
    pub intersection_merge_cache_entries: usize,
    /// Number of times the intersection-merge cache returned a hit.
    pub intersection_merge_cache_hits: u64,
    /// Number of times the intersection-merge cache was probed and missed.
    pub intersection_merge_cache_misses: u64,
    /// Number of memoized `instantiate_type` results.
    pub instantiation_cache_entries: usize,
    /// Number of times the instantiation cache returned a hit.
    pub instantiation_cache_hits: u64,
    /// Number of times the instantiation cache was probed and missed.
    pub instantiation_cache_misses: u64,
    /// Number of memoized `remove_subtypes_for_bct` results.
    pub subtype_reduction_cache_entries: usize,
    /// Number of times the subtype-reduction cache returned a hit.
    pub subtype_reduction_cache_hits: u64,
    /// Number of times the subtype-reduction cache was probed and missed.
    pub subtype_reduction_cache_misses: u64,
    /// Relation (subtype + assignability) cache statistics.
    pub relation: RelationCacheStats,
}

impl QueryCacheStatistics {
    /// Merge another snapshot into this one (for aggregating per-file caches in parallel builds).
    pub const fn merge(&mut self, other: &QueryCacheStatistics) {
        self.eval_cache_entries += other.eval_cache_entries;
        self.application_eval_cache_entries += other.application_eval_cache_entries;
        self.application_eval_cache_hits += other.application_eval_cache_hits;
        self.application_eval_cache_misses += other.application_eval_cache_misses;
        self.element_access_cache_entries += other.element_access_cache_entries;
        self.object_spread_cache_entries += other.object_spread_cache_entries;
        self.property_cache_entries += other.property_cache_entries;
        self.variance_cache_entries += other.variance_cache_entries;
        self.canonical_cache_entries += other.canonical_cache_entries;
        self.intersection_merge_cache_entries += other.intersection_merge_cache_entries;
        self.intersection_merge_cache_hits += other.intersection_merge_cache_hits;
        self.intersection_merge_cache_misses += other.intersection_merge_cache_misses;
        self.instantiation_cache_entries += other.instantiation_cache_entries;
        self.instantiation_cache_hits += other.instantiation_cache_hits;
        self.instantiation_cache_misses += other.instantiation_cache_misses;
        self.subtype_reduction_cache_entries += other.subtype_reduction_cache_entries;
        self.subtype_reduction_cache_hits += other.subtype_reduction_cache_hits;
        self.subtype_reduction_cache_misses += other.subtype_reduction_cache_misses;
        self.relation.subtype_hits += other.relation.subtype_hits;
        self.relation.subtype_misses += other.relation.subtype_misses;
        self.relation.subtype_entries += other.relation.subtype_entries;
        self.relation.assignability_hits += other.relation.assignability_hits;
        self.relation.assignability_misses += other.relation.assignability_misses;
        self.relation.assignability_entries += other.relation.assignability_entries;
    }
}

impl QueryCacheStatistics {
    /// Estimate total in-memory size of all caches in bytes.
    ///
    /// Uses conservative per-entry estimates for `FxHashMap` bucket metadata plus
    /// key/value sizes. Heap allocations inside values are intentionally excluded.
    #[must_use]
    pub const fn estimated_size_bytes(&self) -> usize {
        const BUCKET_OVERHEAD: usize = 64;

        let eval = self.eval_cache_entries * (BUCKET_OVERHEAD + 13);

        let app_eval = self.application_eval_cache_entries * (BUCKET_OVERHEAD + 37);

        let elem = self.element_access_cache_entries * (BUCKET_OVERHEAD + 21);

        let spread = self.object_spread_cache_entries * (BUCKET_OVERHEAD + 4 + 24 + 256);

        let prop = self.property_cache_entries * (BUCKET_OVERHEAD + 25);

        let variance = self.variance_cache_entries * (BUCKET_OVERHEAD + 16);

        let canonical = self.canonical_cache_entries * (BUCKET_OVERHEAD + 8);

        let intersection_merge = self.intersection_merge_cache_entries * (BUCKET_OVERHEAD + 12);

        let subtype = self.relation.subtype_entries * (BUCKET_OVERHEAD + 13);

        let assignability = self.relation.assignability_entries * (BUCKET_OVERHEAD + 13);

        let instantiation = self.instantiation_cache_entries * (BUCKET_OVERHEAD + 65);

        let subtype_reduction = self.subtype_reduction_cache_entries * (BUCKET_OVERHEAD + 73);

        eval + app_eval
            + elem
            + spread
            + prop
            + variance
            + canonical
            + intersection_merge
            + subtype
            + assignability
            + instantiation
            + subtype_reduction
    }
}

impl std::fmt::Display for QueryCacheStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "QueryCache statistics:")?;
        writeln!(f, "  eval_cache:             {}", self.eval_cache_entries)?;
        writeln!(
            f,
            "  application_eval_cache: {} entries ({} hits, {} misses)",
            self.application_eval_cache_entries,
            self.application_eval_cache_hits,
            self.application_eval_cache_misses,
        )?;
        writeln!(
            f,
            "  element_access_cache:   {}",
            self.element_access_cache_entries
        )?;
        writeln!(
            f,
            "  object_spread_cache:    {}",
            self.object_spread_cache_entries
        )?;
        writeln!(
            f,
            "  property_cache:         {}",
            self.property_cache_entries
        )?;
        writeln!(
            f,
            "  variance_cache:         {}",
            self.variance_cache_entries
        )?;
        writeln!(
            f,
            "  canonical_cache:        {}",
            self.canonical_cache_entries
        )?;
        writeln!(
            f,
            "  intersection_merge:     {} entries ({} hits, {} misses)",
            self.intersection_merge_cache_entries,
            self.intersection_merge_cache_hits,
            self.intersection_merge_cache_misses,
        )?;
        writeln!(
            f,
            "  subtype_cache:          {} entries ({} hits, {} misses)",
            self.relation.subtype_entries, self.relation.subtype_hits, self.relation.subtype_misses,
        )?;
        writeln!(
            f,
            "  assignability_cache:    {} entries ({} hits, {} misses)",
            self.relation.assignability_entries,
            self.relation.assignability_hits,
            self.relation.assignability_misses,
        )?;
        writeln!(
            f,
            "  instantiation_cache:    {} entries ({} hits, {} misses)",
            self.instantiation_cache_entries,
            self.instantiation_cache_hits,
            self.instantiation_cache_misses,
        )?;
        writeln!(
            f,
            "  subtype_reduction:      {} entries ({} hits, {} misses)",
            self.subtype_reduction_cache_entries,
            self.subtype_reduction_cache_hits,
            self.subtype_reduction_cache_misses,
        )?;
        write!(
            f,
            "  estimated_size:         {} bytes ({:.1} KB)",
            self.estimated_size_bytes(),
            self.estimated_size_bytes() as f64 / 1024.0,
        )
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
    /// Substitution-independent evaluation cache (see the `closed_eval` module
    /// in `evaluate`). Keyed by `(TypeId, no_unchecked_indexed_access)`.
    closed_eval_cache: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    application_eval_cache: RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    element_access_cache: RefCell<FxHashMap<ElementAccessTypeCacheKey, TypeId>>,
    object_spread_properties_cache: RefCell<FxHashMap<TypeId, Vec<PropertyInfo>>>,
    subtype_cache: RefCell<FxHashMap<RelationCacheKey, bool>>,
    /// Separate cache for assignability to prevent loose results from poisoning subtype checks.
    assignability_cache: RefCell<FxHashMap<RelationCacheKey, bool>>,
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

    /// Estimate the in-memory size of all caches in bytes.
    ///
    /// Accounts for `FxHashMap` bucket overhead, key/value sizes, and heap
    /// allocations inside cached values (e.g., `Vec<PropertyInfo>` in the
    /// object-spread cache, `Arc<[Variance]>` in the variance cache).
    ///
    /// This is more accurate than `QueryCacheStatistics::estimated_size_bytes()`
    /// because it reads actual map capacities and heap contents.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // FxHashMap per-bucket overhead: hash + key + value + alignment padding.
        const BUCKET_OVERHEAD: usize = 64;

        let mut size = std::mem::size_of::<Self>();

        // eval_cache: (TypeId, bool) -> TypeId
        {
            let map = self.eval_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<EvaluationCacheKey>()
                    + std::mem::size_of::<TypeId>());
        }

        // application_eval_cache: (DefId, SmallVec<[TypeId; 4]>, bool) -> TypeId
        {
            let map = self.application_eval_cache.borrow();
            let base_entry = BUCKET_OVERHEAD
                + std::mem::size_of::<ApplicationEvalCacheKey>()
                + std::mem::size_of::<TypeId>();
            size += map.capacity() * base_entry;
            // SmallVec spills to heap when > 4 elements; account for spilled entries.
            for key in map.keys() {
                if key.1.spilled() {
                    size += key.1.capacity() * std::mem::size_of::<TypeId>();
                }
            }
        }

        // element_access_cache
        {
            let map = self.element_access_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<ElementAccessTypeCacheKey>()
                    + std::mem::size_of::<TypeId>());
        }

        // object_spread_properties_cache: TypeId -> Vec<PropertyInfo>
        {
            let map = self.object_spread_properties_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Vec<PropertyInfo>>());
            for props in map.values() {
                size += props.capacity() * std::mem::size_of::<PropertyInfo>();
            }
        }

        // subtype_cache
        {
            let map = self.subtype_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<RelationCacheKey>()
                    + std::mem::size_of::<bool>());
        }

        // assignability_cache
        {
            let map = self.assignability_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<RelationCacheKey>()
                    + std::mem::size_of::<bool>());
        }

        // property_cache
        {
            let map = self.property_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<PropertyAccessCacheKey>()
                    + std::mem::size_of::<PropertyAccessResult>());
        }

        // variance_cache: DefId -> Arc<[Variance]>
        {
            let map = self.variance_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<DefId>()
                    + std::mem::size_of::<Arc<[Variance]>>());
            // Account for the Arc-allocated slice contents
            for arc in map.values() {
                size += arc.len() * std::mem::size_of::<Variance>();
            }
        }

        // canonical_cache
        {
            let map = self.canonical_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + 2 * std::mem::size_of::<TypeId>());
        }

        // intersection_merge_cache: TypeId -> Option<TypeId>
        {
            let map = self.intersection_merge_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Option<TypeId>>());
        }

        // instantiation_cache: (TypeId, CanonicalSubst, u8, Option<TypeId>) -> TypeId
        // CanonicalSubst's inline SmallVec buffer is included in the
        // `InstantiationCacheKey` size; spilled entries pay extra heap.
        size += self.instantiation_cache.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<InstantiationCacheKey>()
                + std::mem::size_of::<TypeId>());

        // subtype_reduction_cache: (SortedTypeIds, u8) -> Arc<[TypeId]>
        // Inline buffer is part of `SubtypeReductionKey`; the cached value
        // is `Arc<[TypeId]>` (16 bytes) plus the heap slice it points at.
        size += self.subtype_reduction_cache.capacity()
            * (BUCKET_OVERHEAD
                + std::mem::size_of::<SubtypeReductionKey>()
                + std::mem::size_of::<std::sync::Arc<[TypeId]>>());

        size
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
    ) -> &RefCell<FxHashMap<RelationCacheKey, bool>> {
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

    fn lookup_policy_relation_cache(
        &self,
        relation: CachedPolicyRelation,
        key: RelationCacheKey,
    ) -> Option<bool> {
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

    fn insert_policy_relation_cache(
        &self,
        relation: CachedPolicyRelation,
        key: RelationCacheKey,
        result: bool,
    ) {
        self.relation_local_cache(relation)
            .borrow_mut()
            .insert(key, result);
        if let Some(shared) = self.shared {
            relation.shared_slot(shared).insert(key, result);
        }
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

        if let Some(result) = self.lookup_policy_relation_cache(relation, key) {
            if let Some(query_id) = trace_query_id {
                query_trace::relation_end(query_id, trace_op, result, true);
            }
            return result;
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
        self.application_eval_cache_misses
            .set(self.application_eval_cache_misses.get() + 1);
        None
    }

    fn insert_application_eval_cache(&self, key: ApplicationEvalCacheKey, result: TypeId) {
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
            ),
            result,
        );
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
            EvaluationCacheKey::new(type_id, no_unchecked_indexed_access),
            result,
        );
    }
}
