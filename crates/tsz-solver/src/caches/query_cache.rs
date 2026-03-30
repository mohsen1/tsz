//! Cached query database implementation for the solver.
//!
//! `QueryCache` wraps a `TypeInterner` with memoization for evaluation,
//! relation, property, and element access queries. This is the concrete
//! database implementation used by the checker at runtime.

use crate::caches::db::{QueryDatabase, TypeDatabase};
use crate::caches::query_trace;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::objects::element_access::ElementAccessResult;
use crate::operations::property::PropertyAccessResult;
use crate::relations::compat::CompatChecker;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, StringIntrinsicKind, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo, Variance, Visibility,
};
use dashmap::DashMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

type EvalCacheKey = (TypeId, bool);
type ApplicationEvalCacheKey = (DefId, smallvec::SmallVec<[TypeId; 4]>, bool);
type ElementAccessTypeCacheKey = (TypeId, TypeId, Option<u32>, bool);
type PropertyAccessCacheKey = (TypeId, Atom, bool);

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
pub struct SharedQueryCache {
    eval_cache: DashMap<EvalCacheKey, TypeId>,
    subtype_cache: DashMap<RelationCacheKey, bool>,
    assignability_cache: DashMap<RelationCacheKey, bool>,
}

impl SharedQueryCache {
    pub fn new() -> Self {
        SharedQueryCache {
            eval_cache: DashMap::new(),
            subtype_cache: DashMap::new(),
            assignability_cache: DashMap::new(),
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
    /// Relation (subtype + assignability) cache statistics.
    pub relation: RelationCacheStats,
}

impl QueryCacheStatistics {
    /// Merge another snapshot into this one (for aggregating per-file caches in parallel builds).
    pub const fn merge(&mut self, other: &QueryCacheStatistics) {
        self.eval_cache_entries += other.eval_cache_entries;
        self.application_eval_cache_entries += other.application_eval_cache_entries;
        self.element_access_cache_entries += other.element_access_cache_entries;
        self.object_spread_cache_entries += other.object_spread_cache_entries;
        self.property_cache_entries += other.property_cache_entries;
        self.variance_cache_entries += other.variance_cache_entries;
        self.canonical_cache_entries += other.canonical_cache_entries;
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
    /// Uses per-entry cost estimates based on `FxHashMap` bucket overhead (~64 bytes)
    /// plus the key and value sizes. This is intentionally conservative — it does not
    /// account for heap allocations inside values like `Vec<PropertyInfo>` or
    /// `Arc<[Variance]>`, but captures the dominant cost (hash table metadata).
    ///
    /// For the `object_spread_cache`, we assume an average of 4 properties per entry
    /// since the actual `Vec` contents are not tracked in the statistics snapshot.
    #[must_use]
    pub const fn estimated_size_bytes(&self) -> usize {
        // FxHashMap overhead per bucket: hash (8) + key + value + padding.
        // We use 64 bytes as a conservative per-bucket overhead constant.
        const BUCKET_OVERHEAD: usize = 64;

        // eval_cache: (TypeId, bool) -> TypeId  ≈ 8 + 1 + 4 = 13 bytes key+value
        let eval = self.eval_cache_entries * (BUCKET_OVERHEAD + 13);

        // application_eval_cache: (DefId, SmallVec<[TypeId;4]>, bool) -> TypeId
        // SmallVec<[TypeId;4]> inline = 4*4 + len + cap = ~24 bytes; DefId=8, bool=1, TypeId=4
        let app_eval = self.application_eval_cache_entries * (BUCKET_OVERHEAD + 37);

        // element_access_cache: (TypeId, TypeId, Option<u32>, bool) -> TypeId  ≈ 4+4+8+1+4 = 21
        let elem = self.element_access_cache_entries * (BUCKET_OVERHEAD + 21);

        // object_spread_cache: TypeId -> Vec<PropertyInfo>
        // Vec header = 24 bytes; average ~4 PropertyInfo entries at ~64 bytes each = 256
        let spread = self.object_spread_cache_entries * (BUCKET_OVERHEAD + 4 + 24 + 256);

        // property_cache: (TypeId, Atom, bool) -> PropertyAccessResult  ≈ 4+4+1 + 16 = 25
        let prop = self.property_cache_entries * (BUCKET_OVERHEAD + 25);

        // variance_cache: DefId -> Arc<[Variance]>  ≈ 8 + 8(Arc ptr) = 16
        let variance = self.variance_cache_entries * (BUCKET_OVERHEAD + 16);

        // canonical_cache: TypeId -> TypeId  ≈ 4+4 = 8
        let canonical = self.canonical_cache_entries * (BUCKET_OVERHEAD + 8);

        // subtype_cache: RelationCacheKey -> bool  ≈ 12 + 1 = 13
        let subtype = self.relation.subtype_entries * (BUCKET_OVERHEAD + 13);

        // assignability_cache: RelationCacheKey -> bool  ≈ 12 + 1 = 13
        let assignability = self.relation.assignability_entries * (BUCKET_OVERHEAD + 13);

        eval + app_eval + elem + spread + prop + variance + canonical + subtype + assignability
    }
}

impl std::fmt::Display for QueryCacheStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "QueryCache statistics:")?;
        writeln!(f, "  eval_cache:             {}", self.eval_cache_entries)?;
        writeln!(
            f,
            "  application_eval_cache: {}",
            self.application_eval_cache_entries
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
    eval_cache: RefCell<FxHashMap<EvalCacheKey, TypeId>>,
    application_eval_cache: RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    element_access_cache: RefCell<FxHashMap<ElementAccessTypeCacheKey, TypeId>>,
    object_spread_properties_cache: RefCell<FxHashMap<TypeId, Vec<PropertyInfo>>>,
    subtype_cache: RefCell<FxHashMap<RelationCacheKey, bool>>,
    /// CRITICAL: Separate cache for assignability to prevent cache poisoning.
    /// This ensures that loose assignability results (e.g., any is assignable to number)
    /// don't contaminate strict subtype checks.
    assignability_cache: RefCell<FxHashMap<RelationCacheKey, bool>>,
    property_cache: RefCell<FxHashMap<PropertyAccessCacheKey, PropertyAccessResult>>,
    /// Task #41: Variance cache for generic type parameters.
    /// Stores computed variance masks for `DefIds` to enable O(1) generic assignability.
    variance_cache: RefCell<FxHashMap<DefId, Arc<[Variance]>>>,
    /// Task #49: Canonical cache for O(1) structural identity checks.
    /// Maps `TypeId` -> canonical `TypeId` for structurally identical types.
    canonical_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Cache for intersection-to-merged-object results.
    /// Avoids expensive `collect_properties` calls for the same intersection target
    /// across multiple `SubtypeChecker` instances (common in constraint checking).
    /// `Some(type_id)` = successfully merged, `None` = not eligible for merging.
    intersection_merge_cache: RefCell<FxHashMap<TypeId, Option<TypeId>>>,
    subtype_cache_hits: Cell<u64>,
    subtype_cache_misses: Cell<u64>,
    assignability_cache_hits: Cell<u64>,
    assignability_cache_misses: Cell<u64>,
    no_unchecked_indexed_access: Cell<bool>,
    /// Optional shared cross-file cache for multi-file project checking.
    /// When present, local cache misses fall through to the shared DashMap cache,
    /// and local cache inserts are also written to the shared cache.
    shared: Option<&'a SharedQueryCache>,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        QueryCache {
            interner,
            eval_cache: RefCell::new(FxHashMap::default()),
            application_eval_cache: RefCell::new(FxHashMap::default()),
            element_access_cache: RefCell::new(FxHashMap::default()),
            object_spread_properties_cache: RefCell::new(FxHashMap::default()),
            subtype_cache: RefCell::new(FxHashMap::default()),
            assignability_cache: RefCell::new(FxHashMap::default()),
            property_cache: RefCell::new(FxHashMap::default()),
            variance_cache: RefCell::new(FxHashMap::default()),
            canonical_cache: RefCell::new(FxHashMap::default()),
            intersection_merge_cache: RefCell::new(FxHashMap::default()),
            subtype_cache_hits: Cell::new(0),
            subtype_cache_misses: Cell::new(0),
            assignability_cache_hits: Cell::new(0),
            assignability_cache_misses: Cell::new(0),
            no_unchecked_indexed_access: Cell::new(interner.no_unchecked_indexed_access()),
            shared: None,
        }
    }

