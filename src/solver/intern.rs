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
use crate::solver::types::*;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

const SHARD_BITS: u32 = 6;
const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
pub(crate) const PROPERTY_MAP_THRESHOLD: usize = 24;
const TYPE_LIST_INLINE: usize = 8;
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 10000;

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

/// A single shard of the type interned storage.
///
/// Uses DashMap for lock-free concurrent access to type mappings.
struct TypeShard {
    /// Map from TypeKey to local index within this shard
    key_to_index: DashMap<TypeKey, u32, FxBuildHasher>,
    /// Atomic counter for allocating new indices in this shard
    next_index: AtomicU32,
    /// Map from local index to TypeKey (using Arc for shared access)
    /// Note: We use a separate Vec-like structure indexed by local_index
    /// This is stored in a DashMap for lock-free concurrent access
    index_to_key: DashMap<u32, Arc<TypeKey>, FxBuildHasher>,
}

impl TypeShard {
    fn new() -> Self {
        TypeShard {
            key_to_index: DashMap::with_hasher(FxBuildHasher),
            next_index: AtomicU32::new(0),
            index_to_key: DashMap::with_hasher(FxBuildHasher),
        }
    }
}

/// Lock-free slice interner using DashMap for concurrent access.
struct ConcurrentSliceInterner<T> {
    items: DashMap<u32, Arc<[T]>, FxBuildHasher>,
    next_id: AtomicU32,
}

