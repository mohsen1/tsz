//! Core implementation of the type interning engine.
//!
//! This module contains all data structures and methods for the `TypeInterner`,
//! including sharded storage, concurrent slice/value interners, and type
//! construction convenience methods.

use crate::def::DefId;
use crate::intern::display_provenance::{DisplayProvenanceStore, ProvenanceLookup};
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IntrinsicKind, LiteralValue, MappedType, MappedTypeId, ObjectFlags,
    ObjectShape, ObjectShapeId, PropertyInfo, PropertyLookup, TemplateLiteralId, TemplateSpan,
    TupleElement, TupleListId, TypeApplication, TypeApplicationId, TypeData, TypeId, TypeListId,
    TypeParamInfo,
};
use crate::visitor::is_identity_comparable_type;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use smallvec::SmallVec;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc, OnceLock, RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use tsz_common::interner::{Atom, ShardedInterner};

// ---------------------------------------------------------------------------
// Thread-local direct-mapped lookup cache
// ---------------------------------------------------------------------------
// On single-threaded workloads (all benchmarks, CLI), every `lookup()` call
// goes through `RwLock::read()` which costs ~15-25 ns per call (atomic CAS on
// the reader count, memory fence, deref, fence, atomic decrement). A 1024-entry
// direct-mapped cache turns >90% of lookups into a single array index + compare
// (~1-2 ns). The cache is keyed by `TypeId.0` with the tag stored alongside
// the data, so collisions just evict (no correctness issue).

const LOOKUP_CACHE_BITS: u32 = 10;
const LOOKUP_CACHE_SIZE: usize = 1 << LOOKUP_CACHE_BITS; // 1024
#[allow(dead_code)]
const LOOKUP_CACHE_MASK: u32 = (LOOKUP_CACHE_SIZE as u32) - 1;

/// A single cache entry: (tag = TypeId raw value, cached TypeData, owning
/// interner `instance_id`).
///
/// `tag == 0` means empty (`TypeId::NONE` is never looked up for user types).
/// `instance_id` scopes the cache entry to the interner that inserted it, so
/// a stale entry from a previous `TypeInterner` on the same thread is
/// detected and treated as a miss — even though the raw `tag` may collide
/// with a different type in the new interner. Without this, the thread-local
/// cache was disabled entirely, forcing every `lookup()` through a
/// `RwLock::read()` (~15-25 ns per call).
#[derive(Clone, Copy)]
struct LookupCacheEntry {
    tag: u32,
    instance_id: u32,
    data: TypeData,
}

// LookupCacheEntry is used by TypeInternerCache below.

// ---------------------------------------------------------------------------
// Thread-local combined cache for both lookup and intern
// ---------------------------------------------------------------------------
// Combines both caches into a single struct to reduce thread_local! accesses.
// On macOS, each thread_local! access goes through __tls_get_addr (~10-15ns).
// By combining into one TLS access, we halve the overhead.

const INTERN_CACHE_BITS: u32 = 9;
const INTERN_CACHE_SIZE: usize = 1 << INTERN_CACHE_BITS; // 512
#[allow(dead_code)]
const INTERN_CACHE_MASK: u64 = (INTERN_CACHE_SIZE as u64) - 1;

#[derive(Clone, Copy)]
struct InternCacheEntry {
    /// `FxHash` of the TypeData, used as tag
    hash: u64,
    /// Owning interner `instance_id` for cross-interner safety.
    instance_id: u32,
    /// The TypeData that was interned
    key: TypeData,
    /// The resulting TypeId
    result: TypeId,
}

/// Combined thread-local cache for both `lookup()` and `intern()` directions.
///
/// Uses per-slot `Cell<T>` values for interior mutability. Both cache entry
/// types are `Copy`, so each probe/insert remains one direct slot `get`/`set`
/// with no `unsafe` and no manual `Send`/`Sync` impls. The cache is reached
/// only through `thread_local!`, which requires neither bound.
struct TypeInternerCache {
    lookup: [Cell<LookupCacheEntry>; LOOKUP_CACHE_SIZE],
    intern: [Cell<InternCacheEntry>; INTERN_CACHE_SIZE],
}

const EMPTY_LOOKUP_ENTRY: LookupCacheEntry = LookupCacheEntry {
    tag: 0,
    instance_id: 0,
    data: TypeData::Error,
};

const EMPTY_INTERN_ENTRY: InternCacheEntry = InternCacheEntry {
    hash: 0,
    instance_id: 0,
    key: TypeData::Error,
    result: TypeId::NONE,
};

#[allow(dead_code)]
impl TypeInternerCache {
    const fn new() -> Self {
        Self {
            lookup: [const { Cell::new(EMPTY_LOOKUP_ENTRY) }; LOOKUP_CACHE_SIZE],
            intern: [const { Cell::new(EMPTY_INTERN_ENTRY) }; INTERN_CACHE_SIZE],
        }
    }

    #[inline(always)]
    const fn lookup_probe(&self, id: TypeId, instance_id: u32) -> Option<TypeData> {
        let idx = (id.0 & LOOKUP_CACHE_MASK) as usize;
        let entry = self.lookup[idx].get();
        if entry.tag == id.0 && entry.instance_id == instance_id {
            Some(entry.data)
        } else {
            None
        }
    }

    #[inline(always)]
    fn lookup_insert(&self, id: TypeId, instance_id: u32, data: TypeData) {
        let idx = (id.0 & LOOKUP_CACHE_MASK) as usize;
        self.lookup[idx].set(LookupCacheEntry {
            tag: id.0,
            instance_id,
            data,
        });
    }

    #[inline(always)]
    fn intern_probe(&self, hash: u64, instance_id: u32, key: &TypeData) -> Option<TypeId> {
        let idx = (hash & INTERN_CACHE_MASK) as usize;
        let entry = self.intern[idx].get();
        if entry.hash == hash && entry.instance_id == instance_id && &entry.key == key {
            Some(entry.result)
        } else {
            None
        }
    }

    #[inline(always)]
    fn intern_insert(&self, hash: u64, instance_id: u32, key: TypeData, result: TypeId) {
        let idx = (hash & INTERN_CACHE_MASK) as usize;
        self.intern[idx].set(InternCacheEntry {
            hash,
            instance_id,
            key,
            result,
        });
    }
}

thread_local! {
    static TL_CACHE: TypeInternerCache = const { TypeInternerCache::new() };
}

/// Global counter for assigning unique `instance_id`s to `TypeInterner`
/// instances. `0` is reserved as "empty/no-interner" so it will never match
/// a real entry stored in the thread-local cache.
static NEXT_INTERNER_INSTANCE_ID: AtomicU32 = AtomicU32::new(1);

/// Clear the thread-local type interner cache.
///
/// This MUST be called between independent compilation sessions (e.g., in batch
/// mode) to prevent stale cached entries from a previous `TypeInterner` instance
/// from being returned for `TypeId` values that have been reused by a new interner.
/// Without this, the lookup cache may return `TypeData` from a dropped interner,
/// causing incorrect type resolution and panics.
pub fn clear_thread_local_cache() {
    TL_CACHE.with(|cache| {
        for cell in &cache.lookup {
            cell.set(EMPTY_LOOKUP_ENTRY);
        }
        for cell in &cache.intern {
            cell.set(EMPTY_INTERN_ENTRY);
        }
    });
}