    /// Create a `QueryCache` backed by a shared cross-file cache.
    ///
    /// Local `RefCell`-based caches provide zero-overhead single-threaded access.
    /// On local miss, the shared `DashMap` cache is consulted. Results are written
    /// to both local and shared caches for cross-file benefit.
    pub fn new_with_shared(interner: &'a TypeInterner, shared: &'a SharedQueryCache) -> Self {
        QueryCache {
            interner,
            eval_cache: RefCell::new(FxHashMap::default()),
            application_eval_cache: RefCell::new(FxHashMap::default()),
            element_access_cache: RefCell::new(FxHashMap::default()),
            object_spread_properties_cache: RefCell::new(FxHashMap::default()),
            subtype_cache: RefCell::new(FxHashMap::default()),
            assignability_cache: RefCell::new(FxHashMap::default()),
            property_cache: RefCell::new(FxHashMap::default()),
            variance_cache: RefCell::new(FxHashMap::default()),
            canonical_cache: RefCell::new(FxHashMap::default()),
            intersection_merge_cache: RefCell::new(FxHashMap::default()),
            subtype_cache_hits: Cell::new(0),
            subtype_cache_misses: Cell::new(0),
            assignability_cache_hits: Cell::new(0),
            assignability_cache_misses: Cell::new(0),
            no_unchecked_indexed_access: Cell::new(interner.no_unchecked_indexed_access()),
            shared: Some(shared),
        }
    }

