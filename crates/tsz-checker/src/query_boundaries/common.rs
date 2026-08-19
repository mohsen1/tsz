use tsz_solver::computation as c;
use tsz_solver::{
    CallSignature, CallableShape, ObjectShape, TupleElement, TypeApplication, TypeId,
    operations::widening,
};

#[allow(unused_imports)]
pub(crate) use tsz_solver::TypeParamOrigin;
pub(crate) use tsz_solver::computation::CompatChecker;
#[allow(unused_imports)]
pub(crate) use tsz_solver::construction::TypeInterner;
pub(crate) use tsz_solver::construction::{QueryDatabase, TypeDatabase};
pub(crate) use tsz_solver::narrowing::{
    CachedChainType, CachedPropertyType, NarrowingCache, NarrowingContext,
    OptionalPropertyChainKey, TypeGuard, TypeofKind,
};
pub(crate) use tsz_solver::objects::IndexSignatureResolver;
pub(crate) use tsz_solver::operations::property::PropertyAccessResult;
pub(crate) use tsz_solver::operations::{AssignabilityChecker, CallResult};
pub(crate) use tsz_solver::relations::judge::{DefaultJudge, JudgeConfig};
pub(crate) use tsz_solver::relations::subtype::{TypeEnvironment, TypeResolver};
pub(crate) use tsz_solver::type_queries::{
    RemappedMappedIndexAccessResult, TypeIdList, TypeTraversalKind,
    constraint_allows_mutable_array_like, is_remapped_mapped_index_access,
    remapped_mapped_index_access_result,
};
pub(crate) use tsz_solver::{
    FunctionShape, IntrinsicKind, ParamInfo, PendingDiagnosticBuilder, SourceLocation,
    SubtypeFailureReason, TypeFormatter,
    computation::{ContextualTypeContext, TypeSubstitution},
};

pub(crate) use super::construct_signatures::construct_signatures_for_type;
pub(crate) use super::containment_queries::{
    collect_all_types, collect_lazy_def_ids, collect_referenced_types, collect_type_queries,
    constraint_references_type_param_in_resolution_path, contains_application_in_structure,
    contains_conditional_type, contains_current_infer_placeholder, contains_error_type,
    contains_error_type_in_args, contains_file_relative_content, contains_free_type_parameters,
    contains_generic_indexed_access_surface, contains_generic_type_parameters,
    contains_index_access_type, contains_infer_types, contains_keyof_type,
    contains_lazy_or_recursive, contains_never_type, contains_this_type, contains_type_by_id,
    contains_type_parameter_named, contains_type_parameters, has_deferred_conditional_member,
    references_any_type_param_named, return_type_is_unresolved, type_contains_undefined,
    union_of_bare_lazy_def_ids, walk_referenced_types,
};
pub(crate) use super::generic_instantiation::{instantiate_generic, instantiate_type};
pub(crate) use super::shape_predicates::{
    are_same_base_literal_kind, has_call_signatures, has_construct_signatures, has_function_shape,
    has_late_bound_members, has_nonpublic_property, has_property_by_str, is_array_or_tuple_type,
    is_array_type, is_bare_infer_placeholder, is_bigint_type, is_boolean_type, is_callable_type,
    is_conditional_type, is_constructor_like_type,
    is_deferred_indexed_access_or_intersection_with_one, is_definitely_nullish,
    is_distributive_conditional_with_deferred_check, is_empty_object_type, is_enum_type,
    is_error_type, is_evaluable_meta_type, is_fresh_object_type, is_function_type,
    is_generic_application, is_generic_application_with_type_params, is_generic_mapped_application,
    is_generic_mapped_type, is_generic_type, is_genuine_error_type,
    is_homomorphic_mapped_type_context, is_index_access_type, is_intersection_type, is_keyof_type,
    is_lazy_type, is_literal_or_primitive_or_compound_of_those, is_literal_type,
    is_literal_type_through_type_constraints, is_mapped_type,
    is_mapped_type_with_readonly_modifier, is_merged_intersection_object, is_module_namespace_type,
    is_nullish_type, is_number_literal, is_number_type, is_object_like_type,
    is_object_or_mapped_type, is_only_null_or_undefined, is_plain_object_type, is_primitive_type,
    is_readonly_tuple_fixed_element, is_spread_marker_tuple, is_string_intrinsic_type,
    is_string_literal, is_string_type, is_structurally_deferred_type, is_symbol_or_unique_symbol,
    is_symbol_type, is_template_literal_type, is_this_type, is_tuple_like_type, is_tuple_type,
    is_type_deeply_any, is_type_parameter, is_type_parameter_like,
    is_type_parameter_or_intersection_with_type_parameter, is_type_query_type, is_union_type,
    is_unique_symbol_type, is_unit_type, is_unresolved_inference_result,
    is_valid_mapped_type_key_type, is_widening_primitive_intrinsic,
    mapped_type_is_deferred_generic, numeric_literal_index_valid_for_object,
    type_has_displayable_name, type_has_readonly_members, type_id_is_known_to_db,
    type_parameter_has_conditional_constraint, type_parameter_has_mapped_constraint,
    union_contains_tuple,
};
pub(crate) use super::type_rewrite::replace_type_queries_and_lazies_with;

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    tsz_solver::type_queries::classify_for_traversal(db, type_id)
}

