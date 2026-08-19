//! Structural shape predicates exposed at the query boundary.
//!
//! Boolean classification queries over `TypeId` (`is_*` / `has_*` and
//! friends): each answers a single structural fact by delegating to the
//! owning solver query. Moved out of the broad `common` quarantine as an
//! #8225 paydown slice; `common` re-exports every name so call sites are
//! unchanged.

use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::relations::subtype::TypeResolver;

pub(crate) fn has_function_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter_like(db, type_id)
}

pub(crate) fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unique_symbol_type(db, type_id)
}

pub(crate) fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_keyof_type(db, type_id)
}

pub(crate) fn is_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, type_id)
}

pub(crate) fn is_type_deeply_any(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_deeply_any(db, type_id)
}

pub(crate) fn has_property_by_str(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    tsz_solver::type_queries::type_has_property_by_str(db, type_id, name)
}

pub(crate) fn has_nonpublic_property(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    tsz_solver::type_queries::has_nonpublic_property(db, type_id, name)
}

pub(crate) fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_type(db, type_id)
}

pub(crate) fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

/// Check if a type parameter has a constraint that contains a conditional type.
/// This is used to suppress false-positive TS2339 errors when accessing properties
/// on generic conditional types like `Parameters<T>["length"]` where the property
/// may exist on the resolved conditional type but we can't determine it until
/// the type parameter is instantiated.
pub(crate) fn type_parameter_has_conditional_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::type_parameter_has_conditional_constraint_db(db, type_id)
}

/// Check if a type parameter has a constraint that contains a generic mapped type.
/// This is used to suppress false-positive TS2339 errors when accessing properties
/// on type parameters with mapped type constraints like `T extends { [K in keyof U]: V }`
/// where U is another type parameter. The mapped type cannot be fully resolved until
/// U is instantiated.
pub(crate) fn type_parameter_has_mapped_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_parameter_has_mapped_constraint_db(db, type_id)
}

pub(crate) fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_mapped_type(db, type_id)
}

/// `true` when `type_id` is an anonymous object/object-with-index/mapped shape
/// — i.e. a structural object body whose apparent type is itself.
pub(crate) fn is_object_or_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_or_mapped_type(db, type_id)
}

/// Check if a type is a generic application type with type parameters in its arguments.
/// For example, `Options<State, Actions>` where `State` or `Actions` are type parameters.
pub(crate) fn is_generic_application_with_type_params(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if let Some(app) = tsz_solver::type_queries::get_type_application(db, type_id) {
        // Check if any type argument contains type parameters
        return app
            .args
            .iter()
            .any(|&arg| super::containment_queries::contains_type_parameters(db, arg));
    }
    false
}

/// Check whether an application's aliased body is a generic mapped type after
/// substituting the application's type arguments.
pub(crate) fn is_generic_mapped_application<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_generic_mapped_application_db(db, resolver, type_id)
}

/// Check if a type is a *generic* mapped type — one whose key constraint still
/// contains type parameters (e.g., `{ [K in keyof T]: ... }` where T is unresolved).
/// Mapped types with concrete key types (like `Partial<ConcreteType>`) return false
/// because they resolve to object types with statically known members.
/// This matches tsc's `isGenericMappedType`.
pub(crate) fn is_generic_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_mapped_type_db(db, type_id)
}

pub(crate) fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}

pub(crate) fn is_unit_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unit_type(db, type_id)
}

pub(crate) fn is_empty_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_empty_object_type(db, type_id)
}

/// True for the wide, non-nullable primitive intrinsics (`string`, `number`,
/// `boolean`, `bigint`, `symbol`) — the ones whose `T & {}` brand is identity.
pub(crate) fn is_widening_primitive_intrinsic(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_widening_primitive_intrinsic(db, type_id)
}

/// True when a type would render with a user-visible name (interface, class,
/// type alias, type parameter, application, lazy ref, intrinsic, etc.). False
/// for anonymous structural shapes like `{ p: number; q: string; }`. Used by
/// diagnostic display to decide whether to keep `keyof <name>` form or fall
/// back to the evaluated key union.
pub(crate) fn type_has_displayable_name(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(shape) = super::common::object_shape_for_type(db, type_id) {
        if shape.symbol.is_some() {
            return true;
        }
        return db.get_display_alias(type_id).is_some();
    }
    db.lookup(type_id).is_some()
}

