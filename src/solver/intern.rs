//! Type interning for structural deduplication.
//!
//! This module implements the type interning engine that converts
//! TypeKey structures into lightweight TypeId handles.
//!
//! Benefits:
//! - O(1) type equality (just compare TypeId values)
//! - Memory efficient (each unique structure stored once)
//! - Cache-friendly (work with u32 arrays instead of heap objects)

use crate::interner::{Atom, ShardedInterner};
use crate::solver::types::*;
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

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

struct TypeShard {
    key_to_index: RwLock<FxHashMap<TypeKey, u32>>,
    index_to_key: RwLock<Vec<TypeKey>>,
}

impl TypeShard {
    fn new() -> Self {
        TypeShard {
            key_to_index: RwLock::new(FxHashMap::default()),
            index_to_key: RwLock::new(Vec::new()),
        }
    }
}

struct SliceInterner<T> {
    items: Vec<Arc<[T]>>,
    map: FxHashMap<Arc<[T]>, u32>,
}

impl<T> SliceInterner<T>
where
    T: Eq + Hash,
{
    fn new() -> Self {
        let empty: Arc<[T]> = Arc::from(Vec::new());
        let mut map = FxHashMap::default();
        map.insert(empty.clone(), 0);
        SliceInterner {
            items: vec![empty],
            map,
        }
    }

    fn intern(&mut self, items: Vec<T>) -> u32 {
        if items.is_empty() {
            return 0;
        }

        if let Some(&id) = self.map.get(items.as_slice()) {
            return id;
        }

        let arc: Arc<[T]> = items.into();
        let id = self.items.len() as u32;
        self.items.push(arc.clone());
        self.map.insert(arc, id);
        id
    }

    fn get(&self, id: u32) -> Option<Arc<[T]>> {
        self.items.get(id as usize).cloned()
    }

    fn empty(&self) -> Arc<[T]> {
        self.items[0].clone()
    }
}

struct ValueInterner<T> {
    items: Vec<Arc<T>>,
    map: FxHashMap<Arc<T>, u32>,
}

impl<T> ValueInterner<T>
where
    T: Eq + Hash,
{
    fn new() -> Self {
        ValueInterner {
            items: Vec::new(),
            map: FxHashMap::default(),
        }
    }

    fn intern(&mut self, value: T) -> u32 {
        if let Some(&id) = self.map.get(&value) {
            return id;
        }

        let arc = Arc::new(value);
        let id = self.items.len() as u32;
        self.items.push(arc.clone());
        self.map.insert(arc, id);
        id
    }

    fn get(&self, id: u32) -> Option<Arc<T>> {
        self.items.get(id as usize).cloned()
    }
}

/// Type interning table.
/// Thread-safe via RwLock for concurrent access.
pub struct TypeInterner {
    /// Sharded storage for user-defined types
    shards: [TypeShard; SHARD_COUNT],
    /// String interner for property names and string literals
    /// Thread-safe for concurrent access during type construction
    pub string_interner: ShardedInterner,
    type_lists: RwLock<SliceInterner<TypeId>>,
    tuple_lists: RwLock<SliceInterner<TupleElement>>,
    template_lists: RwLock<SliceInterner<TemplateSpan>>,
    object_shapes: RwLock<ValueInterner<ObjectShape>>,
    object_property_maps: RwLock<Vec<Option<Arc<FxHashMap<Atom, usize>>>>>,
    function_shapes: RwLock<ValueInterner<FunctionShape>>,
    callable_shapes: RwLock<ValueInterner<CallableShape>>,
    conditional_types: RwLock<ValueInterner<ConditionalType>>,
    mapped_types: RwLock<ValueInterner<MappedType>>,
    applications: RwLock<ValueInterner<TypeApplication>>,
}

