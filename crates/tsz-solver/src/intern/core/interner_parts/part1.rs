use crate::def::DefId;

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
    /// Per-`TypeId` caches for deep `contains_*` content walks (immutable per
    /// `TypeId`; O(1) on repeat shapes). `contains_resolver_dependent_cache`
    /// backs `is_substitution_dependent_type`, the `closed_eval_cache` gate.
    pub(crate) contains_type_params_cache: DashMap<TypeId, bool, FxBuildHasher>,
    pub(crate) contains_lazy_or_recursive_cache: DashMap<TypeId, bool, FxBuildHasher>,
    pub(crate) contains_unresolved_application_cache: DashMap<TypeId, bool, FxBuildHasher>,
    pub(crate) contains_resolver_dependent_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// Alias-opaque `contains Conditional` cache for the closed-eval gate.
    /// The answer is immutable per `TypeId` and avoids repeated subtree walks
    /// on dense recursive mapped/conditional/index-access expansions.
    pub(crate) contains_conditional_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// The global Array base type (e.g., Array<T> from lib.d.ts).
    /// Uses `AtomicU32` (with `u32::MAX` as sentinel for `None`) instead of
    /// `RwLock` so file checkers can overwrite the prime checker's value without
    /// lock contention on this frequently-read field.
    pub(super) array_base_type: AtomicU32,
    /// Display-order Array base type used for keyof/mapped diagnostics.
    /// This may differ from `array_base_type` when the semantic base and the
    /// lib-merged display surface are not the same lowered type.
    pub(super) array_display_base_type: AtomicU32,
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
    /// Display properties for fresh object literal types.
    ///
    /// When object literal properties are widened (e.g., `"hello"` → `string`),
    /// the pre-widened types are stored here for display in error messages.
    /// This implements tsc's "freshness" model where error messages show
    /// literal types (`{ x: "hello" }`) even though the type system uses
    /// widened types (`{ x: string }`).
    ///
    /// Key: `ObjectShapeId` of the widened (interned) shape.
    /// Value: Vec of `PropertyInfo` with original (non-widened) `type_ids`.
    pub(super) display_properties: DashMap<TypeId, Arc<Vec<PropertyInfo>>, FxBuildHasher>,
    /// Reverse mapping from evaluated Application results back to their
    /// original Application TypeId for diagnostic display.
    ///
    /// When `Application(Lazy(Dictionary), [string])` evaluates to
    /// `ObjectWithIndex({ [index: string]: string })`, this maps
    /// the `ObjectWithIndex` TypeId back to the Application TypeId.
    /// The formatter checks this to show `Dictionary<string>` instead
    /// of `{ [index: string]: string; }` in error messages.
    pub(super) display_alias: DashMap<TypeId, TypeId, FxBuildHasher>,
    /// Application bases whose type-alias body is a conditional type.
    ///
    /// Conditional aliases often evaluate to a branch with its own display
    /// surface. Keep this small provenance bit so application-preferring alias
    /// storage can avoid repainting an already-recorded branch intersection.
    pub(super) conditional_alias_bases: DashMap<TypeId, (), FxBuildHasher>,
    /// As-written origin members for a Union TypeId, used to preserve top-level
    /// alias names that would otherwise be lost during union flattening.
    ///
    /// When a user writes `T | null` and `T` is a type alias whose body is itself
    /// a union (e.g., `type T = "a" | "b" | undefined`), tsc's `getUnionType`
    /// flattens the inputs into `"a" | "b" | undefined | null`, but the printer
    /// still displays `T | null` by consulting the union's `origin` field.
    ///
    /// tsz captures the equivalent information here: the checker records the
    /// *unflattened* member list (e.g., `[Lazy(T), null]`) for the resulting
    /// flattened union. The formatter consults this map before falling through
    /// to structural display.
    ///
    /// Key: the flattened Union `TypeId` returned to the checker.
    /// Value: the unflattened input member list, in the order the user wrote.
    pub(super) display_union_origin: DashMap<TypeId, Arc<Vec<TypeId>>, FxBuildHasher>,
    /// Flag set when union normalization detects that a union type is too complex
    /// to represent (would require > 1M pairwise subtype comparisons during
    /// reduction). Mirrors tsc's `removeSubtypes` complexity heuristic that
    /// emits TS2590. The checker reads and clears this flag to emit the diagnostic.
    pub(super) union_too_complex: AtomicBool,
    /// Flag set when tuple synthesis detects that a spread would produce a tuple
    /// with more than `MAX_REPRESENTABLE_TUPLE_LENGTH` elements. The checker reads
    /// and clears this to emit TS2799 instead of TS2589.
    pub(super) tuple_too_large: AtomicBool,
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
