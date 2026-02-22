//! Cached query database implementation for the solver.
//!
//! `QueryCache` wraps a `TypeInterner` with memoization for evaluation,
//! relation, property, and element access queries. This is the concrete
//! database implementation used by the checker at runtime.

use crate::caches::db::{QueryDatabase, TypeDatabase};
use crate::caches::query_trace;
use crate::def::DefId;
use crate::element_access::ElementAccessResult;
use crate::intern::TypeInterner;
use crate::operations::property::PropertyAccessResult;
use crate::relations::compat::CompatChecker;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, StringIntrinsicKind, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

type EvalCacheKey = (TypeId, bool);
type ApplicationEvalCacheKey = (DefId, Vec<TypeId>, bool);
type ElementAccessTypeCacheKey = (TypeId, TypeId, Option<u32>, bool);
type PropertyAccessCacheKey = (TypeId, Atom, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationCacheProbe {
    Hit(bool),
    MissNotCached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationCacheStats {
    pub subtype_hits: u64,
    pub subtype_misses: u64,
    pub subtype_entries: usize,
    pub assignability_hits: u64,
    pub assignability_misses: u64,
    pub assignability_entries: usize,
}

/// Query database wrapper with basic caching.
pub struct QueryCache<'a> {
    interner: &'a TypeInterner,
    eval_cache: RwLock<FxHashMap<EvalCacheKey, TypeId>>,
    application_eval_cache: RwLock<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    element_access_cache: RwLock<FxHashMap<ElementAccessTypeCacheKey, TypeId>>,
    object_spread_properties_cache: RwLock<FxHashMap<TypeId, Vec<PropertyInfo>>>,
    subtype_cache: RwLock<FxHashMap<RelationCacheKey, bool>>,
    /// CRITICAL: Separate cache for assignability to prevent cache poisoning.
    /// This ensures that loose assignability results (e.g., any is assignable to number)
    /// don't contaminate strict subtype checks.
    assignability_cache: RwLock<FxHashMap<RelationCacheKey, bool>>,
    property_cache: RwLock<FxHashMap<PropertyAccessCacheKey, PropertyAccessResult>>,
    /// Task #41: Variance cache for generic type parameters.
    /// Stores computed variance masks for `DefIds` to enable O(1) generic assignability.
    variance_cache: RwLock<FxHashMap<DefId, Arc<[Variance]>>>,
    /// Task #49: Canonical cache for O(1) structural identity checks.
    /// Maps `TypeId` -> canonical `TypeId` for structurally identical types.
    canonical_cache: RwLock<FxHashMap<TypeId, TypeId>>,
    subtype_cache_hits: AtomicU64,
    subtype_cache_misses: AtomicU64,
    assignability_cache_hits: AtomicU64,
    assignability_cache_misses: AtomicU64,
    no_unchecked_indexed_access: AtomicBool,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        QueryCache {
            interner,
            eval_cache: RwLock::new(FxHashMap::default()),
            application_eval_cache: RwLock::new(FxHashMap::default()),
            element_access_cache: RwLock::new(FxHashMap::default()),
            object_spread_properties_cache: RwLock::new(FxHashMap::default()),
            subtype_cache: RwLock::new(FxHashMap::default()),
            assignability_cache: RwLock::new(FxHashMap::default()),
            property_cache: RwLock::new(FxHashMap::default()),
            variance_cache: RwLock::new(FxHashMap::default()),
            canonical_cache: RwLock::new(FxHashMap::default()),
            subtype_cache_hits: AtomicU64::new(0),
            subtype_cache_misses: AtomicU64::new(0),
            assignability_cache_hits: AtomicU64::new(0),
            assignability_cache_misses: AtomicU64::new(0),
            no_unchecked_indexed_access: AtomicBool::new(false),
        }
    }

    pub fn clear(&self) {
        // Handle poisoned locks gracefully - if poisoned, clear the cache anyway
        match self.eval_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.element_access_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.application_eval_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.object_spread_properties_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.subtype_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.assignability_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.property_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.variance_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        match self.canonical_cache.write() {
            Ok(mut cache) => cache.clear(),
            Err(e) => e.into_inner().clear(),
        }
        self.reset_relation_cache_stats();
    }

    pub fn relation_cache_stats(&self) -> RelationCacheStats {
        let subtype_entries = match self.subtype_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        };
        let assignability_entries = match self.assignability_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        };
        RelationCacheStats {
            subtype_hits: self.subtype_cache_hits.load(Ordering::Relaxed),
            subtype_misses: self.subtype_cache_misses.load(Ordering::Relaxed),
            subtype_entries,
            assignability_hits: self.assignability_cache_hits.load(Ordering::Relaxed),
            assignability_misses: self.assignability_cache_misses.load(Ordering::Relaxed),
            assignability_entries,
        }
    }

    pub fn reset_relation_cache_stats(&self) {
        self.subtype_cache_hits.store(0, Ordering::Relaxed);
        self.subtype_cache_misses.store(0, Ordering::Relaxed);
        self.assignability_cache_hits.store(0, Ordering::Relaxed);
        self.assignability_cache_misses.store(0, Ordering::Relaxed);
    }

    pub fn probe_subtype_cache(&self, key: RelationCacheKey) -> RelationCacheProbe {
        match self.lookup_subtype_cache(key) {
            Some(result) => RelationCacheProbe::Hit(result),
            None => RelationCacheProbe::MissNotCached,
        }
    }

    pub fn probe_assignability_cache(&self, key: RelationCacheKey) -> RelationCacheProbe {
        match self.lookup_assignability_cache(key) {
            Some(result) => RelationCacheProbe::Hit(result),
            None => RelationCacheProbe::MissNotCached,
        }
    }

    /// Helper to check a cache with poisoned lock handling.
    fn check_cache(
        &self,
        cache: &RwLock<FxHashMap<RelationCacheKey, bool>>,
        key: RelationCacheKey,
    ) -> Option<bool> {
        match cache.read() {
            Ok(cached) => cached.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        }
    }

    /// Helper to insert into a cache with poisoned lock handling.
    fn insert_cache(
        &self,
        cache: &RwLock<FxHashMap<RelationCacheKey, bool>>,
        key: RelationCacheKey,
        result: bool,
    ) {
        match cache.write() {
            Ok(mut c) => {
                c.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
    }

    fn check_property_cache(&self, key: PropertyAccessCacheKey) -> Option<PropertyAccessResult> {
        match self.property_cache.read() {
            Ok(cache) => cache.get(&key).cloned(),
            Err(e) => e.into_inner().get(&key).cloned(),
        }
    }

    fn insert_property_cache(&self, key: PropertyAccessCacheKey, result: PropertyAccessResult) {
        match self.property_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
    }

    fn check_element_access_cache(&self, key: ElementAccessTypeCacheKey) -> Option<TypeId> {
        match self.element_access_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        }
    }

    fn insert_element_access_cache(&self, key: ElementAccessTypeCacheKey, result: TypeId) {
        match self.element_access_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
    }

    fn check_application_eval_cache(&self, key: ApplicationEvalCacheKey) -> Option<TypeId> {
        match self.application_eval_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        }
    }

    fn insert_application_eval_cache(&self, key: ApplicationEvalCacheKey, result: TypeId) {
        match self.application_eval_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
    }

    fn check_object_spread_properties_cache(&self, key: TypeId) -> Option<Vec<PropertyInfo>> {
        match self.object_spread_properties_cache.read() {
            Ok(cache) => cache.get(&key).cloned(),
            Err(e) => e.into_inner().get(&key).cloned(),
        }
    }

    fn insert_object_spread_properties_cache(&self, key: TypeId, value: Vec<PropertyInfo>) {
        match self.object_spread_properties_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, value);
            }
            Err(e) => {
                e.into_inner().insert(key, value);
            }
        }
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

        match key {
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
            _ => Vec::new(),
        }
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

    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId> {
        // Delegate to the interner
        self.interner.get_class_base_type(symbol_id)
    }

    fn is_unit_type(&self, type_id: TypeId) -> bool {
        self.interner.is_unit_type(type_id)
    }
}