impl<T> ConcurrentSliceInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    fn new() -> Self {
        let items = DashMap::with_hasher(FxBuildHasher);
        let empty: Arc<[T]> = Arc::from(Vec::new());
        items.insert(0, empty);
        ConcurrentSliceInterner {
            items,
            next_id: AtomicU32::new(1),
        }
    }

    fn intern(&self, items_slice: &[T]) -> u32 {
        if items_slice.is_empty() {
            return 0;
        }

        // Create a temporary Arc for lookup
        let temp_arc: Arc<[T]> = Arc::from(items_slice.to_vec());

        // Try to insert - if already exists, get existing ID
        for entry in self.items.iter() {
            if entry.value() == &temp_arc {
                return *entry.key();
            }
        }

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.items.insert(id, temp_arc);
        id
    }

    fn get(&self, id: u32) -> Option<Arc<[T]>> {
        self.items.get(&id).map(|e| e.value().clone())
    }

    fn empty(&self) -> Arc<[T]> {
        self.items
            .get(&0)
            .map(|e| e.value().clone())
            .unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// Lock-free value interner using DashMap for concurrent access.
struct ConcurrentValueInterner<T> {
    items: DashMap<u32, Arc<T>, FxBuildHasher>,
    map: DashMap<Arc<T>, u32, FxBuildHasher>,
    next_id: AtomicU32,
}

impl<T> ConcurrentValueInterner<T>
where
    T: Eq + Hash + Clone + Send + Sync + 'static,
{
    fn new() -> Self {
        ConcurrentValueInterner {
            items: DashMap::with_hasher(FxBuildHasher),
            map: DashMap::with_hasher(FxBuildHasher),
            next_id: AtomicU32::new(0),
        }
    }

    fn intern(&self, value: T) -> u32 {
        let value_arc = Arc::new(value);

        // Try to get existing ID
        if let Some(ref_entry) = self.map.get(&value_arc) {
            return *ref_entry.value();
        }

        // Allocate new ID
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Double-check: another thread might have inserted while we allocated
        match self.map.entry(value_arc.clone()) {
            Entry::Vacant(e) => {
                e.insert(id);
                self.items.insert(id, value_arc);
                id
            }
            Entry::Occupied(e) => *e.get(),
        }
    }

    fn get(&self, id: u32) -> Option<Arc<T>> {
        self.items.get(&id).map(|e| e.value().clone())
    }
}

/// Type interning table with lock-free concurrent access.
///
/// Uses sharded DashMap structures for all internal storage, enabling
/// true parallel type checking without lock contention.
pub struct TypeInterner {
    /// Sharded storage for user-defined types (lock-free)
    shards: Vec<TypeShard>,
    /// String interner for property names and string literals (already lock-free)
    pub string_interner: ShardedInterner,
    /// Concurrent interners for type components
    type_lists: ConcurrentSliceInterner<TypeId>,
    tuple_lists: ConcurrentSliceInterner<TupleElement>,
    template_lists: ConcurrentSliceInterner<TemplateSpan>,
    object_shapes: ConcurrentValueInterner<ObjectShape>,
    /// Object property maps: DashMap for lock-free concurrent access
    object_property_maps: DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher>,
    function_shapes: ConcurrentValueInterner<FunctionShape>,
    callable_shapes: ConcurrentValueInterner<CallableShape>,
    conditional_types: ConcurrentValueInterner<ConditionalType>,
    mapped_types: ConcurrentValueInterner<MappedType>,
    applications: ConcurrentValueInterner<TypeApplication>,
}

impl TypeInterner {
    /// Create a new type interner with pre-registered intrinsics
    pub fn new() -> Self {
        let shards: Vec<TypeShard> = (0..SHARD_COUNT).map(|_| TypeShard::new()).collect();

        TypeInterner {
            shards,
            string_interner: {
                let interner = ShardedInterner::new();
                interner.intern_common();
                interner
            },
            type_lists: ConcurrentSliceInterner::new(),
            tuple_lists: ConcurrentSliceInterner::new(),
            template_lists: ConcurrentSliceInterner::new(),
            object_shapes: ConcurrentValueInterner::new(),
            object_property_maps: DashMap::with_hasher(FxBuildHasher),
            function_shapes: ConcurrentValueInterner::new(),
            callable_shapes: ConcurrentValueInterner::new(),
            conditional_types: ConcurrentValueInterner::new(),
            mapped_types: ConcurrentValueInterner::new(),
            applications: ConcurrentValueInterner::new(),
        }
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
                properties: Vec::new(),
                string_index: None,
                number_index: None,
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

        // Try to get existing map (lock-free read)
        if let Some(map) = self.object_property_maps.get(&shape_id) {
            return Some(map.clone());
        }

        // Build the property map
        let mut map = FxHashMap::default();
        for (idx, prop) in shape.properties.iter().enumerate() {
            map.insert(prop.name, idx);
        }
        let map = Arc::new(map);

        // Try to insert - if another thread inserted first, use theirs
        match self.object_property_maps.entry(shape_id) {
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

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];

        // Try to get existing ID (lock-free read)
        if let Some(entry) = shard.key_to_index.get(&key) {
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
        match shard.key_to_index.entry(key.clone()) {
            Entry::Vacant(e) => {
                e.insert(local_index);
                let key_arc = Arc::new(key);
                shard.index_to_key.insert(local_index, key_arc);
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
    /// This uses lock-free DashMap access.
    pub fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        if id.is_intrinsic() || id.is_error() {
            return self.get_intrinsic_key(id);
        }

        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;

        let shard = self.shards.get(shard_idx)?;
        shard
            .index_to_key
            .get(&(local_index as u32))
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

    #[inline]
    fn make_id(&self, local_index: u32, shard_idx: u32) -> TypeId {
        let raw_val = (local_index << SHARD_BITS) | (shard_idx & SHARD_MASK);
        TypeId(TypeId::FIRST_USER + raw_val)
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

    /// Fast path for unions that already fit in registers.
    pub fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
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

    fn normalize_intersection(&self, mut flat: TypeListBuffer) -> TypeId {
        // Deduplicate and sort for consistent hashing
        flat.sort_by_key(|id| id.0);
        flat.dedup();

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
        if self.intersection_has_disjoint_primitives(&flat) {
            return TypeId::NEVER;
        }
        if self.intersection_has_disjoint_object_literals(&flat) {
            return TypeId::NEVER;
        }
        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // If all members are objects, merge them into a single object
        if let Some(merged) = self.try_merge_objects_in_intersection(&flat) {
            return merged;
        }

        let list_id = self.intern_type_list(flat.into_vec());
        self.intern(TypeKey::Intersection(list_id))
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

        for obj in &objects {
            // Merge properties
            for prop in &obj.properties {
                // Check if property already exists
                if let Some(existing) = merged_props.iter_mut().find(|p| p.name == prop.name) {
                    // Property exists - intersect the types for stricter checking
                    // In TypeScript, if same property has different types, use intersection
                    if existing.type_id != prop.type_id {
                        existing.type_id = self.intersection2(existing.type_id, prop.type_id);
                    }
                    if existing.write_type != prop.write_type {
                        existing.write_type =
                            self.intersection2(existing.write_type, prop.write_type);
                    }
                    // Merge flags: required wins over optional, readonly is cumulative
                    // For optional: only optional if ALL are optional (required wins)
                    existing.optional = existing.optional && prop.optional;
                    // For readonly: readonly if ANY is readonly
                    existing.readonly = existing.readonly || prop.readonly;
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
                        value_type: self.intersection2(existing.value_type, idx.value_type),
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
                        value_type: self.intersection2(existing.value_type, idx.value_type),
                        readonly: existing.readonly || idx.readonly,
                    });
                }
                _ => {}
            }
        }

        // Sort properties by name for consistent hashing
        merged_props.sort_by_key(|p| p.name.0);

        let shape = ObjectShape {
            properties: merged_props,
            string_index: merged_string_index,
            number_index: merged_number_index,
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

    fn intersection_has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        let mut class: Option<PrimitiveClass> = None;
        let mut has_primitive = false;
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
            has_primitive = true;
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

        // If we have both primitives and non-primitives (objects), they're disjoint
        if has_primitive && has_non_primitive {
            return true;
        }

        false
    }

    fn is_literal(&self, type_id: TypeId) -> bool {
        matches!(self.lookup(type_id), Some(TypeKey::Literal(_)))
    }

    fn is_object_like_type(&self, type_id: TypeId) -> bool {
        match self.lookup(type_id) {
            Some(TypeKey::Object(_)) | Some(TypeKey::ObjectWithIndex(_)) => true,
            Some(TypeKey::Function(_)) | Some(TypeKey::Callable(_)) => true,
            Some(TypeKey::Array(_)) | Some(TypeKey::Tuple(_)) => true,
            _ => false,
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
            if prop.optional {
                continue;
            }
            let Some(left_set) = self.literal_set_from_type(prop.type_id) else {
                continue;
            };
            let Some(other) = Self::find_property(large, prop.name) else {
                continue;
            };
            if other.optional {
                continue;
            }
            let Some(right_set) = self.literal_set_from_type(other.type_id) else {
                continue;
            };
            if self.literal_sets_disjoint(&left_set, &right_set) {
                return true;
            }
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

    /// Intern an object type with properties
    pub fn object(&self, mut properties: Vec<PropertyInfo>) -> TypeId {
        // Sort by property name for consistent hashing
        properties.sort_by(|a, b| a.name.cmp(&b.name));
        let shape_id = self.intern_object_shape(ObjectShape {
            properties,
            string_index: None,
            number_index: None,
        });
        self.intern(TypeKey::Object(shape_id))
    }

    /// Intern an object type with index signatures
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

    /// Intern a template literal type
    pub fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        if self.template_literal_exceeds_limit(&spans) {
            return TypeId::STRING;
        }
        let list_id = self.intern_template_list(spans);
        self.intern(TypeKey::TemplateLiteral(list_id))
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

    /// Intern a type reference
    pub fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeKey::Ref(symbol))
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
#[path = "intern_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "concurrent_tests.rs"]
mod concurrent_tests;
