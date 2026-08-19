//! Sharded storage primitives for the type interner.
//!
//! These data structures own the concurrent, lazily initialized layout that
//! backs `TypeInterner`: per-shard `TypeData` storage and the slice/value
//! interners for type components. They are pure data layout - the interning,
//! lookup, and construction logic lives in the sibling modules.
//!
//! Fields are visible to the `intern::core` module tree (not fully private)
//! because the intern/lookup hot paths in the parent `interner` module own the
//! write protocol (allocation order, perf counters, shard id) and manipulate
//! these locks directly rather than through accessor methods.

use crate::types::{TypeData, TypeId};
use crate::utils::RwLockExt;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::FxBuildHasher;
use std::hash::Hash;
use std::sync::{
    Arc, OnceLock, RwLock,
    atomic::{AtomicU32, Ordering},
};

/// Pre-fetched ordering key for a single `Application` component (its base or
/// one of its type arguments).
///
/// `compare_application_component` resolves each component through the interner
/// (`builtin_sort_key`, `lookup`, `DefId` of `Lazy`/`Enum`) every time two
/// `Application` union members are compared. Pre-computing this key once per
/// component during the `O(N)` cache pass removes those lookups from the inner
/// `O(N log N)` sort comparator. The fields mirror exactly what the resolving
/// comparator inspects, so the cached comparison is order-identical.
#[derive(Clone, Copy)]
pub(in crate::intern::core) struct AppComponentKey {
    /// `builtin_sort_key(id)` — `Some` for intrinsic/builtin component types.
    pub(in crate::intern::core) builtin_key: Option<u32>,
    /// `type_data_rank` of the component's `TypeData`; `None` when the component
    /// has no resolvable `TypeData` (matches the comparator's `lookup` miss).
    pub(in crate::intern::core) rank: Option<u8>,
    /// Raw `DefId` for `Lazy`/`Enum` components; `None` otherwise. Only consulted
    /// when both components share the same rank, mirroring the resolving path.
    pub(in crate::intern::core) lazy_or_enum_defid: Option<u32>,
    /// Global allocation order of the component (`TypeInterner::lookup_alloc_order`),
    /// consulted before the raw `TypeId` tiebreak. Raw `TypeId`s are sharded
    /// (`shard_idx` interleaved with a per-shard local index), so two components
    /// interned back-to-back can land in different shards and get numerically
    /// unordered raw ids; `alloc_order` is a single global counter and reflects
    /// true creation order, matching `CachedUnionMember`'s own fallback.
    pub(in crate::intern::core) alloc_order: Option<u32>,
    /// Raw `TypeId` of the component, used as the final stable tiebreak.
    pub(in crate::intern::core) raw: u32,
}

/// Cached data for a union member, pre-fetched to avoid redundant DashMap/arena
/// lookups during sort comparisons. Each field corresponds to a lookup that
/// `compare_union_members` would otherwise perform per comparison.
pub(in crate::intern::core) struct CachedUnionMember {
    /// The original TypeId
    pub(in crate::intern::core) id: TypeId,
    /// Result of `builtin_sort_key(id)` - `Some` for intrinsic/builtin types
    pub(in crate::intern::core) builtin_key: Option<u32>,
    /// Result of `self.lookup(id)` - the TypeData for non-builtin types
    pub(in crate::intern::core) data: Option<TypeData>,
    /// For Object/ObjectWithIndex: the symbol's raw u32 (if the shape has a symbol)
    pub(in crate::intern::core) obj_symbol: Option<u32>,
    /// For anonymous Object/ObjectWithIndex: the `ShapeId`'s raw u32
    pub(in crate::intern::core) obj_anon_shape: Option<u32>,
    /// For Callable: the symbol's raw u32 (if the shape has a symbol)
    pub(in crate::intern::core) callable_symbol: Option<u32>,
    /// For string literals: resolved text used by union-member ordering.
    pub(in crate::intern::core) string_literal_text: Option<Arc<str>>,
    /// For `Application` members: pre-fetched component ordering keys (base
    /// followed by each type argument). Lets the comparator order two
    /// `Application` members without any interner lookups. Boxed so that the
    /// far more common non-`Application` members keep `CachedUnionMember` small.
    pub(in crate::intern::core) app_components: Option<Box<[AppComponentKey]>>,
    /// For `Tuple`/`Array` members: pre-fetched element ordering keys, one per
    /// tuple element (or the single array element). Each key is computed from
    /// the element's *widened* type so that tuple/array unions sort like tsc's
    /// `stableTypeOrdering` over widened element types — e.g. `[string, number]`
    /// orders before `[string, boolean]` because `number` precedes `boolean`.
    /// Without this the comparator falls back to allocation (source) order,
    /// which diverges from tsc whenever the members were created in a different
    /// order than their canonical ordering. Boxed to keep the common
    /// non-element-bearing members small.
    pub(in crate::intern::core) elem_components: Option<Box<[AppComponentKey]>>,
    /// Monotonic allocation counter for source-order sorting
    pub(in crate::intern::core) alloc_order: Option<u32>,
}