pub(super) const SHARD_BITS: u32 = 6;
pub(super) const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
pub(super) const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
pub(crate) const PROPERTY_MAP_THRESHOLD: usize = 24;
pub(super) const TYPE_LIST_INLINE: usize = 8;

/// Maximum template literal expansion limit.
/// WASM environments have limited linear memory, so we use a much lower limit
/// to prevent OOM. Native CLI can handle more.
#[cfg(target_arch = "wasm32")]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum number of interned types before the interner returns `TypeId::ERROR`.
///
/// Native and WASM currently share the same 500k policy. The circuit breaker
/// was introduced with matching values on both cfg branches; there is no
/// separate native memory budget yet. Keep both constants visible so any future
/// target-specific change is reviewed explicitly.
///
/// Prevents OOM on pathological inputs (e.g., DOM types + module augmentation
/// that create millions of intermediate types via heritage merging and
/// function shape instantiation). With roughly 200-300 bytes per interned entry
/// (DashMap overhead, `Arc`, shapes), 500k types is roughly a 100-150MB
/// interner budget before fallback.
///
/// When the count is exceeded, new non-intrinsic interning poisons the interner
/// and returns `TypeId::ERROR`. Already-computed ids remain readable for later
/// diagnostics.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;

/// Maximum cumulative evaluation fuel across all `TypeEvaluator` instances.
///
/// Mirrors TypeScript's `instantiationCount` limit (5,000,000 in tsc). This
/// prevents deeply recursive type libraries from consuming unbounded memory
/// through type instantiation that creates new `TypeIds` on each expansion.
///
/// When exceeded, evaluators return `TypeId::ERROR`, matching TS2589.
/// Set lower than tsc's limit because our per-evaluation work is heavier
/// (we eagerly expand where tsc defers).
pub(crate) const MAX_EVALUATION_FUEL: u32 = 2_000_000;

pub(crate) type TypeListBuffer = SmallVec<[TypeId; TYPE_LIST_INLINE]>;
type ObjectPropertyIndex = DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher>;
type ObjectPropertyMap = OnceLock<ObjectPropertyIndex>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InternedTypeLimitContext {
    pub(crate) current_count: usize,
    pub(crate) max_interned_types: usize,
    pub(crate) fallback_type: TypeId,
}

/// Cached data for a union member, pre-fetched to avoid redundant DashMap/arena
/// lookups during sort comparisons. Each field corresponds to a lookup that
/// `compare_union_members` would otherwise perform per comparison.
pub(super) struct CachedUnionMember {
    /// The original TypeId
    pub(super) id: TypeId,
    /// Result of `builtin_sort_key(id)` - `Some` for intrinsic/builtin types
    pub(super) builtin_key: Option<u32>,
    /// Result of `self.lookup(id)` - the TypeData for non-builtin types
    pub(super) data: Option<TypeData>,
    /// For Object/ObjectWithIndex: the symbol's raw u32 (if the shape has a symbol)
    pub(super) obj_symbol: Option<u32>,
    /// For anonymous Object/ObjectWithIndex: the `ShapeId`'s raw u32
    pub(super) obj_anon_shape: Option<u32>,
    /// For Callable: the symbol's raw u32 (if the shape has a symbol)
    pub(super) callable_symbol: Option<u32>,
    /// Monotonic allocation counter for source-order sorting
    pub(super) alloc_order: Option<u32>,
}

/// Inner data for a `TypeShard`, lazily initialized.
pub(super) struct TypeShardInner {
    /// Map from `TypeData` to local index within this shard
    key_to_index: DashMap<TypeData, u32, FxBuildHasher>,
    /// Flat array from local index to `TypeData`.
    /// Sequential indices make a Vec far faster than `DashMap` for reverse lookup.
    /// Protected by `RwLock`: reads are uncontended in single-threaded use (~1 cycle),
    /// writes only happen during intern (append-only).
    index_to_key: RwLock<Vec<TypeData>>,
    /// Per-shard allocation order (parallel to `index_to_key`).
    /// Stores the global monotonic order counter at time of interning.
    alloc_order: RwLock<Vec<u32>>,
}

/// A single shard of the type interned storage.
///
/// Uses `OnceLock` for lazy initialization - `DashMaps` are only allocated
/// when the shard is first accessed, reducing startup overhead.
pub(super) struct TypeShard {
    /// Lazily initialized inner maps
    pub(super) inner: OnceLock<TypeShardInner>,
    /// Atomic counter for allocating new indices in this shard
    /// Kept outside `OnceLock` for fast checks without initialization
    pub(super) next_index: AtomicU32,
}

impl TypeShard {
    const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            next_index: AtomicU32::new(0),
        }
    }

    /// Get the inner maps, initializing on first access
    #[inline]
    fn get_inner(&self) -> &TypeShardInner {
        self.inner.get_or_init(|| TypeShardInner {
            key_to_index: DashMap::with_hasher(FxBuildHasher),
            index_to_key: RwLock::new(Vec::with_capacity(256)),
            alloc_order: RwLock::new(Vec::with_capacity(256)),
        })
    }

    /// Check if a key exists without initializing the shard
    #[inline]
    fn is_empty(&self) -> bool {
        self.next_index.load(Ordering::Relaxed) == 0
    }
}

/// Inner data for `ConcurrentSliceInterner`, lazily initialized.
pub(super) struct SliceInternerInner<T> {
    /// Flat array from ID to slice value. Sequential IDs make Vec optimal for reverse lookup.
    items: RwLock<Vec<Arc<[T]>>>,
    map: DashMap<Arc<[T]>, u32, FxBuildHasher>,
}

/// Slice interner using flat Vec for reverse lookup.
/// Uses lazy initialization to defer allocation until first use.
pub(super) struct ConcurrentSliceInterner<T> {
    pub(super) inner: OnceLock<SliceInternerInner<T>>,
    pub(super) next_id: AtomicU32,
}

