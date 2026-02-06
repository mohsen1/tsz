//! Type database abstraction for the solver.
//!
//! This trait isolates solver logic from concrete storage so we can
//! swap in a query system (e.g., Salsa) without touching core logic.

use crate::def::DefId;
use crate::element_access::{ElementAccessEvaluator, ElementAccessResult};
use crate::intern::TypeInterner;
use crate::narrowing;
use crate::operations_property::PropertyAccessResult;
use crate::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, SymbolRef, TemplateLiteralId,
    TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId, TypeId, TypeKey,
    TypeListId, TypeParamInfo, Variance,
};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

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
    /// Raw intersection without normalization (used to avoid infinite recursion)
    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId;
    fn array(&self, element: TypeId) -> TypeId;
    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId;
    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId;
    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId;
    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId;
    fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::FRESH_LITERAL)
    }
    fn object_with_index(&self, shape: ObjectShape) -> TypeId;
    fn function(&self, shape: FunctionShape) -> TypeId;
    fn callable(&self, shape: CallableShape) -> TypeId;
    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId;
    fn conditional(&self, conditional: ConditionalType) -> TypeId;
    fn mapped(&self, mapped: MappedType) -> TypeId;
    fn reference(&self, symbol: SymbolRef) -> TypeId;
    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId;

    fn literal_string_atom(&self, atom: Atom) -> TypeId;
    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId;
    fn readonly_type(&self, inner: TypeId) -> TypeId;

    /// Get the base class type for a symbol (class/interface).
    /// Returns the TypeId of the extends clause, or None if the symbol doesn't extend anything.
    /// This is used by the BCT algorithm to find common base classes.
    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId>;

    /// Check if a type is a "unit type" (represents exactly one value).
    /// Unit types include literals, enum members, unique symbols, null, undefined, void, never,
    /// and tuples composed entirely of unit types.
    /// Results are cached for O(1) lookup after first computation.
    fn is_unit_type(&self, type_id: TypeId) -> bool;
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

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        TypeInterner::intersect_types_raw2(self, left, right)
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

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        TypeInterner::object_with_flags(self, properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        TypeInterner::object_with_flags_and_symbol(self, properties, flags, symbol)
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

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        TypeInterner::literal_string_atom(self, atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        TypeInterner::union_preserve_members(self, members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        TypeInterner::readonly_type(self, inner)
    }

    fn get_class_base_type(&self, _symbol_id: SymbolId) -> Option<TypeId> {
        // TypeInterner doesn't have access to the Binder, so it can't resolve base classes.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn is_unit_type(&self, type_id: TypeId) -> bool {
        TypeInterner::is_unit_type(self, type_id)
    }
}

/// Implement TypeResolver for TypeInterner with default noop implementations.
///
/// TypeInterner doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or DefIds. This implementation
/// returns None for all resolution operations, which is the correct behavior
/// for contexts that don't have access to type binding information.
impl TypeResolver for TypeInterner {
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

    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.get_array_base_type_params()
    }
}

/// Query layer for higher-level solver operations.
///
/// This is the incremental boundary where caching and (future) salsa hooks live.
/// Inherits from TypeResolver to enable Lazy/Ref type resolution through evaluate_type().
pub trait QueryDatabase: TypeDatabase + TypeResolver {
    /// Expose the underlying TypeDatabase view for legacy entry points.
    fn as_type_database(&self) -> &dyn TypeDatabase;