/// Members of a union type, or `None` if `type_id` is not a union.
///
/// Returns a [`TypeIdList`] — a zero-copy shared view over the interned
/// member list (an O(1) refcount bump) rather than copying into a fresh
/// `Vec` on every call. `TypeIdList` is a drop-in for `Vec<TypeId>` in
/// read-only contexts; the rare caller that needs an owned, mutable buffer
/// calls `.to_vec()`.
pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeIdList> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

/// Current value of the solver's unresolved-`Lazy` taint counter. Snapshot it
/// around a proof computation to detect a dependency on a def body that was
/// not yet registered; such proofs must not be published program-wide.
pub(crate) fn lazy_resolve_failure_count() -> u64 {
    tsz_solver::relations::subtype::lazy_resolve_failure_count()
}

pub(crate) fn lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}

pub(crate) fn application_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeApplicationId> {
    tsz_solver::visitor::application_id(db, type_id)
}

pub(crate) fn mapped_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::MappedTypeId> {
    tsz_solver::type_queries::get_mapped_type_id(db, type_id)
}

pub(crate) fn type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_default(db, type_id)
}

/// Resolve "dangling" free type parameters in a property-access result — those
/// free in `member_type` but not present in `in_scope` (the enclosing generic
/// context's own parameters) — to their declared `default → constraint →
/// unknown`, matching tsc's `fillMissingTypeArguments`.
/// See [`tsz_solver::computation::resolve_unbound_type_params_to_defaults`].
pub(crate) fn resolve_unbound_type_params_to_defaults<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    in_scope: &std::collections::HashSet<TypeId, S>,
) -> TypeId {
    tsz_solver::computation::resolve_unbound_type_params_to_defaults(db, member_type, in_scope)
}

/// Resolve dangling property-member type parameters only when their declaration
/// has a fallback; unconstrained mapped/conditional helpers stay abstract.
pub(crate) fn resolve_unbound_type_params_to_declared_fallbacks<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    in_scope: &std::collections::HashSet<TypeId, S>,
) -> TypeId {
    tsz_solver::computation::resolve_unbound_type_params_to_declared_fallbacks(
        db,
        member_type,
        in_scope,
    )
}

/// Free type parameters of `roots` whose declared name is in `names`, as
/// `(name, TypeId)` pairs (the exact interned parameter ids). See
/// [`tsz_solver::computation::free_type_params_named`].
pub(crate) fn free_type_params_named<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    roots: impl IntoIterator<Item = TypeId>,
    names: &std::collections::HashSet<tsz_common::Atom, S>,
) -> Vec<(tsz_common::Atom, TypeId)> {
    tsz_solver::computation::free_type_params_named(db, roots, names)
}

pub(crate) fn tuple_elements(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}

/// Members of an intersection type, or `None` if `type_id` is not an
/// intersection. Returns a zero-copy [`TypeIdList`]; see `union_members`
/// for the allocation rationale.
pub(crate) fn intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeIdList> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) fn unwrap_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly(db, type_id)
}

// ── Type application query ──