impl<T> ConcurrentSliceInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            next_id: AtomicU32::new(1), // Reserve 0 for empty
        }
    }

    #[inline]
    fn get_inner(&self) -> &SliceInternerInner<T> {
        self.inner.get_or_init(|| {
            let empty: Arc<[T]> = Arc::from(Vec::new());
            let mut items_vec = Vec::with_capacity(256);
            items_vec.push(Arc::clone(&empty)); // id 0 = empty
            let map = DashMap::with_hasher(FxBuildHasher);
            map.insert(empty, 0);
            SliceInternerInner {
                items: RwLock::new(items_vec),
                map,
            }
        })
    }

    #[inline]
    fn intern(&self, items_slice: &[T]) -> u32 {
        if items_slice.is_empty() {
            return 0;
        }

        let inner = self.get_inner();

        // PERF: Try lookup with borrowed slice first to avoid Vec+Arc allocation on cache hits.
        // Arc<[T]>: Borrow<[T]> enables DashMap lookup with &[T] key.
        if let Some(ref_entry) = inner.map.get(items_slice) {
            return *ref_entry.value();
        }

        // Cache miss -- allocate for insertion
        let temp_arc: Arc<[T]> = Arc::from(items_slice.to_vec());

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(std::sync::Arc::clone(&temp_arc)) {
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(id);
                {
                    // T2.4 instrumentation: wrap the write-lock acquisition
                    // so contention on the slice-interner's `items` vec lands
                    // in the lock-wait histogram alongside the per-shard
                    // TypeData writes. With `perf-counters-timing` OFF this
                    // wrapper compiles to a direct closure call.
                    let mut vec = tsz_common::perf_counters::time_shard_write(0, || {
                        inner.items.write().expect("interner items lock poisoned")
                    });
                    while vec.len() < id as usize {
                        vec.push(Arc::clone(&temp_arc));
                    }
                    vec.push(temp_arc);
                }
                id
            }
            dashmap::mapref::entry::Entry::Occupied(e) => *e.get(),
        }
    }

    #[inline]
    fn get(&self, id: u32) -> Option<Arc<[T]>> {
        // For id 0, return from the initialized inner (which has the pre-allocated
        // empty Arc) instead of creating a new Arc::from(Vec::new()) on every call.
        let inner = if id == 0 {
            self.get_inner()
        } else {
            self.inner.get()?
        };
        let vec = inner.items.read().ok()?;
        vec.get(id as usize).cloned()
    }

    #[inline]
    fn empty(&self) -> Arc<[T]> {
        let inner = self.get_inner();
        let vec = inner.items.read().expect("interner items lock poisoned");
        vec.first()
            .cloned()
            .unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// Inner data for `ConcurrentValueInterner`, lazily initialized.
pub(super) struct ValueInternerInner<T> {
    /// Flat array from ID to value. Sequential IDs make Vec optimal for reverse lookup.
    items: RwLock<Vec<Arc<T>>>,
    map: DashMap<Arc<T>, u32, FxBuildHasher>,
}

/// Value interner using flat Vec for reverse lookup.
/// Uses lazy initialization to defer allocation until first use.
pub(super) struct ConcurrentValueInterner<T> {
    pub(super) inner: OnceLock<ValueInternerInner<T>>,
    pub(super) next_id: AtomicU32,
}

impl<T> ConcurrentValueInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            next_id: AtomicU32::new(0),
        }
    }

    #[inline]
    fn get_inner(&self) -> &ValueInternerInner<T> {
        self.inner.get_or_init(|| ValueInternerInner {
            items: RwLock::new(Vec::with_capacity(128)),
            map: DashMap::with_hasher(FxBuildHasher),
        })
    }

    #[inline]
    fn intern(&self, value: T) -> u32 {
        let inner = self.get_inner();

        // PERF: Try lookup with borrowed value first to avoid Arc allocation on cache hits.
        // Most intern calls are for already-interned values, so this saves an Arc::new()
        // (heap allocation + atomic ref count) on the hot path.
        if let Some(ref_entry) = inner.map.get(&value) {
            return *ref_entry.value();
        }

        // Cache miss -- allocate Arc for insertion
        let value_arc = Arc::new(value);

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(std::sync::Arc::clone(&value_arc)) {
            Entry::Vacant(e) => {
                e.insert(id);
                {
                    // T2.4 instrumentation: see the matching wrapper in
                    // `ConcurrentSliceInterner::intern`. Same rationale,
                    // same zero-cost-when-feature-off contract.
                    let mut vec = tsz_common::perf_counters::time_shard_write(0, || {
                        inner.items.write().expect("interner items lock poisoned")
                    });
                    while vec.len() < id as usize {
                        vec.push(Arc::clone(&value_arc));
                    }
                    vec.push(value_arc);
                }
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    #[inline]
    fn get(&self, id: u32) -> Option<Arc<T>> {
        let vec = self.inner.get()?.items.read().ok()?;
        vec.get(id as usize).cloned()
    }

    /// Get value by copy for Copy types, avoiding Arc clone overhead.
    #[inline]
    fn get_copy(&self, id: u32) -> Option<T>
    where
        T: Copy,
    {
        let vec = self.inner.get()?.items.read().ok()?;
        vec.get(id as usize).map(|arc| **arc)
    }
}

/// Type interning table with lock-free concurrent access.
///
/// Uses sharded `DashMap` structures for all internal storage, enabling
/// true parallel type checking without lock contention.
///
/// All internal structures use lazy initialization via `OnceLock` to minimize
/// startup overhead - `DashMaps` are only allocated when first accessed.
pub struct TypeInterner {
    /// Sharded storage for user-defined types (lazily initialized)
    pub(super) shards: Vec<TypeShard>,
    /// String interner for property names and string literals (already lock-free)
    pub string_interner: ShardedInterner,
    /// Concurrent interners for type components (lazily initialized)
    pub(super) type_lists: ConcurrentSliceInterner<TypeId>,
    pub(super) tuple_lists: ConcurrentSliceInterner<TupleElement>,
    pub(super) template_lists: ConcurrentSliceInterner<TemplateSpan>,
    pub(super) object_shapes: ConcurrentValueInterner<ObjectShape>,
    /// Object property maps: lazily initialized `DashMap`
    pub(super) object_property_maps: ObjectPropertyMap,
    pub(super) function_shapes: ConcurrentValueInterner<FunctionShape>,
    pub(super) callable_shapes: ConcurrentValueInterner<CallableShape>,
    pub(super) conditional_types: ConcurrentValueInterner<ConditionalType>,
    pub(super) mapped_types: ConcurrentValueInterner<MappedType>,
    pub(super) applications: ConcurrentValueInterner<TypeApplication>,
    /// Cache for `is_identity_comparable_type` checks (memoized O(1) lookup after first computation)
    pub(super) identity_comparable_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// Cache for `contains_this_type` checks. Result is stable per TypeId
    /// within a single interner, so memoizing project-wide eliminates the
    /// repeated recursive walk that showed up at ~5% of total CPU on
    /// multi-file workloads.
    pub(crate) contains_this_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// Cache for `contains_infer_types_db` checks. Evaluation/cache filtering
    /// and conditional subtype paths ask this repeatedly for the same
    /// conditional/application shapes.
    pub(crate) contains_infer_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// Cache for `contains_type_query_db` checks. Results are immutable per
    /// `TypeId` and shared across evaluator instances.
    pub(crate) contains_type_query_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// The global Array base type (e.g., Array<T> from lib.d.ts).
    /// Uses `AtomicU32` (with `u32::MAX` as sentinel for `None`) instead of
    /// `RwLock` so file checkers can overwrite the prime checker's value without
    /// lock contention on this frequently-read field.
    pub(super) array_base_type: AtomicU32,
    /// Type parameters for the Array base type.
    /// Kept as `OnceLock` since params don't contain `DefIds` and are stable
    /// across checkers (the interner allocates `TypeParam` `TypeIds` centrally).
    pub(super) array_base_type_params: OnceLock<Vec<TypeParamInfo>>,
    /// The global ReadonlyArray base type (e.g., `ReadonlyArray<T>` from lib.d.ts).
    /// Used by property access resolution to correctly reject mutating methods
    /// (`push`, `pop`, etc.) on `readonly T[]` types.
    pub(super) readonly_array_base_type: AtomicU32,
    /// Boxed interface types for primitives (e.g., String interface for `string`).
    /// Registered from lib.d.ts during primordial type setup.
    pub(super) boxed_types: DashMap<IntrinsicKind, TypeId, FxBuildHasher>,
    /// `DefIds` known to be boxed types (e.g., the DefId for the Function interface).
    /// Registered alongside `boxed_types` so subtype checking can identify boxed
    /// types even when `TypeEnvironment` is unavailable.
    pub(super) boxed_def_ids: DashMap<IntrinsicKind, Vec<DefId>, FxBuildHasher>,
    /// `DefIds` known to be the `ThisType` marker interface from lib.d.ts.
    /// Used by `ThisTypeMarkerExtractor` to identify `ThisType<T>` applications
    /// when the base type is `Lazy(DefId)`.
    pub(super) this_type_marker_def_ids: DashMap<DefId, (), FxBuildHasher>,
    /// Global allocation counter for deterministic type ordering.
    /// The sharded interner embeds shard index in TypeId low bits, so raw TypeId
    /// comparison is hash-dependent. This counter provides allocation-order
    /// comparison that approximates tsc's source-order type ID allocation.
    pub(super) alloc_counter: AtomicU32,
    /// Circuit breaker: once set, all intern/lookup calls return early.
    pub(super) poisoned: std::sync::atomic::AtomicBool,
    /// Effective value for `noUncheckedIndexedAccess` used by query-boundary helpers.
    pub(super) no_unchecked_indexed_access: AtomicBool,
    /// Effective value for `exactOptionalPropertyTypes` used by query-boundary helpers.
    pub(super) exact_optional_property_types: AtomicBool,
    /// Dedicated store for diagnostic display provenance records and priority rules.
    ///
    /// Owns alias application mappings, fresh object literal property display,
    /// union origin lists, conditional alias base markers, and the union-too-complex
    /// flag. Methods on this store encode the display policy so the interner
    /// stays focused on canonical identity and deduplication.
    pub(crate) display_provenance: DisplayProvenanceStore,
    /// Global evaluation fuel counter.
    ///
    /// Tracks cumulative evaluation work across ALL `TypeEvaluator` instances.
    /// Mirrors TypeScript's `instantiationCount` which limits total type instantiation
    /// work across the entire program check. Prevents deeply recursive type libraries
    /// (like ts-toolbelt) from consuming unbounded memory through repeated type
    /// instantiation that creates new `TypeIds` on each expansion.
    ///
    /// When this counter exceeds `MAX_EVALUATION_FUEL`, evaluators bail out early
    /// with `TypeId::ERROR`, matching tsc's TS2589 behavior.
    pub(super) evaluation_fuel: AtomicU32,
    /// Unique identifier scoping this interner's entries in the thread-local
    /// lookup/intern cache. See `NEXT_INTERNER_INSTANCE_ID` for context.
    pub(super) instance_id: u32,
}

impl std::fmt::Debug for TypeInterner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeInterner")
            .field("shards", &self.shards.len())
            .finish_non_exhaustive()
    }
}