pub(crate) fn type_id_is_known_to_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id.is_intrinsic() || type_id.is_error() || db.lookup(type_id).is_some()
}

pub(crate) fn is_symbol_or_unique_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_or_unique_symbol(db, type_id)
}

pub(crate) fn is_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_union_type(db, type_id)
}

/// Check if a type is callable (has call signatures or is a function).
pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

/// Check if a type is a type parameter at the top level, or an intersection
/// containing a type parameter member.
///
/// Used by generic call inference to decide whether excess property checking
/// should be skipped for a parameter position.
pub(crate) fn is_type_parameter_or_intersection_with_type_parameter(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_type_parameter_or_intersection_with_type_parameter(db, type_id)
}

/// Check if a type is a deferred (type-parameter-mentioning) indexed access,
/// or an intersection carrying one. Display-policy sibling of the query above.
pub(crate) fn is_deferred_indexed_access_or_intersection_with_one(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_deferred_indexed_access_or_intersection_with_one(db, type_id)
}

/// Check if a type represents an unresolved inference result (error, contains
/// infer types, or transitively references error).
pub(crate) fn is_unresolved_inference_result(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unresolved_inference_result(db, type_id)
}

/// Check whether a mapped type has a `readonly` modifier applied.
pub(crate) fn is_mapped_type_with_readonly_modifier(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::operations::property::is_mapped_type_with_readonly_modifier(db, type_id)
}

/// Check whether a tuple element at a fixed position is readonly.
pub(crate) fn is_readonly_tuple_fixed_element(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: &str,
) -> bool {
    tsz_solver::operations::property::is_readonly_tuple_fixed_element(db, type_id, prop_name)
}

/// Check if a type is a plain object type (properties only, no index signatures).
///
/// Returns `true` for `TypeData::Object` but not `TypeData::ObjectWithIndex`.
/// Used to choose between `factory.object()` and `factory.object_with_index()`.
pub(crate) fn is_plain_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::object_shape_id(db, type_id).is_some()
}

/// Check if a type is a generic type application.
pub(crate) fn is_generic_application(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::query::is_generic_application(db, type_id)
}

/// Check if a type is a template literal type.
pub(crate) fn is_template_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_template_literal_type(db, type_id)
}

/// Check if a type is a string intrinsic type (Uppercase, Lowercase, Capitalize, Uncapitalize).
pub(crate) fn is_string_intrinsic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::string_intrinsic_components(db, type_id).is_some()
}

/// Check whether a type is a "fresh" object literal type (for excess property checking).
pub(crate) fn is_fresh_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::relations::freshness::is_fresh_object_type(db, type_id)
}

/// Check if a type is constructor-like (has construct signatures or is a constructor function).
pub(crate) fn is_constructor_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::data::is_constructor_like_type(db, type_id)
}

pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
}

pub(crate) fn is_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_conditional_type(db, type_id)
}

pub(crate) fn is_evaluable_meta_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_conditional_type(db, type_id)
        || is_index_access_type(db, type_id)
        || is_keyof_type(db, type_id)
}

pub(crate) fn are_same_base_literal_kind(db: &dyn TypeDatabase, a: TypeId, b: TypeId) -> bool {
    tsz_solver::type_queries::are_same_base_literal_kind(db, a, b)
}

/// Check if a type is a valid mapped type key constraint (keyof, string, number,
/// symbol, union of these, or a type parameter with such a constraint).
pub(crate) fn is_valid_mapped_type_key_type(
    db: &dyn tsz_solver::construction::QueryDatabase,
    type_id: TypeId,
) -> bool {
    let evaluator = tsz_solver::operations::BinaryOpEvaluator::new(db);
    evaluator.is_valid_mapped_type_key_type(type_id)
}