/// Inner data for a `TypeShard`, lazily initialized.
pub(in crate::intern::core) struct TypeShardInner {
    /// Map from `TypeData` to local index within this shard
    pub(in crate::intern::core) key_to_index: DashMap<TypeData, u32, FxBuildHasher>,
    /// Flat array from local index to `TypeData`.
    /// Sequential indices make a Vec far faster than `DashMap` for reverse lookup.
    /// Protected by `RwLock`: reads are uncontended in single-threaded use (~1 cycle),
    /// writes only happen during intern (append-only).
    pub(in crate::intern::core) index_to_key: RwLock<Vec<TypeData>>,
    /// Per-shard allocation order (parallel to `index_to_key`).
    /// Stores the global monotonic order counter at time of interning.
    pub(in crate::intern::core) alloc_order: RwLock<Vec<u32>>,
}

/// A single shard of the type interned storage.
///
/// Uses `OnceLock` for lazy initialization - `DashMaps` are only allocated
/// when the shard is first accessed, reducing startup overhead.
pub(in crate::intern::core) struct TypeShard {
    /// Lazily initialized inner maps
    pub(in crate::intern::core) inner: OnceLock<TypeShardInner>,
    /// Atomic counter for allocating new indices in this shard
    /// Kept outside `OnceLock` for fast checks without initialization
    pub(in crate::intern::core) next_index: AtomicU32,
}

impl TypeShard {
    pub(in crate::intern::core) const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            next_index: AtomicU32::new(0),
        }
    }

    /// Get the inner maps, initializing on first access
    #[inline]
    pub(in crate::intern::core) fn get_inner(&self) -> &TypeShardInner {
        self.inner.get_or_init(|| TypeShardInner {
            key_to_index: DashMap::with_hasher(FxBuildHasher),
            index_to_key: RwLock::new(Vec::with_capacity(256)),
            alloc_order: RwLock::new(Vec::with_capacity(256)),
        })
    }

    /// Check if a key exists without initializing the shard
    #[inline]
    pub(in crate::intern::core) fn is_empty(&self) -> bool {
        self.next_index.load(Ordering::Relaxed) == 0
    }
}

/// Arrival-order-immune append protocol for id-indexed interner storage.
///
/// `index` is allocated by an atomic counter *before* the storage lock is
/// taken, so writers may reach the lock out of id order. Growing the vec with
/// `placeholder` clones and then writing at `index` keeps every id mapped to
/// its own data regardless of arrival order. A while-`push` backfill loop is
/// NOT safe here: an earlier-arriving higher id would fill a later-arriving
/// lower id's slot with its own data, permanently misaligning ids and slots.
///
/// Placeholder slots are either overwritten by their rightful owner (which
/// holds the index from its own `fetch_add`) or belong to ids that lost an
/// insertion race and are never published, so they are never observed as
/// long as ids are published only after this write completes.
///
/// `placeholder` is invoked only when earlier ids have not written their
/// slots yet, i.e. only on contended out-of-order arrivals; the common
/// in-order append pays no placeholder cost.
#[inline]
pub(in crate::intern::core) fn write_id_slot<T: Clone>(
    vec: &mut Vec<T>,
    index: usize,
    value: T,
    placeholder: impl FnOnce() -> T,
) {
    match vec.len().cmp(&index) {
        std::cmp::Ordering::Less => {
            vec.resize(index, placeholder());
            vec.push(value);
        }
        std::cmp::Ordering::Equal => vec.push(value),
        std::cmp::Ordering::Greater => vec[index] = value,
    }
}

/// Inner data for `ConcurrentSliceInterner`, lazily initialized.
pub(in crate::intern::core) struct SliceInternerInner<T> {
    /// Flat array from ID to slice value. Sequential IDs make Vec optimal for reverse lookup.
    items: RwLock<Vec<Arc<[T]>>>,
    map: DashMap<Arc<[T]>, u32, FxBuildHasher>,
}

