//! Type interning for structural deduplication.
//!
//! This module implements the type interning engine that converts
//! TypeKey structures into lightweight TypeId handles.
//!
//! Benefits:
//! - O(1) type equality (just compare TypeId values)
//! - Memory efficient (each unique structure stored once)
//! - Cache-friendly (work with u32 arrays instead of heap objects)
//!
//! # Concurrency Strategy
//!
//! The TypeInterner uses a sharded DashMap-based architecture for lock-free
//! concurrent access:
//!
//! - **Sharded Type Storage**: 64 shards based on hash of TypeKey to minimize contention
//! - **DashMap for Interning**: Each shard uses DashMap for lock-free read/write operations
//! - **Arc for Immutability**: Type data is stored in Arc<T> for cheap cloning
//! - **No RwLock<Vec<T>>**: Avoids the read-then-write deadlock pattern
//!
//! This design allows true parallel type checking without lock contention.

use crate::interner::{Atom, ShardedInterner};
use crate::solver::def::DefId;
use crate::solver::types::*;
use crate::solver::visitor::{is_literal_type, is_object_like_type, is_unit_type};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, Ordering},
};

// Re-export for test access

const SHARD_BITS: u32 = 6;
const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
pub(crate) const PROPERTY_MAP_THRESHOLD: usize = 24;
const TYPE_LIST_INLINE: usize = 8;

/// Maximum template literal expansion limit.
/// WASM environments have limited linear memory, so we use a much lower limit
/// to prevent OOM. Native CLI can handle more.
#[cfg(target_arch = "wasm32")]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum number of interned types before aborting.
/// WASM linear memory cannot grow indefinitely, so we cap at 500k types.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_INTERNED_TYPES: usize = 5_000_000;

type TypeListBuffer = SmallVec<[TypeId; TYPE_LIST_INLINE]>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LiteralDomain {
    String,
    Number,
    Boolean,
    Bigint,
}

/// Primitive kind for disjoint intersection checking.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum PrimitiveKind {
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
}

impl PrimitiveKind {
    fn from_literal(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::String(_) => PrimitiveKind::String,
            LiteralValue::Number(_) => PrimitiveKind::Number,
            LiteralValue::Boolean(_) => PrimitiveKind::Boolean,
            LiteralValue::BigInt(_) => PrimitiveKind::BigInt,
        }
    }
}

#[derive(Clone, Debug)]
struct LiteralSet {
    domain: LiteralDomain,
    values: FxHashSet<LiteralValue>,
}

impl LiteralSet {
    fn from_literal(literal: LiteralValue) -> Self {
        let domain = literal_domain(&literal);
        let mut values = FxHashSet::default();
        values.insert(literal);
        LiteralSet { domain, values }
    }
}

fn literal_domain(literal: &LiteralValue) -> LiteralDomain {
    match literal {
        LiteralValue::String(_) => LiteralDomain::String,
        LiteralValue::Number(_) => LiteralDomain::Number,
        LiteralValue::Boolean(_) => LiteralDomain::Boolean,
        LiteralValue::BigInt(_) => LiteralDomain::Bigint,
    }
}

/// Inner data for a TypeShard, lazily initialized.
struct TypeShardInner {
    /// Map from TypeKey to local index within this shard
    key_to_index: DashMap<TypeKey, u32, FxBuildHasher>,
    /// Map from local index to TypeKey (using Arc for shared access)
    index_to_key: DashMap<u32, Arc<TypeKey>, FxBuildHasher>,
}

/// A single shard of the type interned storage.
///
/// Uses OnceLock for lazy initialization - DashMaps are only allocated
/// when the shard is first accessed, reducing startup overhead.
struct TypeShard {
    /// Lazily initialized inner maps
    inner: OnceLock<TypeShardInner>,
    /// Atomic counter for allocating new indices in this shard
    /// Kept outside OnceLock for fast checks without initialization
    next_index: AtomicU32,
}

impl TypeShard {
    fn new() -> Self {
        TypeShard {
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

/// Inner data for ConcurrentSliceInterner, lazily initialized.
struct SliceInternerInner<T> {
    items: DashMap<u32, Arc<[T]>, FxBuildHasher>,
    map: DashMap<Arc<[T]>, u32, FxBuildHasher>,
}

/// Lock-free slice interner using DashMap for concurrent access.
/// Uses lazy initialization to defer DashMap allocation until first use.
struct ConcurrentSliceInterner<T> {
    inner: OnceLock<SliceInternerInner<T>>,
    next_id: AtomicU32,
}

impl<T> ConcurrentSliceInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    fn new() -> Self {
        ConcurrentSliceInterner {
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
            items.insert(0, empty.clone());
            map.insert(empty, 0);
            SliceInternerInner { items, map }
        })
    }

    fn intern(&self, items_slice: &[T]) -> u32 {
        if items_slice.is_empty() {
            return 0;
        }

        let inner = self.get_inner();
        let temp_arc: Arc<[T]> = Arc::from(items_slice.to_vec());

        // Try to get existing ID via reverse map — O(1)
        if let Some(ref_entry) = inner.map.get(&temp_arc) {
            return *ref_entry.value();
        }

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(temp_arc.clone()) {
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(id);
                inner.items.insert(id, temp_arc);
                id
            }
            dashmap::mapref::entry::Entry::Occupied(e) => *e.get(),
        }
    }

    fn get(&self, id: u32) -> Option<Arc<[T]>> {
        // For id 0 (empty), we can return without initializing
        if id == 0 {
            return Some(Arc::from(Vec::new()));
        }
        self.inner.get()?.items.get(&id).map(|e| e.value().clone())
    }

    fn empty(&self) -> Arc<[T]> {
        Arc::from(Vec::new())
    }
}

/// Inner data for ConcurrentValueInterner, lazily initialized.
struct ValueInternerInner<T> {
    items: DashMap<u32, Arc<T>, FxBuildHasher>,
    map: DashMap<Arc<T>, u32, FxBuildHasher>,
}

/// Lock-free value interner using DashMap for concurrent access.
/// Uses lazy initialization to defer DashMap allocation until first use.
struct ConcurrentValueInterner<T> {
    inner: OnceLock<ValueInternerInner<T>>,
    next_id: AtomicU32,
}

impl<T> ConcurrentValueInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    fn new() -> Self {
        ConcurrentValueInterner {
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

    fn intern(&self, value: T) -> u32 {
        let inner = self.get_inner();
        let value_arc = Arc::new(value);

        // Try to get existing ID
        if let Some(ref_entry) = inner.map.get(&value_arc) {
            return *ref_entry.value();
        }

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match inner.map.entry(value_arc.clone()) {
            Entry::Vacant(e) => {
                e.insert(id);
                inner.items.insert(id, value_arc);
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    fn get(&self, id: u32) -> Option<Arc<T>> {
        self.inner.get()?.items.get(&id).map(|e| e.value().clone())
    }
}

/// Type interning table with lock-free concurrent access.
///
/// Uses sharded DashMap structures for all internal storage, enabling
/// true parallel type checking without lock contention.
///
/// All internal structures use lazy initialization via OnceLock to minimize
/// startup overhead - DashMaps are only allocated when first accessed.
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
    /// Object property maps: lazily initialized DashMap
    object_property_maps:
        OnceLock<DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher>>,
    function_shapes: ConcurrentValueInterner<FunctionShape>,
    callable_shapes: ConcurrentValueInterner<CallableShape>,
    conditional_types: ConcurrentValueInterner<ConditionalType>,
    mapped_types: ConcurrentValueInterner<MappedType>,
    applications: ConcurrentValueInterner<TypeApplication>,
    /// Cache for is_unit_type checks (memoized O(1) lookup after first computation)
    unit_type_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// The global Array base type (e.g., Array<T> from lib.d.ts)
    array_base_type: OnceLock<TypeId>,
    /// Type parameters for the Array base type
    array_base_type_params: OnceLock<Vec<TypeParamInfo>>,
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
    /// Uses lazy initialization for all DashMap structures to minimize
    /// startup overhead. DashMaps are only allocated when first accessed.
    pub fn new() -> Self {
        let shards: Vec<TypeShard> = (0..SHARD_COUNT).map(|_| TypeShard::new()).collect();

        TypeInterner {
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
            unit_type_cache: DashMap::with_hasher(FxBuildHasher),
            array_base_type: OnceLock::new(),
            array_base_type_params: OnceLock::new(),
        }
    }

    /// Set the global Array base type (e.g., Array<T> from lib.d.ts).
    ///
    /// This should be called once during primordial type setup when lib.d.ts is processed.
    /// Once set, the value cannot be changed (OnceLock enforces this).
    pub fn set_array_base_type(&self, type_id: TypeId, params: Vec<TypeParamInfo>) {
        let _ = self.array_base_type.set(type_id);
        let _ = self.array_base_type_params.set(params);
    }

    /// Get the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type(&self) -> Option<TypeId> {
        self.array_base_type.get().copied()
    }

    /// Get the type parameters for the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.array_base_type_params
            .get()
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the object property maps, initializing on first access
    #[inline]
    fn get_object_property_maps(
        &self,
    ) -> &DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher> {
        self.object_property_maps
            .get_or_init(|| DashMap::with_hasher(FxBuildHasher))
    }

    /// Check if a type is a "unit type" (represents exactly one value).
    /// Results are cached for O(1) lookup after first computation.
    /// This is used for optimization in BCT and subtype checking.
    #[inline]
    pub fn is_unit_type(&self, type_id: TypeId) -> bool {
        // Fast path: check cache first
        if let Some(cached) = self.unit_type_cache.get(&type_id) {
            return *cached;
        }
        // Compute and cache
        let result = is_unit_type(self, type_id);
        self.unit_type_cache.insert(type_id, result);
        result
    }

    /// Intern a string into an Atom.
    /// This is used when constructing types with property names or string literals.
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

    pub fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.type_lists
            .get(id.0)
            .unwrap_or_else(|| self.type_lists.empty())
    }

    pub fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.tuple_lists
            .get(id.0)
            .unwrap_or_else(|| self.tuple_lists.empty())
    }