pub(crate) fn type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

// ── Evaluation classification ──

pub(crate) use tsz_solver::type_queries::EvaluationNeeded;

pub(crate) fn classify_for_evaluation(db: &dyn TypeDatabase, type_id: TypeId) -> EvaluationNeeded {
    tsz_solver::type_queries::classify_for_evaluation(db, type_id)
}

// ── Predicate / narrowing classification ──

pub(crate) use tsz_solver::type_queries::PredicateSignatureKind;

pub(crate) fn classify_for_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PredicateSignatureKind {
    tsz_solver::type_queries::classify_for_predicate_signature(db, type_id)
}

pub(crate) use tsz_solver::type_queries::{is_narrowing_literal, is_unknown_narrowing_literal};

pub(crate) fn stringify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    tsz_solver::type_queries::stringify_literal_type(db, type_id)
}

// ── Visitor wrappers ──

// ── Type construction wrappers ──

/// Create `type_id | undefined`. Used for optional chain call results.
pub(crate) fn union_with_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    db.union2(type_id, TypeId::UNDEFINED)
}

pub(crate) fn intersection_or_single(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::intersection_or_single(db, types)
}

// ── Union / classifier wrappers ──

pub(crate) fn type_param_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeParamInfo> {
    tsz_solver::type_param_info(db, type_id)
}

// ── Type unwrapping / widening wrappers ──

/// Unwrap `ReadonlyType` or `NoInfer` wrappers, returning the inner type if present.
pub(crate) fn unwrap_readonly_or_noinfer(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::unwrap_readonly_or_noinfer(db, type_id)
}

/// Widen a literal type to its base primitive (e.g. `"hello"` → `string`).
pub(crate) fn widen_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::widen_type(db, type_id)
}

/// Widen a type for diagnostic display, preserving boolean literal intrinsics.
///
/// Like `widen_type` but keeps `true`/`false` literals so narrowed types
/// display correctly (e.g., `string | false` instead of `string | boolean`).
pub(crate) fn widen_type_for_display(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::widen_type_for_display(db, type_id)
}

/// Extract the element type from a rest-argument array/tuple type.
pub(crate) fn rest_argument_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::computation::rest_argument_element_type(db, type_id)
}

// ── Call-related query wrappers ──

/// Get the full function shape for a type, if it is a Function type.
///
/// Unlike `has_function_shape` (which returns bool), this returns the actual
/// `FunctionShape` so callers can inspect parameters, return type, etc.
pub(crate) fn function_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

/// Check if both types are application (generic instantiation) types and the
/// parameter type contains type parameters, indicating the parameter should be
/// preserved without evaluation during generic inference.
pub(crate) fn should_preserve_application_for_inference(
    db: &dyn TypeDatabase,
    param_type: TypeId,
    arg_type: TypeId,
) -> bool {
    tsz_solver::type_queries::should_preserve_application_for_inference(db, param_type, arg_type)
}

/// Unpack a tuple rest parameter into individual positional parameters.
///
/// Converts `...args: [string, number]` into `(arg0: string, arg1: number)`.
pub(crate) fn unpack_tuple_rest_parameter(
    db: &dyn TypeDatabase,
    param: &ParamInfo,
) -> Vec<ParamInfo> {
    tsz_solver::type_queries::unpack_tuple_rest_parameter(db, param)
}

/// Find a named property in an object type by `Atom`.
pub(crate) fn find_property_in_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> Option<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::find_property_in_object(db, type_id, name)
}

/// Extract the inner type of a `keyof T` type, returning `None` if the type is
/// not a keyof type.
pub(crate) fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::keyof_inner_type(db, type_id)
}

/// Instantiate a type, returning the typed result and whether the depth limit
/// was exceeded during instantiation.
pub(crate) fn instantiate_type_with_depth_status(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> c::InstantiationResult {
    c::instantiate_type_with_depth_status(db, type_id, substitution)
}

pub(crate) fn substitute_this_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    c::substitute_this_type_cached(db.as_type_database(), Some(db), type_id, this_type)
}