    /// Register the canonical `Array<T>` base type used by property access resolution.
    ///
    /// Some call paths resolve properties through a `TypeInterner`-backed database,
    /// while others use a `TypeEnvironment`-backed resolver. Implementations should
    /// store this in whichever backing stores they use so `T[]` methods/properties
    /// (e.g. `push`, `length`) resolve consistently.
    fn register_array_base_type(&self, _type_id: TypeId, _type_params: Vec<TypeParamInfo>) {}

    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        crate::evaluate::evaluate_conditional(self.as_type_database(), cond)
    }

    fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.evaluate_index_access_with_options(
            object_type,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        crate::evaluate::evaluate_index_access_with_options(
            self.as_type_database(),
            object_type,
            index_type,
            no_unchecked_indexed_access,
        )
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        crate::evaluate::evaluate_type(self.as_type_database(), type_id)
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        if !no_unchecked_indexed_access {
            return self.evaluate_type(type_id);
        }

        let mut evaluator = crate::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.evaluate(type_id)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        crate::evaluate::evaluate_mapped(self.as_type_database(), mapped)
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        crate::evaluate::evaluate_keyof(self.as_type_database(), operand)
    }

    fn narrow(&self, type_id: TypeId, narrower: TypeId) -> TypeId
    where
        Self: Sized,
    {
        crate::narrowing::NarrowingContext::new(self).narrow(type_id, narrower)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations_property::PropertyAccessResult;

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations_property::PropertyAccessResult;

    fn property_access_type(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations_property::PropertyAccessResult {
        self.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.no_unchecked_indexed_access(),
        )
    }

    fn no_unchecked_indexed_access(&self) -> bool {
        false
    }

    fn set_no_unchecked_indexed_access(&self, _enabled: bool) {}

    fn contextual_property_type(&self, expected: TypeId, prop_name: &str) -> Option<TypeId> {
        let ctx = crate::ContextualTypeContext::with_expected(self.as_type_database(), expected);
        ctx.get_property_type(prop_name)
    }

    fn is_property_readonly(&self, object_type: TypeId, prop_name: &str) -> bool {
        crate::operations_property::property_is_readonly(
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
        crate::operations_property::is_readonly_index_signature(
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

    /// Get the canonical TypeId for a type, achieving O(1) structural identity checks.
    ///
    /// This memoizes the Canonicalizer output so that structurally identical types
    /// (e.g., `type A = Box<Box<string>>` and `type B = Box<Box<string>>`) return
    /// the same canonical TypeId.
    ///
    /// The implementation must:
    /// - Use a fresh Canonicalizer with empty stacks (for absolute De Bruijn indices)
    /// - Only expand TypeAlias (DefKind::TypeAlias), preserving nominal types
    /// - Cache the result for O(1) subsequent lookups
    ///
    /// Task #49: Global Canonical Mapping
    fn canonical_id(&self, type_id: TypeId) -> TypeId;

    /// Subtype check with compiler flags.
    ///
    /// The `flags` parameter is a packed u16 bitmask matching `RelationCacheKey.flags`:
    /// - bit 0: strict_null_checks
    /// - bit 1: strict_function_types
    /// - bit 2: exact_optional_property_types
    /// - bit 3: no_unchecked_indexed_access
    /// - bit 4: disable_method_bivariance
    /// - bit 5: allow_void_return
    /// - bit 6: allow_bivariant_rest
    /// - bit 7: allow_bivariant_param_count
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        // Individual callers can use is_subtype_of_with_flags for explicit flag control
        self.is_subtype_of_with_flags(source, target, 0)
    }

    /// Subtype check with explicit compiler flags.
    ///
    /// The `flags` parameter is a packed u16 bitmask matching `RelationCacheKey.flags`.
    fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        // Default implementation: use SubtypeChecker with default flags
        // (This will be overridden by QueryCache with proper caching)
        crate::subtype::is_subtype_of_with_flags(self.as_type_database(), source, target, flags)
    }

    /// TypeScript assignability check with full compatibility rules (The Lawyer).
    ///
    /// This is distinct from `is_subtype_of`:
    /// - `is_subtype_of` = Strict structural subtyping (The Judge) - for internal solver use
    /// - `is_assignable_to` = Loose with TS rules (The Lawyer) - for Checker diagnostics
    ///
    /// The Lawyer handles:
    /// - Any type propagation (any is assignable to/from everything)
    /// - Legacy null/undefined assignability (without strictNullChecks)
    /// - Weak type detection (excess property checking)
    /// - Empty object accepts any non-nullish value
    /// - Function bivariance (when not in strictFunctionTypes mode)
    ///
    /// Uses separate cache from `is_subtype_of` to prevent cache poisoning.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        // Individual callers can use is_assignable_to_with_flags for explicit flag control
        self.is_assignable_to_with_flags(source, target, 0)
    }

    /// Assignability check with explicit compiler flags.
    ///
    /// The `flags` parameter is a packed u16 bitmask matching `RelationCacheKey.flags`.
    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool;

    /// Look up a cached subtype result for the given key.
    /// Returns `None` if the result is not cached.
    /// Default implementation returns `None` (no caching).
    fn lookup_subtype_cache(&self, _key: RelationCacheKey) -> Option<bool> {
        None
    }

    /// Cache a subtype result for the given key.
    /// Default implementation is a no-op.
    fn insert_subtype_cache(&self, _key: RelationCacheKey, _result: bool) {}

    fn new_inference_context(&self) -> crate::infer::InferenceContext<'_> {
        crate::infer::InferenceContext::new(self.as_type_database())
    }

    /// Task #41: Get the variance mask for a generic type definition.
    ///
    /// Returns the variance of each type parameter for the given DefId.
    /// Returns None if the DefId is not a generic type or variance cannot be determined.
    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>>;
}