impl TypeInterner {
    /// Create a new type interner with pre-registered intrinsics.
    ///
    /// Uses lazy initialization for all `DashMap` structures to minimize
    /// startup overhead. `DashMaps` are only allocated when first accessed.
    pub fn new() -> Self {
        let shards: Vec<TypeShard> = (0..SHARD_COUNT).map(|_| TypeShard::new()).collect();

        Self {
            shards,
            // String interner - common strings are interned on-demand for faster startup
            string_interner: ShardedInterner::new(),
            type_lists: ConcurrentSliceInterner::new(),
            tuple_lists: ConcurrentSliceInterner::new(),
            template_lists: ConcurrentSliceInterner::new(),
            object_shapes: ConcurrentValueInterner::new(),
            object_property_maps: OnceLock::new(),
            function_shapes: ConcurrentValueInterner::new(),
            callable_shapes: ConcurrentValueInterner::new(),
            conditional_types: ConcurrentValueInterner::new(),
            mapped_types: ConcurrentValueInterner::new(),
            applications: ConcurrentValueInterner::new(),
            identity_comparable_cache: DashMap::with_hasher(FxBuildHasher),
            contains_this_cache: DashMap::with_hasher(FxBuildHasher),
            contains_infer_cache: DashMap::with_hasher(FxBuildHasher),
            contains_type_query_cache: DashMap::with_hasher(FxBuildHasher),
            array_base_type: AtomicU32::new(u32::MAX),
            array_base_type_params: OnceLock::new(),
            readonly_array_base_type: AtomicU32::new(u32::MAX),
            boxed_types: DashMap::with_hasher(FxBuildHasher),
            boxed_def_ids: DashMap::with_hasher(FxBuildHasher),
            this_type_marker_def_ids: DashMap::with_hasher(FxBuildHasher),
            alloc_counter: AtomicU32::new(0),
            poisoned: std::sync::atomic::AtomicBool::new(false),
            no_unchecked_indexed_access: AtomicBool::new(false),
            exact_optional_property_types: AtomicBool::new(false),
            display_provenance: DisplayProvenanceStore::default(),
            evaluation_fuel: AtomicU32::new(0),
            instance_id: NEXT_INTERNER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    #[inline]
    pub fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access
            .store(enabled, Ordering::Relaxed);
    }

    #[inline]
    pub fn exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_exact_optional_property_types(&self, enabled: bool) {
        self.exact_optional_property_types
            .store(enabled, Ordering::Relaxed);
    }

    /// Atomically read and clear the "union too complex" flag.
    ///
    /// Returns `true` if a union construction was aborted due to complexity
    /// since the last call to this method. The flag is cleared after reading.
    /// The checker uses this to emit TS2590.
    #[inline]
    pub fn take_union_too_complex(&self) -> bool {
        self.display_provenance.take_union_too_complex()
    }

    /// Mark that a union construction was aborted due to complexity.
    /// Called from `reduce_union_subtypes` when pairwise comparisons would exceed 1M.
    #[inline]
    pub(crate) fn set_union_too_complex(&self) {
        self.display_provenance.set_union_too_complex();
    }

    /// Set the global Array base type (e.g., Array<T> from lib.d.ts).
    ///
    /// The `TypeId` uses `AtomicU32` so each file checker can overwrite the prime
    /// checker's value with one containing correct `DefIds` for its own
    /// `DefinitionStore`. The params use `OnceLock` since they don't contain
    /// `DefIds` and are stable across checkers.
    pub fn set_array_base_type(&self, type_id: TypeId, params: Vec<TypeParamInfo>) {
        self.array_base_type.store(type_id.0, Ordering::Relaxed);
        let _ = self.array_base_type_params.set(params);
    }

    /// Set the global `ReadonlyArray<T>` base type from lib.d.ts.
    pub fn set_readonly_array_base_type(&self, type_id: TypeId) {
        self.readonly_array_base_type
            .store(type_id.0, Ordering::Relaxed);
    }

    /// Get the global `ReadonlyArray<T>` base type, if it has been set.
    #[inline]
    pub fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        let raw = self.readonly_array_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Set the Array base type used for display-order-sensitive queries.
    pub fn set_array_display_base_type(&self, type_id: TypeId) {
        self.display_provenance.set_array_display_base_type(type_id);
    }

    /// Get the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type(&self) -> Option<TypeId> {
        let raw = self.array_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Get the Array base type used for display-order-sensitive queries.
    #[inline]
    pub fn get_array_display_base_type(&self) -> Option<TypeId> {
        self.display_provenance.get_array_display_base_type()
    }

    /// Get the type parameters for the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.array_base_type_params
            .get()
            .map_or(&[], |v| v.as_slice())
    }