/// Shallow `this` substitution for call-return-position use.
///
/// Replaces `ThisType` at structural positions without recursing into named
/// Object/ObjectWithIndex internals, leaving stored interface/class method
/// bodies polymorphic for later property-access-time rebinding.
pub(crate) fn substitute_this_type_at_return_position(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    c::substitute_this_type_at_return_position(db.as_type_database(), Some(db), type_id, this_type)
}

/// Get the enum `DefId` for an enum type.
pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_enum_def_id(db, type_id)
}

/// Get application info (base type + type arguments) for a type application.
pub(crate) fn application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::extended::get_application_info(db, type_id)
}

// ── Literal type classification ──

pub(crate) use tsz_solver::type_queries::extended::LiteralTypeKind;

/// Classify a type as a literal type kind (string, number, bigint, boolean, or not literal).
pub(crate) fn classify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralTypeKind {
    tsz_solver::type_queries::extended::classify_literal_type(db, type_id)
}

// ── Literal/type extraction wrappers ──

pub(crate) use tsz_solver::LiteralValue;

/// Extract the literal value from a literal type.
pub(crate) fn literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<LiteralValue> {
    tsz_solver::literal_value(db, type_id)
}

/// Widen a literal type to its base type (e.g., `3` → `number`).
pub(crate) fn widen_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::widen_literal_type(db, type_id)
}

// ── Contextual/operation wrappers ──

/// Widen "fresh" object literal types to remove freshness tracking.
pub(crate) fn widen_freshness(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::relations::freshness::widen_freshness(db, type_id)
}

/// Re-export of the solver's iterator info type.
pub(crate) use tsz_solver::operations::iterators::IteratorInfo;

/// Get iterator/iterable info from a type.
pub(crate) fn get_iterator_info(
    db: &dyn tsz_solver::construction::QueryDatabase,
    type_id: TypeId,
    is_async: bool,
) -> Option<IteratorInfo> {
    tsz_solver::operations::get_iterator_info(db, type_id, is_async)
}

// ── Data-layer query wrappers ──
// These wrap `tsz_solver::type_queries::data::` functions to keep the
// internal data-access module out of checker code.

/// Get the SymbolId attached to an object type's shape (if any).
pub(crate) fn object_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    tsz_solver::type_queries::data::get_object_symbol(db, type_id)
}

/// Get the enum member's underlying value type (e.g., for `Enum.Member` → its literal type).
pub(crate) fn enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::data::get_enum_member_type(db, type_id)
}

/// Get a callable shape for a type, synthesizing one from a function shape if needed.
pub(crate) fn callable_shape_for_type_extended(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
    tsz_solver::type_queries::data::get_callable_shape_for_type(db, type_id)
}

/// Get the construct return type for a type (union of all construct signature return types).
pub(crate) fn construct_return_type_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::data::construct_return_type_for_type(db, type_id)
}

/// Intersect constructor return types between a constructor type and its base.
pub(crate) fn intersect_constructor_returns(
    db: &dyn tsz_solver::construction::QueryDatabase,
    ctor_type: TypeId,
    base_type: TypeId,
) -> TypeId {
    tsz_solver::type_queries::data::intersect_constructor_returns(db, ctor_type, base_type)
}

/// Get the raw property type by name from an object shape (no full property resolution).
pub(crate) fn raw_property_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: tsz_common::interner::Atom,
) -> Option<TypeId> {
    tsz_solver::type_queries::data::get_raw_property_type(db, type_id, prop_name)
}

/// Collect all callable (function-typed) property types from an object type.
pub(crate) fn collect_callable_property_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<TypeId> {
    tsz_solver::type_queries::data::collect_callable_property_types(db, type_id)
}

/// Find a property by name in a property slice.
///
/// Thin wrapper around `PropertyInfo::find_in_slice` so that checker code
/// does not call solver static methods directly.
pub(crate) fn find_matching_property(
    properties: &[tsz_solver::PropertyInfo],
    name: tsz_common::interner::Atom,
) -> Option<&tsz_solver::PropertyInfo> {
    tsz_solver::PropertyInfo::find_in_slice(properties, name)
}

// ── This-type query ──

// ── Conditional type query ──

// ── Type parameter constraint query ──

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

// ── Mapped type query ──