impl TypeInterner {
    /// Create a new type interner with pre-registered intrinsics
    pub fn new() -> Self {
        TypeInterner {
            shards: std::array::from_fn(|_| TypeShard::new()),
            string_interner: {
                let interner = ShardedInterner::new();
                interner.intern_common();
                interner
            },
            type_lists: RwLock::new(SliceInterner::new()),
            tuple_lists: RwLock::new(SliceInterner::new()),
            template_lists: RwLock::new(SliceInterner::new()),
            object_shapes: RwLock::new(ValueInterner::new()),
            object_property_maps: RwLock::new(Vec::new()),
            function_shapes: RwLock::new(ValueInterner::new()),
            callable_shapes: RwLock::new(ValueInterner::new()),
            conditional_types: RwLock::new(ValueInterner::new()),
            mapped_types: RwLock::new(ValueInterner::new()),
            applications: RwLock::new(ValueInterner::new()),
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
        let lists = self.type_lists.read().expect("type_lists lock poisoned");
        lists.get(id.0).unwrap_or_else(|| lists.empty())
    }

    pub fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        let lists = self.tuple_lists.read().expect("tuple_lists lock poisoned");
        lists.get(id.0).unwrap_or_else(|| lists.empty())
    }

    pub fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        let lists = self.template_lists.read().expect("template_lists lock poisoned");
        lists.get(id.0).unwrap_or_else(|| lists.empty())
    }

    pub fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.object_shapes
            .read()
            .expect("object_shapes lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
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

        let Some(map) = self.object_property_map(shape_id, &shape) else {
            return PropertyLookup::Uncached;
        };

        match map.get(&name) {
            Some(&idx) => PropertyLookup::Found(idx),
            None => PropertyLookup::NotFound,
        }
    }

    fn object_property_map(
        &self,
        shape_id: ObjectShapeId,
        shape: &ObjectShape,
    ) -> Option<Arc<FxHashMap<Atom, usize>>> {
        if shape.properties.len() < PROPERTY_MAP_THRESHOLD {
            return None;
        }

        {
            let maps = self.object_property_maps.read().expect("object_property_maps lock poisoned");
            if let Some(Some(map)) = maps.get(shape_id.0 as usize) {
                return Some(map.clone());
            }
        }

        let mut map = FxHashMap::default();
        for (idx, prop) in shape.properties.iter().enumerate() {
            map.insert(prop.name, idx);
        }
        let map = Arc::new(map);

        let mut maps = self.object_property_maps.write().expect("object_property_maps lock poisoned");
        if maps.len() <= shape_id.0 as usize {
            maps.resize_with(shape_id.0 as usize + 1, || None);
        }
        if let Some(Some(existing)) = maps.get(shape_id.0 as usize) {
            return Some(existing.clone());
        }
        maps[shape_id.0 as usize] = Some(map.clone());
        Some(map)
    }

    pub fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.function_shapes
            .read()
            .expect("function_shapes lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
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
        self.callable_shapes
            .read()
            .expect("callable_shapes lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
                Arc::new(CallableShape {
                    call_signatures: Vec::new(),
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    ..Default::default()
                })
            })
    }

    pub fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.conditional_types
            .read()
            .expect("conditional_types lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
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
        self.mapped_types
            .read()
            .expect("mapped_types lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
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
        self.applications
            .read()
            .expect("applications lock poisoned")
            .get(id.0)
            .unwrap_or_else(|| {
                Arc::new(TypeApplication {
                    base: TypeId::ERROR,
                    args: Vec::new(),
                })
            })
    }

    /// Intern a type key and return its TypeId.
    /// If the key already exists, returns the existing TypeId.
    /// Otherwise, creates a new TypeId and stores the key.
    pub fn intern(&self, key: TypeKey) -> TypeId {
        if let Some(id) = self.get_intrinsic_id(&key) {
            return id;
        }

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];

        {
            let map = shard.key_to_index.read().expect("shard key_to_index lock poisoned");
            if let Some(&local_index) = map.get(&key) {
                return self.make_id(local_index, shard_idx as u32);
            }
        }

        let mut map = shard.key_to_index.write().expect("shard key_to_index lock poisoned");
        let mut storage = shard.index_to_key.write().expect("shard index_to_key lock poisoned");

        if let Some(&local_index) = map.get(&key) {
            return self.make_id(local_index, shard_idx as u32);
        }

        let local_index = storage.len() as u32;
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Return error type instead of panicking
            return TypeId::ERROR;
        }

        storage.push(key.clone());
        map.insert(key, local_index);

        self.make_id(local_index, shard_idx as u32)
    }

    /// Look up the TypeKey for a given TypeId
    pub fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        if id.is_intrinsic() || id.is_error() {
            return self.get_intrinsic_key(id);
        }

        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;

        let shard = self.shards.get(shard_idx)?;
        let storage = shard.index_to_key.read().expect("shard index_to_key lock poisoned");
        storage.get(local_index as usize).cloned()
    }

    fn intern_type_list(&self, members: Vec<TypeId>) -> TypeListId {
        let mut lists = self.type_lists.write().expect("type_lists lock poisoned");
        TypeListId(lists.intern(members))
    }

    fn intern_tuple_list(&self, elements: Vec<TupleElement>) -> TupleListId {
        let mut lists = self.tuple_lists.write().expect("tuple_lists lock poisoned");
        TupleListId(lists.intern(elements))
    }

    fn intern_template_list(&self, spans: Vec<TemplateSpan>) -> TemplateLiteralId {
        let mut lists = self.template_lists.write().expect("template_lists lock poisoned");
        TemplateLiteralId(lists.intern(spans))
    }

    fn intern_object_shape(&self, shape: ObjectShape) -> ObjectShapeId {
        let mut shapes = self.object_shapes.write().expect("object_shapes lock poisoned");
        ObjectShapeId(shapes.intern(shape))
    }

    fn intern_function_shape(&self, shape: FunctionShape) -> FunctionShapeId {
        let mut shapes = self.function_shapes.write().expect("function_shapes lock poisoned");
        FunctionShapeId(shapes.intern(shape))
    }

    fn intern_callable_shape(&self, shape: CallableShape) -> CallableShapeId {
        let mut shapes = self.callable_shapes.write().expect("callable_shapes lock poisoned");
        CallableShapeId(shapes.intern(shape))
    }

    fn intern_conditional_type(&self, conditional: ConditionalType) -> ConditionalTypeId {
        let mut types = self.conditional_types.write().expect("conditional_types lock poisoned");
        ConditionalTypeId(types.intern(conditional))
    }

    fn intern_mapped_type(&self, mapped: MappedType) -> MappedTypeId {
        let mut types = self.mapped_types.write().expect("mapped_types lock poisoned");
        MappedTypeId(types.intern(mapped))
    }

    fn intern_application(&self, application: TypeApplication) -> TypeApplicationId {
        let mut apps = self.applications.write().expect("applications lock poisoned");
        TypeApplicationId(apps.intern(application))
    }

    /// Get the number of interned types
    pub fn len(&self) -> usize {
        let mut total = TypeId::FIRST_USER as usize;
        for shard in &self.shards {
            total += shard.index_to_key.read().expect("shard index_to_key lock poisoned").len();
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
                        existing.write_type = self.intersection2(existing.write_type, prop.write_type);
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
                (Some(idx), None) => merged_string_index = Some(IndexSignature {
                    key_type: idx.key_type,
                    value_type: idx.value_type,
                    readonly: idx.readonly,
                }),
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
                (Some(idx), None) => merged_number_index = Some(IndexSignature {
                    key_type: idx.key_type,
                    value_type: idx.value_type,
                    readonly: idx.readonly,
                }),
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
        Some(self.intern(TypeKey::Object(shape_id)))
    }

    fn intersection_has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        let mut class: Option<PrimitiveClass> = None;
        let mut has_primitive = false;
        let mut has_non_primitive = false;
        let mut literals: smallvec::SmallVec<[TypeId; 4]> = SmallVec::new();

        for &member in members {
            let Some(member_class) = self.primitive_class_for(member) else {
                // Not a primitive - check if it's an object-like type
                has_non_primitive = self.is_object_like_type(member);
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
        let mut has_symbol = false;

        // First pass: identify which primitive types are present
        for &type_id in flat.iter() {
            match type_id {
                TypeId::STRING => has_string = true,
                TypeId::NUMBER => has_number = true,
                TypeId::BOOLEAN => has_boolean = true,
                TypeId::BIGINT => has_bigint = true,
                TypeId::SYMBOL => has_symbol = true,
                _ => {
                    if let Some(TypeKey::Intrinsic(kind)) = self.lookup(type_id) {
                        match kind {
                            IntrinsicKind::String => has_string = true,
                            IntrinsicKind::Number => has_number = true,
                            IntrinsicKind::Boolean => has_boolean = true,
                            IntrinsicKind::Bigint => has_bigint = true,
                            IntrinsicKind::Symbol => has_symbol = true,
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
