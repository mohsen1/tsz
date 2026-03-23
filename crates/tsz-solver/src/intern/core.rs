//! Core implementation of the type interning engine.
//!
//! This module contains all data structures and methods for the `TypeInterner`,
//! including sharded storage, concurrent slice/value interners, and type
//! construction convenience methods.

use crate::def::DefId;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IntrinsicKind, LiteralValue, MappedType, MappedTypeId, ObjectFlags,
    ObjectShape, ObjectShapeId, OrderedFloat, PropertyInfo, PropertyLookup, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::visitor::is_identity_comparable_type;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc, OnceLock, RwLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use tsz_common::interner::{Atom, ShardedInterner};

pub(super) const SHARD_BITS: u32 = 6;
pub(super) const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
pub(super) const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
pub(crate) const PROPERTY_MAP_THRESHOLD: usize = 24;
const TYPE_LIST_INLINE: usize = 8;

/// Maximum template literal expansion limit.
/// WASM environments have limited linear memory, so we use a much lower limit
/// to prevent OOM. Native CLI can handle more.
#[cfg(target_arch = "wasm32")]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum number of interned types before the interner returns ERROR.
/// Prevents OOM on pathological inputs (e.g., DOM types + module augmentation
/// that create millions of intermediate types via heritage merging and
/// function shape instantiation). With ~200-300 bytes per interned entry
/// (DashMap overhead, Arc, shapes), 2M types ≈ 400-600MB.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;

pub(crate) type TypeListBuffer = SmallVec<[TypeId; TYPE_LIST_INLINE]>;
type ObjectPropertyIndex = DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher>;
type ObjectPropertyMap = OnceLock<ObjectPropertyIndex>;

/// Inner data for a `TypeShard`, lazily initialized.
struct TypeShardInner {
    /// Map from `TypeData` to local index within this shard
    key_to_index: DashMap<TypeData, u32, FxBuildHasher>,
    /// Map from local index to `TypeData` (stored inline since TypeData is Copy)
    index_to_key: DashMap<u32, TypeData, FxBuildHasher>,
}

/// A single shard of the type interned storage.
///
/// Uses `OnceLock` for lazy initialization - `DashMaps` are only allocated
/// when the shard is first accessed, reducing startup overhead.
struct TypeShard {
    /// Lazily initialized inner maps
    inner: OnceLock<TypeShardInner>,
    /// Atomic counter for allocating new indices in this shard
    /// Kept outside `OnceLock` for fast checks without initialization
    next_index: AtomicU32,
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
            index_to_key: DashMap::with_hasher(FxBuildHasher),
        })
    }

    /// Check if a key exists without initializing the shard
    #[inline]
    fn is_empty(&self) -> bool {
        self.next_index.load(Ordering::Relaxed) == 0
    }
}

/// Inner data for `ConcurrentSliceInterner`, lazily initialized.
struct SliceInternerInner<T> {
    items: DashMap<u32, Arc<[T]>, FxBuildHasher>,
    map: DashMap<Arc<[T]>, u32, FxBuildHasher>,
}

/// Lock-free slice interner using `DashMap` for concurrent access.
/// Uses lazy initialization to defer `DashMap` allocation until first use.
struct ConcurrentSliceInterner<T> {
    inner: OnceLock<SliceInternerInner<T>>,
    next_id: AtomicU32,
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
            let items = DashMap::with_hasher(FxBuildHasher);
            let map = DashMap::with_hasher(FxBuildHasher);
            let empty: Arc<[T]> = Arc::from(Vec::new());
            items.insert(0, std::sync::Arc::clone(&empty));
            map.insert(empty, 0);
            SliceInternerInner { items, map }
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