/// Implement `TypeResolver` for `QueryCache` with default noop implementations.
///
/// `QueryCache` doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or `DefIds`. This implementation
/// returns None for all resolution operations.
impl TypeResolver for QueryCache<'_> {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_type_params(&self, _symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        None
    }

    fn get_lazy_type_params(&self, _def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        None
    }

    fn def_to_symbol_id(&self, _def_id: DefId) -> Option<SymbolId> {
        None
    }

    fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
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

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.interner.set_array_base_type(type_id, type_params);
    }

    fn register_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        self.interner.set_boxed_type(kind, type_id);
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_options(type_id, self.no_unchecked_indexed_access())
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
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
        let key = (type_id, no_unchecked_indexed_access);
        // Handle poisoned locks gracefully
        let cached = match self.eval_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            if let Some(query_id) = trace_query_id {
                query_trace::unary_end(query_id, "evaluate_type_with_options", result, true);
            }
            return result;
        }

        let mut evaluator =
            crate::evaluation::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator = evaluator.with_query_db(self);
        let result = evaluator.evaluate(type_id);
        match self.eval_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
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
        self.check_application_eval_cache((def_id, args.to_vec(), no_unchecked_indexed_access))
    }

    fn insert_application_eval_cache(
        &self,
        def_id: DefId,
        args: &[TypeId],
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        self.insert_application_eval_cache(
            (def_id, args.to_vec(), no_unchecked_indexed_access),
            result,
        );
    }

    fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
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
        // Handle poisoned locks gracefully
        let cached = match self.subtype_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            if let Some(query_id) = trace_query_id {
                query_trace::relation_end(query_id, "is_subtype_of_with_flags", result, true);
            }
            return result;
        }

        let result = crate::relations::subtype::is_subtype_of_with_flags(
            self.as_type_database(),
            source,
            target,
            flags,
        );
        match self.subtype_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
        if let Some(query_id) = trace_query_id {
            query_trace::relation_end(query_id, "is_subtype_of_with_flags", result, false);
        }
        result
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
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
            if let Some(query_id) = trace_query_id {
                query_trace::relation_end(query_id, "is_assignable_to_with_flags", result, true);
            }
            return result;
        }

        // Use CompatChecker with all compatibility rules
        let mut checker = CompatChecker::new(self.as_type_database());

        // FIX: Apply flags to ensure checker matches the cache key configuration
        // This prevents cache poisoning where results from non-strict checks
        // leak into strict checks (Gap C fix)
        checker.apply_flags(flags);

        let result = checker.is_assignable(source, target);

        self.insert_cache(&self.assignability_cache, key, result);
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
        let result = match self.subtype_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };
        if result.is_some() {
            self.subtype_cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.subtype_cache_misses.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    fn insert_subtype_cache(&self, key: RelationCacheKey, result: bool) {
        match self.subtype_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
    }

    fn lookup_assignability_cache(&self, key: RelationCacheKey) -> Option<bool> {
        let result = match self.assignability_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };
        if result.is_some() {
            self.assignability_cache_hits
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.assignability_cache_misses
                .fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    fn insert_assignability_cache(&self, key: RelationCacheKey, result: bool) {
        match self.assignability_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
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
        self.insert_property_cache(key, result.clone());
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
        self.no_unchecked_indexed_access.load(Ordering::Relaxed)
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access
            .store(enabled, Ordering::Relaxed);
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        // 1. Check cache first (lock-free read)
        if let Ok(cache) = self.variance_cache.read()
            && let Some(cached) = cache.get(&def_id)
        {
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
        match self.variance_cache.write() {
            Ok(mut cache) => {
                cache.insert(def_id, Arc::clone(&result));
            }
            Err(e) => {
                e.into_inner().insert(def_id, Arc::clone(&result));
            }
        }

        Some(result)
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // Check cache first
        let cached = match self.canonical_cache.read() {
            Ok(cache) => cache.get(&type_id).copied(),
            Err(e) => e.into_inner().get(&type_id).copied(),
        };

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
        match self.canonical_cache.write() {
            Ok(mut cache) => {
                cache.insert(type_id, canonical);
            }
            Err(e) => {
                e.into_inner().insert(type_id, canonical);
            }
        }

        canonical
    }
}

#[cfg(test)]
#[path = "../../tests/db_tests.rs"]
mod tests;