pub(crate) fn mapped_type_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::MappedType>> {
    tsz_solver::type_queries::get_mapped_type(db, type_id)
}

/// Reduce a mapped-type property access `Mapped[key_literal]` to the mapped
/// template instantiated for that key (tsc's homomorphic mapped-type indexing).
/// Returns `None` when `type_id` is not a mapped type or `key_literal` is not a
/// string-literal key.
pub(crate) fn mapped_property_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    key_literal: TypeId,
) -> Option<TypeId> {
    let mapped = tsz_solver::type_queries::get_mapped_type(db, type_id)?;
    string_literal_value(db, key_literal)?;
    Some(
        tsz_solver::type_queries::instantiate_mapped_template_for_property(
            db,
            mapped.template,
            mapped.type_param.name,
            key_literal,
        ),
    )
}

// ── Index access types query ──

pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

// ── String literal value extraction ──

pub(crate) fn string_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_common::interner::Atom> {
    tsz_solver::type_queries::get_string_literal_value(db, type_id)
}

// ── Number literal value extraction ──

pub(crate) fn number_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<f64> {
    tsz_solver::type_queries::get_number_literal_value(db, type_id)
}

// ── Same base literal kind comparison ──

pub(crate) fn widen_literal_to_primitive(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::widen_literal_to_primitive(db, type_id)
}

pub(crate) use tsz_solver::type_queries::ContextualLiteralAllowKind;

pub(crate) fn classify_for_contextual_literal(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ContextualLiteralAllowKind {
    tsz_solver::type_queries::classify_for_contextual_literal(db, type_id)
}

// ── Literal value classification ──

pub(crate) use tsz_solver::type_queries::LiteralValueKind;

pub(crate) fn classify_for_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> LiteralValueKind {
    tsz_solver::type_queries::classify_for_literal_value(db, type_id)
}

/// Information about an indexed access type (e.g., `T[K]`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct IndexedAccessTypeInfo {
    pub object_type: TypeId,
    pub index_type: TypeId,
}

/// Get the indexed access type info for a type if it represents an indexed access.
/// Returns `Some(IndexedAccessTypeInfo)` if the type is an index access type like `T[K]`.
pub(crate) fn get_indexed_access_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<IndexedAccessTypeInfo> {
    tsz_solver::type_queries::get_index_access_types(db, type_id).map(
        |(object_type, index_type)| IndexedAccessTypeInfo {
            object_type,
            index_type,
        },
    )
}

/// Check if a type is the result of a conditional type with unresolved inference.
/// This is used to suppress false-positive TS2339 errors when accessing properties
/// on types that depend on unresolved conditional type inference.
///
/// For example, in `FirstParameter<typeof h>['foo']` where `h` is a generic function,
/// the conditional type `FirstParameter<T>` may not be resolved yet during inference,
/// and we should suppress the property-not-found error.
pub(crate) fn type_is_conditional_type_result_with_unresolved_inference(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    // If this is directly a conditional type, check if it's unresolved
    if let Some(conditional) = tsz_solver::type_queries::get_conditional_type(db, type_id) {
        // Check if the check type contains type parameters (unresolved)
        if tsz_solver::type_queries::contains_type_parameters_db(db, conditional.check_type)
            || tsz_solver::type_queries::contains_type_parameters_db(db, conditional.extends_type)
        {
            return true;
        }
        // Check if either branch contains type parameters
        if tsz_solver::type_queries::contains_type_parameters_db(db, conditional.true_type)
            || tsz_solver::type_queries::contains_type_parameters_db(db, conditional.false_type)
        {
            return true;
        }
    }

    // Check if this type contains conditional types that are unresolved
    if contains_conditional_type(db, type_id) {
        // Check if the type also contains type parameters
        if contains_type_parameters(db, type_id) {
            return true;
        }
    }

    false
}

// ── Merged object shape query ──

use tsz_solver::PropertyInfo;

