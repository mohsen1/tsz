//! Type database abstraction for the solver.
//!
//! This trait isolates solver logic from concrete storage so we can
//! swap in a query system (e.g., Salsa) without touching core logic.

use crate::ObjectLiteralBuilder;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::intern::type_factory::TypeFactory;
use crate::narrowing;
use crate::objects::element_access::{ElementAccessEvaluator, ElementAccessResult};
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, StringIntrinsicKind, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

/// Query interface for the solver.
///
/// This keeps solver components generic and prevents them from reaching
/// into concrete storage structures directly.
pub trait TypeDatabase {
    fn intern(&self, key: TypeData) -> TypeId;
    fn lookup(&self, id: TypeId) -> Option<TypeData>;
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

    /// Get conditional type by value (Copy, no Arc overhead).
    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        *self.conditional_type(id)
    }
    /// Get mapped type by value (Copy, no Arc overhead).
    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        *self.mapped_type(id)
    }
    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication>;

    fn literal_string(&self, value: &str) -> TypeId;
    fn literal_number(&self, value: f64) -> TypeId;
    fn literal_boolean(&self, value: bool) -> TypeId;
    fn literal_bigint(&self, value: &str) -> TypeId;
    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId;

    fn union(&self, members: Vec<TypeId>) -> TypeId;
    /// Create a union from a borrowed slice, avoiding allocation when callers
    /// already have an `Arc<[TypeId]>` or `&[TypeId]`.
    fn union_from_slice(&self, members: &[TypeId]) -> TypeId;
    /// Create a union with literal-only reduction (no subtype reduction).
    /// Matches tsc's `UnionReduction.Literal` behavior for type annotations.
    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId;
    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId;
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
    /// Get the TypeId for an already-interned Object shape (O(1) cache hit).
    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId;
    /// Get the TypeId for an already-interned `ObjectWithIndex` shape.
    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId;
    /// Create a fresh object type with both widened properties (for type checking)
    /// and display properties (for error messages, implementing tsc's freshness model).
    fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        // Default: just create a fresh object (implementations can store display props)
        let _ = display_properties;
        self.object_fresh(widened_properties)
    }
    fn object_with_index(&self, shape: ObjectShape) -> TypeId;
    fn function(&self, shape: FunctionShape) -> TypeId;
    fn callable(&self, shape: CallableShape) -> TypeId;
    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId;
    fn conditional(&self, conditional: ConditionalType) -> TypeId;
    fn mapped(&self, mapped: MappedType) -> TypeId;
    fn reference(&self, symbol: SymbolRef) -> TypeId;
    fn lazy(&self, def_id: DefId) -> TypeId;
    fn bound_parameter(&self, index: u32) -> TypeId;
    fn recursive(&self, depth: u32) -> TypeId;
    fn type_param(&self, info: TypeParamInfo) -> TypeId;
    fn type_query(&self, symbol: SymbolRef) -> TypeId;
    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId;
    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId;

    fn literal_string_atom(&self, atom: Atom) -> TypeId;
    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId;
    fn readonly_type(&self, inner: TypeId) -> TypeId;
    fn keyof(&self, inner: TypeId) -> TypeId;
    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId;
    fn this_type(&self) -> TypeId;
    fn no_infer(&self, inner: TypeId) -> TypeId;
    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId;
    fn infer(&self, info: TypeParamInfo) -> TypeId;
    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId;

    /// Create a string intrinsic type by name ("Uppercase", "Lowercase", "Capitalize", "Uncapitalize").
    /// Returns `TypeId::ERROR` for unrecognized names.
    fn string_intrinsic_by_name(&self, name: &str, type_arg: TypeId) -> TypeId {
        match name {
            "Uppercase" => self.string_intrinsic(StringIntrinsicKind::Uppercase, type_arg),
            "Lowercase" => self.string_intrinsic(StringIntrinsicKind::Lowercase, type_arg),
            "Capitalize" => self.string_intrinsic(StringIntrinsicKind::Capitalize, type_arg),
            "Uncapitalize" => self.string_intrinsic(StringIntrinsicKind::Uncapitalize, type_arg),
            _ => TypeId::ERROR,
        }
    }

    /// Store display-only properties for a fresh object literal.
    ///
    /// These are the pre-widened property types shown in error messages.
    /// The `shape_id` is the widened (interned) shape; `props` contains
    /// the original literal types from the source code.
    fn store_display_properties(&self, _type_id: TypeId, _props: Vec<PropertyInfo>) {}

    /// Retrieve display-only properties for a fresh object literal.
    ///
    /// Returns `None` if no display properties were stored.
    fn get_display_properties(&self, _type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        None
    }

    /// Store a reverse mapping from an evaluated Application result back to
    /// its original Application TypeId for diagnostic display.
    fn store_display_alias(&self, _evaluated: TypeId, _application: TypeId) {}

    /// Look up the original Application TypeId for a type produced by
    /// evaluating an Application. Returns `None` if no mapping exists.
    fn get_display_alias(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Atomically read and clear the "union too complex" flag.
    ///
    /// Returns `true` if a union construction was aborted due to complexity
    /// since the last call. The checker uses this to emit TS2590.
    fn take_union_too_complex(&self) -> bool {
        false
    }

    /// Get the base class type for a symbol (class/interface).
    /// Returns the `TypeId` of the extends clause, or None if the symbol doesn't extend anything.
    /// This is used by the BCT algorithm to find common base classes.
    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId>;

    /// Check if a type can be compared by `TypeId` identity alone (O(1) equality).
    /// Identity-comparable types include literals, enum members, unique symbols, null, undefined,
    /// void, never, and tuples composed entirely of identity-comparable types.
    /// Results are cached for O(1) lookup after first computation.
    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool;

    /// Get the boxed interface type for a primitive intrinsic kind.
    ///
    /// For example, `IntrinsicKind::Function` returns the `TypeId` of the `Function` interface
    /// from lib.d.ts. This bypasses `TypeResolver` (which may fail due to `RefCell` borrow
    /// conflicts) by reading directly from the interner's `DashMap`.
    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }

    /// Check if a `DefId` corresponds to a boxed type of the given kind.
    ///
    /// For example, checking if a `DefId` represents the `Function` interface.
    /// This bypasses `TypeResolver` by reading directly from the interner's storage.
    fn is_boxed_def_id(&self, _def_id: DefId, _kind: IntrinsicKind) -> bool {
        false
    }

    /// Check if a `DefId` corresponds to the `ThisType` marker interface.
    fn is_this_type_marker_def_id(&self, _def_id: DefId) -> bool {
        false
    }
}