    pub fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.template_lists
            .get(id.0)
            .unwrap_or_else(|| self.template_lists.empty())
    }

    pub fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.object_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(ObjectShape {
                flags: ObjectFlags::empty(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
            })
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
    /// This uses a lock-free pattern with DashMap to avoid the read-then-write
    /// deadlock that existed in the previous RwLock<Vec> implementation.
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
            return Some(map.clone());
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
                e.insert(map.clone());
                Some(map)
            }
            Entry::Occupied(e) => Some(e.get().clone()),
        }
    }

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

    pub fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.applications.get(id.0).unwrap_or_else(|| {
            Arc::new(TypeApplication {
                base: TypeId::ERROR,
                args: Vec::new(),
            })
        })
    }

    /// Intern a type key and return its TypeId.
    /// If the key already exists, returns the existing TypeId.
    /// Otherwise, creates a new TypeId and stores the key.
    ///
    /// This uses a lock-free pattern with DashMap for concurrent access.
    pub fn intern(&self, key: TypeKey) -> TypeId {
        if let Some(id) = self.get_intrinsic_id(&key) {
            return id;
        }

        // Circuit breaker: prevent OOM by limiting total interned types
        // This is especially important for WASM where linear memory is limited.
        // We do a cheap approximate check (summing shard indices) to avoid
        // iterating all shards on every intern call.
        if self.approximate_count() > MAX_INTERNED_TYPES {
            return TypeId::ERROR;
        }

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
        match inner.key_to_index.entry(key.clone()) {
            Entry::Vacant(e) => {
                e.insert(local_index);
                let key_arc = Arc::new(key);
                inner.index_to_key.insert(local_index, key_arc);
                self.make_id(local_index, shard_idx as u32)
            }
            Entry::Occupied(e) => {
                // Another thread inserted first, use their ID
                let existing_index = *e.get();
                self.make_id(existing_index, shard_idx as u32)
            }
        }
    }

    /// Look up the TypeKey for a given TypeId.
    ///
    /// This uses lock-free DashMap access with lazy shard initialization.
    pub fn lookup(&self, id: TypeId) -> Option<TypeKey> {
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
            .map(|r| r.value().as_ref().clone())
    }

    fn intern_type_list(&self, members: Vec<TypeId>) -> TypeListId {
        TypeListId(self.type_lists.intern(&members))
    }

    fn intern_tuple_list(&self, elements: Vec<TupleElement>) -> TupleListId {
        TupleListId(self.tuple_lists.intern(&elements))
    }

    fn intern_template_list(&self, spans: Vec<TemplateSpan>) -> TemplateLiteralId {
        TemplateLiteralId(self.template_lists.intern(&spans))
    }

    pub fn intern_object_shape(&self, shape: ObjectShape) -> ObjectShapeId {
        ObjectShapeId(self.object_shapes.intern(shape))
    }

    fn intern_function_shape(&self, shape: FunctionShape) -> FunctionShapeId {
        FunctionShapeId(self.function_shapes.intern(shape))
    }

    fn intern_callable_shape(&self, shape: CallableShape) -> CallableShapeId {
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
    #[inline]
    fn approximate_count(&self) -> usize {
        // Sample first 4 shards and extrapolate (assumes uniform distribution)
        let mut sample_total: usize = 0;
        for shard in self.shards.iter().take(4) {
            sample_total += shard.next_index.load(Ordering::Relaxed) as usize;
        }
        // Extrapolate to all 64 shards
        (sample_total * SHARD_COUNT / 4) + TypeId::FIRST_USER as usize
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

    fn get_intrinsic_id(&self, key: &TypeKey) -> Option<TypeId> {
        match key {
            TypeKey::Intrinsic(kind) => Some(kind.to_type_id()),
            TypeKey::Error => Some(TypeId::ERROR),
            _ => None,
        }
    }

    fn get_intrinsic_key(&self, id: TypeId) -> Option<TypeKey> {
        match id {
            TypeId::NONE => Some(TypeKey::Error),
            TypeId::ERROR => Some(TypeKey::Error),
            TypeId::NEVER => Some(TypeKey::Intrinsic(IntrinsicKind::Never)),
            TypeId::UNKNOWN => Some(TypeKey::Intrinsic(IntrinsicKind::Unknown)),
            TypeId::ANY => Some(TypeKey::Intrinsic(IntrinsicKind::Any)),
            TypeId::VOID => Some(TypeKey::Intrinsic(IntrinsicKind::Void)),
            TypeId::UNDEFINED => Some(TypeKey::Intrinsic(IntrinsicKind::Undefined)),
            TypeId::NULL => Some(TypeKey::Intrinsic(IntrinsicKind::Null)),
            TypeId::BOOLEAN => Some(TypeKey::Intrinsic(IntrinsicKind::Boolean)),
            TypeId::NUMBER => Some(TypeKey::Intrinsic(IntrinsicKind::Number)),
            TypeId::STRING => Some(TypeKey::Intrinsic(IntrinsicKind::String)),
            TypeId::BIGINT => Some(TypeKey::Intrinsic(IntrinsicKind::Bigint)),
            TypeId::SYMBOL => Some(TypeKey::Intrinsic(IntrinsicKind::Symbol)),
            TypeId::OBJECT => Some(TypeKey::Intrinsic(IntrinsicKind::Object)),
            TypeId::BOOLEAN_TRUE => Some(TypeKey::Literal(LiteralValue::Boolean(true))),
            TypeId::BOOLEAN_FALSE => Some(TypeKey::Literal(LiteralValue::Boolean(false))),
            TypeId::FUNCTION => Some(TypeKey::Intrinsic(IntrinsicKind::Function)),
            TypeId::PROMISE_BASE => Some(TypeKey::Intrinsic(IntrinsicKind::Object)), // Promise base treated as object
            _ => None,
        }
    }

    // =========================================================================
    // Convenience methods for common type constructions
    // =========================================================================

    /// Intern an intrinsic type
    pub fn intrinsic(&self, kind: IntrinsicKind) -> TypeId {
        kind.to_type_id()
    }

    /// Intern a literal string type
    pub fn literal_string(&self, value: &str) -> TypeId {
        let atom = self.intern_string(value);
        self.intern(TypeKey::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal string type from an already-interned Atom
    pub fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.intern(TypeKey::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal number type
    pub fn literal_number(&self, value: f64) -> TypeId {
        self.intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(value))))
    }

    /// Intern a literal boolean type
    pub fn literal_boolean(&self, value: bool) -> TypeId {
        self.intern(TypeKey::Literal(LiteralValue::Boolean(value)))
    }

    /// Intern a literal bigint type
    pub fn literal_bigint(&self, value: &str) -> TypeId {
        let atom = self.intern_string(value);
        self.intern(TypeKey::Literal(LiteralValue::BigInt(atom)))
    }

    /// Intern a literal bigint type, allowing a sign prefix without extra clones.
    pub fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        if !negative {
            return self.literal_bigint(digits);
        }

        let mut value = String::with_capacity(digits.len() + 1);
        value.push('-');
        value.push_str(digits);
        let atom = self.string_interner.intern_owned(value);
        self.intern(TypeKey::Literal(LiteralValue::BigInt(atom)))
    }

    /// Intern a union type, normalizing and deduplicating members
    pub fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.union_from_iter(members)
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
            if let Some(TypeKey::Union(inner)) = self.lookup(member) {
                let members = self.type_list(inner);
                flat.extend(members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        flat.sort_by_key(|id| id.0);
        flat.dedup();
        flat.retain(|id| *id != TypeId::NEVER);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list(flat.into_vec());
        self.intern(TypeKey::Union(list_id))
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

    fn union_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut iter = members.into_iter();
        let Some(first) = iter.next() else {
            return TypeId::NEVER;
        };
        let Some(second) = iter.next() else {
            return first;
        };

        let mut flat: TypeListBuffer = SmallVec::new();
        self.push_union_member(&mut flat, first);
        self.push_union_member(&mut flat, second);
        for member in iter {
            self.push_union_member(&mut flat, member);
        }

        self.normalize_union(flat)
    }

    fn push_union_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeKey::Union(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

    fn normalize_union(&self, mut flat: TypeListBuffer) -> TypeId {
        // Deduplicate and sort for consistent hashing
        flat.sort_by_key(|id| id.0);
        flat.dedup();

        // Handle special cases
        if flat.contains(&TypeId::ERROR) {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        // If any member is `any`, the union is `any`
        if flat.contains(&TypeId::ANY) {
            return TypeId::ANY;
        }
        // If any member is `unknown`, the union is `unknown`
        if flat.contains(&TypeId::UNKNOWN) {
            return TypeId::UNKNOWN;
        }
        // Remove `never` from unions
        flat.retain(|id| *id != TypeId::NEVER);
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

        // Reduce union using subtype checks (e.g., {a: 1} | {a: 1 | number} => {a: 1 | number})
        // Skip reduction if union contains complex types (TypeParameters, Lazy, etc.)
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeKey::TypeParameter(_) | TypeKey::Lazy(_))
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

        let list_id = self.intern_type_list(flat.into_vec());
        self.intern(TypeKey::Union(list_id))
    }

    /// Intern an intersection type, normalizing and deduplicating members
    pub fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.intersection_from_iter(members)
    }

    /// Fast path for two-member intersections.
    pub fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.intersection_from_iter([left, right])
    }

    /// Create an intersection type WITHOUT triggering normalize_intersection
    ///
    /// This is a low-level operation used by the SubtypeChecker to merge
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
            if let Some(TypeKey::Intersection(inner)) = self.lookup(member) {
                let inner_members = self.type_list(inner);
                flat.extend(inner_members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        // Abort reduction if any member is a Lazy type.
        // The interner (Judge) cannot resolve symbols, so if we have unresolved types,
        // we must preserve the intersection as-is without attempting to merge or reduce.
        let has_unresolved = flat
            .iter()
            .any(|&id| matches!(self.lookup(id), Some(TypeKey::Lazy(_))));
        if has_unresolved {
            // Basic dedup without any simplification
            flat.sort_by_key(|id| id.0);
            flat.dedup();
            let list_id = self.intern_type_list(flat.into_vec());
            return self.intern(TypeKey::Intersection(list_id));
        }

        // =========================================================
        // Canonicalization: Handle callable order preservation
        // =========================================================
        // In TypeScript, intersections of functions represent overloads, and
        // order matters. We need to separate callables from non-callables.

        let has_callables = flat.iter().any(|&id| self.is_callable_type(id));

        if !has_callables {
            // Fast path: No callables, sort everything for canonicalization
            flat.sort_by_key(|id| id.0);
            flat.dedup();
        } else {
            // Slow path: Separate callables and others to preserve order
            let mut callables = SmallVec::<[TypeId; 4]>::new();

            // Retain only non-callables in 'flat', move callables to 'callables'
            // This preserves the order of callables as they are extracted
            let mut i = 0;
            while i < flat.len() {
                if self.is_callable_type(flat[i]) {
                    callables.push(flat.remove(i));
                } else {
                    i += 1;
                }
            }

            // Sort and dedup non-callables
            flat.sort_by_key(|id| id.0);
            flat.dedup();

            // Deduplicate callables (preserving order)
            let mut seen = FxHashSet::default();
            callables.retain(|id| seen.insert(*id));

            // Merge: Put non-callables first (canonical), then callables (ordered)
            flat.extend(callables);
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
        let list_id = self.intern_type_list(flat.into_vec());
        self.intern(TypeKey::Intersection(list_id))
    }

    /// Convenience wrapper for raw intersection of two types
    pub fn intersect_types_raw2(&self, a: TypeId, b: TypeId) -> TypeId {
        self.intersect_types_raw(vec![a, b])
    }

    fn intersection_from_iter<I>(&self, members: I) -> TypeId
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

    fn push_intersection_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeKey::Intersection(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

    /// Check if a type is a function or callable (order-sensitive in intersections).
    ///
    /// In TypeScript, intersection types are commutative for structural types
    /// (objects, primitives) but non-commutative for call signatures.
    /// For example: `((a: string) => void) & ((a: number) => void)` has a different
    /// overload order than the reverse.
    fn is_callable_type(&self, id: TypeId) -> bool {
        matches!(
            self.lookup(id),
            Some(TypeKey::Function(_)) | Some(TypeKey::Callable(_))
        )
    }

    fn normalize_intersection(&self, mut flat: TypeListBuffer) -> TypeId {
        // FIX: Do not blindly sort all members. Callables must preserve order
        // for correct overload resolution. Non-callables should be sorted for
        // canonicalization.

        // 1. Check if we have any callables (fast path optimization)
        let has_callables = flat.iter().any(|&id| self.is_callable_type(id));

        if !has_callables {
            // Fast path: No callables, sort everything for canonicalization
            flat.sort_by_key(|id| id.0);
            flat.dedup();
        } else {
            // Slow path: Separate callables and others without heap allocation
            // Use SmallVec to keep stack allocation benefits
            let mut callables = SmallVec::<[TypeId; 4]>::new();

            // Retain only non-callables in 'flat', move callables to 'callables'
            // This preserves the order of callables as they are extracted
            let mut i = 0;
            while i < flat.len() {
                if self.is_callable_type(flat[i]) {
                    callables.push(flat.remove(i));
                } else {
                    i += 1;
                }
            }

            // 2. Sort non-callables (which are left in 'flat')
            flat.sort_by_key(|id| id.0);
            flat.dedup();

            // 3. Deduplicate callables (preserving order)
            // Using a set for O(1) lookups while maintaining insertion order
            let mut seen = FxHashSet::default();
            callables.retain(|id| seen.insert(*id));

            // 4. Merge: Put non-callables first (canonical), then callables (ordered)
            // This creates a canonical form where structural types appear before signatures
            flat.extend(callables);
        }

        // Handle special cases
        if flat.contains(&TypeId::ERROR) {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        // If any member is `never`, the intersection is `never`
        if flat.contains(&TypeId::NEVER) {
            return TypeId::NEVER;
        }
        // If any member is `any`, the intersection is `any`
        if flat.contains(&TypeId::ANY) {
            return TypeId::ANY;
        }
        // Remove `unknown` from intersections (identity element)
        flat.retain(|id| *id != TypeId::UNKNOWN);

        // Abort reduction if any member is a Lazy type.
        // The interner (Judge) cannot resolve symbols, so if we have unresolved types,
        // we must preserve the intersection as-is without attempting to merge or reduce.
        // This prevents incorrect reductions on type aliases like `type A = { x: number }`.
        let has_unresolved = flat
            .iter()
            .any(|&id| matches!(self.lookup(id), Some(TypeKey::Lazy(_))));
        if has_unresolved {
            let list_id = self.intern_type_list(flat.into_vec());
            return self.intern(TypeKey::Intersection(list_id));
        }

        // NOTE: narrow_literal_primitive_intersection was removed (Task #43) because it was too aggressive.
        // It caused incorrect behavior in mixed intersections like "a" & string & { x: 1 }.
        // The reduce_intersection_subtypes() at the end correctly handles literal/primitive narrowing
        // via is_subtype_shallow checks without losing other intersection members.

        if self.intersection_has_disjoint_primitives(&flat) {
            return TypeId::NEVER;
        }
        if self.intersection_has_disjoint_object_literals(&flat) {
            return TypeId::NEVER;
        }
        // Check if null/undefined intersects with any object type
        // null & object = never, undefined & object = never
        // Note: This is different from branded types like string & { __brand: T }
        // which are valid, but null/undefined are ALWAYS disjoint from object types
        if self.intersection_has_null_undefined_with_object(&flat) {
            return TypeId::NEVER;
        }

        // Distributivity: A & (B | C) → (A & B) | (A & C)
        // This enables better normalization and is required for soundness
        // Must be done before object/callable merging to ensure we operate on distributed members
        if let Some(distributed) = self.distribute_intersection_over_unions(&flat) {
            return distributed;
        }

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // =========================================================
        // Task #43: Partial Merging Strategy
        // =========================================================
        // Instead of all-or-nothing merging, extract objects and callables
        // from mixed intersections, merge them separately, then combine.
        //
        // Example: { a: string } & { b: number } & ((x: number) => void)
        // → Merge objects: { a: string; b: number }
        // → Merge callables: (x: number) => void
        // → Result: Callable with properties (merging both)

        // Step 1: Extract and merge objects from mixed intersection
        let (merged_object, remaining_after_objects) = self.extract_and_merge_objects(&flat);

        // Step 2: Extract and merge callables from remaining members
        let (merged_callable, remaining_after_callables) =
            self.extract_and_merge_callables(&remaining_after_objects);

        // Step 3: Rebuild flat with merged results in canonical form
        // Canonical form: [non-callables sorted, callables ordered]
        let mut final_flat: TypeListBuffer = SmallVec::new();

        // Add remaining non-object, non-callable members (these are non-callables)
        final_flat.extend(remaining_after_callables.iter().copied());

        // Add merged object if present (objects are non-callables)
        if let Some(obj_id) = merged_object {
            final_flat.push(obj_id);
        }

        // Sort all non-callables for canonicalization
        final_flat.sort_by_key(|id| id.0);
        final_flat.dedup();

        // Add merged callable if present (callables must come after non-callables)
        if let Some(call_id) = merged_callable {
            final_flat.push(call_id);
        }

        // Early exit if simplified to single type
        if final_flat.len() == 1 {
            return final_flat[0];
        }

        // Update flat reference for subsequent checks
        flat = final_flat;

        // Reduce intersection using subtype checks (e.g., {a: 1} & {a: 1 | number} => {a: 1})
        // Skip reduction if intersection contains complex types (TypeParameters, Lazy, etc.)
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeKey::TypeParameter(_) | TypeKey::Lazy(_))
            )
        });
        if !has_complex {
            self.reduce_intersection_subtypes(&mut flat);
        }

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list(flat.into_vec());
        self.intern(TypeKey::Intersection(list_id))
    }

    fn try_merge_callables_in_intersection(&self, members: &[TypeId]) -> Option<TypeId> {
        let mut call_signatures: Vec<CallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;

        // Collect all call signatures and properties
        for &member in members {
            match self.lookup(member) {
                Some(TypeKey::Function(func_id)) => {
                    let func = self.function_shape(func_id);
                    call_signatures.push(CallSignature {
                        type_params: func.type_params.clone(),
                        params: func.params.clone(),
                        this_type: func.this_type,
                        return_type: func.return_type,
                        type_predicate: func.type_predicate.clone(),
                        is_method: func.is_method,
                    });
                }
                Some(TypeKey::Callable(callable_id)) => {
                    let callable = self.callable_shape(callable_id);
                    // Add all call signatures
                    for sig in &callable.call_signatures {
                        call_signatures.push(sig.clone());
                    }
                    // Merge properties
                    for prop in &callable.properties {
                        if let Some(existing) = properties.iter_mut().find(|p| p.name == prop.name)
                        {
                            // Intersect property types using raw intersection to avoid infinite recursion
                            existing.type_id =
                                self.intersect_types_raw2(existing.type_id, prop.type_id);
                            existing.write_type =
                                self.intersect_types_raw2(existing.write_type, prop.write_type);
                            existing.optional = existing.optional && prop.optional;
                            // Intersection: readonly if ANY constituent is readonly (cumulative)
                            existing.readonly = existing.readonly || prop.readonly;
                        } else {
                            properties.push(prop.clone());
                        }
                    }
                    // Merge index signatures
                    match (&callable.string_index, &string_index) {
                        (Some(idx), None) => string_index = Some(idx.clone()),
                        (Some(idx), Some(existing)) => {
                            string_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly if ANY constituent is readonly (cumulative)
                                readonly: existing.readonly || idx.readonly,
                            });
                        }
                        _ => {}
                    }
                    match (&callable.number_index, &number_index) {
                        (Some(idx), None) => number_index = Some(idx.clone()),
                        (Some(idx), Some(existing)) => {
                            number_index = Some(IndexSignature {
                                key_type: existing.key_type,
                                value_type: self
                                    .intersect_types_raw2(existing.value_type, idx.value_type),
                                // Intersection: readonly if ANY constituent is readonly (cumulative)
                                readonly: existing.readonly || idx.readonly,
                            });
                        }
                        _ => {}
                    }
                }
                _ => return None, // Not all callables, can't merge
            }
        }

        if call_signatures.is_empty() {
            return None;
        }

        // Sort properties by name for consistent hashing
        properties.sort_by_key(|p| p.name.0);

        let callable_shape = CallableShape {
            call_signatures,
            construct_signatures: Vec::new(),
            properties,
            string_index,
            number_index,
            symbol: None,
        };

        let shape_id = self.intern_callable_shape(callable_shape);
        Some(self.intern(TypeKey::Callable(shape_id)))
    }

    fn try_merge_objects_in_intersection(&self, members: &[TypeId]) -> Option<TypeId> {
        let mut objects: Vec<Arc<ObjectShape>> = Vec::new();

        // Check if all members are objects
        for &member in members {
            match self.lookup(member) {
                Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                    objects.push(self.object_shape(shape_id));
                }
                _ => return None, // Not all objects, can't merge
            }
        }

        // Merge all object properties
        let mut merged_props: Vec<PropertyInfo> = Vec::new();
        let mut merged_string_index: Option<IndexSignature> = None;
        let mut merged_number_index: Option<IndexSignature> = None;
        let mut merged_flags = ObjectFlags::empty();

        for obj in &objects {
            // Propagate FRESH_LITERAL flag if any constituent has it
            merged_flags |= obj.flags & ObjectFlags::FRESH_LITERAL;
            // Merge properties
            for prop in &obj.properties {
                // Check if property already exists
                if let Some(existing) = merged_props.iter_mut().find(|p| p.name == prop.name) {
                    // Property exists - intersect the types for stricter checking
                    // In TypeScript, if same property has different types, use intersection
                    // Use raw intersection to avoid infinite recursion
                    if existing.type_id != prop.type_id {
                        existing.type_id =
                            self.intersect_types_raw2(existing.type_id, prop.type_id);
                    }
                    if existing.write_type != prop.write_type {
                        existing.write_type =
                            self.intersect_types_raw2(existing.write_type, prop.write_type);
                    }
                    // Merge flags: required wins over optional, readonly is cumulative
                    // For optional: only optional if ALL are optional (required wins)
                    existing.optional = existing.optional && prop.optional;
                    // For readonly: readonly if ANY is readonly (readonly is cumulative)
                    // { readonly a: number } & { a: number } = { readonly a: number }
                    existing.readonly = existing.readonly || prop.readonly;
                    // For visibility: most restrictive wins (Private > Protected > Public)
                    // { private a: number } & { public a: number } = { private a: number }
                    existing.visibility = match (existing.visibility, prop.visibility) {
                        (Visibility::Private, _) | (_, Visibility::Private) => Visibility::Private,
                        (Visibility::Protected, _) | (_, Visibility::Protected) => {
                            Visibility::Protected
                        }
                        (Visibility::Public, Visibility::Public) => Visibility::Public,
                    };
                } else {
                    merged_props.push(prop.clone());
                }
            }

            // Merge index signatures
            match (&obj.string_index, &merged_string_index) {
                (Some(idx), None) => {
                    merged_string_index = Some(IndexSignature {
                        key_type: idx.key_type,
                        value_type: idx.value_type,
                        readonly: idx.readonly,
                    })
                }
                (Some(idx), Some(existing)) => {
                    merged_string_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly if ANY constituent is readonly (cumulative)
                        readonly: existing.readonly || idx.readonly,
                    });
                }
                _ => {}
            }

            match (&obj.number_index, &merged_number_index) {
                (Some(idx), None) => {
                    merged_number_index = Some(IndexSignature {
                        key_type: idx.key_type,
                        value_type: idx.value_type,
                        readonly: idx.readonly,
                    })
                }
                (Some(idx), Some(existing)) => {
                    merged_number_index = Some(IndexSignature {
                        key_type: existing.key_type,
                        value_type: self.intersect_types_raw2(existing.value_type, idx.value_type),
                        // Intersection: readonly if ANY constituent is readonly (cumulative)
                        readonly: existing.readonly || idx.readonly,
                    });
                }
                _ => {}
            }
        }

        // Sort properties by name for consistent hashing
        merged_props.sort_by_key(|p| p.name.0);

        let shape = ObjectShape {
            flags: merged_flags,
            properties: merged_props,
            string_index: merged_string_index,
            number_index: merged_number_index,
            symbol: None,
        };

        let shape_id = self.intern_object_shape(shape);
        // Preserve index signatures when present.
        if self.object_shape(shape_id).string_index.is_some()
            || self.object_shape(shape_id).number_index.is_some()
        {
            Some(self.intern(TypeKey::ObjectWithIndex(shape_id)))
        } else {
            Some(self.intern(TypeKey::Object(shape_id)))
        }
    }

    /// Task #43: Extract objects from a mixed intersection, merge them, and return
    /// the merged object along with remaining non-object members.
    ///
    /// This implements partial merging for intersections like:
    /// `{ a: string } & { b: number } & string`
    /// → Extracts: `{ a: string }`, `{ b: number }`
    /// → Merges to: `{ a: string; b: number }`
    /// → Returns: (Some({ a: string; b: number }), [string])
    fn extract_and_merge_objects(
        &self,
        members: &[TypeId],
    ) -> (Option<TypeId>, SmallVec<[TypeId; 4]>) {
        let mut objects: Vec<TypeId> = Vec::new();
        let mut remaining: SmallVec<[TypeId; 4]> = SmallVec::new();

        // Separate objects from non-objects
        for &member in members {
            match self.lookup(member) {
                Some(TypeKey::Object(_)) | Some(TypeKey::ObjectWithIndex(_)) => {
                    objects.push(member);
                }
                _ => {
                    remaining.push(member);
                }
            }
        }

        // If no objects, return early
        if objects.is_empty() {
            return (None, remaining);
        }

        // If only one object, return it as-is
        if objects.len() == 1 {
            return (Some(objects[0]), remaining);
        }

        // Merge all objects using existing merge logic
        if let Some(merged) = self.try_merge_objects_in_intersection(&objects) {
            (Some(merged), remaining)
        } else {
            // Merge failed (shouldn't happen), return objects as-is
            remaining.extend(objects);
            (None, remaining)
        }
    }

    /// Task #43: Extract callables from a mixed intersection, merge them, and return
    /// the merged callable along with remaining non-callable members.
    ///
    /// This implements partial merging for intersections like:
    /// `((x: string) => void) & ((x: number) => void) & { a: number }`
    /// → Extracts: `(x: string) => void`, `(x: number) => void`
    /// → Merges to: Callable with overloads
    /// → Returns: (Some(Callable), [{ a: number }])
    fn extract_and_merge_callables(
        &self,
        members: &[TypeId],
    ) -> (Option<TypeId>, SmallVec<[TypeId; 4]>) {
        let mut callables: Vec<TypeId> = Vec::new();
        let mut remaining: SmallVec<[TypeId; 4]> = SmallVec::new();

        // Separate callables from non-callables
        for &member in members {
            if self.is_callable_type(member) {
                callables.push(member);
            } else {
                remaining.push(member);
            }
        }

        // If no callables, return early
        if callables.is_empty() {
            return (None, remaining);
        }

        // If only one callable, return it as-is
        if callables.len() == 1 {
            return (Some(callables[0]), remaining);
        }

        // Merge all callables using existing merge logic
        if let Some(merged) = self.try_merge_callables_in_intersection(&callables) {
            (Some(merged), remaining)
        } else {
            // Merge failed, return callables as-is
            remaining.extend(callables);
            (None, remaining)
        }
    }

    fn intersection_has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        let mut class: Option<PrimitiveClass> = None;
        let mut has_non_primitive = false;
        let mut literals: smallvec::SmallVec<[TypeId; 4]> = SmallVec::new();

        for &member in members {
            // If the member is an empty object type (no props or indexes), it does not conflict
            // with primitives. In TypeScript, `string & {}` is just `string`, so we must not
            // mark this as disjoint.
            let mut mark_non_primitive = false;
            match self.lookup(member) {
                Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                    let shape = self.object_shape(shape_id);
                    if !(shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none())
                    {
                        mark_non_primitive = true;
                    }
                }
                Some(TypeKey::Function(_))
                | Some(TypeKey::Callable(_))
                | Some(TypeKey::Array(_))
                | Some(TypeKey::Tuple(_)) => {
                    mark_non_primitive = true;
                }
                _ => {}
            }
            let Some(member_class) = self.primitive_class_for(member) else {
                has_non_primitive = has_non_primitive || mark_non_primitive;
                continue;
            };
            if let Some(existing) = class {
                if existing != member_class {
                    return true;
                }
            } else {
                class = Some(member_class);
            }

            // Track literals to detect different values of the same primitive type
            if self.is_literal(member) {
                literals.push(member);
            }
        }

        // Check if we have multiple different literals of the same primitive class
        // e.g., "hello" & "world" = never, 1 & 2 = never
        if literals.len() > 1 {
            // Check if all literals are the same value
            let first = literals[0];
            if !literals.iter().all(|&lit| lit == first) {
                return true;
            }
        }

        // NOTE: We do NOT check `has_primitive && has_non_primitive` here.
        // TypeScript allows branded types like `string & { __brand: "UserId" }`.
        // This pattern is used for nominal typing and should NOT reduce to never.
        // The check was removed because it incorrectly broke valid branded types.

        false
    }

    /// Check if null or undefined intersects with any object type.
    ///
    /// In TypeScript, `null & object` and `undefined & object` reduce to `never`
    /// because null/undefined are disjoint from all object types.
    ///
    /// This is different from branded types like `string & { __brand: "UserId" }`
    /// which are valid and should NOT reduce to never.
    fn intersection_has_null_undefined_with_object(&self, members: &[TypeId]) -> bool {
        let mut has_null_or_undefined = false;
        let mut has_object_type = false;

        for &member in members {
            // Check for null or undefined
            if member == TypeId::NULL || member == TypeId::UNDEFINED || member == TypeId::VOID {
                has_null_or_undefined = true;
            } else {
                // Check if this is an object type (not empty object)
                match self.lookup(member) {
                    Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                        // Empty objects {} do NOT count - `null & {}` is valid in some contexts
                        let shape = self.object_shape(shape_id);
                        if !shape.properties.is_empty()
                            || shape.string_index.is_some()
                            || shape.number_index.is_some()
                        {
                            has_object_type = true;
                        }
                    }
                    // Array, tuple, function, callable are all object types that are disjoint from null/undefined
                    Some(TypeKey::Array(_))
                    | Some(TypeKey::Tuple(_))
                    | Some(TypeKey::Function(_))
                    | Some(TypeKey::Callable(_)) => {
                        has_object_type = true;
                    }
                    _ => {}
                }
            }

            // Early exit: if we have both, the intersection is never
            if has_null_or_undefined && has_object_type {
                return true;
            }
        }

        false
    }

    /// Check if an intersection contains disjoint primitive types (e.g., string & number = never).
    ///
    /// In TypeScript, certain primitive types are disjoint and their intersection is never:
    /// - string & number = never
    /// - string & boolean = never
    /// - number & boolean = never
    /// - bigint & number = never
    /// - bigint & string = never
    /// - symbol & (any other primitive except itself) = never
    ///
    /// Note: Literals of the same primitive type are NOT disjoint (e.g., "a" & "b" is valid).
    fn has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        use std::collections::HashSet;

        let mut primitive_kinds: HashSet<PrimitiveKind> = HashSet::new();

        for &member in members {
            let kind = self.get_primitive_kind(member);
            if let Some(k) = kind {
                // Check for disjoint with existing primitives
                for &existing_kind in &primitive_kinds {
                    if Self::are_primitives_disjoint(k, existing_kind) {
                        return true;
                    }
                }
                primitive_kinds.insert(k);
            }
        }

        false
    }

    /// Get the primitive kind of a type (if it's a primitive or literal of a primitive).
    fn get_primitive_kind(&self, type_id: TypeId) -> Option<PrimitiveKind> {
        match self.lookup(type_id) {
            // Direct primitives
            Some(TypeKey::Intrinsic(IntrinsicKind::String)) => Some(PrimitiveKind::String),
            Some(TypeKey::Intrinsic(IntrinsicKind::Number)) => Some(PrimitiveKind::Number),
            Some(TypeKey::Intrinsic(IntrinsicKind::Boolean)) => Some(PrimitiveKind::Boolean),
            Some(TypeKey::Intrinsic(IntrinsicKind::Bigint)) => Some(PrimitiveKind::BigInt),
            Some(TypeKey::Intrinsic(IntrinsicKind::Symbol)) => Some(PrimitiveKind::Symbol),
            // Literals - they inherit the kind of their base type
            Some(TypeKey::Literal(lit)) => Some(PrimitiveKind::from_literal(&lit)),
            // Template literals are string-like
            Some(TypeKey::TemplateLiteral(_)) => Some(PrimitiveKind::String),
            _ => None,
        }
    }

    /// Check if two primitive kinds are disjoint (their intersection is never).
    fn are_primitives_disjoint(a: PrimitiveKind, b: PrimitiveKind) -> bool {
        use PrimitiveKind::*;
        match (a, b) {
            // Same kind is never disjoint
            (String, String)
            | (Number, Number)
            | (Boolean, Boolean)
            | (BigInt, BigInt)
            | (Symbol, Symbol) => false,
            // String is disjoint from number, boolean, bigint, symbol
            (String, Number) | (String, Boolean) | (String, BigInt) | (String, Symbol) => true,
            // Number is disjoint from string, boolean, bigint, symbol
            (Number, String) | (Number, Boolean) | (Number, BigInt) | (Number, Symbol) => true,
            // Boolean is disjoint from string, number, bigint, symbol
            (Boolean, String) | (Boolean, Number) | (Boolean, BigInt) | (Boolean, Symbol) => true,
            // BigInt is disjoint from string, number, boolean, symbol
            (BigInt, String) | (BigInt, Number) | (BigInt, Boolean) | (BigInt, Symbol) => true,
            // Symbol is disjoint from everything except itself (already handled above)
            (Symbol, String) | (Symbol, Number) | (Symbol, Boolean) | (Symbol, BigInt) => true,
        }
    }

    /// Check if a type is a literal type.
    /// Uses the visitor pattern from solver::visitor.
    fn is_literal(&self, type_id: TypeId) -> bool {
        is_literal_type(self, type_id)
    }

    /// Check if a type is object-like (object, array, tuple, function, etc.).
    /// Uses the visitor pattern from solver::visitor.
    #[allow(dead_code)] // Infrastructure for type introspection
    fn is_object_like_type(&self, type_id: TypeId) -> bool {
        // Note: The visitor's is_object_like_type doesn't include functions
        // This version explicitly includes functions for object-likeness
        match self.lookup(type_id) {
            Some(TypeKey::Function(_)) | Some(TypeKey::Callable(_)) => true,
            _ => is_object_like_type(self, type_id),
        }
    }

    fn intersection_has_disjoint_object_literals(&self, members: &[TypeId]) -> bool {
        let mut objects: Vec<Arc<ObjectShape>> = Vec::new();

        for &member in members {
            let Some(key) = self.lookup(member) else {
                continue;
            };
            match key {
                TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                    objects.push(self.object_shape(shape_id));
                }
                _ => {}
            }
        }

        if objects.len() < 2 {
            return false;
        }

        for i in 0..objects.len() {
            for j in (i + 1)..objects.len() {
                if self.object_literals_disjoint(
                    objects[i].properties.as_slice(),
                    objects[j].properties.as_slice(),
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn object_literals_disjoint(&self, left: &[PropertyInfo], right: &[PropertyInfo]) -> bool {
        let (small, large) = if left.len() <= right.len() {
            (left, right)
        } else {
            (right, left)
        };

        for prop in small {
            let Some(other) = Self::find_property(large, prop.name) else {
                continue;
            };

            // If BOTH are optional, the object intersection is NOT never
            // (the property itself just becomes never).
            if prop.optional && other.optional {
                continue;
            }

            // Note: We don't check for disjoint primitive property types here.
            // { a: string } & { a: number } should result in { a: never }, not never.
            // The property type intersection is handled in try_merge_objects_in_intersection.

            // Check literal sets for discriminant-based reduction
            // { kind: "a" } & { kind: "b" } should be never
            // Also handles { kind: "a" } & { kind?: "b" } => never
            if let Some(left_set) = self.literal_set_from_type(prop.type_id) {
                if let Some(right_set) = self.literal_set_from_type(other.type_id) {
                    if self.literal_sets_disjoint(&left_set, &right_set) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if two property types are disjoint (Rule #21: Intersection Reduction).
    /// Returns true if the intersection of these types would be never.
    #[allow(dead_code)]
    fn property_types_disjoint(&self, left: TypeId, right: TypeId) -> bool {
        // Same type is not disjoint
        if left == right {
            return false;
        }

        // Check for disjoint primitives: string & number, boolean & string, etc.
        let left_class = self.primitive_class_for(left);
        let right_class = self.primitive_class_for(right);

        if let (Some(lc), Some(rc)) = (left_class, right_class) {
            // Different primitive classes are disjoint
            return lc != rc;
        }

        false
    }

    fn literal_sets_disjoint(&self, left: &LiteralSet, right: &LiteralSet) -> bool {
        if left.domain != right.domain {
            return true;
        }
        !left.values.iter().any(|value| right.values.contains(value))
    }

    fn literal_set_from_type(&self, type_id: TypeId) -> Option<LiteralSet> {
        let key = self.lookup(type_id)?;
        match key {
            TypeKey::Literal(literal) => Some(LiteralSet::from_literal(literal)),
            TypeKey::Union(members) => {
                let members = self.type_list(members);
                let mut domain: Option<LiteralDomain> = None;
                let mut values = FxHashSet::default();
                for &member in members.iter() {
                    let Some(TypeKey::Literal(literal)) = self.lookup(member) else {
                        return None;
                    };
                    let literal_domain = literal_domain(&literal);
                    if let Some(existing) = domain {
                        if existing != literal_domain {
                            return None;
                        }
                    } else {
                        domain = Some(literal_domain);
                    }
                    values.insert(literal);
                }
                domain.map(|domain| LiteralSet { domain, values })
            }
            _ => None,
        }
    }

    fn find_property(props: &[PropertyInfo], name: Atom) -> Option<&PropertyInfo> {
        props
            .binary_search_by(|prop| prop.name.cmp(&name))
            .ok()
            .map(|idx| &props[idx])
    }

    fn primitive_class_for(&self, type_id: TypeId) -> Option<PrimitiveClass> {
        match type_id {
            TypeId::STRING => return Some(PrimitiveClass::String),
            TypeId::NUMBER => return Some(PrimitiveClass::Number),
            TypeId::BOOLEAN => return Some(PrimitiveClass::Boolean),
            TypeId::BIGINT => return Some(PrimitiveClass::Bigint),
            TypeId::SYMBOL => return Some(PrimitiveClass::Symbol),
            TypeId::NULL => return Some(PrimitiveClass::Null),
            TypeId::UNDEFINED | TypeId::VOID => return Some(PrimitiveClass::Undefined),
            _ => {}
        }

        let key = self.lookup(type_id)?;

        match key {
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::String => Some(PrimitiveClass::String),
                IntrinsicKind::Number => Some(PrimitiveClass::Number),
                IntrinsicKind::Boolean => Some(PrimitiveClass::Boolean),
                IntrinsicKind::Bigint => Some(PrimitiveClass::Bigint),
                IntrinsicKind::Symbol => Some(PrimitiveClass::Symbol),
                IntrinsicKind::Null => Some(PrimitiveClass::Null),
                IntrinsicKind::Undefined | IntrinsicKind::Void => Some(PrimitiveClass::Undefined),
                _ => None,
            },
            TypeKey::Literal(literal) => match literal {
                LiteralValue::String(_) => Some(PrimitiveClass::String),
                LiteralValue::Number(_) => Some(PrimitiveClass::Number),
                LiteralValue::Boolean(_) => Some(PrimitiveClass::Boolean),
                LiteralValue::BigInt(_) => Some(PrimitiveClass::Bigint),
            },
            TypeKey::UniqueSymbol(_) => Some(PrimitiveClass::Symbol),
            TypeKey::TemplateLiteral(_) => Some(PrimitiveClass::String),
            _ => None,
        }
    }

    /// Shallow subtype check that avoids infinite recursion.
    /// Uses TypeId identity for nested components instead of recursive checking.
    /// This is safe for use during normalization because it only uses lookup() and
    /// never calls intern() or evaluate().
    fn is_subtype_shallow(&self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }

        // Skip reduction for type parameters and lazy types
        // These need full type resolution to determine subtyping
        if matches!(
            (self.lookup(source), self.lookup(target)),
            (
                Some(TypeKey::TypeParameter(_)) | _,
                Some(TypeKey::TypeParameter(_))
            ) | (Some(TypeKey::Lazy(_)) | _, Some(TypeKey::Lazy(_)))
        ) {
            return false;
        }

        // Handle Top/Bottom types
        if target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }

        // Handle Literal to Primitive
        // Only if target is NOT a literal (we don't want "a" <: "b")
        if self
            .lookup(source)
            .is_some_and(|k| matches!(k, TypeKey::Literal(_)))
        {
            if self
                .lookup(target)
                .is_some_and(|k| matches!(k, TypeKey::Literal(_)))
            {
                // Both are literals - only subtype if identical (handled above)
                return false;
            }
            if let Some(lit_set) = self.literal_set_from_type(source) {
                if let Some(target_class) = self.primitive_class_for(target) {
                    if self.literal_domain_matches_primitive(lit_set.domain, target_class) {
                        return true;
                    }
                }
            }
        }

        // Handle Objects (Shallow structural check)
        // Uses TypeId equality for properties to avoid recursion.
        // Supports width subtyping (source can have extra properties).
        // Skips index signatures (too complex for shallow check).
        let s_key = self.lookup(source);
        let t_key = self.lookup(target);
        match (s_key, t_key) {
            (
                Some(TypeKey::Object(s_id) | TypeKey::ObjectWithIndex(s_id)),
                Some(TypeKey::Object(t_id) | TypeKey::ObjectWithIndex(t_id)),
            ) => self.is_object_shape_subtype_shallow(s_id, t_id),
            _ => false,
        }
    }

    /// Shallow object shape subtype check.
    ///
    /// Compares properties using TypeId equality (no recursion) to enable
    /// safe object reduction in unions/intersections without infinite recursion.
    ///
    /// ## Subtyping Rules:
    /// - **Width subtyping**: Source can have extra properties
    /// - **Type Identity**: Common properties must have identical TypeIds (no deep check)
    /// - **Optional**: Required <: Optional is true, Optional <: Required is false
    /// - **Readonly**: Mutable <: Readonly is true, Readonly <: Mutable is false
    /// - **Nominal**: If target has a symbol, source must have the same symbol
    /// - **Index Signatures**: Skipped (too complex for shallow check)
    ///
    /// ## Example Reductions:
    /// - `{a: 1} | {a: 1, b: 2}` → `{a: 1}` (a absorbs a, b)
    /// - `{a: 1, b: 2} & {a: 1}` → `{a: 1, b: 2}` (keeps more specific)
    ///
    /// Uses O(N+M) two-pointer scan since properties are sorted by Atom.
    fn is_object_shape_subtype_shallow(&self, s_id: ObjectShapeId, t_id: ObjectShapeId) -> bool {
        let s = self.object_shape(s_id);
        let t = self.object_shape(t_id);

        // 1. Nominal check: if target is a class instance, source must match
        if t.symbol.is_some() && s.symbol != t.symbol {
            return false;
        }

        // 2. Conservative: Index signatures make subtyping complex (deferred to Solver)
        if t.string_index.is_some() || t.number_index.is_some() {
            return false;
        }

        // 2.5. Disjoint properties check: if source and target have completely different
        // properties, they are not in a subtype relationship. This prevents incorrect
        // reductions like `{b?: number} | {a?: number}` from being reduced to `{a?: number}`.
        let has_any_property_overlap = s
            .properties
            .iter()
            .any(|sp| t.properties.iter().any(|tp| sp.name == tp.name));
        if !has_any_property_overlap {
            return false;
        }

        // 3. Structural scan: Source must satisfy all Target properties
        // Properties are sorted by Atom, so we can use two-pointer scan for O(N+M)
        let mut s_idx = 0;
        let s_props = &s.properties;

        for t_prop in &t.properties {
            // Advance source pointer to match target property name
            while s_idx < s_props.len() && s_props[s_idx].name < t_prop.name {
                s_idx += 1;
            }

            if s_idx < s_props.len() && s_props[s_idx].name == t_prop.name {
                let sp = &s_props[s_idx];

                // Rule: Type Identity (no recursion)
                if sp.type_id != t_prop.type_id {
                    return false;
                }

                // Rule: Required <: Optional (Optional <: Required is False)
                if !t_prop.optional && sp.optional {
                    return false;
                }

                // Rule: Mutable <: Readonly (Readonly <: Mutable is False)
                if !t_prop.readonly && sp.readonly {
                    return false;
                }

                s_idx += 1;
            } else {
                // Property missing in source: only allowed if target property is optional
                if !t_prop.optional {
                    return false;
                }
            }
        }

        true
    }

    /// Check if a literal domain matches a primitive class.
    fn literal_domain_matches_primitive(
        &self,
        domain: LiteralDomain,
        class: PrimitiveClass,
    ) -> bool {
        match (domain, class) {
            (LiteralDomain::String, PrimitiveClass::String) => true,
            (LiteralDomain::Number, PrimitiveClass::Number) => true,
            (LiteralDomain::Boolean, PrimitiveClass::Boolean) => true,
            (LiteralDomain::Bigint, PrimitiveClass::Bigint) => true,
            _ => false,
        }
    }

    /// Absorb literal types into their corresponding primitive types.
    /// e.g., "a" | string | number => string | number
    /// e.g., 1 | 2 | number => number
    /// e.g., true | boolean => boolean
    ///
    /// This is called after deduplication and before creating the union.
    fn absorb_literals_into_primitives(&self, flat: &mut TypeListBuffer) {
        // Group types by primitive class
        let mut has_string = false;
        let mut has_number = false;
        let mut has_boolean = false;
        let mut has_bigint = false;
        let mut _has_symbol = false;

        // First pass: identify which primitive types are present
        for &type_id in flat.iter() {
            match type_id {
                TypeId::STRING => has_string = true,
                TypeId::NUMBER => has_number = true,
                TypeId::BOOLEAN => has_boolean = true,
                TypeId::BIGINT => has_bigint = true,
                TypeId::SYMBOL => _has_symbol = true,
                _ => {
                    if let Some(TypeKey::Intrinsic(kind)) = self.lookup(type_id) {
                        match kind {
                            IntrinsicKind::String => has_string = true,
                            IntrinsicKind::Number => has_number = true,
                            IntrinsicKind::Boolean => has_boolean = true,
                            IntrinsicKind::Bigint => has_bigint = true,
                            IntrinsicKind::Symbol => _has_symbol = true,
                            _ => {}
                        }
                    }
                }
            }
        }

        // Second pass: remove literal types that have a corresponding primitive
        flat.retain(|type_id| {
            // Check for boolean literal intrinsics
            if *type_id == TypeId::BOOLEAN_TRUE || *type_id == TypeId::BOOLEAN_FALSE {
                return !has_boolean;
            }

            // Keep if it's not a literal type
            let Some(TypeKey::Literal(literal)) = self.lookup(*type_id) else {
                return true;
            };

            // Remove literal if the corresponding primitive is present
            match literal {
                LiteralValue::String(_) => !has_string,
                LiteralValue::Number(_) => !has_number,
                LiteralValue::Boolean(_) => !has_boolean,
                LiteralValue::BigInt(_) => !has_bigint,
            }
        });
    }

    /// Remove redundant types from a union using shallow subtype checks.
    /// If A <: B, then A | B = B (A is redundant).
    fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
        // OPTIMIZATION: Skip reduction if all types are unit types.
        // Unit types (literals, enum members, tuples of unit types) are disjoint -
        // none can be a subtype of another. This avoids O(N²) comparisons for cases
        // like enumLiteralsSubtypeReduction.ts which has 512 distinct enum-tuple types.
        //
        // EXTENDED OPTIMIZATION: Also skip for types that is_subtype_shallow can't compare.
        // is_subtype_shallow only handles: identical types (dedup already handled),
        // literal-to-primitive, and top/bottom types. For arrays, tuples, and objects
        // it always returns false, making the O(N²) loop pointless.
        if flat.len() > 2 {
            let all_non_reducible = flat.iter().all(|&ty| {
                // Unit types (literals, enum members, tuples of unit types) are disjoint
                if self.is_unit_type(ty) {
                    return true;
                }
                // Arrays, tuples, and objects always return false in is_subtype_shallow
                matches!(
                    self.lookup(ty),
                    Some(
                        TypeKey::Array(_)
                            | TypeKey::Tuple(_)
                            | TypeKey::Object(_)
                            | TypeKey::ObjectWithIndex(_)
                            | TypeKey::Enum(_, _)
                    )
                )
            });
            if all_non_reducible {
                return;
            }
        }

        let mut i = 0;
        while i < flat.len() {
            let mut redundant = false;
            for j in 0..flat.len() {
                if i == j {
                    continue;
                }
                // If i is a subtype of j, i is redundant in a union
                if self.is_subtype_shallow(flat[i], flat[j]) {
                    redundant = true;
                    break;
                }
            }
            if redundant {
                flat.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Remove redundant types from an intersection using shallow subtype checks.
    /// If A <: B, then A & B = A (B is redundant).
    fn reduce_intersection_subtypes(&self, flat: &mut TypeListBuffer) {
        let mut i = 0;
        while i < flat.len() {
            let mut redundant = false;
            for j in 0..flat.len() {
                if i == j {
                    continue;
                }
                // If j is a subtype of i, i is the supertype and redundant in an intersection
                if self.is_subtype_shallow(flat[j], flat[i]) {
                    redundant = true;
                    break;
                }
            }
            if redundant {
                flat.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Distribute an intersection over unions: A & (B | C) → (A & B) | (A & C)
    ///
    /// This is a critical normalization rule for the Judge layer that enables
    /// better simplification and canonical form detection.
    ///
    /// # Cardinality Guard
    /// To prevent exponential explosion (e.g., (A|B) & (C|D) & (E|F)...),
    /// we limit distribution to cases where the resulting union would have ≤ 25 members.
    ///
    /// # Returns
    /// - Some(result) if distribution was applied and should replace the intersection
    /// - None if no distribution occurred (no union members, or would exceed cardinality limit)
    fn distribute_intersection_over_unions(&self, flat: &TypeListBuffer) -> Option<TypeId> {
        // Find all union members in the intersection and calculate total combinations
        let mut union_indices = Vec::new();
        let mut total_combinations = 1;

        for (i, &id) in flat.iter().enumerate() {
            if let Some(TypeKey::Union(members)) = self.lookup(id) {
                let member_count = self.type_list(members).len();

                // Calculate total combinations: product of all union sizes
                // e.g., (A|B|C) & (D|E) → 3 * 2 = 6 combinations
                total_combinations *= member_count;

                // Conservative guard: abort early if would exceed 25 members
                if total_combinations > 25 {
                    return None; // Too many combinations, skip distribution
                }

                union_indices.push(i);
            }
        }

        // No unions to distribute
        if union_indices.is_empty() {
            return None;
        }

        // Build the distributed union
        // Start with the first non-union member as the base
        let base_members: Vec<_> = flat
            .iter()
            .enumerate()
            .filter(|(i, _)| !union_indices.contains(i))
            .map(|(_, &id)| id)
            .collect();

        // If all members are unions, start with an empty intersection (unknown)
        let initial_intersection = if base_members.is_empty() {
            vec![]
        } else {
            base_members
        };

        // Recursively distribute: for each union, create intersections with all combinations
        let mut combinations = vec![initial_intersection];

        for &union_idx in &union_indices {
            let union_type = flat[union_idx];
            let TypeKey::Union(union_members) = self.lookup(union_type)? else {
                continue;
            };
            let union_members = self.type_list(union_members);

            // For each existing combination, create new combinations with each union member
            let mut new_combinations = Vec::new();
            for combination in &combinations {
                for &union_member in union_members.iter() {
                    let mut new_combination = combination.clone();
                    new_combination.push(union_member);
                    new_combinations.push(new_combination);
                }
            }
            combinations = new_combinations;
        }

        // Convert each combination to an intersection TypeId
        let intersection_results: Vec<_> = combinations
            .iter()
            .map(|combination| self.intersection(combination.clone()))
            .collect();

        // Return the union of all intersections
        Some(self.union(intersection_results))
    }

    /// Intern an array type
    pub fn array(&self, element: TypeId) -> TypeId {
        self.intern(TypeKey::Array(element))
    }

    /// Intern a readonly array type
    /// Returns a distinct type from mutable arrays to enforce readonly semantics
    pub fn readonly_array(&self, element: TypeId) -> TypeId {
        let array_type = self.array(element);
        self.intern(TypeKey::ReadonlyType(array_type))
    }

    /// Intern a tuple type
    pub fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let list_id = self.intern_tuple_list(elements);
        self.intern(TypeKey::Tuple(list_id))
    }

    /// Intern a readonly tuple type
    /// Returns a distinct type from mutable tuples to enforce readonly semantics
    pub fn readonly_tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let tuple_type = self.tuple(elements);
        self.intern(TypeKey::ReadonlyType(tuple_type))
    }

    /// Wrap any type in a ReadonlyType marker
    /// This is used for the `readonly` type operator
    pub fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.intern(TypeKey::ReadonlyType(inner))
    }

    /// Intern an object type with properties.
    pub fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::empty())
    }

    /// Intern a fresh object type with properties.
    pub fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::FRESH_LITERAL)
    }

    /// Intern an object type with properties and custom flags.
    pub fn object_with_flags(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
    ) -> TypeId {
        // Sort by property name for consistent hashing
        properties.sort_by(|a, b| a.name.cmp(&b.name));
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        });
        self.intern(TypeKey::Object(shape_id))
    }

    /// Intern an object type with properties, custom flags, and optional symbol.
    /// This is used for interfaces that need symbol tracking but no index signatures.
    pub fn object_with_flags_and_symbol(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<crate::binder::SymbolId>,
    ) -> TypeId {
        // Sort by property name for consistent hashing
        properties.sort_by(|a, b| a.name.cmp(&b.name));
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol,
        });
        self.intern(TypeKey::Object(shape_id))
    }

    /// Intern an object type with index signatures.
    pub fn object_with_index(&self, mut shape: ObjectShape) -> TypeId {
        // Sort properties by name for consistent hashing
        shape.properties.sort_by(|a, b| a.name.cmp(&b.name));
        let shape_id = self.intern_object_shape(shape);
        self.intern(TypeKey::ObjectWithIndex(shape_id))
    }

    /// Intern a function type
    pub fn function(&self, shape: FunctionShape) -> TypeId {
        let shape_id = self.intern_function_shape(shape);
        self.intern(TypeKey::Function(shape_id))
    }

    /// Intern a callable type with overloaded signatures
    pub fn callable(&self, shape: CallableShape) -> TypeId {
        let shape_id = self.intern_callable_shape(shape);
        self.intern(TypeKey::Callable(shape_id))
    }

    fn template_span_cardinality(&self, type_id: TypeId) -> Option<usize> {
        match self.lookup(type_id) {
            Some(TypeKey::Literal(LiteralValue::String(_))) => Some(1),
            Some(TypeKey::Union(list_id)) => {
                let members = self.type_list(list_id);
                let mut count = 0usize;
                for member in members.iter() {
                    if let Some(TypeKey::Literal(LiteralValue::String(_))) = self.lookup(*member) {
                        count += 1;
                    } else {
                        return None;
                    }
                }
                Some(count)
            }
            _ => None,
        }
    }

    fn template_literal_exceeds_limit(&self, spans: &[TemplateSpan]) -> bool {
        let mut total = 1usize;
        for span in spans {
            let span_count = match span {
                TemplateSpan::Text(_) => Some(1),
                TemplateSpan::Type(type_id) => self.template_span_cardinality(*type_id),
            };
            let Some(span_count) = span_count else {
                return false;
            };
            total = total.saturating_mul(span_count);
            if total > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                return true;
            }
        }
        false
    }

    /// Check if a template literal can be expanded to a union of string literals.
    /// Returns true if all type interpolations are string literals or unions of string literals.
    fn can_expand_template_literal(&self, spans: &[TemplateSpan]) -> bool {
        for span in spans {
            if let TemplateSpan::Type(type_id) = span {
                if self.template_span_cardinality(*type_id).is_none() {
                    return false;
                }
            }
        }
        true
    }

    /// Get the string literal values from a type (single literal or union of literals).
    /// Returns None if the type is not a string literal or union of string literals.
    fn get_string_literal_values(&self, type_id: TypeId) -> Option<Vec<String>> {
        match self.lookup(type_id) {
            Some(TypeKey::Literal(LiteralValue::String(atom))) => {
                Some(vec![self.resolve_atom_ref(atom).to_string()])
            }
            Some(TypeKey::Union(list_id)) => {
                let members = self.type_list(list_id);
                let mut values = Vec::with_capacity(members.len());
                for member in members.iter() {
                    if let Some(TypeKey::Literal(LiteralValue::String(atom))) = self.lookup(*member)
                    {
                        values.push(self.resolve_atom_ref(atom).to_string());
                    } else {
                        return None;
                    }
                }
                Some(values)
            }
            _ => None,
        }
    }

    /// Expand a template literal with union interpolations into a union of string literals.
    /// For example: `prefix-${"a" | "b"}-suffix` -> "prefix-a-suffix" | "prefix-b-suffix"
    fn expand_template_literal_to_union(&self, spans: &[TemplateSpan]) -> TypeId {
        // Collect text parts and interpolation alternatives
        let mut parts: Vec<Vec<String>> = Vec::new();

        for span in spans {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.resolve_atom_ref(*atom).to_string();
                    parts.push(vec![text]);
                }
                TemplateSpan::Type(type_id) => {
                    if let Some(values) = self.get_string_literal_values(*type_id) {
                        parts.push(values);
                    } else {
                        // Should not happen if can_expand_template_literal returned true
                        return TypeId::STRING;
                    }
                }
            }
        }

        // Generate all combinations using Cartesian product
        let mut combinations: Vec<String> = vec![String::new()];

        for part in &parts {
            let mut new_combinations = Vec::with_capacity(combinations.len() * part.len());
            for prefix in &combinations {
                for suffix in part {
                    let mut combined = prefix.clone();
                    combined.push_str(suffix);
                    new_combinations.push(combined);
                }
            }
            combinations = new_combinations;

            // Safety check: should not exceed limit at this point, but verify
            if combinations.len() > TEMPLATE_LITERAL_EXPANSION_LIMIT {
                return TypeId::STRING;
            }
        }

        // Create union of string literals
        if combinations.is_empty() {
            return TypeId::NEVER;
        }

        if combinations.len() == 1 {
            return self.literal_string(&combinations[0]);
        }

        let members: Vec<TypeId> = combinations
            .iter()
            .map(|s| self.literal_string(s))
            .collect();

        self.union(members)
    }

    /// Normalize template literal spans by merging consecutive text spans
    fn normalize_template_spans(&self, spans: Vec<TemplateSpan>) -> Vec<TemplateSpan> {
        if spans.len() <= 1 {
            return spans;
        }

        let mut normalized = Vec::with_capacity(spans.len());
        let mut pending_text: Option<String> = None;
        let mut has_consecutive_texts = false;

        for span in &spans {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.resolve_atom_ref(*atom).to_string();
                    if let Some(ref mut pt) = pending_text {
                        pt.push_str(&text);
                        has_consecutive_texts = true;
                    } else {
                        pending_text = Some(text);
                    }
                }
                TemplateSpan::Type(type_id) => {
                    // Task #47: Remove empty string literals from interpolations
                    // An empty string literal contributes nothing to the template
                    if let Some(TypeKey::Literal(LiteralValue::String(s))) = self.lookup(*type_id) {
                        let s = self.resolve_atom_ref(s);
                        if s.is_empty() {
                            // Skip this empty string literal
                            // Flush pending text first
                            if let Some(text) = pending_text.take() {
                                if !text.is_empty() {
                                    normalized.push(TemplateSpan::Text(self.intern_string(&text)));
                                }
                            }
                            // Don't add the empty type span - continue to next span
                            continue;
                        }
                    }

                    // Flush any pending text before adding a type span
                    if let Some(text) = pending_text.take() {
                        if !text.is_empty() {
                            normalized.push(TemplateSpan::Text(self.intern_string(&text)));
                        }
                    }
                    normalized.push(TemplateSpan::Type(*type_id));
                }
            }
        }

        // Flush any remaining pending text
        if let Some(text) = pending_text {
            if !text.is_empty() {
                normalized.push(TemplateSpan::Text(self.intern_string(&text)));
            }
        }

        // If no normalization occurred, return original to avoid unnecessary allocation
        if !has_consecutive_texts && normalized.len() == spans.len() {
            return spans;
        }

        normalized
    }

    /// Intern a template literal type
    pub fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        // Task #47: High-level absorption and widening (Pass 1)
        // These checks must happen BEFORE structural normalization

        // Never absorption: if any part is never, the whole type is never
        for span in &spans {
            if let TemplateSpan::Type(type_id) = span {
                if *type_id == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }
        }

        // Unknown and Any widening: if any part is unknown or any, the whole type is string
        // Note: string intrinsic does NOT widen (it's used for pattern matching)
        for span in &spans {
            if let TemplateSpan::Type(type_id) = span {
                if *type_id == TypeId::UNKNOWN || *type_id == TypeId::ANY {
                    return TypeId::STRING;
                }
            }
        }

        // Normalize spans by merging consecutive text spans (Pass 2)
        let normalized = self.normalize_template_spans(spans);

        // Check if expansion would exceed the limit
        if self.template_literal_exceeds_limit(&normalized) {
            return TypeId::STRING;
        }

        // Try to expand to union of string literals if all interpolations are expandable
        if self.can_expand_template_literal(&normalized) {
            // Check if there are any type interpolations
            let has_type_interpolations = normalized
                .iter()
                .any(|s| matches!(s, TemplateSpan::Type(_)));

            if has_type_interpolations {
                return self.expand_template_literal_to_union(&normalized);
            }

            // If only text spans, combine them into a single string literal
            if normalized
                .iter()
                .all(|s| matches!(s, TemplateSpan::Text(_)))
            {
                let mut combined = String::new();
                for span in &normalized {
                    if let TemplateSpan::Text(atom) = span {
                        combined.push_str(&self.resolve_atom_ref(*atom));
                    }
                }
                return self.literal_string(&combined);
            }
        }

        let list_id = self.intern_template_list(normalized);
        self.intern(TypeKey::TemplateLiteral(list_id))
    }

    /// Get the interpolation positions from a template literal type
    /// Returns indices of type interpolation spans
    pub fn template_literal_interpolation_positions(&self, type_id: TypeId) -> Vec<usize> {
        match self.lookup(type_id) {
            Some(TypeKey::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, span)| match span {
                        TemplateSpan::Type(_) => Some(idx),
                        _ => None,
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get the span at a given position from a template literal type
    pub fn template_literal_get_span(&self, type_id: TypeId, index: usize) -> Option<TemplateSpan> {
        match self.lookup(type_id) {
            Some(TypeKey::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.get(index).cloned()
            }
            _ => None,
        }
    }

    /// Get the number of spans in a template literal type
    pub fn template_literal_span_count(&self, type_id: TypeId) -> usize {
        match self.lookup(type_id) {
            Some(TypeKey::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.len()
            }
            _ => 0,
        }
    }

    /// Check if a template literal contains only text (no interpolations)
    /// Also returns true for string literals (which are the result of text-only template expansion)
    pub fn template_literal_is_text_only(&self, type_id: TypeId) -> bool {
        match self.lookup(type_id) {
            Some(TypeKey::TemplateLiteral(spans_id)) => {
                let spans = self.template_list(spans_id);
                spans.iter().all(|span| span.is_text())
            }
            // String literals are the result of text-only template expansion
            Some(TypeKey::Literal(LiteralValue::String(_))) => true,
            _ => false,
        }
    }

    /// Intern a conditional type
    pub fn conditional(&self, conditional: ConditionalType) -> TypeId {
        let conditional_id = self.intern_conditional_type(conditional);
        self.intern(TypeKey::Conditional(conditional_id))
    }

    /// Intern a mapped type
    pub fn mapped(&self, mapped: MappedType) -> TypeId {
        let mapped_id = self.intern_mapped_type(mapped);
        self.intern(TypeKey::Mapped(mapped_id))
    }

    /// Intern a type reference (deprecated - use lazy() with DefId instead).
    ///
    /// This method is kept for backward compatibility with tests and legacy code.
    /// It converts SymbolRef to DefId and creates TypeKey::Lazy.
    ///
    /// **Phase 1 migration**: New code should use `lazy(def_id)` instead.
    pub fn reference(&self, symbol: SymbolRef) -> TypeId {
        // Convert SymbolRef to DefId by wrapping the raw u32 value
        // This maintains the same identity while using the new TypeKey::Lazy variant
        let def_id = DefId(symbol.0);
        self.intern(TypeKey::Lazy(def_id))
    }

    /// Intern a lazy type reference (DefId-based).
    ///
    /// This is the replacement for `reference()` that uses Solver-owned
    /// DefIds instead of Binder-owned SymbolRefs.
    ///
    /// Phase 1 migration: Use this method for all new type references
    /// to enable O(1) type equality across Binder and Solver boundaries.
    pub fn lazy(&self, def_id: DefId) -> TypeId {
        self.intern(TypeKey::Lazy(def_id))
    }

    /// Intern a generic type application
    pub fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        let app_id = self.intern_application(TypeApplication { base, args });
        self.intern(TypeKey::Application(app_id))
    }
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/intern_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/concurrent_tests.rs"]
mod concurrent_tests;