/// Get the fully merged object shape for a type, including properties from
/// intersection members, union members, and merged declarations.
///
/// This is the canonical boundary for property-level analysis that needs
/// to account for merged types (e.g., `{ a: string } & { b: number }` should
/// have both `a` and `b` properties available).
pub(crate) fn get_merged_object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ObjectShape> {
    // First, get the base shape if it exists
    let base_shape = tsz_solver::type_queries::get_object_shape(db, type_id)?;

    // Collect properties from intersection members
    let mut merged_props: Vec<PropertyInfo> = base_shape.properties.to_vec();
    let mut has_string_index = base_shape.string_index.is_some();
    let mut has_number_index = base_shape.number_index.is_some();
    let mut has_symbol_index = base_shape.symbol_index.is_some();

    // Add properties from intersection members
    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id) {
        for member in members {
            if let Some(member_shape) = tsz_solver::type_queries::get_object_shape(db, member) {
                for prop in member_shape.properties.iter() {
                    // Check if property already exists
                    if !merged_props.iter().any(|p| p.name == prop.name) {
                        merged_props.push(prop.clone());
                    }
                }
                has_string_index = has_string_index || member_shape.string_index.is_some();
                has_number_index = has_number_index || member_shape.number_index.is_some();
                has_symbol_index = has_symbol_index || member_shape.symbol_index.is_some();
            }
        }
    }

    // Sort properties by declaration order for consistent results
    merged_props.sort_by_key(|p| p.declaration_order);

    Some(ObjectShape {
        flags: base_shape.flags,
        properties: merged_props,
        string_index: if has_string_index {
            base_shape.string_index
        } else {
            None
        },
        number_index: if has_number_index {
            base_shape.number_index
        } else {
            None
        },
        symbol_index: if has_symbol_index {
            base_shape.symbol_index
        } else {
            None
        },
        symbol: base_shape.symbol,
    })
}

pub(crate) fn needs_evaluation_for_merge(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::needs_evaluation_for_merge(db, type_id)
}

pub(crate) fn return_type_for_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_return_type(db, type_id)
}

pub(crate) fn type_shape_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    tsz_solver::type_queries::get_type_shape_symbol(db, type_id)
}

pub(crate) fn find_property_by_str(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> Option<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::find_property_in_type_by_str(db, type_id, name)
}

pub(crate) fn array_applicable_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_applicable_type(db, type_id)
}

pub(crate) fn homomorphic_mapped_source(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::homomorphic_mapped_source(db, type_id)
}

pub(crate) fn map_compound_members_if_changed(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    f: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::map_compound_members_if_changed(db, type_id, f)
}

pub(crate) use tsz_solver::type_queries::AugmentationTargetKind;
pub(crate) use tsz_solver::type_queries::classifiers::InterfaceMergeKind;
pub(crate) use tsz_solver::type_queries::extended::NamespaceMemberKind;
pub(crate) use tsz_solver::type_queries::extended::TypeResolutionKind;

pub(crate) fn classify_namespace_member(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> NamespaceMemberKind {
    tsz_solver::type_queries::classify_namespace_member(db, type_id)
}

pub(crate) fn classify_for_interface_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> InterfaceMergeKind {
    tsz_solver::type_queries::classify_for_interface_merge(db, type_id)
}

pub(crate) fn classify_for_type_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeResolutionKind {
    tsz_solver::type_queries::classify_for_type_resolution(db, type_id)
}

pub(crate) fn object_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::ObjectShapeId> {
    tsz_solver::type_queries::get_object_shape_id(db, type_id)
}

pub(crate) fn classify_for_augmentation(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AugmentationTargetKind {
    tsz_solver::type_queries::classify_for_augmentation(db, type_id)
}

pub(crate) fn classify_type_query(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> tsz_solver::type_queries::TypeQueryKind {
    tsz_solver::type_queries::classify_type_query(db, type_id)
}

pub(crate) fn create_string_literal_type(db: &dyn TypeDatabase, value: &str) -> TypeId {
    tsz_solver::type_queries::create_string_literal_type(db, value)
}

pub(crate) fn extract_contextual_type_params(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::TypeParamInfo>> {
    tsz_solver::type_queries::extract_contextual_type_params(db, type_id)
}

pub(crate) fn find_property_in_object_by_str(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> Option<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::find_property_in_object_by_str(db, type_id, name)
}

pub(crate) fn types_are_comparable_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::type_queries::flow::types_are_comparable_for_assertion(db, source, target)
}

pub(crate) fn get_application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_application_base(db, type_id)
}

pub(crate) fn get_application_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_application_lazy_def_id(db, type_id)
}

pub(crate) fn get_base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::get_base_constraint_of_type(db, type_id)
}