/// Slice interner using flat Vec for reverse lookup.
/// Uses lazy initialization to defer allocation until first use.
pub(in crate::intern::core) struct ConcurrentSliceInterner<T> {
    pub(in crate::intern::core) inner: OnceLock<SliceInternerInner<T>>,
    pub(in crate::intern::core) next_id: AtomicU32,
}

impl<T> ConcurrentSliceInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    pub(in crate::intern::core) const fn new() -> Self {
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
    pub(in crate::intern::core) fn intern(&self, items_slice: &[T]) -> u32 {
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
            Entry::Vacant(e) => {
                {
                    // T2.4 instrumentation: wrap the write-lock acquisition
                    // so contention on the slice-interner's `items` vec lands
                    // in the lock-wait histogram alongside the per-shard
                    // TypeData writes. With `perf-counters-timing` OFF this
                    // wrapper compiles to a direct closure call.
                    let mut vec = tsz_common::perf_counters::time_shard_write(0, || {
                        inner.items.write_unpoisoned("interner.items")
                    });
                    // Gap slots get an empty slice.
                    write_id_slot(&mut vec, id as usize, temp_arc, || Arc::from(Vec::new()));
                }
                // Publish the id only after its slot is readable so a
                // concurrent map hit can never observe an unwritten slot.
                e.insert(id);
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    #[inline]
    pub(in crate::intern::core) fn get(&self, id: u32) -> Option<Arc<[T]>> {
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
    pub(in crate::intern::core) fn empty(&self) -> Arc<[T]> {
        let inner = self.get_inner();
        let vec = inner.items.read_unpoisoned("interner.items");
        vec.first()
            .cloned()
            .unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// Inner data for `ConcurrentValueInterner`, lazily initialized.
pub(in crate::intern::core) struct ValueInternerInner<T> {
    /// Flat array from ID to value. Sequential IDs make Vec optimal for reverse lookup.
    items: RwLock<Vec<Arc<T>>>,
    map: DashMap<Arc<T>, u32, FxBuildHasher>,
}

/// Value interner using flat Vec for reverse lookup.
/// Uses lazy initialization to defer allocation until first use.
pub(in crate::intern::core) struct ConcurrentValueInterner<T> {
    pub(in crate::intern::core) inner: OnceLock<ValueInternerInner<T>>,
    pub(in crate::intern::core) next_id: AtomicU32,
}

impl<T> ConcurrentValueInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    pub(in crate::intern::core) const fn new() -> Self {
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
    pub(in crate::intern::core) fn intern(&self, value: T) -> u32 {
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
                {
                    // T2.4 instrumentation: see the matching wrapper in
                    // `ConcurrentSliceInterner::intern`. Same rationale,
                    // same zero-cost-when-feature-off contract.
                    let mut vec = tsz_common::perf_counters::time_shard_write(0, || {
                        inner.items.write_unpoisoned("interner.items")
                    });
                    // No empty value exists for arbitrary `T`; gap slots get
                    // this value's `Arc` and rightful owners overwrite their
                    // own index.
                    write_id_slot(&mut vec, id as usize, Arc::clone(&value_arc), || value_arc);
                }
                // Publish the id only after its slot is readable so a
                // concurrent map hit can never observe an unwritten slot.
                e.insert(id);
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    /// Insert a value under a fresh id WITHOUT registering it in the dedup
    /// map. Used for entries whose identity carries out-of-band data (e.g. a
    /// display mask stored beside the arena): a structurally equal value
    /// interned through `intern` must never resolve to this id, and vice
    /// versa. Callers own dedup among unique-inserted entries.
    pub(in crate::intern::core) fn insert_unique(&self, value_arc: Arc<T>) -> u32 {
        let inner = self.get_inner();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut vec = tsz_common::perf_counters::time_shard_write(0, || {
            inner.items.write_unpoisoned("interner.items")
        });
        write_id_slot(&mut vec, id as usize, Arc::clone(&value_arc), || value_arc);
        id
    }

    #[inline]
    pub(in crate::intern::core) fn get(&self, id: u32) -> Option<Arc<T>> {
        let vec = self.inner.get()?.items.read().ok()?;
        vec.get(id as usize).cloned()
    }

    /// Get value by copy for Copy types, avoiding Arc clone overhead.
    #[inline]
    pub(in crate::intern::core) fn get_copy(&self, id: u32) -> Option<T>
    where
        T: Copy,
    {
        let vec = self.inner.get()?.items.read().ok()?;
        vec.get(id as usize).map(|arc| **arc)
    }
}