/// Returns `true` if `type_id` is itself a literal/primitive, or a union or
/// intersection composed entirely of literal/primitive members.
///
/// Used for diagnostic display: when a generic type-alias application reduces
/// to such a "terminal" form (e.g. `KeysExtendedBy<M, number>` reducing to
/// `"b"`), tsc drops the alias name and shows the resolved literal/primitive
/// in error messages. Object/interface results keep the alias form.
pub(crate) fn is_literal_or_primitive_or_compound_of_those(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_literal_or_primitive_or_compound_of_those(db, type_id)
}

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_array_type(db, type_id)
}

pub(crate) fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_tuple_type(db, type_id)
}

pub(crate) fn is_intersection_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_intersection_type(db, type_id)
}

pub(crate) fn is_merged_intersection_object(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_merged_intersection_object(db, type_id)
}

pub(crate) fn has_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_call_signatures(db, type_id)
}

pub(crate) fn is_type_query_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_type_query_type(db, type_id)
}

pub(crate) fn is_only_null_or_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_only_null_or_undefined(db, type_id)
}

pub(crate) fn is_array_or_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_or_tuple_type(db, type_id)
}

pub(crate) fn is_bare_infer_placeholder(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_bare_infer_placeholder_db(db, type_id)
}

pub(crate) fn is_boolean_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_boolean_type(db, type_id)
}

pub(crate) fn is_bigint_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_bigint_type(db, type_id)
}

pub(crate) fn is_homomorphic_mapped_type_context(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_homomorphic_mapped_type_context(db, type_id)
}

pub(crate) fn is_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_literal_type(db, type_id)
}

pub(crate) fn is_number_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_number_literal(db, type_id)
}

pub(crate) fn is_number_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_number_type(db, type_id)
}

pub(crate) fn is_spread_marker_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_spread_marker_tuple(db, type_id)
}

pub(crate) fn is_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_type(db, type_id)
}

pub(crate) fn is_tuple_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_tuple_like_type(db, type_id)
}

pub(crate) fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}

pub(crate) fn numeric_literal_index_valid_for_object(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    object_type: TypeId,
) -> bool {
    tsz_solver::type_queries::numeric_literal_index_valid_for_object(db, index_type, object_type)
}

pub(crate) fn type_has_readonly_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_has_readonly_members(db, type_id)
}

pub(crate) fn union_contains_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::union_contains_tuple(db, type_id)
}

pub(crate) fn is_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_literal(db, type_id)
}

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_object_like_type(db, type_id)
}

pub(crate) fn is_enum_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_enum_type(db, type_id)
}

pub(crate) fn is_lazy_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_lazy_type(db, type_id)
}

pub(crate) fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_primitive_type(db, type_id)
}

pub(crate) fn is_literal_type_through_type_constraints(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::visitor::is_literal_type_through_type_constraints(db, type_id)
}

pub(crate) fn has_late_bound_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::has_late_bound_members(db, type_id)
}

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_function_type(db, type_id)
}

pub(crate) fn is_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_error_type(db, type_id)
}

/// Like [`is_error_type`] but does NOT treat a display-preserving
/// `UnresolvedTypeName` as an error.
///
/// Used where a *genuine* internal `error` sentinel (the cycle/fuel sentinel
/// `TypeId::ERROR` / `TypeData::Error`) must be distinguished from a deferrable
/// cross-file reference that simply has not been bound yet.
pub(crate) fn is_genuine_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_genuine_error_type(db, type_id)
}

pub(crate) fn is_module_namespace_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_module_namespace_type(db, type_id)
}

pub(crate) fn is_nullish_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::narrowing::is_nullish_type(db, type_id)
}

pub(crate) fn is_structurally_deferred_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_structurally_deferred_type(db, type_id)
}

pub(crate) fn is_distributive_conditional_with_deferred_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_distributive_conditional_with_deferred_check(db, type_id)
}

pub(crate) fn mapped_type_is_deferred_generic(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::mapped_type_is_deferred_generic(db, type_id)
}

pub(crate) fn is_definitely_nullish(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::narrowing::is_definitely_nullish(db, type_id)
}