    pub fn clear(&self) {
        self.eval_cache.borrow_mut().clear();
        self.element_access_cache.borrow_mut().clear();
        self.application_eval_cache.borrow_mut().clear();
        self.object_spread_properties_cache.borrow_mut().clear();
        self.subtype_cache.borrow_mut().clear();
        self.assignability_cache.borrow_mut().clear();
        self.property_cache.borrow_mut().clear();
        self.variance_cache.borrow_mut().clear();
        self.canonical_cache.borrow_mut().clear();
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
            element_access_cache_entries: self.element_access_cache.borrow().len(),
            object_spread_cache_entries: self.object_spread_properties_cache.borrow().len(),
            property_cache_entries: self.property_cache.borrow().len(),
            variance_cache_entries: self.variance_cache.borrow().len(),
            canonical_cache_entries: self.canonical_cache.borrow().len(),
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
                    + std::mem::size_of::<EvalCacheKey>()
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
            for (key, _) in map.iter() {
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
            for (_, props) in map.iter() {
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
            for (_, arc) in map.iter() {
                size += arc.len() * std::mem::size_of::<Variance>();
            }
        }

        // canonical_cache
        {
            let map = self.canonical_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + 2 * std::mem::size_of::<TypeId>());
        }

        size
    }

    pub fn reset_relation_cache_stats(&self) {
        self.subtype_cache_hits.set(0);
        self.subtype_cache_misses.set(0);
        self.assignability_cache_hits.set(0);
        self.assignability_cache_misses.set(0);
    }

    pub fn probe_subtype_cache(&self, key: RelationCacheKey) -> RelationCacheProbe {
        match self.lookup_subtype_cache(key) {
            Some(result) => RelationCacheProbe::Hit(result),
            None => RelationCacheProbe::MissNotCached,
        }
    }