impl QueryDatabase for TypeInterner {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.set_array_base_type(type_id, type_params);
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
                    number_index: Some(crate::types::IndexSignature {
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
                    number_index: Some(crate::types::IndexSignature {
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
                    Some(crate::types::IndexSignature {
                        key_type: TypeId::STRING,
                        value_type: self
                            .union(string_indices.iter().map(|s| s.value_type).collect()),
                        readonly: string_indices.iter().all(|s| s.readonly),
                    })
                };

                let number_index = if number_indices.is_empty() {
                    None
                } else {
                    Some(crate::types::IndexSignature {
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

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        self.is_assignable_to_with_flags(source, target, 0)
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, _flags: u16) -> bool {
        // Default implementation: use CompatChecker
        // TODO: Apply flags to CompatChecker once it supports apply_flags
        use crate::compat::CompatChecker;
        let mut checker = CompatChecker::new(self);
        checker.is_assignable(source, target)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations_property::PropertyAccessResult {
        // TypeInterner doesn't have TypeResolver capability, so it can't resolve Lazy types
        // Use PropertyAccessEvaluator with QueryDatabase (self implements both TypeDatabase and TypeResolver)
        let evaluator = crate::operations_property::PropertyAccessEvaluator::new(self);
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations_property::PropertyAccessResult {
        let mut evaluator = crate::operations_property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn get_type_param_variance(&self, _def_id: DefId) -> Option<Arc<[Variance]>> {
        // TypeInterner doesn't have access to type parameter information.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // TypeInterner doesn't have caching, so compute directly
        use crate::canonicalize::Canonicalizer;
        let mut canon = Canonicalizer::new(self, self);
        canon.canonicalize(type_id)
    }
}

type EvalCacheKey = (TypeId, bool);
type PropertyAccessCacheKey = (TypeId, Atom, bool);

/// Query database wrapper with basic caching.
pub struct QueryCache<'a> {
    interner: &'a TypeInterner,
    eval_cache: RwLock<FxHashMap<EvalCacheKey, TypeId>>,
    subtype_cache: RwLock<FxHashMap<RelationCacheKey, bool>>,
    /// CRITICAL: Separate cache for assignability to prevent cache poisoning.
    /// This ensures that loose assignability results (e.g., any is assignable to number)
    /// don't contaminate strict subtype checks.
    assignability_cache: RwLock<FxHashMap<RelationCacheKey, bool>>,
    property_cache: RwLock<FxHashMap<PropertyAccessCacheKey, PropertyAccessResult>>,
    /// Task #41: Variance cache for generic type parameters.
    /// Stores computed variance masks for DefIds to enable O(1) generic assignability.
    variance_cache: RwLock<FxHashMap<DefId, Arc<[Variance]>>>,
    /// Task #49: Canonical cache for O(1) structural identity checks.
    /// Maps TypeId -> canonical TypeId for structurally identical types.
    canonical_cache: RwLock<FxHashMap<TypeId, TypeId>>,
    no_unchecked_indexed_access: AtomicBool,
}

impl<'a> QueryCache<'a> {
    pub fn new(interner: &'a TypeInterner) -> Self {
        QueryCache {
            interner,
            eval_cache: RwLock::new(FxHashMap::default()),
            subtype_cache: RwLock::new(FxHashMap::default()),
            assignability_cache: RwLock::new(FxHashMap::default()),
            property_cache: RwLock::new(FxHashMap::default()),
            variance_cache: RwLock::new(FxHashMap::default()),
            canonical_cache: RwLock::new(FxHashMap::default()),
            no_unchecked_indexed_access: AtomicBool::new(false),
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

    #[cfg(test)]
    pub fn assignability_cache_len(&self) -> usize {
        match self.assignability_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    #[cfg(test)]
    pub fn property_cache_len(&self) -> usize {
        match self.property_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
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

    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId> {
        // Delegate to the interner
        self.interner.get_class_base_type(symbol_id)
    }

    fn is_unit_type(&self, type_id: TypeId) -> bool {
        self.interner.is_unit_type(type_id)
    }
}

/// Implement TypeResolver for QueryCache with default noop implementations.
///
/// QueryCache doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or DefIds. This implementation
/// returns None for all resolution operations.
///
/// Note: BinderTypeDatabase provides the actual TypeResolver implementation
/// with full type binding capabilities.
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

    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
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

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_options(type_id, self.no_unchecked_indexed_access())
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        let key = (type_id, no_unchecked_indexed_access);
        // Handle poisoned locks gracefully
        let cached = match self.eval_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            return result;
        }

        let mut evaluator = crate::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        let result = evaluator.evaluate(type_id);
        match self.eval_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
        result
    }

    fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        // Task A: Use passed flags instead of hardcoded 0,0
        // TODO: Configure SubtypeChecker with these flags
        let key = RelationCacheKey::subtype(source, target, flags, 0);
        // Handle poisoned locks gracefully
        let cached = match self.subtype_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            return result;
        }

        let result = crate::subtype::is_subtype_of_with_flags(
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
        result
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        // Task A: Use passed flags instead of hardcoded 0,0
        let key = RelationCacheKey::assignability(source, target, flags, 0);

        if let Some(result) = self.check_cache(&self.assignability_cache, key) {
            return result;
        }

        // Use CompatChecker with all compatibility rules
        use crate::compat::CompatChecker;
        let mut checker = CompatChecker::new(self.as_type_database());

        // FIX: Apply flags to ensure checker matches the cache key configuration
        // This prevents cache poisoning where results from non-strict checks
        // leak into strict checks (Gap C fix)
        checker.apply_flags(flags);

        let result = checker.is_assignable(source, target);

        self.insert_cache(&self.assignability_cache, key, result);
        result
    }

    /// Convenience wrapper for is_subtype_of with default flags.
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of_with_flags(source, target, 0) // Default non-strict mode for backward compatibility
    }

    /// Convenience wrapper for is_assignable_to with default flags.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to_with_flags(source, target, 0) // Default non-strict mode for backward compatibility
    }

    fn lookup_subtype_cache(&self, key: RelationCacheKey) -> Option<bool> {
        match self.subtype_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        }
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
    ) -> crate::operations_property::PropertyAccessResult {
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
    ) -> crate::operations_property::PropertyAccessResult {
        // QueryCache doesn't have full TypeResolver capability, so use PropertyAccessEvaluator
        // with the current QueryDatabase.
        let prop_atom = self.interner.intern_string(prop_name);
        let key = (object_type, prop_atom, no_unchecked_indexed_access);
        if let Some(result) = self.check_property_cache(key) {
            return result;
        }

        let mut evaluator = crate::operations_property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        let result = evaluator.resolve_property_access(object_type, prop_name);
        self.insert_property_cache(key, result.clone());
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
        // Check cache first
        let cached = match self.variance_cache.read() {
            Ok(cache) => cache.get(&def_id).cloned(),
            Err(e) => e.into_inner().get(&def_id).cloned(),
        };

        cached
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

/// Wrapper that combines QueryCache with Binder access for class hierarchy lookups.
///
/// This is used by the Checker to provide the TypeDatabase with the ability to
/// resolve base classes (extends clauses) for nominal types.
pub struct BinderTypeDatabase<'a> {
    pub query_cache: &'a QueryCache<'a>,
    pub binder: &'a tsz_binder::BinderState,
    pub type_env: Rc<RefCell<crate::subtype::TypeEnvironment>>,
    /// Cached array base type params (to avoid RefCell lifetime issues)
    #[allow(dead_code)]
    cached_array_base_params: std::sync::Mutex<Option<Box<[TypeParamInfo]>>>,
}

impl<'a> BinderTypeDatabase<'a> {
    pub fn new(
        query_cache: &'a QueryCache<'a>,
        binder: &'a tsz_binder::BinderState,
        type_env: Rc<RefCell<crate::subtype::TypeEnvironment>>,
    ) -> Self {
        Self {
            query_cache,
            binder,
            type_env,
            cached_array_base_params: std::sync::Mutex::new(None),
        }
    }

    pub fn clear(&self) {
        self.query_cache.clear();
    }
}

impl TypeDatabase for BinderTypeDatabase<'_> {
    fn intern(&self, key: TypeKey) -> TypeId {
        self.query_cache.intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeKey> {
        self.query_cache.lookup(id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        self.query_cache.intern_string(s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        self.query_cache.resolve_atom(atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.query_cache.resolve_atom_ref(atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.query_cache.type_list(id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.query_cache.tuple_list(id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.query_cache.template_list(id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.query_cache.object_shape(id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        self.query_cache.object_property_index(shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.query_cache.function_shape(id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.query_cache.callable_shape(id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.query_cache.conditional_type(id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.query_cache.mapped_type(id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.query_cache.type_application(id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        self.query_cache.literal_string(value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        self.query_cache.literal_number(value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        self.query_cache.literal_boolean(value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        self.query_cache.literal_bigint(value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.query_cache.literal_bigint_with_sign(negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.query_cache.union(members)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.query_cache.union2(left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.query_cache.union3(first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.query_cache.intersection(members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.query_cache.intersection2(left, right)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.query_cache.intersect_types_raw2(left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        self.query_cache.array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.query_cache.tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.query_cache.object(properties)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        self.query_cache.object_with_flags(properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        self.query_cache
            .object_with_flags_and_symbol(properties, flags, symbol)
    }

    fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.query_cache.object_fresh(properties)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.query_cache.object_with_index(shape)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        self.query_cache.function(shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        self.query_cache.callable(shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.query_cache.template_literal(spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.query_cache.conditional(conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        self.query_cache.mapped(mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.query_cache.reference(symbol)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.query_cache.application(base, args)
    }

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.query_cache.literal_string_atom(atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        self.query_cache.union_preserve_members(members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.query_cache.readonly_type(inner)
    }

    fn get_class_base_type(&self, _symbol_id: SymbolId) -> Option<TypeId> {
        // TODO: Look up the symbol in the binder and find its extends clause
        // This requires accessing the class declaration node and heritage clauses
        // For now, return None - BCT will fall back to union creation
        None
    }

    fn is_unit_type(&self, type_id: TypeId) -> bool {
        self.query_cache.is_unit_type(type_id)
    }
}

impl TypeResolver for BinderTypeDatabase<'_> {
    #[allow(deprecated)]
    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.type_env.borrow().resolve_ref(symbol, interner)
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.type_env.borrow().resolve_lazy(def_id, interner)
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        self.type_env.borrow().get_type_params(symbol)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.type_env.borrow().get_lazy_type_params(def_id)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.type_env.borrow().def_to_symbol_id(def_id)
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        self.type_env.borrow().symbol_to_def_id(symbol)
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.type_env.borrow().get_boxed_type(kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.type_env.borrow().get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        // NOTE: Cannot easily return &[] from RefCell due to lifetime issues
        // Returning empty slice for now - this is acceptable since array types
        // are typically handled through other mechanisms
        &[]
    }

    fn get_lazy_export(&self, def_id: DefId, name: Atom) -> Option<TypeId> {
        self.type_env.borrow().get_lazy_export(def_id, name)
    }

    fn get_lazy_enum_member(&self, def_id: DefId, name: Atom) -> Option<TypeId> {
        self.type_env.borrow().get_lazy_enum_member(def_id, name)
    }

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        self.type_env.borrow().is_numeric_enum(def_id)
    }

    fn get_base_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.type_env.borrow().get_base_type(type_id, interner)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        self.type_env.borrow().get_def_kind(def_id)
    }
}

impl QueryDatabase for BinderTypeDatabase<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.query_cache
            .interner
            .set_array_base_type(type_id, type_params.clone());
        self.type_env
            .borrow_mut()
            .set_array_base_type(type_id, type_params);
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_options(type_id, self.query_cache.no_unchecked_indexed_access())
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        use crate::evaluate::TypeEvaluator;

        let key = (type_id, no_unchecked_indexed_access);
        // Handle poisoned locks gracefully
        let cached = match self.query_cache.eval_cache.read() {
            Ok(cache) => cache.get(&key).copied(),
            Err(e) => e.into_inner().get(&key).copied(),
        };

        if let Some(result) = cached {
            return result;
        }

        // CRITICAL: Use TypeEvaluator with type_env as resolver
        // This ensures Lazy types are resolved using the TypeEnvironment
        let type_env = self.type_env.borrow();
        let mut evaluator = TypeEvaluator::with_resolver(self.as_type_database(), &*type_env);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);

        let result = evaluator.evaluate(type_id);

        match self.query_cache.eval_cache.write() {
            Ok(mut cache) => {
                cache.insert(key, result);
            }
            Err(e) => {
                e.into_inner().insert(key, result);
            }
        }
        result
    }

    fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.query_cache
            .evaluate_index_access(object_type, index_type)
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        self.query_cache.evaluate_index_access_with_options(
            object_type,
            index_type,
            no_unchecked_indexed_access,
        )
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        // CRITICAL: Borrow type_env to use as resolver for proper Lazy type resolution
        // This fixes the NoopResolver issue that prevents type alias resolution in mapped types
        let type_env = self.type_env.borrow();
        let mut evaluator =
            crate::evaluate::TypeEvaluator::with_resolver(self.as_type_database(), &*type_env);
        evaluator.evaluate_mapped(mapped)
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        // CRITICAL: Borrow type_env to use as resolver for proper Lazy type resolution
        // This fixes the NoopResolver issue that prevents type alias resolution in keyof
        let type_env = self.type_env.borrow();
        let mut evaluator =
            crate::evaluate::TypeEvaluator::with_resolver(self.as_type_database(), &*type_env);
        evaluator.evaluate_keyof(operand)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations_property::PropertyAccessResult {
        self.query_cache
            .resolve_property_access(object_type, prop_name)
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations_property::PropertyAccessResult {
        self.query_cache.resolve_property_access_with_options(
            object_type,
            prop_name,
            no_unchecked_indexed_access,
        )
    }

    fn property_access_type(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations_property::PropertyAccessResult {
        self.query_cache
            .property_access_type(object_type, prop_name)
    }

    fn contextual_property_type(&self, expected: TypeId, prop_name: &str) -> Option<TypeId> {
        // Resolve Lazy types before property lookup
        let resolved = if let Some(crate::TypeKey::Lazy(def_id)) = self.lookup(expected) {
            self.resolve_lazy(def_id, self.as_type_database())
                .unwrap_or(expected)
        } else {
            expected
        };
        self.query_cache
            .contextual_property_type(resolved, prop_name)
    }

    fn is_property_readonly(&self, object_type: TypeId, prop_name: &str) -> bool {
        self.query_cache
            .is_property_readonly(object_type, prop_name)
    }

    fn is_readonly_index_signature(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        self.query_cache
            .is_readonly_index_signature(object_type, wants_string, wants_number)
    }

    fn no_unchecked_indexed_access(&self) -> bool {
        self.query_cache.no_unchecked_indexed_access()
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.query_cache.set_no_unchecked_indexed_access(enabled);
    }

    fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        self.query_cache
            .resolve_element_access(object_type, index_type, literal_index)
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        self.query_cache.get_index_signatures(type_id)
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        self.query_cache.is_nullish_type(type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        self.query_cache.remove_nullish(type_id)
    }

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.query_cache.is_subtype_of(source, target)
    }

    fn is_subtype_of_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        self.query_cache
            .is_subtype_of_with_flags(source, target, flags)
    }

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        self.query_cache.is_assignable_to(source, target)
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        self.query_cache
            .is_assignable_to_with_flags(source, target, flags)
    }

    fn lookup_subtype_cache(&self, key: RelationCacheKey) -> Option<bool> {
        self.query_cache.lookup_subtype_cache(key)
    }

    fn insert_subtype_cache(&self, key: RelationCacheKey, result: bool) {
        self.query_cache.insert_subtype_cache(key, result)
    }

    fn new_inference_context(&self) -> crate::infer::InferenceContext<'_> {
        crate::infer::InferenceContext::new(self)
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        // Use fully qualified syntax to disambiguate between
        // QueryDatabase and TypeResolver traits (both have this method)
        <QueryCache<'_> as QueryDatabase>::get_type_param_variance(self.query_cache, def_id)
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // Delegate to QueryCache which has the canonical cache
        self.query_cache.canonical_id(type_id)
    }
}

#[cfg(test)]
#[path = "tests/db_tests.rs"]
mod tests;