pub(crate) fn get_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn get_callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
    tsz_solver::type_queries::get_callable_shape_for_type(db, type_id)
}

pub(crate) fn get_construct_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::get_construct_signatures(db, type_id)
}

pub(crate) fn get_fixed_tuple_length(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    tsz_solver::type_queries::get_fixed_tuple_length(db, type_id)
}

/// Upper bound (exclusive) on the source element indices that array-literal /
/// argument elaboration may report per-element against a tuple *target* that
/// contains a rest element.
///
/// tsc's `generateLimitedTupleElements` skips any source element whose index
/// has no *fixed* slot in the tuple-like target (`isTupleLikeType(target) &&
/// !getPropertyOfType(target, `${i}`)`), so only the leading fixed prefix
/// (required or optional slots) before the first rest element is ever drilled
/// into at the element level. Positions covered by the rest element — and any
/// trailing fixed elements, whose position depends on the source length — fall
/// back to the whole-tuple relation, which renders the
/// `Type at position(s) i[ through j] in source …` chain.
///
/// Returns `Some(leading_fixed_count)` when the target tuple pairs a rest
/// element with at least one fixed element, capping element drill-in to that
/// prefix. Returns `None` when there is no rest element (a closed tuple —
/// every position is a fixed slot and drills in) or when the tuple is a lone
/// rest element (`[...T[]]`, which tsc normalizes to the array `T[]` — not
/// tuple-like, so every element drills in).
///
/// For `[number, ...string[]]`: returns `Some(1)`.
/// For `[number, ...string[], number]`: returns `Some(1)`.
/// For `[...string[], number]`: returns `Some(0)`.
/// For `[...string[]]`: returns `None` (array-like — drill every element).
/// For `[number, string]`: returns `None` (no rest element).
pub(crate) fn tuple_leading_fixed_drill_cap(
    elements: &[tsz_solver::TupleElement],
) -> Option<usize> {
    // Fixed-length tuple spreads (`[a, ...[b, c]]`) are flattened into fixed
    // elements before interning, so a `rest` marker here is always a genuine
    // variable-length rest.
    let first_rest_pos = elements.iter().position(|e| e.rest)?;
    if elements.iter().all(|e| e.rest) {
        return None;
    }
    Some(first_rest_pos)
}

pub(crate) fn get_invalid_index_type_member(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_invalid_index_type_member(db, type_id)
}

pub(crate) fn get_noinfer_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_noinfer_inner(db, type_id)
}

pub(crate) fn get_private_brand_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    tsz_solver::type_queries::get_private_brand_name(db, type_id)
}

pub(crate) fn get_private_field_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    tsz_solver::type_queries::get_private_field_name(db, type_id)
}

pub(crate) fn get_readonly_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_readonly_inner(db, type_id)
}

pub(crate) fn get_tuple_element_type_union(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_tuple_element_type_union(db, type_id)
}

pub(crate) fn get_type_query_symbol_ref(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::type_queries::get_type_query_symbol_ref(db, type_id)
}

pub(crate) fn keyof_object_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::keyof_object_properties(db, type_id)
}

#[allow(unused_imports)]
pub(crate) use tsz_solver::type_queries::{
    ArrayLikeKind, BodyArgPreservation, ConstructorCheckKind, IdentityMappedInfo, IndexKeyKind,
    LazyTypeKind, MappedSourceKind, TypeQueryKind, UnionMembersKind,
};

pub(crate) fn get_construct_return_type_union(
    db: &dyn TypeDatabase,
    shape_id: tsz_solver::CallableShapeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_construct_return_type_union(db, shape_id)
}

pub(crate) fn get_conditional_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::ConditionalTypeId> {
    tsz_solver::type_queries::get_conditional_type_id(db, type_id)
}

pub(crate) fn callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::CallableShapeId> {
    tsz_solver::visitor::callable_shape_id(db, type_id)
}

pub(crate) fn enum_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(tsz_solver::def::DefId, TypeId)> {
    tsz_solver::visitor::enum_components(db, type_id)
}