    /// Helper to check a relation cache.
    fn check_cache(
        &self,
        cache: &RefCell<FxHashMap<RelationCacheKey, bool>>,
        key: RelationCacheKey,
    ) -> Option<bool> {
        cache.borrow().get(&key).copied()
    }

    /// Helper to insert into a relation cache.
    fn insert_cache(
        &self,
        cache: &RefCell<FxHashMap<RelationCacheKey, bool>>,
        key: RelationCacheKey,
        result: bool,
    ) {
        cache.borrow_mut().insert(key, result);
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
        self.application_eval_cache.borrow().get(&key).copied()
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
                let non_nullish: Vec<TypeId> = members
                    .iter()
                    .copied()
                    .filter(|m| !m.is_nullable())
                    .collect();

                if non_nullish.is_empty() {
                    return Vec::new();
                }

                // Collect properties per member
                let mut all_props: Vec<Vec<PropertyInfo>> = Vec::new();
                for &member in &non_nullish {
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
                        let optional = was_optional || has_nullish || count < non_nullish.len();
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
                        }
                    })
                    .collect()
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

impl TypeDatabase for QueryCache<'_> {
    fn intern(&self, key: TypeData) -> TypeId {
        self.interner.intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        self.interner.lookup(id)
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

    fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.interner.store_display_properties(type_id, props);
    }

    fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.interner.get_display_properties(type_id)
    }

    fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        self.interner.store_display_alias(evaluated, application);
    }

    fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.interner.get_display_alias(type_id)
    }

    fn take_union_too_complex(&self) -> bool {
        self.interner.take_union_too_complex()
    }

    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId> {
        // Delegate to the interner
        self.interner.get_class_base_type(symbol_id)
    }

    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        self.interner.is_identity_comparable_type(type_id)
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
}