impl TypeDatabase for TypeInterner {
    fn intern(&self, key: TypeData) -> TypeId {
        Self::intern(self, key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        Self::lookup(self, id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        Self::intern_string(self, s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        Self::resolve_atom(self, atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        Self::resolve_atom_ref(self, atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        Self::type_list(self, id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        Self::tuple_list(self, id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        Self::template_list(self, id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        Self::object_shape(self, id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        Self::object_property_index(self, shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        Self::function_shape(self, id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        Self::callable_shape(self, id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        Self::conditional_type(self, id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        Self::mapped_type(self, id)
    }

    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        TypeInterner::get_conditional(self, id)
    }

    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        TypeInterner::get_mapped(self, id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        Self::type_application(self, id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        Self::literal_string(self, value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        Self::literal_number(self, value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        Self::literal_boolean(self, value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        Self::literal_bigint(self, value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        Self::literal_bigint_with_sign(self, negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        Self::union(self, members)
    }

    fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        Self::union_from_slice(self, members)
    }

    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        Self::union_literal_reduce(self, members)
    }

    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        Self::union_from_sorted_vec(self, flat)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::union2(self, left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        Self::union3(self, first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        Self::intersection(self, members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::intersection2(self, left, right)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::intersect_types_raw2(self, left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        Self::array(self, element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        Self::tuple(self, elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        Self::object(self, properties)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        Self::object_with_flags(self, properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        Self::object_with_flags_and_symbol(self, properties, flags, symbol)
    }

    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        Self::object_type_from_shape(self, shape_id)
    }

    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        Self::object_with_index_type_from_shape(self, shape_id)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        Self::object_with_index(self, shape)
    }

    fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        Self::object_fresh_with_display(self, widened_properties, display_properties)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        Self::function(self, shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        Self::callable(self, shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        Self::template_literal(self, spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        Self::conditional(self, conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        Self::mapped(self, mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        Self::reference(self, symbol)
    }

    fn lazy(&self, def_id: DefId) -> TypeId {
        Self::lazy(self, def_id)
    }

    fn bound_parameter(&self, index: u32) -> TypeId {
        Self::bound_parameter(self, index)
    }

    fn recursive(&self, depth: u32) -> TypeId {
        Self::recursive(self, depth)
    }

    fn type_param(&self, info: TypeParamInfo) -> TypeId {
        Self::type_param(self, info)
    }

    fn type_query(&self, symbol: SymbolRef) -> TypeId {
        Self::type_query(self, symbol)
    }

    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        Self::enum_type(self, def_id, structural_type)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        Self::application(self, base, args)
    }

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        Self::literal_string_atom(self, atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        Self::union_preserve_members(self, members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        Self::readonly_type(self, inner)
    }

    fn keyof(&self, inner: TypeId) -> TypeId {
        Self::keyof(self, inner)
    }

    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        Self::index_access(self, object_type, index_type)
    }

    fn this_type(&self) -> TypeId {
        Self::this_type(self)
    }

    fn no_infer(&self, inner: TypeId) -> TypeId {
        Self::no_infer(self, inner)
    }

    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        Self::unique_symbol(self, symbol)
    }

    fn infer(&self, info: TypeParamInfo) -> TypeId {
        Self::infer(self, info)
    }

    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        Self::string_intrinsic(self, kind, type_arg)
    }

    fn store_display_properties(&self, type_id: TypeId, props: Vec<PropertyInfo>) {
        Self::store_display_properties(self, type_id, props);
    }

    fn get_display_properties(&self, type_id: TypeId) -> Option<Arc<Vec<PropertyInfo>>> {
        Self::get_display_properties(self, type_id)
    }

    fn store_display_alias(&self, evaluated: TypeId, application: TypeId) {
        Self::store_display_alias(self, evaluated, application);
    }

    fn get_display_alias(&self, type_id: TypeId) -> Option<TypeId> {
        Self::get_display_alias(self, type_id)
    }

    fn take_union_too_complex(&self) -> bool {
        Self::take_union_too_complex(self)
    }

    fn get_class_base_type(&self, _symbol_id: SymbolId) -> Option<TypeId> {
        // TypeInterner doesn't have access to the Binder, so it can't resolve base classes.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        Self::is_identity_comparable_type(self, type_id)
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        Self::get_boxed_type(self, kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        Self::is_boxed_def_id(self, def_id, kind)
    }

    fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        Self::is_this_type_marker_def_id(self, def_id)
    }
}

/// Implement `TypeResolver` for `TypeInterner` with noop resolution.
///
/// `TypeInterner` doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or `DefIds`. Only `resolve_ref`
/// (required) is explicitly implemented; all other resolution methods
/// inherit the trait's default `None`/`false` behavior. The three boxed/array
/// methods delegate to `TypeInterner`'s own inherent methods.
impl TypeResolver for TypeInterner {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        TypeInterner::get_boxed_type(self, kind)
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
/// Inherits from `TypeResolver` to enable Lazy/Ref type resolution through `evaluate_type()`.
pub trait QueryDatabase: TypeDatabase + TypeResolver {
    /// Expose the underlying `TypeDatabase` view for legacy entry points.
    fn as_type_database(&self) -> &dyn TypeDatabase;

    /// Expose the `TypeResolver` view for inference contexts that need
    /// to expand type alias Applications (variance-aware inference).
    fn as_type_resolver(&self) -> &dyn TypeResolver;

    /// Expose the checked construction surface for type constructors.
    #[inline]
    fn factory(&self) -> TypeFactory<'_> {
        TypeFactory::new(self.as_type_database())
    }

    /// Register the canonical `Array<T>` base type used by property access resolution.
    ///
    /// Some call paths resolve properties through a `TypeInterner`-backed database,
    /// while others use a `TypeEnvironment`-backed resolver. Implementations should
    /// store this in whichever backing stores they use so `T[]` methods/properties
    /// (e.g. `push`, `length`) resolve consistently.
    fn register_array_base_type(&self, _type_id: TypeId, _type_params: Vec<TypeParamInfo>) {}

    /// Register a boxed interface type for a primitive intrinsic kind.
    ///
    /// Similar to `register_array_base_type`, this ensures that property access
    /// resolution can find the correct interface type (e.g., String, Number) for
    /// primitive types, regardless of which database backend is used.
    fn register_boxed_type(&self, _kind: IntrinsicKind, _type_id: TypeId) {}

    /// Register a `DefId` as belonging to a boxed type.
    fn register_boxed_def_id(&self, _kind: IntrinsicKind, _def_id: DefId) {}

    /// Register a `DefId` as belonging to the `ThisType` marker interface.
    fn register_this_type_def_id(&self, _def_id: DefId) {}

    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        crate::evaluation::evaluate::evaluate_conditional(self.as_type_database(), cond)
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
        crate::evaluation::evaluate::evaluate_index_access_with_options(
            self.as_type_database(),
            object_type,
            index_type,
            no_unchecked_indexed_access,
        )
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        crate::evaluation::evaluate::evaluate_type(self.as_type_database(), type_id)
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        if !no_unchecked_indexed_access {
            return self.evaluate_type(type_id);
        }

        let mut evaluator =
            crate::evaluation::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.evaluate(type_id)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        crate::evaluation::evaluate::evaluate_mapped(self.as_type_database(), mapped)
    }

    /// Look up a shared cache entry for evaluated generic applications.
    fn lookup_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store an evaluated generic application result in the shared cache.
    fn insert_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        crate::evaluation::evaluate::evaluate_keyof(self.as_type_database(), operand)
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
    ) -> crate::operations::property::PropertyAccessResult;

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult;

    fn property_access_type(
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

    fn no_unchecked_indexed_access(&self) -> bool {
        false
    }

    fn set_no_unchecked_indexed_access(&self, _enabled: bool) {}

    fn contextual_property_type(&self, expected: TypeId, prop_name: &str) -> Option<TypeId> {
        let ctx = crate::ContextualTypeContext::with_expected(self.as_type_database(), expected);
        ctx.get_property_type(prop_name)
    }

    fn is_property_readonly(&self, object_type: TypeId, prop_name: &str) -> bool {
        crate::operations::property::property_is_readonly(
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
        crate::operations::property::is_readonly_index_signature(
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
        let mut evaluator = ElementAccessEvaluator::new(self.as_type_database());
        let flag = self.no_unchecked_indexed_access();
        evaluator.set_no_unchecked_indexed_access(flag);
        evaluator.resolve_element_access(object_type, index_type, literal_index)
    }

    /// Resolve element access type with cache-friendly error normalization.
    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        match self.resolve_element_access(object_type, index_type, literal_index) {
            crate::element_access::ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        }
    }

    /// Collect properties that can be spread into object literals.
    fn collect_object_spread_properties(&self, spread_type: TypeId) -> Vec<PropertyInfo> {
        let builder = ObjectLiteralBuilder::new(self.as_type_database());
        builder.collect_spread_properties(spread_type)
    }

    /// Get index signatures for a type
    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo;

    /// Check if a type contains null or undefined
    fn is_nullish_type(&self, type_id: TypeId) -> bool;

    /// Remove null and undefined from a type
    fn remove_nullish(&self, type_id: TypeId) -> TypeId;

    /// Get the canonical `TypeId` for a type, achieving O(1) structural identity checks.
    ///
    /// This memoizes the Canonicalizer output so that structurally identical types
    /// (e.g., `type A = Box<Box<string>>` and `type B = Box<Box<string>>`) return
    /// the same canonical `TypeId`.
    ///
    /// The implementation must:
    /// - Use a fresh Canonicalizer with empty stacks (for absolute De Bruijn indices)
    /// - Only expand `TypeAlias` (`DefKind::TypeAlias`), preserving nominal types
    /// - Cache the result for O(1) subsequent lookups
    ///
    /// Task #49: Global Canonical Mapping
    fn canonical_id(&self, type_id: TypeId) -> TypeId;

    /// Subtype check with compiler flags.
    ///
    /// The `flags` parameter is a packed u16 bitmask matching `RelationCacheKey.flags`:
    /// - bit 0: `strict_null_checks`
    /// - bit 1: `strict_function_types`
    /// - bit 2: `exact_optional_property_types`
    /// - bit 3: `no_unchecked_indexed_access`
    /// - bit 4: `disable_method_bivariance`
    /// - bit 5: `allow_void_return`
    /// - bit 6: `allow_bivariant_rest`
    /// - bit 7: `allow_bivariant_param_count`
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
        crate::relations::subtype::is_subtype_of_with_flags(
            self.as_type_database(),
            source,
            target,
            flags,
        )
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

    /// Look up a cached assignability result for the given key.
    /// Returns `None` if the result is not cached.
    /// Default implementation returns `None` (no caching).
    fn lookup_assignability_cache(&self, _key: RelationCacheKey) -> Option<bool> {
        None
    }

    /// Cache an assignability result for the given key.
    /// Default implementation is a no-op.
    fn insert_assignability_cache(&self, _key: RelationCacheKey, _result: bool) {}

    #[allow(dead_code, private_interfaces)] // Reserved for full inference pipeline integration
    fn new_inference_context(&self) -> crate::inference::infer::InferenceContext<'_> {
        crate::inference::infer::InferenceContext::new(self.as_type_database())
    }

    /// Task #41: Get the variance mask for a generic type definition.
    ///
    /// Returns the variance of each type parameter for the given `DefId`.
    /// Returns None if the `DefId` is not a generic type or variance cannot be determined.
    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>>;
}

impl QueryDatabase for TypeInterner {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.set_array_base_type(type_id, type_params);
    }

    fn register_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        TypeInterner::set_boxed_type(self, kind, type_id);
    }

    fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        TypeInterner::register_boxed_def_id(self, kind, def_id);
    }

    fn register_this_type_def_id(&self, def_id: DefId) {
        TypeInterner::register_this_type_def_id(self, def_id);
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        match self.lookup(type_id) {
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.object_shape(shape_id);
                IndexInfo {
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                }
            }
            Some(TypeData::Array(element)) => {
                // Arrays have number index signature with element type
                IndexInfo {
                    string_index: None,
                    number_index: Some(crate::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: element,
                        readonly: false,
                        param_name: None,
                    }),
                }
            }
            Some(TypeData::Tuple(elements_id)) => {
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
                        param_name: None,
                    }),
                }
            }
            Some(TypeData::Union(members_id)) => {
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
                        param_name: None,
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
                        param_name: None,
                    })
                };

                IndexInfo {
                    string_index,
                    number_index,
                }
            }
            Some(TypeData::Intersection(members_id)) => {
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
        narrowing::remove_nullish_query(self, type_id)
    }

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        self.is_assignable_to_with_flags(source, target, 0)
    }

    fn is_assignable_to_with_flags(&self, source: TypeId, target: TypeId, flags: u16) -> bool {
        use crate::relations::compat::CompatChecker;
        let mut checker = CompatChecker::new(self);
        if flags != 0 {
            checker.apply_flags(flags);
        }
        checker.is_assignable(source, target)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations::property::PropertyAccessResult {
        // TypeInterner doesn't have TypeResolver capability, so it can't resolve Lazy types
        // Use PropertyAccessEvaluator with QueryDatabase (self implements both TypeDatabase and TypeResolver)
        let evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult {
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        let mut evaluator = ElementAccessEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(TypeInterner::no_unchecked_indexed_access(self));
        evaluator.resolve_element_access(object_type, index_type, literal_index)
    }

    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        match self.resolve_element_access(object_type, index_type, literal_index) {
            ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        }
    }

    fn no_unchecked_indexed_access(&self) -> bool {
        TypeInterner::no_unchecked_indexed_access(self)
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        TypeInterner::set_no_unchecked_indexed_access(self, enabled);
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