        // Cache miss — allocate for insertion
        let temp_arc: Arc<[T]> = Arc::from(items_slice.to_vec());

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(std::sync::Arc::clone(&temp_arc)) {
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(id);
                inner.items.insert(id, temp_arc);
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
            // If inner isn't initialized yet, the only valid id is 0 (empty).
            // Initialize lazily so we reuse the pre-allocated empty Arc.
            self.get_inner()
        } else {
            self.inner.get()?
        };
        inner
            .items
            .get(&id)
            .map(|e| std::sync::Arc::clone(e.value()))
    }

    #[inline]
    fn empty(&self) -> Arc<[T]> {
        // Reuse the pre-allocated empty Arc from the inner cache (id 0)
        // instead of creating Arc::from(Vec::new()) on every call.
        if let Some(inner) = self.inner.get()
            && let Some(e) = inner.items.get(&0)
        {
            return Arc::clone(e.value());
        }
        // Fallback: initialize inner (which creates the empty entry at id 0)
        let inner = self.get_inner();
        inner
            .items
            .get(&0)
            .map(|e| Arc::clone(e.value()))
            .unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// Inner data for `ConcurrentValueInterner`, lazily initialized.
struct ValueInternerInner<T> {
    items: DashMap<u32, Arc<T>, FxBuildHasher>,
    map: DashMap<Arc<T>, u32, FxBuildHasher>,
}

/// Lock-free value interner using `DashMap` for concurrent access.
/// Uses lazy initialization to defer `DashMap` allocation until first use.
struct ConcurrentValueInterner<T> {
    inner: OnceLock<ValueInternerInner<T>>,
    next_id: AtomicU32,
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
            items: DashMap::with_hasher(FxBuildHasher),
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

        // Cache miss — allocate Arc for insertion
        let value_arc = Arc::new(value);

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(std::sync::Arc::clone(&value_arc)) {
            Entry::Vacant(e) => {
                e.insert(id);
                inner.items.insert(id, value_arc);
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    #[inline]
    fn get(&self, id: u32) -> Option<Arc<T>> {
        self.inner
            .get()?
            .items
            .get(&id)
            .map(|e| std::sync::Arc::clone(e.value()))
    }

    /// Get value by copy for Copy types, avoiding Arc clone overhead.
    #[inline]
    fn get_copy(&self, id: u32) -> Option<T>
    where
        T: Copy,
    {
        self.inner.get()?.items.get(&id).map(|e| **e.value())
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
    shards: Vec<TypeShard>,
    /// String interner for property names and string literals (already lock-free)
    pub string_interner: ShardedInterner,
    /// Concurrent interners for type components (lazily initialized)
    type_lists: ConcurrentSliceInterner<TypeId>,
    tuple_lists: ConcurrentSliceInterner<TupleElement>,
    template_lists: ConcurrentSliceInterner<TemplateSpan>,
    object_shapes: ConcurrentValueInterner<ObjectShape>,
    /// Object property maps: lazily initialized `DashMap`
    object_property_maps: ObjectPropertyMap,
    function_shapes: ConcurrentValueInterner<FunctionShape>,
    callable_shapes: ConcurrentValueInterner<CallableShape>,
    conditional_types: ConcurrentValueInterner<ConditionalType>,
    mapped_types: ConcurrentValueInterner<MappedType>,
    applications: ConcurrentValueInterner<TypeApplication>,
    /// Cache for `is_identity_comparable_type` checks (memoized O(1) lookup after first computation)
    identity_comparable_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// The global Array base type (e.g., Array<T> from lib.d.ts).
    /// Uses `RwLock` instead of `OnceLock` so file checkers can overwrite the
    /// prime checker's value (which contains stale `DefIds` from a temporary
    /// `DefinitionStore`).
    array_base_type: RwLock<Option<TypeId>>,
    /// Type parameters for the Array base type.
    /// Kept as `OnceLock` since params don't contain `DefIds` and are stable
    /// across checkers (the interner allocates `TypeParam` `TypeIds` centrally).
    array_base_type_params: OnceLock<Vec<TypeParamInfo>>,
    /// Boxed interface types for primitives (e.g., String interface for `string`).
    /// Registered from lib.d.ts during primordial type setup.
    boxed_types: DashMap<IntrinsicKind, TypeId, FxBuildHasher>,
    /// `DefIds` known to be boxed types (e.g., the DefId for the Function interface).
    /// Registered alongside `boxed_types` so subtype checking can identify boxed
    /// types even when `TypeEnvironment` is unavailable.
    boxed_def_ids: DashMap<IntrinsicKind, Vec<DefId>, FxBuildHasher>,
    /// `DefIds` known to be the `ThisType` marker interface from lib.d.ts.
    /// Used by `ThisTypeMarkerExtractor` to identify `ThisType<T>` applications
    /// when the base type is `Lazy(DefId)`.
    this_type_marker_def_ids: DashMap<DefId, (), FxBuildHasher>,
    /// Global allocation counter for deterministic type ordering.
    /// The sharded interner embeds shard index in TypeId low bits, so raw TypeId
    /// comparison is hash-dependent. This counter provides allocation-order
    /// comparison that approximates tsc's source-order type ID allocation.
    alloc_counter: AtomicU32,
    /// Circuit breaker: once set, all intern/lookup calls return early.
    poisoned: std::sync::atomic::AtomicBool,
    /// Maps TypeId -> allocation order for types that need ordering.
    /// Only populated for non-intrinsic types. Used by `compare_union_members`.
    alloc_order: DashMap<TypeId, u32, FxBuildHasher>,
    /// Effective value for `noUncheckedIndexedAccess` used by query-boundary helpers.
    no_unchecked_indexed_access: AtomicBool,
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
    display_properties: DashMap<TypeId, Arc<Vec<PropertyInfo>>, FxBuildHasher>,
    /// Flag set when union normalization detects that a union type is too complex
    /// to represent (would require > 1M pairwise subtype comparisons during
    /// reduction). Mirrors tsc's `removeSubtypes` complexity heuristic that
    /// emits TS2590. The checker reads and clears this flag to emit the diagnostic.
    union_too_complex: AtomicBool,
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
            array_base_type: RwLock::new(None),
            array_base_type_params: OnceLock::new(),
            boxed_types: DashMap::with_hasher(FxBuildHasher),
            boxed_def_ids: DashMap::with_hasher(FxBuildHasher),
            this_type_marker_def_ids: DashMap::with_hasher(FxBuildHasher),
            alloc_counter: AtomicU32::new(0),
            poisoned: std::sync::atomic::AtomicBool::new(false),
            alloc_order: DashMap::with_hasher(FxBuildHasher),
            no_unchecked_indexed_access: AtomicBool::new(false),
            display_properties: DashMap::with_hasher(FxBuildHasher),
            union_too_complex: AtomicBool::new(false),
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

    /// Atomically read and clear the "union too complex" flag.
    ///
    /// Returns `true` if a union construction was aborted due to complexity
    /// since the last call to this method. The flag is cleared after reading.
    /// The checker uses this to emit TS2590.
    #[inline]
    pub fn take_union_too_complex(&self) -> bool {
        self.union_too_complex.swap(false, Ordering::Relaxed)
    }

    /// Mark that a union construction was aborted due to complexity.
    /// Called from `reduce_union_subtypes` when pairwise comparisons would exceed 1M.
    #[inline]
    pub(crate) fn set_union_too_complex(&self) {
        self.union_too_complex.store(true, Ordering::Relaxed);
    }

    /// Set the global Array base type (e.g., Array<T> from lib.d.ts).
    ///
    /// The `TypeId` uses `RwLock` so each file checker can overwrite the prime
    /// checker's value with one containing correct `DefIds` for its own
    /// `DefinitionStore`. The params use `OnceLock` since they don't contain
    /// `DefIds` and are stable across checkers.
    pub fn set_array_base_type(&self, type_id: TypeId, params: Vec<TypeParamInfo>) {
        *self.array_base_type.write().expect("RwLock not poisoned") = Some(type_id);
        let _ = self.array_base_type_params.set(params);
    }

    /// Get the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type(&self) -> Option<TypeId> {
        *self.array_base_type.read().expect("RwLock not poisoned")
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
    #[inline]
    pub fn intern(&self, key: TypeData) -> TypeId {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return TypeId::ERROR;
        }
        if let Some(id) = self.get_intrinsic_id(&key) {
            return id;
        }

        // Circuit breaker 1: type count limit.
        if self.approximate_count() > MAX_INTERNED_TYPES {
            self.poisoned
                .store(true, std::sync::atomic::Ordering::Relaxed);
            return TypeId::ERROR;
        }
        // Note: infinite-loop protection is handled by solver-level recursion
        // depth limits and fuel budgets, not here. The type count limit above
        // prevents unbounded memory growth from new type creation.

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];
        let inner = shard.get_inner();

        // Try to get existing ID (lock-free read)
        if let Some(entry) = inner.key_to_index.get(&key) {
            let local_index = *entry.value();
            return self.make_id(local_index, shard_idx as u32);
        }

        // Allocate new index
        let local_index = shard.next_index.fetch_add(1, Ordering::Relaxed);
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Return error type instead of panicking
            return TypeId::ERROR;
        }

        // Double-check: another thread might have inserted while we allocated
        match inner.key_to_index.entry(key) {
            Entry::Vacant(e) => {
                e.insert(local_index);
                inner.index_to_key.insert(local_index, key);
                let id = self.make_id(local_index, shard_idx as u32);
                // Record allocation order for deterministic union member sorting.
                let order = self.alloc_counter.fetch_add(1, Ordering::Relaxed);
                self.alloc_order.insert(id, order);
                id
            }
            Entry::Occupied(e) => {
                // Another thread inserted first, use their ID
                let existing_index = *e.get();
                self.make_id(existing_index, shard_idx as u32)
            }
        }
    }

    /// Look up the `TypeData` for a given `TypeId`.
    ///
    /// This uses lock-free `DashMap` access with lazy shard initialization.
    #[inline]
    pub fn lookup(&self, id: TypeId) -> Option<TypeData> {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        if id.is_intrinsic() || id.is_error() {
            return self.get_intrinsic_key(id);
        }

        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;

        let shard = self.shards.get(shard_idx)?;
        // If shard is empty, no types have been interned there yet
        if shard.is_empty() {
            return None;
        }
        shard
            .get_inner()
            .index_to_key
            .get(&{ local_index })
            .map(|r| *r.value())
    }

    pub(super) fn intern_type_list(&self, members: Vec<TypeId>) -> TypeListId {
        TypeListId(self.type_lists.intern(&members))
    }

    /// Intern a type list from a slice, avoiding Vec conversion when the caller
    /// already has a `SmallVec` or slice reference.
    pub(super) fn intern_type_list_from_slice(&self, members: &[TypeId]) -> TypeListId {
        TypeListId(self.type_lists.intern(members))
    }

    fn intern_tuple_list(&self, elements: Vec<TupleElement>) -> TupleListId {
        TupleListId(self.tuple_lists.intern(&elements))
    }

    pub(crate) fn intern_template_list(&self, spans: Vec<TemplateSpan>) -> TemplateLiteralId {
        TemplateLiteralId(self.template_lists.intern(&spans))
    }

    pub fn intern_object_shape(&self, shape: ObjectShape) -> ObjectShapeId {
        ObjectShapeId(self.object_shapes.intern(shape))
    }

    /// Store display-only properties for a fresh object literal.
    ///
    /// These are the pre-widened property types shown in error messages.
    /// The `shape_id` is the widened (interned) shape; `props` contains
    /// the original literal types from the source code.
    pub fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        self.display_properties.insert(type_id, Arc::new(props));
    }

    /// Retrieve display-only properties for a fresh object literal.
    ///
    /// Returns `None` if no display properties were stored (i.e., the
    /// object type was not a fresh literal or had no widened properties).
    pub fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        self.display_properties.get(&type_id).map(|r| r.clone())
    }

    fn intern_function_shape(&self, shape: FunctionShape) -> FunctionShapeId {
        FunctionShapeId(self.function_shapes.intern(shape))
    }

    pub(super) fn intern_callable_shape(&self, shape: CallableShape) -> CallableShapeId {
        CallableShapeId(self.callable_shapes.intern(shape))
    }

    fn intern_conditional_type(&self, conditional: ConditionalType) -> ConditionalTypeId {
        ConditionalTypeId(self.conditional_types.intern(conditional))
    }

    fn intern_mapped_type(&self, mapped: MappedType) -> MappedTypeId {
        MappedTypeId(self.mapped_types.intern(mapped))
    }

    fn intern_application(&self, application: TypeApplication) -> TypeApplicationId {
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

    // =========================================================================
    // Convenience methods for common type constructions
    // =========================================================================

    /// Intern an intrinsic type
    pub const fn intrinsic(&self, kind: IntrinsicKind) -> TypeId {
        kind.to_type_id()
    }

    /// Intern a literal string type
    pub fn literal_string(&self, value: &str) -> TypeId {
        let atom = self.intern_string(value);
        self.intern(TypeData::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal string type from an already-interned Atom
    pub fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal number type
    pub fn literal_number(&self, value: f64) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(value))))
    }

    /// Intern a literal boolean type
    pub fn literal_boolean(&self, value: bool) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::Boolean(value)))
    }

    /// Intern a literal bigint type
    pub fn literal_bigint(&self, value: &str) -> TypeId {
        let atom = self.intern_string(&self.normalize_bigint_literal(value));
        self.intern(TypeData::Literal(LiteralValue::BigInt(atom)))
    }

    /// Intern a literal bigint type, allowing a sign prefix without extra clones.
    pub fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        let normalized = self.normalize_bigint_literal(digits);
        if normalized == "0" {
            return self.literal_bigint(&normalized);
        }
        if !negative {
            return self.literal_bigint(&normalized);
        }

        let mut value = String::with_capacity(normalized.len() + 1);
        value.push('-');
        value.push_str(&normalized);
        let atom = self.string_interner.intern_owned(value);
        self.intern(TypeData::Literal(LiteralValue::BigInt(atom)))
    }

    fn normalize_bigint_literal(&self, value: &str) -> String {
        let stripped = value.replace('_', "");
        if stripped.is_empty() {
            return "0".to_string();
        }

        let (base, digits) = if stripped.starts_with("0x") || stripped.starts_with("0X") {
            (16, &stripped[2..])
        } else if stripped.starts_with("0o") || stripped.starts_with("0O") {
            (8, &stripped[2..])
        } else if stripped.starts_with("0b") || stripped.starts_with("0B") {
            (2, &stripped[2..])
        } else {
            (10, stripped.as_str())
        };

        if digits.is_empty() {
            return "0".to_string();
        }

        if base == 10 {
            let normalized = digits.trim_start_matches('0');
            return if normalized.is_empty() {
                "0".to_string()
            } else {
                normalized.to_string()
            };
        }

        let mut decimal: Vec<u8> = vec![0];
        for ch in digits.chars() {
            let Some(digit) = ch.to_digit(base) else {
                return "0".to_string();
            };
            let digit = digit as u16;
            let mut carry = digit;
            let base = base as u16;
            for dec in decimal.iter_mut() {
                let value = u16::from(*dec) * base + carry;
                *dec = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                decimal.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        while decimal.len() > 1 && *decimal.last().unwrap_or(&0) == 0 {
            decimal.pop();
        }

        let mut out = String::with_capacity(decimal.len());
        for digit in decimal.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        out
    }

    /// Intern a union type, normalizing and deduplicating members.
    /// This performs full normalization including subtype reduction
    /// (matching tsc's `UnionReduction.Subtype` behavior).
    pub fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.union_from_iter(members)
    }

    /// Create a union from a borrowed slice, avoiding allocation when callers
    /// already have an `Arc<[TypeId]>` or `&[TypeId]`.
    pub fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        self.union_from_iter(members.iter().copied())
    }

    /// Intern a union type with literal-only reduction (no subtype reduction).
    ///
    /// This matches tsc's `UnionReduction.Literal` behavior, which is the default
    /// for type annotations. It absorbs literals into primitives (e.g., `"a" | string`
    /// → `string`) but does NOT remove structural subtypes (e.g., `C | D` where
    /// `D extends C` stays as `C | D`).
    ///
    /// Use this for union types from type annotations where the source-level
    /// union structure must be preserved.
    pub fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        self.union_literal_reduce_from_iter(members)
    }

    /// Intern a union type from a vector that is already sorted and deduped.
    /// This is an O(N) operation that avoids redundant sorting.
    pub fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list(flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Intern a union type while preserving member structure.
    ///
    /// This keeps unknown/literal members intact for property access checks.
    pub fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        if members.is_empty() {
            return TypeId::NEVER;
        }

        let mut flat: TypeListBuffer = SmallVec::new();
        for member in members {
            if let Some(TypeData::Union(inner)) = self.lookup(member) {
                let members = self.type_list(inner);
                flat.extend(members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        flat.sort_by(|a, b| self.compare_union_members(*a, *b));
        flat.dedup();
        flat.retain(|id| *id != TypeId::NEVER);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Fast path for unions that already fit in registers.
    pub fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        // Fast paths to avoid expensive normalize_union for trivial cases
        if left == right {
            return left;
        }
        if left == TypeId::NEVER {
            return right;
        }
        if right == TypeId::NEVER {
            return left;
        }
        self.union_from_iter([left, right])
    }

    /// Fast path for three-member unions without heap allocations.
    pub fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.union_from_iter([first, second, third])
    }

    pub(crate) fn union_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let flat = self.collect_union_members(members);
        match flat.len() {
            0 => TypeId::NEVER,
            1 => flat[0],
            _ => self.normalize_union(flat),
        }
    }

    fn union_literal_reduce_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let flat = self.collect_union_members(members);
        match flat.len() {
            0 => TypeId::NEVER,
            1 => flat[0],
            _ => self.normalize_union_literal_only(flat),
        }
    }

    fn collect_union_members<I>(&self, members: I) -> TypeListBuffer
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut iter = members.into_iter();
        let Some(first) = iter.next() else {
            return SmallVec::new();
        };
        let Some(second) = iter.next() else {
            let mut buf = SmallVec::new();
            buf.push(first);
            return buf;
        };

        let mut flat: TypeListBuffer = SmallVec::new();
        self.push_union_member(&mut flat, first);
        self.push_union_member(&mut flat, second);
        for member in iter {
            self.push_union_member(&mut flat, member);
        }
        flat
    }

    pub(super) fn push_union_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeData::Union(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

    /// Sort key for union member ordering of built-in/intrinsic types.
    ///
    /// tsc sorts union members by type.id (allocation order). Built-in types get
    /// remapped keys so they sort consistently (e.g., null/undefined last)
    /// regardless of our internal TypeId numbering.
    ///
    /// Returns `Some(key)` for types with fixed sort positions, `None` for
    /// non-built-in types that should use semantic comparison instead.
    const fn builtin_sort_key(id: TypeId) -> Option<u32> {
        match id {
            TypeId::NUMBER => Some(9),
            TypeId::STRING => Some(8),
            TypeId::BIGINT => Some(10),
            TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE => Some(11),
            TypeId::BOOLEAN_FALSE => Some(12),
            TypeId::VOID => Some(13),
            TypeId::UNDEFINED => Some(14),
            TypeId::NULL => Some(15),
            TypeId::SYMBOL => Some(16),
            TypeId::OBJECT => Some(17),
            TypeId::FUNCTION => Some(18),
            _ if id.is_intrinsic() => Some(id.0),
            _ => None,
        }
    }

    /// Compare two union members for ordering.
    ///
    /// For built-in/intrinsic types: uses fixed sort keys for consistent ordering
    /// (e.g., null/undefined always last).
    ///
    /// For non-built-in types of the same category: uses semantic identity
    /// (literal content, DefId, SymbolId) to approximate tsc's source-order
    /// allocation. This ensures e.g. `"A" | "B" | "C"` instead of arbitrary
    /// interning order, and `C | D` for `class C {}; class D extends C {}`.
    ///
    /// Fallback: raw TypeId comparison.
    fn compare_union_members(&self, a: TypeId, b: TypeId) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Fast path: built-in types have fixed sort positions
        let builtin_a = Self::builtin_sort_key(a);
        let builtin_b = Self::builtin_sort_key(b);
        match (builtin_a, builtin_b) {
            (Some(ka), Some(kb)) => return ka.cmp(&kb),
            (Some(ka), None) => {
                // Built-in vs non-built-in: built-in types sort by their
                // fixed key, non-built-in types are at position >= 100
                return ka.cmp(&100);
            }
            (None, Some(kb)) => {
                return 100u32.cmp(&kb);
            }
            (None, None) => {}
        }

        // Both are non-built-in types. Use semantic identity for ordering
        // where TypeId creation order doesn't match tsc's source-order allocation.
        // The sharded interner embeds shard index in TypeId, making raw TypeId
        // comparison hash-dependent. Semantic comparison ensures deterministic order.
        if let (Some(data_a), Some(data_b)) = (self.lookup(a), self.lookup(b)) {
            match (&data_a, &data_b) {
                // Short string literals (1-2 chars): sort by content to match tsc's
                // lib.d.ts pre-allocation order. tsc pre-creates common short string
                // literal types during lib processing, giving them lower type IDs
                // than user-code types. Since these happen to be ordered by content
                // in tsc, content-based sorting matches tsc's display. For longer
                // strings, alloc_order fallback preserves source encounter order.
                //
                // Short strings sort BEFORE long strings (matching tsc where
                // lib-pre-allocated types have lower IDs). This ensures transitivity.
                (
                    TypeData::Literal(LiteralValue::String(sa)),
                    TypeData::Literal(LiteralValue::String(sb)),
                ) => {
                    let str_a = self.string_interner.resolve(*sa);
                    let str_b = self.string_interner.resolve(*sb);
                    let a_short = str_a.len() <= 2;
                    let b_short = str_b.len() <= 2;
                    match (a_short, b_short) {
                        (true, true) => {
                            // Both short: sort by content
                            let cmp = str_a.cmp(&str_b);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less, // short < long
                        (false, true) => return Ordering::Greater, // long > short
                        (false, false) => {
                            // Both long: fall through to allocation order
                        }
                    }
                }
                // Small number literals (0-9): sort numerically to match tsc's
                // lib.d.ts pre-allocation order for common small numbers.
                // Small numbers sort BEFORE large numbers for transitivity.
                (
                    TypeData::Literal(LiteralValue::Number(na)),
                    TypeData::Literal(LiteralValue::Number(nb)),
                ) => {
                    let a_small = na.0.abs() < 10.0;
                    let b_small = nb.0.abs() < 10.0;
                    match (a_small, b_small) {
                        (true, true) => {
                            let cmp = na.0.partial_cmp(&nb.0).unwrap_or(Ordering::Equal);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less,
                        (false, true) => return Ordering::Greater,
                        (false, false) => {
                            // Both large: fall through to allocation order
                        }
                    }
                }
                // Lazy type references and Enum types: sort by DefId (source declaration order)
                (TypeData::Lazy(d1), TypeData::Lazy(d2))
                | (TypeData::Enum(d1, _), TypeData::Enum(d2, _)) => {
                    let cmp = d1.0.cmp(&d2.0);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                // Object types: sort by SymbolId (declaration order), then by ShapeId
                (TypeData::Object(s1), TypeData::Object(s2))
                | (TypeData::ObjectWithIndex(s1), TypeData::ObjectWithIndex(s2))
                | (TypeData::Object(s1), TypeData::ObjectWithIndex(s2))
                | (TypeData::ObjectWithIndex(s1), TypeData::Object(s2)) => {
                    let shape1 = self.object_shape(*s1);
                    let shape2 = self.object_shape(*s2);
                    if let (Some(sym1), Some(sym2)) = (shape1.symbol, shape2.symbol) {
                        let cmp = sym1.0.cmp(&sym2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    // For anonymous objects (no symbol), use ShapeId (allocation order,
                    // which follows source encounter order for structurally distinct objects)
                    if shape1.symbol.is_none() && shape2.symbol.is_none() {
                        let cmp = s1.0.cmp(&s2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                // Callable types: sort by SymbolId (declaration order)
                (TypeData::Callable(s1), TypeData::Callable(s2)) => {
                    let shape1 = self.callable_shape(*s1);
                    let shape2 = self.callable_shape(*s2);
                    if let (Some(sym1), Some(sym2)) = (shape1.symbol, shape2.symbol) {
                        let cmp = sym1.0.cmp(&sym2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                // Application types (generic instantiations like I1<number>):
                // sort by base type first (which typically resolves to a Lazy(DefId)),
                // then by type arguments lexicographically.
                (TypeData::Application(app1), TypeData::Application(app2)) => {
                    let a1 = self.type_application(*app1);
                    let a2 = self.type_application(*app2);
                    let cmp = self.compare_union_members(a1.base, a2.base);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                    // Same base type — compare args lexicographically
                    for (arg1, arg2) in a1.args.iter().zip(a2.args.iter()) {
                        let cmp = self.compare_union_members(*arg1, *arg2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    let cmp = a1.args.len().cmp(&a2.args.len());
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                _ => {}
            }
        }
        // Fallback: compare by allocation order (monotonic counter).
        // This approximates tsc's type ID allocation order, unlike raw TypeId
        // comparison which is hash-dependent due to the sharded interner.
        let order_a = self.alloc_order.get(&a).map(|r| *r.value());
        let order_b = self.alloc_order.get(&b).map(|r| *r.value());
        match (order_a, order_b) {
            (Some(oa), Some(ob)) => oa.cmp(&ob),
            // Intrinsic types have no alloc_order entry; use raw TypeId
            _ => a.0.cmp(&b.0),
        }
    }

    pub(super) fn normalize_union(&self, mut flat: TypeListBuffer) -> TypeId {
        // Deduplicate and sort for consistent identity.
        // Sort order uses semantic comparison to match tsc's union display.
        flat.sort_by(|a, b| self.compare_union_members(*a, *b));
        flat.dedup();

        // Single-pass scan for special sentinel types instead of multiple contains() calls.
        // Each contains() is O(N); scanning once is O(N) total instead of O(4N).
        let mut has_error = false;
        let mut has_any = false;
        let mut has_unknown = false;
        let mut has_never = false;
        for &id in flat.iter() {
            if id == TypeId::ERROR {
                has_error = true;
                break; // ERROR trumps everything
            }
            if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            } else if id == TypeId::NEVER {
                has_never = true;
            }
        }
        if has_error {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        if has_any {
            return TypeId::ANY;
        }
        if has_unknown {
            return TypeId::UNKNOWN;
        }
        // Remove `never` from unions (only scan if we found any)
        if has_never {
            flat.retain(|id| *id != TypeId::NEVER);
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Absorb literal types into their corresponding primitive types
        // e.g., "a" | string | number => string | number
        // e.g., 1 | 2 | number => number
        // e.g., true | boolean => boolean
        self.absorb_literals_into_primitives(&mut flat);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Large object unions are expensive to subtype-reduce (O(n²)), but they are
        // still valid types. Preserve them and skip subtype reduction instead of
        // collapsing the whole union to `error`, which poisons downstream computed
        // types such as `keyof BigUnion` and `BigUnion["name"]`.
        if flat.len() > 1000 {
            let has_object_types = flat.iter().any(|&id| {
                matches!(
                    self.lookup(id),
                    Some(
                        TypeData::Object(_)
                            | TypeData::ObjectWithIndex(_)
                            | TypeData::Intersection(_)
                    )
                )
            });
            if has_object_types {
                return self.normalize_union_literal_only(flat);
            }
        }

        // Reduce union using subtype checks (e.g., {a: 1} | {a: 1 | number} => {a: 1 | number})
        // Skip reduction if union contains complex types (TypeParameters, Lazy, etc.)
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::TypeParameter(_) | TypeData::Lazy(_))
            )
        });
        if !has_complex {
            self.reduce_union_subtypes(&mut flat);
        }

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Normalize a union with literal-only reduction (no subtype reduction).
    ///
    /// This matches tsc's `UnionReduction.Literal` behavior. It performs all the
    /// same normalization as `normalize_union` (sort, dedup, special cases, literal
    /// absorption) but skips the `reduce_union_subtypes` step.
    fn normalize_union_literal_only(&self, mut flat: TypeListBuffer) -> TypeId {
        flat.sort_by(|a, b| self.compare_union_members(*a, *b));
        flat.dedup();

        // Single-pass scan for special sentinel types
        let mut has_error = false;
        let mut has_any = false;
        let mut has_unknown = false;
        let mut has_never = false;
        for &id in flat.iter() {
            if id == TypeId::ERROR {
                has_error = true;
                break;
            }
            if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            } else if id == TypeId::NEVER {
                has_never = true;
            }
        }
        if has_error {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        if has_any {
            return TypeId::ANY;
        }
        if has_unknown {
            return TypeId::UNKNOWN;
        }
        if has_never {
            flat.retain(|id| *id != TypeId::NEVER);
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        self.absorb_literals_into_primitives(&mut flat);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // NOTE: No subtype reduction here — this is the key difference from normalize_union.
        // tsc's UnionReduction.Literal only absorbs literals into primitives.

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Intern an intersection type, normalizing and deduplicating members
    pub fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.intersection_from_iter(members)
    }

    /// Fast path for two-member intersections.
    pub fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.intersection_from_iter([left, right])
    }

    /// Create an intersection type WITHOUT triggering `normalize_intersection`
    ///
    /// This is a low-level operation used by the `SubtypeChecker` to merge
    /// properties from intersection members without causing infinite recursion.
    ///
    /// # Safety
    /// Only use this when you need to synthesize a type for intermediate checking.
    /// Do NOT use for final compiler output (like .d.ts generation) as the
    /// resulting type will be "unsimplified".
    pub fn intersect_types_raw(&self, members: Vec<TypeId>) -> TypeId {
        // Use SmallVec to keep stack allocation benefits
        let mut flat: TypeListBuffer = SmallVec::new();

        for member in members {
            // Structural flattening is safe and cheap
            if let Some(TypeData::Intersection(inner)) = self.lookup(member) {
                let inner_members = self.type_list(inner);
                flat.extend(inner_members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        // Preserve source/declaration order of intersection members to match tsc.
        // Only perform order-preserving dedup.
        {
            let mut seen = FxHashSet::default();
            flat.retain(|id| seen.insert(*id));
        }

        // =========================================================
        // O(1) Fast Paths (Safe to do without recursion)
        // =========================================================

        // 1. If any member is Never, the result is Never
        if flat.contains(&TypeId::NEVER) {
            return TypeId::NEVER;
        }

        // 2. If any member is Any, the result is Any (unless Never is present)
        if flat.contains(&TypeId::ANY) {
            return TypeId::ANY;
        }

        // 3. Remove Unknown (Identity element for intersection)
        flat.retain(|id| *id != TypeId::UNKNOWN);

        // 4. Check for disjoint primitives (e.g., string & number = never)
        // If we have multiple intrinsic primitive types that are disjoint, return never
        if self.has_disjoint_primitives(&flat) {
            return TypeId::NEVER;
        }

        // =========================================================
        // Final Construction
        // =========================================================

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Create the intersection directly without calling normalize_intersection
        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Intersection(list_id))
    }

    /// Convenience wrapper for raw intersection of two types
    pub fn intersect_types_raw2(&self, a: TypeId, b: TypeId) -> TypeId {
        self.intersect_types_raw(vec![a, b])
    }

    pub(super) fn intersection_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut iter = members.into_iter();
        let Some(first) = iter.next() else {
            return TypeId::UNKNOWN;
        };
        let Some(second) = iter.next() else {
            return first;
        };

        let mut flat: TypeListBuffer = SmallVec::new();
        self.push_intersection_member(&mut flat, first);
        self.push_intersection_member(&mut flat, second);
        for member in iter {
            self.push_intersection_member(&mut flat, member);
        }

        self.normalize_intersection(flat)
    }

    pub(super) fn push_intersection_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeData::Intersection(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

    // Intersection normalization, empty object elimination, callable/object
    // merging, and distribution are in `intersection.rs`.

    /// Intern an array type
    pub fn array(&self, element: TypeId) -> TypeId {
        self.intern(TypeData::Array(element))
    }

    /// Canonical `this` type.
    pub fn this_type(&self) -> TypeId {
        self.intern(TypeData::ThisType)
    }

    /// Intern a readonly array type
    /// Returns a distinct type from mutable arrays to enforce readonly semantics
    pub fn readonly_array(&self, element: TypeId) -> TypeId {
        let array_type = self.array(element);
        self.intern(TypeData::ReadonlyType(array_type))
    }

    /// Intern a tuple type.
    ///
    /// Normalizes optional element types: for `optional=true` elements, strips
    /// explicit `undefined` from union types since the optionality already implies
    /// `| undefined`. This ensures `[1, 2?]` and `[1, (2 | undefined)?]` produce
    /// the same interned TypeId, matching tsc's behavior.
    pub fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let elements = self.normalize_optional_tuple_elements(elements);
        let list_id = self.intern_tuple_list(elements);
        self.intern(TypeData::Tuple(list_id))
    }

    /// For optional tuple elements, strip `undefined` from the element type
    /// since the `optional` flag already implies `| undefined`.
    fn normalize_optional_tuple_elements(
        &self,
        mut elements: Vec<TupleElement>,
    ) -> Vec<TupleElement> {
        for elem in &mut elements {
            if elem.optional && !elem.rest {
                elem.type_id = self.strip_undefined_from_type(elem.type_id);
            }
        }
        elements
    }

    /// Remove `undefined` from a union type. If the type is not a union or
    /// doesn't contain `undefined`, returns the type unchanged.
    fn strip_undefined_from_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::UNDEFINED {
            return type_id;
        }
        if let Some(TypeData::Union(list_id)) = self.lookup(type_id) {
            let members = self.type_list(list_id);
            if members.contains(&TypeId::UNDEFINED) {
                let filtered: Vec<TypeId> = members
                    .iter()
                    .copied()
                    .filter(|&m| m != TypeId::UNDEFINED)
                    .collect();
                return match filtered.len() {
                    0 => TypeId::NEVER,
                    1 => filtered[0],
                    _ => self.union_from_sorted_vec(filtered),
                };
            }
        }
        type_id
    }

    /// Intern a readonly tuple type
    /// Returns a distinct type from mutable tuples to enforce readonly semantics
    pub fn readonly_tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let tuple_type = self.tuple(elements);
        self.intern(TypeData::ReadonlyType(tuple_type))
    }

    /// Wrap any type in a `ReadonlyType` marker
    /// This is used for the `readonly` type operator
    pub fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::ReadonlyType(inner))
    }

    /// Wrap a type in a `NoInfer` marker.
    pub fn no_infer(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::NoInfer(inner))
    }

    /// Create a `unique symbol` type for a symbol declaration.
    pub fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::UniqueSymbol(symbol))
    }

    /// Create an `infer` binder with the provided info.
    pub fn infer(&self, info: TypeParamInfo) -> TypeId {
        self.intern(TypeData::Infer(info))
    }

    pub fn bound_parameter(&self, index: u32) -> TypeId {
        self.intern(TypeData::BoundParameter(index))
    }

    pub fn recursive(&self, depth: u32) -> TypeId {
        self.intern(TypeData::Recursive(depth))
    }

    /// Wrap a type in a `KeyOf` marker.
    pub fn keyof(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::KeyOf(inner))
    }

    /// Build an indexed access type (`T[K]`).
    pub fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.intern(TypeData::IndexAccess(object_type, index_type))
    }

    /// Build a nominal enum type that preserves `DefId` identity and carries
    /// structural member information for compatibility with primitive relations.
    pub fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        self.intern(TypeData::Enum(def_id, structural_type))
    }

    /// Intern an object type with properties.
    pub fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::empty())
    }

    /// Intern a fresh object type with properties.
    pub fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::FRESH_LITERAL)
    }

    /// Intern a fresh object type with both widened properties (for type checking)
    /// and display properties (for error messages).
    ///
    /// This implements tsc's "freshness" model where object literal types
    /// preserve literal types for error display but use widened types for
    /// assignability checking.
    pub fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        // Capture display property declaration order before interning
        let mut display_props = display_properties;
        for (i, prop) in display_props.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        display_props.sort_by_key(|a| a.name);

        // Intern the widened properties as the canonical type
        let type_id = self.object_with_flags(widened_properties, ObjectFlags::FRESH_LITERAL);

        // Store display properties keyed by TypeId (not ObjectShapeId)
        self.store_display_properties(type_id, display_props);

        type_id
    }

    /// Intern an object type with properties and custom flags.
    pub fn object_with_flags(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
    ) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        // declaration_order is excluded from Hash/Eq, so it doesn't affect identity.
        for (i, prop) in properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort by property name for consistent hashing
        properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        });
        self.intern(TypeData::Object(shape_id))
    }

    /// Intern an object type with properties, custom flags, and optional symbol.
    /// This is used for interfaces that need symbol tracking but no index signatures.
    pub fn object_with_flags_and_symbol(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<tsz_binder::SymbolId>,
    ) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        for (i, prop) in properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort by property name for consistent hashing
        properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol,
        });
        self.intern(TypeData::Object(shape_id))
    }

    /// Intern an object type with index signatures.
    pub fn object_with_index(&self, mut shape: ObjectShape) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        for (i, prop) in shape.properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort properties by name for consistent hashing
        shape.properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(shape);
        self.intern(TypeData::ObjectWithIndex(shape_id))
    }

    /// Get the TypeId for an already-interned Object shape.
    /// This is O(1) since it's an interner cache hit.
    pub fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.intern(TypeData::Object(shape_id))
    }

    /// Get the TypeId for an already-interned `ObjectWithIndex` shape.
    pub fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.intern(TypeData::ObjectWithIndex(shape_id))
    }

    /// Intern a function type
    pub fn function(&self, shape: FunctionShape) -> TypeId {
        let shape_id = self.intern_function_shape(shape);
        self.intern(TypeData::Function(shape_id))
    }

    /// Intern a callable type with overloaded signatures
    pub fn callable(&self, shape: CallableShape) -> TypeId {
        let shape_id = self.intern_callable_shape(shape);
        self.intern(TypeData::Callable(shape_id))
    }

    /// Intern a conditional type
    pub fn conditional(&self, conditional: ConditionalType) -> TypeId {
        let conditional_id = self.intern_conditional_type(conditional);
        self.intern(TypeData::Conditional(conditional_id))
    }

    /// Intern a mapped type
    pub fn mapped(&self, mapped: MappedType) -> TypeId {
        let mapped_id = self.intern_mapped_type(mapped);
        self.intern(TypeData::Mapped(mapped_id))
    }

    /// Build a string intrinsic (`Uppercase`, `Lowercase`, etc.) marker.
    pub fn string_intrinsic(
        &self,
        kind: crate::types::StringIntrinsicKind,
        type_arg: TypeId,
    ) -> TypeId {
        self.intern(TypeData::StringIntrinsic { kind, type_arg })
    }

    /// Intern a type reference (deprecated - use `lazy()` with `DefId` instead).
    ///
    /// This method is kept for backward compatibility with tests and legacy code.
    /// It converts `SymbolRef` to `DefId` and creates `TypeData::Lazy`.
    ///
    /// Deprecated: new code should use `lazy(def_id)` instead.
    pub fn reference(&self, symbol: SymbolRef) -> TypeId {
        // Convert SymbolRef to DefId by wrapping the raw u32 value
        // This maintains the same identity while using the new TypeData::Lazy variant
        let def_id = DefId(symbol.0);
        self.intern(TypeData::Lazy(def_id))
    }

    /// Intern a lazy type reference (DefId-based).
    ///
    /// This is the replacement for `reference()` that uses Solver-owned
    /// `DefIds` instead of Binder-owned `SymbolRefs`.
    ///
    /// Use this method for all new type references
    /// to enable O(1) type equality across Binder and Solver boundaries.
    pub fn lazy(&self, def_id: DefId) -> TypeId {
        self.intern(TypeData::Lazy(def_id))
    }

    /// Intern a type parameter.
    pub fn type_param(&self, info: TypeParamInfo) -> TypeId {
        self.intern(TypeData::TypeParameter(info))
    }

    /// Intern a type query (`typeof value`) marker.
    pub fn type_query(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::TypeQuery(symbol))
    }

    /// Intern a module namespace type.
    pub fn module_namespace(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::ModuleNamespace(symbol))
    }

    /// Intern a generic type application
    pub fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        let app_id = self.intern_application(TypeApplication { base, args });
        self.intern(TypeData::Application(app_id))
    }

    /// Estimated in-memory size of the entire type interner in bytes.
    ///
    /// This is a best-effort heuristic for memory pressure tracking and
    /// eviction decisions in the LSP. It reads only atomic counters and
    /// `DashMap::len()` calls — no per-entry iteration.
    ///
    /// The estimate accounts for:
    /// - Per-type overhead in sharded storage (two `DashMap` entries per type)
    /// - Sub-interners for type lists, tuple lists, template lists, shapes
    /// - Auxiliary caches (`identity_comparable`, `alloc_order`, `display_properties`)
    /// - Fixed-size fields (`array_base_type`, `boxed_types`, etc.)
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // --- Sharded type storage ---
        // Each interned type lives in two DashMaps: key_to_index and index_to_key.
        // DashMap overhead per entry is roughly 64 bytes (bucket + hash + padding).
        // TypeData is Copy and small (~32 bytes), stored inline in both maps.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;
        let type_data_size = std::mem::size_of::<TypeData>();
        let per_type_cost = 2 * (DASHMAP_ENTRY_OVERHEAD + type_data_size + 4/* u32 key/value */);

        let type_count = self.len();
        size += type_count * per_type_cost;

        // Shard Vec allocation
        size += self.shards.capacity() * std::mem::size_of::<TypeShard>();

        // --- Slice interners (type_lists, tuple_lists, template_lists) ---
        // Each entry: two DashMap entries (id->Arc<[T]> and Arc<[T]>->id) + Arc heap alloc.
        // Average slice length is ~3 elements for type lists, ~2 for tuples/templates.
        let type_list_count = self.type_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_type_list_elements = 3usize;
        size += type_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TypeId]>>()
                + avg_type_list_elements * std::mem::size_of::<TypeId>());

        let tuple_list_count = self.tuple_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_tuple_elements = 2usize;
        size += tuple_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TupleElement]>>()
                + avg_tuple_elements * std::mem::size_of::<TupleElement>());

        let template_list_count = self.template_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_template_elements = 2usize;
        size += template_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TemplateSpan]>>()
                + avg_template_elements * std::mem::size_of::<TemplateSpan>());

        // --- Value interners (object/function/callable/conditional/mapped/application shapes) ---
        // Each entry: two DashMap entries + Arc<T> heap alloc.
        let value_interner_cost = |count: usize, value_size: usize| -> usize {
            count * (2 * DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<usize>() * 2 + value_size)
        };

        size += value_interner_cost(
            self.object_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ObjectShape>(),
        );
        size += value_interner_cost(
            self.function_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<FunctionShape>(),
        );
        size += value_interner_cost(
            self.callable_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<CallableShape>(),
        );
        size += value_interner_cost(
            self.conditional_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ConditionalType>(),
        );
        size += value_interner_cost(
            self.mapped_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<MappedType>(),
        );
        size += value_interner_cost(
            self.applications.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<TypeApplication>(),
        );

        // --- Auxiliary caches ---
        size += self.identity_comparable_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        size +=
            self.alloc_order.len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 4);
        size += self.display_properties.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<Arc<Vec<PropertyInfo>>>());
        size += self.boxed_types.len() * (DASHMAP_ENTRY_OVERHEAD + 16);
        size += self.boxed_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 32);
        size += self.this_type_marker_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 8);

        // Object property map index (if initialized)
        if let Some(prop_map) = self.object_property_maps.get() {
            size += prop_map.len() * (DASHMAP_ENTRY_OVERHEAD + 128);
        }

        size
    }
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}