impl QueryDatabase for QueryCache<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.interner.set_array_base_type(type_id, type_params);
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

        let key = (type_id, no_unchecked_indexed_access);
        let cached = self.eval_cache.borrow().get(&key).copied();

        if let Some(result) = cached {
            return result;
        }

        // L2: Check shared cross-file cache before doing expensive evaluation.
        if let Some(shared) = self.shared {
            if let Some(result) = shared.eval_cache.get(&key).map(|r| *r) {
                self.eval_cache.borrow_mut().insert(key, result);
                return result;
            }
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

        let mut evaluator =
            crate::evaluation::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator = evaluator.with_query_db(self);
        let result = evaluator.evaluate(type_id);

        // PERF: Persist intermediate evaluation results from this session into
        // the long-lived eval_cache. During recursive mapped type expansion
        // (e.g., DeepPartial<T>), the evaluator computes many sub-results
        // that would otherwise be recomputed in subsequent top-level evaluate
        // calls. Only persist entries where the result differs from the input
        // (identity mappings are free to recompute) and skip intrinsics.
        {
            let mut cache = self.eval_cache.borrow_mut();
            cache.insert(key, result);
            // Also write to shared cache for cross-file benefit.
            if let Some(shared) = self.shared {
                shared.eval_cache.insert(key, result);
            }
            for (intermediate_id, intermediate_result) in evaluator.drain_cache() {
                if intermediate_id != intermediate_result && !intermediate_id.is_intrinsic() {
                    let ikey = (intermediate_id, no_unchecked_indexed_access);
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
        self.insert_application_eval_cache(
            (
                def_id,
                smallvec::SmallVec::from_slice(args),
                no_unchecked_indexed_access,
            ),
            result,
        );
    }

    fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        // Fast identity/top/bottom paths — avoid cache key construction, RefCell
        // borrow, and SubtypeChecker allocation entirely.
        if source == target
            || target == TypeId::UNKNOWN
            || source == TypeId::NEVER
            || source == TypeId::ERROR
            || target == TypeId::ERROR
        {
            return true;
        }
        if target == TypeId::NEVER {
            return false;
        }
        // `any` is assignable to/from everything except `never` (already handled above).
        // At the top-level (depth 0), allow_any is always true in SubtypeChecker,
        // so this is safe regardless of flags.
        if source == TypeId::ANY || target == TypeId::ANY {
            return true;
        }

        let trace_enabled = query_trace::enabled();
        let trace_query_id = trace_enabled.then(|| {
            let query_id = query_trace::next_query_id();
            query_trace::relation_start(
                query_id,
                "is_subtype_of_with_flags",
                source,
                target,
                flags,
            );
            query_id
        });
        let key = RelationCacheKey::subtype(source, target, flags, 0);
        let cached = self.subtype_cache.borrow().get(&key).copied();

        if let Some(result) = cached {
            self.subtype_cache_hits
                .set(self.subtype_cache_hits.get() + 1);
            if let Some(query_id) = trace_query_id {
                query_trace::relation_end(query_id, "is_subtype_of_with_flags", result, true);
            }
            return result;
        }

        // L2: Check shared cross-file cache.
        if let Some(shared) = self.shared {
            if let Some(result) = shared.subtype_cache.get(&key).map(|r| *r) {
                self.subtype_cache.borrow_mut().insert(key, result);
                self.subtype_cache_hits
                    .set(self.subtype_cache_hits.get() + 1);
                if let Some(query_id) = trace_query_id {
                    query_trace::relation_end(query_id, "is_subtype_of_with_flags", result, true);
                }
                return result;
            }
        }

        self.subtype_cache_misses
            .set(self.subtype_cache_misses.get() + 1);

        let result = crate::relations::subtype::is_subtype_of_with_flags(
            self.as_type_database(),
            source,
            target,
            flags,
        );
        self.subtype_cache.borrow_mut().insert(key, result);
        // Write to shared cache for cross-file benefit.
        if let Some(shared) = self.shared {
            shared.subtype_cache.insert(key, result);
        }
        if let Some(query_id) = trace_query_id {
            query_trace::relation_end(query_id, "is_subtype_of_with_flags", result, false);
        }
        result
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        // Fast identity/top/bottom paths — avoid cache key construction, RefCell
        // borrow, and CompatChecker allocation entirely.
        if source == target
            || target == TypeId::UNKNOWN
            || source == TypeId::NEVER
            || source == TypeId::ERROR
            || target == TypeId::ERROR
        {
            return true;
        }
        if target == TypeId::NEVER && source != TypeId::NEVER {
            return false;
        }
        // `any` is assignable to/from everything except `never` (already handled above).
        // CompatChecker defaults to allow_any_suppression=true (non-sound mode),
        // and apply_flags does not change it, so this is safe.
        if source == TypeId::ANY || target == TypeId::ANY {
            return true;
        }

        let trace_enabled = query_trace::enabled();
        let trace_query_id = trace_enabled.then(|| {
            let query_id = query_trace::next_query_id();
            query_trace::relation_start(
                query_id,
                "is_assignable_to_with_flags",
                source,
                target,
                flags,
            );
            query_id
        });
        // Task A: Use passed flags instead of hardcoded 0,0
        let key = RelationCacheKey::assignability(source, target, flags, 0);

        if let Some(result) = self.check_cache(&self.assignability_cache, key) {
            self.assignability_cache_hits
                .set(self.assignability_cache_hits.get() + 1);
            if let Some(query_id) = trace_query_id {
                query_trace::relation_end(query_id, "is_assignable_to_with_flags", result, true);
            }
            return result;
        }

        // L2: Check shared cross-file cache.
        if let Some(shared) = self.shared {
            if let Some(result) = shared.assignability_cache.get(&key).map(|r| *r) {
                self.assignability_cache.borrow_mut().insert(key, result);
                self.assignability_cache_hits
                    .set(self.assignability_cache_hits.get() + 1);
                if let Some(query_id) = trace_query_id {
                    query_trace::relation_end(
                        query_id,
                        "is_assignable_to_with_flags",
                        result,
                        true,
                    );
                }
                return result;
            }
        }

        self.assignability_cache_misses
            .set(self.assignability_cache_misses.get() + 1);

        // Use CompatChecker with all compatibility rules
        let mut checker = CompatChecker::new(self.as_type_database());

        // FIX: Apply flags to ensure checker matches the cache key configuration
        // This prevents cache poisoning where results from non-strict checks
        // leak into strict checks (Gap C fix)
        checker.apply_flags(flags);

        let result = checker.is_assignable(source, target);

        self.insert_cache(&self.assignability_cache, key, result);
        // Write to shared cache for cross-file benefit.
        if let Some(shared) = self.shared {
            shared.assignability_cache.insert(key, result);
        }
        if let Some(query_id) = trace_query_id {
            query_trace::relation_end(query_id, "is_assignable_to_with_flags", result, false);
        }
        result
    }

    /// Convenience wrapper for `is_subtype_of` with default flags.
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of_with_flags(source, target, 0) // Default non-strict mode for backward compatibility
    }

    /// Convenience wrapper for `is_assignable_to` with default flags.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to_with_flags(source, target, 0) // Default non-strict mode for backward compatibility
    }

    fn lookup_subtype_cache(&self, key: RelationCacheKey) -> Option<bool> {
        let result = self.subtype_cache.borrow().get(&key).copied();
        if result.is_some() {
            self.subtype_cache_hits
                .set(self.subtype_cache_hits.get() + 1);
        } else {
            self.subtype_cache_misses
                .set(self.subtype_cache_misses.get() + 1);
        }
        result
    }

    fn insert_subtype_cache(&self, key: RelationCacheKey, result: bool) {
        self.subtype_cache.borrow_mut().insert(key, result);
    }

    fn lookup_assignability_cache(&self, key: RelationCacheKey) -> Option<bool> {
        let result = self.assignability_cache.borrow().get(&key).copied();
        if result.is_some() {
            self.assignability_cache_hits
                .set(self.assignability_cache_hits.get() + 1);
        } else {
            self.assignability_cache_misses
                .set(self.assignability_cache_misses.get() + 1);
        }
        result
    }

    fn insert_assignability_cache(&self, key: RelationCacheKey, result: bool) {
        self.assignability_cache.borrow_mut().insert(key, result);
    }

    fn lookup_intersection_merge(&self, intersection_id: TypeId) -> Option<Option<TypeId>> {
        self.intersection_merge_cache
            .borrow()
            .get(&intersection_id)
            .copied()
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
        let key = (object_type, prop_atom, no_unchecked_indexed_access);
        if let Some(result) = self.check_property_cache(key) {
            return result;
        }

        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        let result = evaluator.resolve_property_access(object_type, prop_name);
        self.insert_property_cache(key, result);
        result
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

    fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access.get()
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access.set(enabled);
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        // 1. Check cache first
        if let Some(cached) = self.variance_cache.borrow().get(&def_id) {
            return Some(Arc::clone(cached));
        }

        // 2. Compute variance using the type's body
        // This requires the database to also be a TypeResolver (which QueryDatabase is)
        let params = self.get_lazy_type_params(def_id)?;
        if params.is_empty() {
            return None;
        }

        let body = self.resolve_lazy(def_id, self.as_type_database())?;

        let mut variances = Vec::with_capacity(params.len());
        for param in &params {
            // Compute variance for each type parameter
            let v = crate::relations::variance::compute_variance(self, body, param.name);
            variances.push(v);
        }
        let result = Arc::from(variances);

        // 3. Store in cache
        self.variance_cache
            .borrow_mut()
            .insert(def_id, Arc::clone(&result));

        Some(result)
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