pub(crate) fn union_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeListId> {
    tsz_solver::visitor::union_list_id(db, type_id)
}

/// Factory for `BinaryOpEvaluator` — single construction point through the boundary.
///
/// All checker code that needs binary-op evaluation must construct the evaluator
/// through this function instead of calling `BinaryOpEvaluator::new()` directly.
pub(crate) fn new_binary_op_evaluator(
    db: &dyn tsz_solver::construction::QueryDatabase,
) -> tsz_solver::operations::BinaryOpEvaluator<'_> {
    tsz_solver::operations::BinaryOpEvaluator::new(db)
}

// ── Visitor aliases (same-name wrappers for inline-call migration) ─────────

pub(crate) fn intersection_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeListId> {
    tsz_solver::visitor::intersection_list_id(db, type_id)
}

pub(crate) fn tuple_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TupleListId> {
    tsz_solver::visitor::tuple_list_id(db, type_id)
}

pub(crate) fn unique_symbol_ref(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::visitor::unique_symbol_ref(db, type_id)
}

pub(crate) fn object_with_index_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::ObjectShapeId> {
    tsz_solver::visitor::object_with_index_shape_id(db, type_id)
}

pub(crate) fn no_infer_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::visitor::no_infer_inner_type(db, type_id)
}

/// Alias for `readonly_inner_type` — same semantics, consistent naming.
pub(crate) fn readonly_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::visitor::readonly_inner_type(db, type_id)
}

/// Alias for `type_query_symbol` — extracts the symbol ref from a `typeof T` type.
pub(crate) fn type_query_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::visitor::type_query_symbol(db, type_id)
}

pub(crate) fn remove_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::narrowing::remove_undefined(db, type_id)
}

pub(crate) fn remove_nullish(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::narrowing::remove_nullish(db, type_id)
}

pub(crate) fn function_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::FunctionShapeId> {
    tsz_solver::function_shape_id(db, type_id)
}

pub(crate) fn evaluate_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::computation::evaluate_type(db, type_id)
}

pub(crate) fn widen_type_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::widen_type_deep(db, type_id)
}

pub(crate) fn string_intrinsic_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(tsz_solver::StringIntrinsicKind, TypeId)> {
    tsz_solver::string_intrinsic_components(db, type_id)
}

pub(crate) fn instantiate_function_with_type_args(
    db: &dyn TypeDatabase,
    function_type: TypeId,
    type_args: &[TypeId],
) -> Option<TypeId> {
    c::instantiate_function_with_type_args(db, function_type, type_args)
}

pub(crate) fn normalize_object_union_members_for_write_target(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Vec<TypeId>> {
    tsz_solver::operations::normalize_object_union_members_for_write_target(db, members)
}

pub(crate) fn index_access_parts(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::index_access_parts(db, type_id)
}

pub(crate) fn split_nullish_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<TypeId>, Option<TypeId>) {
    tsz_solver::narrowing::split_nullish_type(db, type_id)
}

pub(crate) fn instantiate_type_preserving_meta(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    c::instantiate_type_preserving_meta_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        substitution,
    )
}

pub(crate) fn get_base_type_for_comparison(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::get_base_type_for_comparison(db, type_id)
}

pub(crate) fn apply_contextual_type(
    db: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    tsz_solver::computation::apply_contextual_type(db, expr_type, contextual_type)
}

pub(crate) fn resolve_default_type_args(
    db: &dyn TypeDatabase,
    type_params: &[tsz_solver::TypeParamInfo],
) -> Vec<TypeId> {
    tsz_solver::resolve_default_type_args(db, type_params)
}

pub(crate) use super::operator_wrappers::{
    is_assignment_operator, is_compound_assignment_operator,
    is_logical_compound_assignment_operator, map_compound_assignment_to_binary,
};

pub(crate) fn format_excess_property_name(name: &str) -> std::borrow::Cow<'_, str> {
    tsz_solver::format_excess_property_name(name)
}

pub(crate) fn classify_identity_mapped(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<tsz_solver::type_queries::mapped::IdentityMappedInfo> {
    tsz_solver::type_queries::classify_identity_mapped(db, mapped_id)
}
