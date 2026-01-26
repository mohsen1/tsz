//! Type database abstraction for the solver.
//!
//! This trait isolates solver logic from concrete storage so we can
//! swap in a query system (e.g., Salsa) without touching core logic.

use crate::interner::Atom;
use crate::solver::element_access::{ElementAccessEvaluator, ElementAccessResult};
use crate::solver::intern::TypeInterner;
use crate::solver::narrowing;
use crate::solver::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, MappedType, MappedTypeId, ObjectShape, ObjectShapeId, PropertyInfo,
    PropertyLookup, SymbolRef, TemplateLiteralId, TemplateSpan, TupleElement, TupleListId,
    TypeApplication, TypeApplicationId, TypeId, TypeKey, TypeListId,
};
use rustc_hash::FxHashMap;
use std::sync::{Arc, RwLock};

/// Query interface for the solver.
///
/// This keeps solver components generic and prevents them from reaching
/// into concrete storage structures directly.
pub trait TypeDatabase {
    fn intern(&self, key: TypeKey) -> TypeId;
    fn lookup(&self, id: TypeId) -> Option<TypeKey>;
    fn intern_string(&self, s: &str) -> Atom;
    fn resolve_atom(&self, atom: Atom) -> String;
    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str>;
    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]>;
    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]>;
    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]>;
    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape>;
    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup;
    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape>;
    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape>;
    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType>;
    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType>;
    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication>;

    fn literal_string(&self, value: &str) -> TypeId;
    fn literal_number(&self, value: f64) -> TypeId;
    fn literal_boolean(&self, value: bool) -> TypeId;
    fn literal_bigint(&self, value: &str) -> TypeId;
    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId;

    fn union(&self, members: Vec<TypeId>) -> TypeId;
    fn union2(&self, left: TypeId, right: TypeId) -> TypeId;
    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId;
    fn intersection(&self, members: Vec<TypeId>) -> TypeId;
    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId;
    fn array(&self, element: TypeId) -> TypeId;
    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId;
    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId;
    fn object_with_index(&self, shape: ObjectShape) -> TypeId;
    fn function(&self, shape: FunctionShape) -> TypeId;
    fn callable(&self, shape: CallableShape) -> TypeId;
    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId;
    fn conditional(&self, conditional: ConditionalType) -> TypeId;
    fn mapped(&self, mapped: MappedType) -> TypeId;
    fn reference(&self, symbol: SymbolRef) -> TypeId;
    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId;
}