    /// Set a boxed interface type for a primitive intrinsic kind.
    ///
    /// Called during primordial type setup when lib.d.ts is processed.
    /// For example, `set_boxed_type(IntrinsicKind::String, type_id_of_String_interface)`
    /// enables property access on `string` values to resolve through the String interface.
    pub fn set_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
    }

    /// Get the boxed interface type for a primitive intrinsic kind.
    #[inline]
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).map(|r| *r)
    }

    /// Register a DefId as belonging to a boxed type.
    pub fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        self.boxed_def_ids.entry(kind).or_default().push(def_id);
    }

    /// Check if a DefId corresponds to a boxed type of the given kind.
    pub fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        self.boxed_def_ids
            .get(&kind)
            .is_some_and(|ids| ids.contains(&def_id))
    }

    /// Register a DefId as belonging to the `ThisType` marker interface.
    pub fn register_this_type_def_id(&self, def_id: DefId) {
        self.this_type_marker_def_ids.insert(def_id, ());
    }

    /// Check if a DefId corresponds to the `ThisType` marker interface.
    pub fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        self.this_type_marker_def_ids.contains_key(&def_id)
    }

    /// Get the object property maps, initializing on first access
    #[inline]
    fn get_object_property_maps(&self) -> &ObjectPropertyIndex {
        self.object_property_maps
            .get_or_init(|| DashMap::with_hasher(FxBuildHasher))
    }

    /// Check if a type can be compared by `TypeId` identity alone (O(1) equality).
    /// Results are cached for O(1) lookup after first computation.
    /// This is used for optimization in BCT and subtype checking.
    #[inline]
    pub fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        // Fast path: check cache first
        if let Some(cached) = self.identity_comparable_cache.get(&type_id) {
            return *cached;
        }
        // Compute and cache
        let result = is_identity_comparable_type(self, type_id);
        self.identity_comparable_cache.insert(type_id, result);
        result
    }

    /// Intern a string into an Atom.
    /// This is used when constructing types with property names or string literals.
    #[inline]
    pub fn intern_string(&self, s: &str) -> Atom {
        tsz_common::perf_counters::record_interner_string_intern_call();
        self.string_interner.intern(s)
    }

    /// Resolve an Atom back to its string value.
    /// This is used when formatting types for error messages.
    pub fn resolve_atom(&self, atom: Atom) -> String {
        self.string_interner.resolve(atom).to_string()
    }

    /// Resolve an Atom without allocating a new String.
    pub fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.string_interner.resolve(atom)
    }

    #[inline]
    pub fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.type_lists
            .get(id.0)
            .unwrap_or_else(|| self.type_lists.empty())
    }

    #[inline]
    pub fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.tuple_lists
            .get(id.0)
            .unwrap_or_else(|| self.tuple_lists.empty())
    }

    #[inline]
    pub fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.template_lists
            .get(id.0)
            .unwrap_or_else(|| self.template_lists.empty())
    }

    #[inline]
    pub fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.object_shapes.get(id.0).unwrap_or_else(|| {
            // Use a cached static empty shape to avoid heap allocation on every miss.
            static EMPTY_SHAPE: OnceLock<Arc<ObjectShape>> = OnceLock::new();
            Arc::clone(EMPTY_SHAPE.get_or_init(|| {
                Arc::new(ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                })
            }))
        })
    }

    pub fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        let shape = self.object_shape(shape_id);
        if shape.properties.len() < PROPERTY_MAP_THRESHOLD {
            return PropertyLookup::Uncached;
        }

        match self.object_property_map(shape_id, &shape) {
            Some(map) => match map.get(&name) {
                Some(&idx) => PropertyLookup::Found(idx),
                None => PropertyLookup::NotFound,
            },
            None => PropertyLookup::Uncached,
        }
    }

    /// Get or create a property map for an object shape.
    ///
    /// This uses a lock-free pattern with `DashMap` to avoid the read-then-write
    /// deadlock that existed in the previous `RwLock`<Vec> implementation.
    fn object_property_map(
        &self,
        shape_id: ObjectShapeId,
        shape: &ObjectShape,
    ) -> Option<Arc<FxHashMap<Atom, usize>>> {
        if shape.properties.len() < PROPERTY_MAP_THRESHOLD {
            return None;
        }

        let maps = self.get_object_property_maps();

        // Try to get existing map (lock-free read)
        if let Some(map) = maps.get(&shape_id) {
            return Some(std::sync::Arc::clone(&map));
        }

        // Build the property map
        let mut map = FxHashMap::default();
        for (idx, prop) in shape.properties.iter().enumerate() {
            map.insert(prop.name, idx);
        }
        let map = Arc::new(map);

        // Try to insert - if another thread inserted first, use theirs
        match maps.entry(shape_id) {
            Entry::Vacant(e) => {
                e.insert(std::sync::Arc::clone(&map));
                Some(map)
            }
            Entry::Occupied(e) => Some(std::sync::Arc::clone(e.get())),
        }
    }

    #[inline]
    pub fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.function_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(FunctionShape {
                type_params: Vec::new(),
                params: Vec::new(),
                this_type: None,
                return_type: TypeId::ERROR,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            })
        })
    }

    #[inline]
    pub fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.callable_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(CallableShape {
                call_signatures: Vec::new(),
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                ..Default::default()
            })
        })
    }

    /// Get a conditional type by value (no Arc clone overhead).
    /// Preferred over `conditional_type()` since `ConditionalType` is Copy.
    #[inline]
    pub fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        self.conditional_types
            .get_copy(id.0)
            .unwrap_or(ConditionalType {
                check_type: TypeId::ERROR,
                extends_type: TypeId::ERROR,
                true_type: TypeId::ERROR,
                false_type: TypeId::ERROR,
                is_distributive: false,
            })
    }

    /// Get a mapped type by value (no Arc clone overhead).
    /// Preferred over `mapped_type()` since `MappedType` is Copy.
    #[inline]
    pub fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        self.mapped_types.get_copy(id.0).unwrap_or(MappedType {
            type_param: TypeParamInfo {
                is_const: false,
                name: self.intern_string("_"),
                constraint: None,
                default: None,
            },
            constraint: TypeId::ERROR,
            name_type: None,
            template: TypeId::ERROR,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    #[inline]
    pub fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.conditional_types.get(id.0).unwrap_or_else(|| {
            Arc::new(ConditionalType {
                check_type: TypeId::ERROR,
                extends_type: TypeId::ERROR,
                true_type: TypeId::ERROR,
                false_type: TypeId::ERROR,
                is_distributive: false,
            })
        })
    }

    #[inline]
    pub fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.mapped_types.get(id.0).unwrap_or_else(|| {
            Arc::new(MappedType {
                type_param: TypeParamInfo {
                    is_const: false,
                    name: self.intern_string("_"),
                    constraint: None,
                    default: None,
                },
                constraint: TypeId::ERROR,
                name_type: None,
                template: TypeId::ERROR,
                readonly_modifier: None,
                optional_modifier: None,
            })
        })
    }

    #[inline]
    pub fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.applications.get(id.0).unwrap_or_else(|| {
            Arc::new(TypeApplication {
                base: TypeId::ERROR,
                args: Vec::new(),
            })
        })
    }

    /// Intern a type key and return its `TypeId`.
    /// If the key already exists, returns the existing `TypeId`.
    /// Otherwise, creates a new `TypeId` and stores the key.
    ///
    /// This uses a lock-free pattern with `DashMap` for concurrent access.
    ///
    /// Consults a thread-local cache scoped by this interner's `instance_id`
    /// before falling through to the `DashMap` lookup.
    #[inline]
    pub fn intern(&self, key: TypeData) -> TypeId {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return TypeId::ERROR;
        }
        // T2.4 instrumentation. Semantics:
        //   intern_calls   = number of non-poisoned `intern()` entries
        //   intern_hits    = returned an existing `TypeId` (intrinsic, TL
        //                    hit, shard read hit, or race-loss occupied
        //                    insert)
        //   intern_misses  = stored a new `TypeData` (vacant insert)
        // Invariant:
        //   intern_calls = intern_hits + intern_misses + slow_path_errors
        // where `slow_path_errors` is the count of calls that hit the
        // `intern_slow` circuit breakers (max-types, u32-overflow). It is
        // observable as the residual `intern_calls - intern_hits -
        // intern_misses` and is not separately bucketed today.
        //
        // We gate once with `enabled_fast()` (one `OnceLock<bool>` read)
        // and cache the resulting `&'static PerfCounters` pointer in `pc`.
        // An enabled run pays the gate read plus one `counters()`
        // `OnceLock<PerfCounters>` deref per `intern()` call (vs. one per
        // increment). A disabled run pays only the gate read: subsequent
        // `if let Some(c) = pc` checks are predictable branches on a
        // local `None`, so the increment body is consistently skipped.
        let pc = if tsz_common::perf_counters::enabled_fast() {
            Some(tsz_common::perf_counters::counters())
        } else {
            None
        };
        if let Some(c) = pc {
            c.interner_intern_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(id) = self.get_intrinsic_id(&key) {
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return id;
        }

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        // Fast path: thread-local cache hit scoped by this interner's
        // instance_id.
        if let Some(id) = TL_CACHE.with(|c| c.intern_probe(hash, self.instance_id, &key)) {
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return id;
        }

        let result = self.intern_slow(key, hash, pc);
        if result != TypeId::ERROR {
            TL_CACHE.with(|c| c.intern_insert(hash, self.instance_id, key, result));
        }
        result
    }

    /// Allocate a fresh `TypeId` for declaration-scoped types that carry
    /// identity beyond their structural payload.
    ///
    /// The stored `TypeData` is still available through `lookup`, but this
    /// intentionally bypasses `key_to_index` and the thread-local intern cache
    /// so two declarations with the same surface name and constraint do not
    /// collapse to one semantic type parameter.
    pub(crate) fn intern_fresh(&self, key: TypeData) -> TypeId {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return TypeId::ERROR;
        }
        if self.interned_type_limit_exceeded() {
            return self.poison_due_to_interned_type_limit();
        }

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let shard_idx = (hash as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];
        let inner = shard.get_inner();

        let local_index = shard.next_index.fetch_add(1, Ordering::Relaxed);
        if local_index > (u32::MAX >> SHARD_BITS) {
            return TypeId::ERROR;
        }

        let order = self.alloc_counter.fetch_add(1, Ordering::Relaxed);
        {
            let mut vec = tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                inner
                    .index_to_key
                    .write()
                    .expect("interner index_to_key lock poisoned")
            });
            let mut ord = tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                inner
                    .alloc_order
                    .write()
                    .expect("interner alloc_order lock poisoned")
            });
            let target_len = local_index as usize + 1;
            if vec.len() < target_len {
                vec.resize(target_len, TypeData::Error);
                ord.resize(target_len, u32::MAX);
            }
            vec[local_index as usize] = key;
            ord[local_index as usize] = order;
        }

        self.make_id(local_index, shard_idx as u32)
    }

    /// Slow path for `intern`: goes through `DashMap` and RwLock-protected storage.
    ///
    /// `pc` is the cached counter pointer from the public `intern()` entry,
    /// `Some` only when `enabled_fast()` was true at the call site. Threading
    /// it through avoids re-deref'ing the `OnceLock` and re-checking the gate
    /// in this slow path, and lets the caller make the lifetime of the cache
    /// pointer explicit.
    #[inline(never)]
    fn intern_slow(
        &self,
        key: TypeData,
        hash: u64,
        pc: Option<&'static tsz_common::perf_counters::PerfCounters>,
    ) -> TypeId {
        // Circuit breaker 1: type count limit. Returning `TypeId::ERROR` here
        // intentionally does not credit a hit or miss — the residual
        // `calls - hits - misses` exposes circuit-breaker activations.
        if self.interned_type_limit_exceeded() {
            return self.poison_due_to_interned_type_limit();
        }

        let shard_idx = (hash as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];
        let inner = shard.get_inner();

        // Try to get existing ID (lock-free read)
        if let Some(entry) = inner.key_to_index.get(&key) {
            let local_index = *entry.value();
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return self.make_id(local_index, shard_idx as u32);
        }

        // Allocate new index
        let local_index = shard.next_index.fetch_add(1, Ordering::Relaxed);
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Circuit breaker 2: u32 overflow. Same rationale as #1: not
            // credited as hit or miss; observable via the residual.
            return TypeId::ERROR;
        }

        // Double-check: another thread might have inserted while we allocated
        match inner.key_to_index.entry(key) {
            Entry::Vacant(e) => {
                e.insert(local_index);
                // Record allocation order for deterministic union member sorting.
                let order = self.alloc_counter.fetch_add(1, Ordering::Relaxed);
                {
                    // T2.4 instrumentation: time the shard's write-lock
                    // acquisitions. With `perf-counters-timing` ON, each
                    // observation lands in the lock-wait histogram. With it
                    // OFF (default) the wrapper compiles to a direct call —
                    // no `Instant::now()`, no atomic touch.
                    let mut vec =
                        tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                            inner
                                .index_to_key
                                .write()
                                .expect("interner index_to_key lock poisoned")
                        });
                    let mut ord =
                        tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                            inner
                                .alloc_order
                                .write()
                                .expect("interner alloc_order lock poisoned")
                        });
                    let target_len = local_index as usize + 1;
                    if vec.len() < target_len {
                        vec.resize(target_len, TypeData::Error);
                        ord.resize(target_len, u32::MAX);
                    }
                    vec[local_index as usize] = key;
                    ord[local_index as usize] = order;
                }
                if let Some(c) = pc {
                    c.interner_intern_misses
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.make_id(local_index, shard_idx as u32)
            }
            Entry::Occupied(e) => {
                // Another thread inserted first, use their ID. We bumped
                // `next_index` above and won't recycle it, so this is a hit
                // from the caller's POV (no new TypeData was stored).
                let existing_index = *e.get();
                if let Some(c) = pc {
                    c.interner_intern_hits
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.make_id(existing_index, shard_idx as u32)
            }
        }
    }

    /// Look up the `TypeData` for a given `TypeId`.
    ///
    /// Uses a thread-local direct-mapped cache for O(1) lookups on cache hits,
    /// falling back to `RwLock`-protected shard storage on misses. Cache
    /// entries are scoped by `self.instance_id` so a stale entry from a
    /// previous `TypeInterner` on the same thread (conformance runner, batch
    /// mode) is detected and treated as a miss.
    #[inline]
    pub fn lookup(&self, id: TypeId) -> Option<TypeData> {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        if id.is_intrinsic() || id.is_error() {
            return self.get_intrinsic_key(id);
        }

        // Fast path: thread-local cache hit scoped by this interner's
        // instance_id.
        if let Some(data) = TL_CACHE.with(|c| c.lookup_probe(id, self.instance_id)) {
            return Some(data);
        }

        let data = self.lookup_slow(id)?;
        TL_CACHE.with(|c| c.lookup_insert(id, self.instance_id, data));
        Some(data)
    }

    /// Slow path for `lookup`: goes through RwLock-protected shard storage.
    #[inline(never)]
    fn lookup_slow(&self, id: TypeId) -> Option<TypeData> {
        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;

        let shard = self.shards.get(shard_idx)?;
        // If shard is empty, no types have been interned there yet
        if shard.is_empty() {
            return None;
        }
        // Use inner.get() instead of get_or_init() -- if shard is non-empty,
        // inner is guaranteed initialized (intern sets it before incrementing counter).
        let inner = shard.inner.get()?;
        let vec = inner.index_to_key.read().ok()?;
        vec.get(local_index as usize).copied()
    }

    /// Look up the allocation order for a given `TypeId`.
    /// Returns `None` for intrinsic/error types (they have no alloc order).
    #[inline]
    pub(crate) fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        if id.is_intrinsic() || id.is_error() {
            return None;
        }
        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;
        let shard = self.shards.get(shard_idx)?;
        if shard.is_empty() {
            return None;
        }
        let inner = shard.inner.get()?;
        let ord = inner.alloc_order.read().ok()?;
        let val = ord.get(local_index as usize).copied()?;
        if val == u32::MAX { None } else { Some(val) }
    }

    pub(in crate::intern) fn intern_type_list(&self, members: Vec<TypeId>) -> TypeListId {
        tsz_common::perf_counters::record_interner_type_list_intern_call();
        TypeListId(self.type_lists.intern(&members))
    }

    /// Intern a type list from a slice, avoiding Vec conversion when the caller
    /// already has a `SmallVec` or slice reference.
    pub(in crate::intern) fn intern_type_list_from_slice(&self, members: &[TypeId]) -> TypeListId {
        tsz_common::perf_counters::record_interner_type_list_intern_call();
        TypeListId(self.type_lists.intern(members))
    }

    pub(super) fn intern_tuple_list(&self, elements: Vec<TupleElement>) -> TupleListId {
        TupleListId(self.tuple_lists.intern(&elements))
    }

    pub(crate) fn intern_template_list(&self, spans: Vec<TemplateSpan>) -> TemplateLiteralId {
        TemplateLiteralId(self.template_lists.intern(&spans))
    }

    pub fn intern_object_shape(&self, shape: ObjectShape) -> ObjectShapeId {
        tsz_common::perf_counters::record_interner_object_shape_intern_call();
        ObjectShapeId(self.object_shapes.intern(shape))
    }

    /// Store pre-widened property types for a fresh object literal type.
    pub fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.display_provenance
            .record_fresh_object_properties(type_id, props);
    }

    /// Retrieve pre-widened property types for a fresh object literal type.
    pub fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.display_provenance.get_fresh_object_properties(type_id)
    }

    /// Record that `evaluated` was produced by evaluating `application`.
    pub fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        self.display_provenance
            .record_alias_application(self, evaluated, application);
    }

    /// Prefer a concrete `Application` display alias over structural provenance
    /// recorded while evaluating the alias body.
    pub fn store_display_alias_preferring_application(
        &self,
        evaluated: TypeId,
        application: TypeId,
    ) {
        self.display_provenance
            .record_alias_application_preferring_application(self, evaluated, application);
    }

    /// Look up the alias application recorded for `type_id`.
    pub fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        self.display_provenance.get_alias(type_id)
    }

    /// Mark an application base whose type-alias body is a conditional type.
    pub fn mark_conditional_alias_base(&self, base: TypeId) {
        self.display_provenance.mark_conditional_alias_base(base);
    }

    pub fn is_conditional_alias_base(&self, base: TypeId) -> bool {
        self.display_provenance.is_conditional_alias_base(base)
    }

    /// Record the as-written origin members for a flattened Union `TypeId`.
    pub fn store_union_origin(&self, union_type_id: TypeId, origin_members: Vec<TypeId>) {
        self.display_provenance
            .record_union_origin(self, union_type_id, origin_members);
    }

    /// Replace the display origin with a more specific tsc-compatible member order.
    pub fn replace_union_origin_for_display(
        &self,
        union_type_id: TypeId,
        origin_members: Vec<TypeId>,
    ) {
        self.display_provenance
            .replace_union_origin(self, union_type_id, origin_members);
    }

    /// Look up the as-written origin members for a flattened Union `TypeId`.
    pub fn get_union_origin(&self, type_id: TypeId) -> Option<Arc<Vec<TypeId>>> {
        self.display_provenance.get_union_origin(type_id)
    }

    pub(super) fn intern_function_shape(&self, shape: FunctionShape) -> FunctionShapeId {
        tsz_common::perf_counters::record_interner_function_shape_intern_call();
        FunctionShapeId(self.function_shapes.intern(shape))
    }

    pub(in crate::intern) fn intern_callable_shape(&self, shape: CallableShape) -> CallableShapeId {
        tsz_common::perf_counters::record_interner_callable_shape_intern_call();
        CallableShapeId(self.callable_shapes.intern(shape))
    }

    pub(super) fn intern_conditional_type(
        &self,
        conditional: ConditionalType,
    ) -> ConditionalTypeId {
        tsz_common::perf_counters::record_interner_conditional_intern_call();
        ConditionalTypeId(self.conditional_types.intern(conditional))
    }

    pub(super) fn intern_mapped_type(&self, mapped: MappedType) -> MappedTypeId {
        tsz_common::perf_counters::record_interner_mapped_intern_call();
        MappedTypeId(self.mapped_types.intern(mapped))
    }

    pub(super) fn intern_application(&self, application: TypeApplication) -> TypeApplicationId {
        tsz_common::perf_counters::record_interner_application_intern_call();
        TypeApplicationId(self.applications.intern(application))
    }

    /// Get the number of interned types (lock-free read)
    pub fn len(&self) -> usize {
        let mut total = TypeId::FIRST_USER as usize;
        for shard in &self.shards {
            total += shard.next_index.load(Ordering::Relaxed) as usize;
        }
        total
    }

    /// Check if the interner is empty (only has intrinsics)
    pub fn is_empty(&self) -> bool {
        self.len() <= TypeId::FIRST_USER as usize
    }

    /// Get an approximate count of interned types.
    /// This is cheaper than `len()` as it samples only a few shards.
    /// Used for the circuit breaker to avoid OOM.
    /// Uses the global allocation counter for an exact count (single atomic load)
    /// instead of sampling shards and extrapolating.
    #[inline]
    fn approximate_count(&self) -> usize {
        self.alloc_counter.load(Ordering::Relaxed) as usize
    }

    #[inline]
    const fn interned_type_limit_exceeded_for_count(count: usize) -> bool {
        count > MAX_INTERNED_TYPES
    }

    #[inline]
    fn interned_type_limit_exceeded(&self) -> bool {
        Self::interned_type_limit_exceeded_for_count(self.approximate_count())
    }

    #[inline]
    fn interned_type_limit_context(&self) -> InternedTypeLimitContext {
        InternedTypeLimitContext {
            current_count: self.approximate_count(),
            max_interned_types: MAX_INTERNED_TYPES,
            fallback_type: TypeId::ERROR,
        }
    }

    #[inline]
    fn poison_due_to_interned_type_limit(&self) -> TypeId {
        let context = self.interned_type_limit_context();
        if self
            .poisoned
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            tracing::warn!(
                target: "tsz::solver::interner",
                interned_type_count = context.current_count,
                max_interned_types = context.max_interned_types,
                fallback_type_id = context.fallback_type.0,
                fallback_type = "TypeId::ERROR",
                "interned type limit exceeded; poisoning type interner"
            );
        }
        context.fallback_type
    }

    /// Consume evaluation fuel and return whether fuel is exhausted.
    ///
    /// This is a global budget across all `TypeEvaluator` instances. When exhausted,
    /// the current evaluation should bail out with ERROR, but the interner remains
    /// readable so already-computed project types do not turn into opaque `Type(N)`
    /// placeholders in later diagnostics.
    #[inline]
    pub fn consume_evaluation_fuel(&self, amount: u32) -> bool {
        let prev = self.evaluation_fuel.fetch_add(amount, Ordering::Relaxed);
        prev.wrapping_add(amount) > MAX_EVALUATION_FUEL
    }

    /// Check whether global evaluation fuel is exhausted without consuming any.
    #[inline]
    pub fn is_evaluation_fuel_exhausted(&self) -> bool {
        self.evaluation_fuel.load(Ordering::Relaxed) > MAX_EVALUATION_FUEL
    }

    #[inline]
    fn make_id(&self, local_index: u32, shard_idx: u32) -> TypeId {
        let raw_val = (local_index << SHARD_BITS) | (shard_idx & SHARD_MASK);
        let id = TypeId(TypeId::FIRST_USER + raw_val);

        // SAFETY: Assert that we're not overflowing into the local ID space (MSB=1).
        // Global TypeIds must have MSB=0 (0x7FFFFFFF-) to allow ScopedTypeInterner
        // to use the upper half (0x80000000+) for ephemeral types.
        debug_assert!(
            id.is_global(),
            "Global TypeId overflow: {id:?} - would conflict with local ID space"
        );

        id
    }

    const fn get_intrinsic_id(&self, key: &TypeData) -> Option<TypeId> {
        match key {
            TypeData::Intrinsic(kind) => Some(kind.to_type_id()),
            TypeData::Error => Some(TypeId::ERROR),
            // Map boolean literals to their intrinsic IDs to avoid duplicates
            TypeData::Literal(LiteralValue::Boolean(true)) => Some(TypeId::BOOLEAN_TRUE),
            TypeData::Literal(LiteralValue::Boolean(false)) => Some(TypeId::BOOLEAN_FALSE),
            _ => None,
        }
    }

    const fn get_intrinsic_key(&self, id: TypeId) -> Option<TypeData> {
        match id {
            TypeId::NONE | TypeId::ERROR => Some(TypeData::Error),
            TypeId::NEVER => Some(TypeData::Intrinsic(IntrinsicKind::Never)),
            TypeId::UNKNOWN => Some(TypeData::Intrinsic(IntrinsicKind::Unknown)),
            TypeId::ANY => Some(TypeData::Intrinsic(IntrinsicKind::Any)),
            TypeId::VOID => Some(TypeData::Intrinsic(IntrinsicKind::Void)),
            TypeId::UNDEFINED => Some(TypeData::Intrinsic(IntrinsicKind::Undefined)),
            TypeId::NULL => Some(TypeData::Intrinsic(IntrinsicKind::Null)),
            TypeId::BOOLEAN => Some(TypeData::Intrinsic(IntrinsicKind::Boolean)),
            TypeId::NUMBER => Some(TypeData::Intrinsic(IntrinsicKind::Number)),
            TypeId::STRING => Some(TypeData::Intrinsic(IntrinsicKind::String)),
            TypeId::BIGINT => Some(TypeData::Intrinsic(IntrinsicKind::Bigint)),
            TypeId::SYMBOL => Some(TypeData::Intrinsic(IntrinsicKind::Symbol)),
            TypeId::OBJECT | TypeId::PROMISE_BASE => {
                Some(TypeData::Intrinsic(IntrinsicKind::Object))
            }
            TypeId::BOOLEAN_TRUE => Some(TypeData::Literal(LiteralValue::Boolean(true))),
            TypeId::BOOLEAN_FALSE => Some(TypeData::Literal(LiteralValue::Boolean(false))),
            TypeId::FUNCTION => Some(TypeData::Intrinsic(IntrinsicKind::Function)),
            _ => None,
        }
    }
}

impl ProvenanceLookup for TypeInterner {
    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        TypeInterner::lookup(self, id)
    }

    fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        TypeInterner::lookup_alloc_order(self, id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        TypeInterner::type_application(self, id)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        TypeInterner::type_list(self, id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        TypeInterner::object_shape(self, id)
    }

    fn contains_generic_type_parameters(&self, id: TypeId) -> bool {
        crate::type_queries::contains_generic_type_parameters_db(self, id)
    }
}

#[cfg(test)]
#[path = "interner_tests.rs"]
mod tests;