impl TypeDatabase for TypeInterner {
    fn intern(&self, key: TypeKey) -> TypeId {
        TypeInterner::intern(self, key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        TypeInterner::lookup(self, id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        TypeInterner::intern_string(self, s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        TypeInterner::resolve_atom(self, atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        TypeInterner::resolve_atom_ref(self, atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        TypeInterner::type_list(self, id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        TypeInterner::tuple_list(self, id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        TypeInterner::template_list(self, id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        TypeInterner::object_shape(self, id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        TypeInterner::object_property_index(self, shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        TypeInterner::function_shape(self, id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        TypeInterner::callable_shape(self, id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        TypeInterner::conditional_type(self, id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        TypeInterner::mapped_type(self, id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        TypeInterner::type_application(self, id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        TypeInterner::literal_string(self, value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        TypeInterner::literal_number(self, value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        TypeInterner::literal_boolean(self, value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        TypeInterner::literal_bigint(self, value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        TypeInterner::literal_bigint_with_sign(self, negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        TypeInterner::union(self, members)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        TypeInterner::union2(self, left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        TypeInterner::union3(self, first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        TypeInterner::intersection(self, members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        TypeInterner::intersection2(self, left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        TypeInterner::array(self, element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        TypeInterner::tuple(self, elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        TypeInterner::object(self, properties)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        TypeInterner::object_with_index(self, shape)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        TypeInterner::function(self, shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        TypeInterner::callable(self, shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        TypeInterner::template_literal(self, spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        TypeInterner::conditional(self, conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        TypeInterner::mapped(self, mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        TypeInterner::reference(self, symbol)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        TypeInterner::application(self, base, args)
    }
}

/// Query layer for higher-level solver operations.
///
/// This is the incremental boundary where caching and (future) salsa hooks live.
pub trait QueryDatabase: TypeDatabase {
    /// Expose the underlying TypeDatabase view for legacy entry points.
    fn as_type_database(&self) -> &dyn TypeDatabase;

    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        crate::solver::evaluate::evaluate_conditional(self.as_type_database(), cond)
    }

    fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        crate::solver::evaluate::evaluate_index_access(
            self.as_type_database(),
            object_type,
            index_type,
        )
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        crate::solver::evaluate::evaluate_index_access_with_options(
            self.as_type_database(),
            object_type,
            index_type,
            no_unchecked_indexed_access,
        )
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        crate::solver::evaluate::evaluate_type(self.as_type_database(), type_id)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        crate::solver::evaluate::evaluate_mapped(self.as_type_database(), mapped)
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        crate::solver::evaluate::evaluate_keyof(self.as_type_database(), operand)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::solver::PropertyAccessResult {
        let evaluator =
            crate::solver::operations::PropertyAccessEvaluator::new(self.as_type_database());
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn property_access_type(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::solver::PropertyAccessResult {
        self.resolve_property_access(object_type, prop_name)
    }

    fn contextual_property_type(&self, expected: TypeId, prop_name: &str) -> Option<TypeId> {
        let ctx =
            crate::solver::ContextualTypeContext::with_expected(self.as_type_database(), expected);
        ctx.get_property_type(prop_name)
    }

    fn is_property_readonly(&self, object_type: TypeId, prop_name: &str) -> bool {
        crate::solver::operations::property_is_readonly(
            self.as_type_database(),
            object_type,
            prop_name,
        )
    }

    fn is_readonly_index_signature(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        crate::solver::operations::is_readonly_index_signature(
            self.as_type_database(),
            object_type,
            wants_string,
            wants_number,
        )
    }

    /// Resolve element access (array/tuple indexing) with detailed error reporting
    fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        let evaluator = ElementAccessEvaluator::new(self.as_type_database());
        evaluator.resolve_element_access(object_type, index_type, literal_index)
    }

    /// Get index signatures for a type
    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo;

    /// Check if a type contains null or undefined
    fn is_nullish_type(&self, type_id: TypeId) -> bool;

    /// Remove null and undefined from a type
    fn remove_nullish(&self, type_id: TypeId) -> TypeId;

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        crate::solver::subtype::is_subtype_of(self.as_type_database(), source, target)
    }

    fn new_inference_context(&self) -> crate::solver::infer::InferenceContext<'_> {
        crate::solver::infer::InferenceContext::new(self.as_type_database())
    }
}

impl QueryDatabase for TypeInterner {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        match self.lookup(type_id) {
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.object_shape(shape_id);
                IndexInfo {
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                }
            }
            Some(TypeKey::Array(element)) => {
                // Arrays have number index signature with element type
                IndexInfo {
                    string_index: None,
                    number_index: Some(crate::solver::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: element,
                        readonly: false,
                    }),
                }
            }
            Some(TypeKey::Tuple(elements_id)) => {
                // Tuples have number index signature with union of element types
                let elements = self.tuple_list(elements_id);
                let element_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                let value_type = if element_types.is_empty() {
                    TypeId::UNDEFINED
                } else if element_types.len() == 1 {
                    element_types[0]
                } else {
                    self.union(element_types)
                };
                IndexInfo {
                    string_index: None,
                    number_index: Some(crate::solver::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                    }),
                }
            }
            Some(TypeKey::Union(members_id)) => {
                // For unions, collect index signatures from all members
                let members = self.type_list(members_id);
                let mut string_indices = Vec::new();
                let mut number_indices = Vec::new();

                for &member in members.iter() {
                    let info = self.get_index_signatures(member);
                    if let Some(sig) = info.string_index {
                        string_indices.push(sig);
                    }
                    if let Some(sig) = info.number_index {
                        number_indices.push(sig);
                    }
                }

                // Union of the value types
                let string_index = if string_indices.is_empty() {
                    None
                } else {
                    Some(crate::solver::types::IndexSignature {
                        key_type: TypeId::STRING,
                        value_type: self
                            .union(string_indices.iter().map(|s| s.value_type).collect()),
                        readonly: string_indices.iter().all(|s| s.readonly),
                    })
                };

                let number_index = if number_indices.is_empty() {
                    None
                } else {
                    Some(crate::solver::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: self
                            .union(number_indices.iter().map(|s| s.value_type).collect()),
                        readonly: number_indices.iter().all(|s| s.readonly),
                    })
                };

                IndexInfo {
                    string_index,
                    number_index,
                }
            }
            Some(TypeKey::Intersection(members_id)) => {
                // For intersections, combine index signatures
                let members = self.type_list(members_id);
                let mut string_index = None;
                let mut number_index = None;

                for &member in members.iter() {
                    let info = self.get_index_signatures(member);
                    if let Some(sig) = info.string_index {
                        string_index = Some(sig);
                    }
                    if let Some(sig) = info.number_index {
                        number_index = Some(sig);
                    }
                }

                IndexInfo {
                    string_index,
                    number_index,
                }
            }
            _ => IndexInfo::default(),
        }
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        narrowing::is_nullish_type(self, type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        narrowing::remove_nullish(self, type_id)
    }
}

/// Query database wrapper with basic caching.
pub struct QueryCache<'a> {
    interner: &'a TypeInterner,
    eval_cache: RwLock<FxHashMap<TypeId, TypeId>>,
    subtype_cache: RwLock<FxHashMap<(TypeId, TypeId), bool>>,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        QueryCache {
            interner,
            eval_cache: RwLock::new(FxHashMap::default()),
            subtype_cache: RwLock::new(FxHashMap::default()),
        }
    }

    pub fn clear(&self) {
        // Handle poisoned locks gracefully - if poisoned, clear the cache anyway
        match self.eval_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.subtype_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
    }

    #[cfg(test)]
    pub fn eval_cache_len(&self) -> usize {
        match self.eval_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    #[cfg(test)]
    pub fn subtype_cache_len(&self) -> usize {
        match self.subtype_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }
}

impl TypeDatabase for QueryCache<'_> {
    fn intern(&self, key: TypeKey) -> TypeId {
        self.interner.intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeKey> {
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

    fn array(&self, element: TypeId) -> TypeId {
        self.interner.array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.interner.tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.interner.object(properties)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.interner.object_with_index(shape)
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

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.interner.application(base, args)
    }
}

impl QueryDatabase for QueryCache<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        // Handle poisoned locks gracefully
        let cached = match self.eval_cache.read() {
            Ok(cache) => cache.get(&type_id).copied(),
            Err(e) => e.into_inner().get(&type_id).copied(),
        };

        if let Some(result) = cached {
            return result;
        }

        let result = crate::solver::evaluate::evaluate_type(self.as_type_database(), type_id);
        match self.eval_cache.write() {
            Ok(mut cache) => {
                cache.insert(type_id, result);
            }
            Err(e) => {
                e.into_inner().insert(type_id, result);
            }
        }
        result
    }

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        let key = (source, target);
        // Handle poisoned locks gracefully
        let cached = match self.subtype_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            return result;
        }

        let result = crate::solver::subtype::is_subtype_of(self.as_type_database(), source, target);
        match self.subtype_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
        result
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
        // Delegate to the interner
        self.interner.remove_nullish(type_id)
    }
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
