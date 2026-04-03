//! Shared type query boundary functions used across multiple boundary modules.
//!
//! When a solver query is needed by multiple checker modules, define the
//! canonical thin-wrapper here and re-export it from the per-module boundary
//! files. This eliminates duplicate function bodies while preserving the
//! per-module namespace pattern that callers rely on.

use tsz_solver::{
    CallSignature, CallableShape, ObjectShape, TupleElement, TypeApplication, TypeDatabase, TypeId,
    TypePredicate,
};

// Re-export solver value types used by checker call computation.
pub(crate) use tsz_solver::ContextualTypeContext;
pub(crate) use tsz_solver::FunctionShape;
pub(crate) use tsz_solver::ObjectFlags;
pub(crate) use tsz_solver::ParamInfo;
#[allow(unused_imports)]
pub(crate) use tsz_solver::TypeInterner;

pub(crate) use tsz_solver::type_queries::TypeTraversalKind;

/// Re-export of the solver's property access result type.
///
/// Wraps `tsz_solver::operations::property::PropertyAccessResult`.
/// This is the result enum returned by property access evaluation in the solver.
pub(crate) use tsz_solver::operations::property::PropertyAccessResult;

/// Re-export of the solver's type substitution mapping.
///
/// Wraps `tsz_solver::TypeSubstitution`.
/// Used to build type parameter -> type argument mappings for instantiation.
pub(crate) use tsz_solver::TypeSubstitution;

/// Re-export of the solver's call resolution result type.
///
/// Wraps `tsz_solver::CallResult`.
/// This is the result enum returned by call/new expression resolution.
pub(crate) use tsz_solver::CallResult;

/// Thin wrapper around `tsz_solver::instantiate_type`.
///
/// Applies a `TypeSubstitution` to a type, producing a new type with type
/// parameters replaced by their corresponding type arguments.
pub(crate) fn instantiate_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &tsz_solver::TypeSubstitution,
) -> TypeId {
    tsz_solver::instantiate_type(db, type_id, substitution)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    tsz_solver::type_queries::classify_for_traversal(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
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

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

pub(crate) fn contains_lazy_or_recursive(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_lazy_or_recursive_db(db, type_id)
}

pub(crate) fn contains_application_in_structure(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_application_in_structure(db, type_id)
}

pub(crate) fn is_type_deeply_any(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_deeply_any(db, type_id)
}

pub(crate) fn has_property_by_str(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    tsz_solver::type_queries::type_has_property_by_str(db, type_id, name)
}

pub(crate) fn contains_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_error_type_db(db, type_id)
}

pub(crate) fn contains_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_never_type_db(db, type_id)
}

pub(crate) fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_type(db, type_id)
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

pub(crate) fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

pub(crate) fn type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_default(db, type_id)
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
    // Get the constraint of the type parameter
    if let Some(constraint) = tsz_solver::type_queries::get_type_parameter_constraint(db, type_id) {
        // Check if the constraint contains a conditional type
        return contains_conditional_type(db, constraint);
    }
    false
}

/// Recursively check if a type contains a conditional type.
fn contains_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if tsz_solver::type_queries::is_conditional_type(db, type_id) {
        return true;
    }

    // Check type application arguments
    if let Some(app) = tsz_solver::type_queries::get_type_application(db, type_id) {
        if app
            .args
            .iter()
            .any(|&arg| contains_conditional_type(db, arg))
        {
            return true;
        }
    }

    // Check intersection members
    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id) {
        if members
            .iter()
            .any(|&member| contains_conditional_type(db, member))
        {
            return true;
        }
    }

    // Check union members
    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) {
        if members
            .iter()
            .any(|&member| contains_conditional_type(db, member))
        {
            return true;
        }
    }

    // Check index access types
    if let Some((object_type, index_type)) =
        tsz_solver::type_queries::get_index_access_types(db, type_id)
    {
        return contains_conditional_type(db, object_type)
            || contains_conditional_type(db, index_type);
    }

    false
}

pub(crate) fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_mapped_type(db, type_id)
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
            .any(|&arg| contains_type_parameters(db, arg));
    }
    false
}

/// Check if a type is a *generic* mapped type — one whose key constraint still
/// contains type parameters (e.g., `{ [K in keyof T]: ... }` where T is unresolved).
/// Mapped types with concrete key types (like `Partial<ConcreteType>`) return false
/// because they resolve to object types with statically known members.
/// This matches tsc's `isGenericMappedType`.
pub(crate) fn is_generic_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(mapped) = tsz_solver::type_queries::get_mapped_type(db, type_id) {
        // Match tsc's isGenericMappedType: only check constraint and name_type.
        // The template always contains the mapped type's own iteration variable
        // which is NOT an external type parameter.
        tsz_solver::type_queries::contains_type_parameters_db(db, mapped.constraint)
            || mapped
                .name_type
                .is_some_and(|nt| tsz_solver::type_queries::contains_type_parameters_db(db, nt))
    } else {
        false
    }
}

pub(crate) fn construct_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_construct_signatures(db, type_id)
}

pub(crate) fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
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

pub(crate) fn intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) fn is_unit_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unit_type(db, type_id)
}

pub(crate) fn is_empty_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_empty_object_type(db, type_id)
}

pub(crate) fn is_symbol_or_unique_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_or_unique_symbol(db, type_id)
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

pub(crate) fn is_narrowing_literal(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::is_narrowing_literal(db, type_id)
}

pub(crate) fn stringify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    tsz_solver::type_queries::stringify_literal_type(db, type_id)
}

// ── Visitor wrappers ──

pub(crate) fn collect_referenced_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<TypeId> {
    tsz_solver::visitor::collect_referenced_types(db, type_id)
}

pub(crate) fn collect_enum_def_ids(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_solver::def::DefId> {
    tsz_solver::visitor::collect_enum_def_ids(db, type_id)
}

// ── Redeclaration widening helpers ──

/// Widen a literal return type in a function-shaped type for TS2403 comparison.
///
/// For `Function` types (e.g., `(s: string) => 3`), widens the return type
/// from a literal to its base (e.g., `3` → `number`). Returns the original
/// type unchanged if it is not a `Function` or no widening is needed.
///
/// This is a thin boundary wrapper that keeps direct `type_queries` and
/// `widen_literal_type` calls out of checker modules.
pub(crate) fn widen_function_literal_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id) else {
        return type_id;
    };
    let widened_return = tsz_solver::widen_literal_type(db, shape.return_type);
    if widened_return != shape.return_type {
        tsz_solver::type_queries::replace_function_return_type(db, type_id, widened_return)
    } else {
        type_id
    }
}

/// Widen literal return types in callable call-signatures for TS2403 comparison.
///
/// For `Callable` types (e.g., `{ (s: string): 3 }`), widens each call
/// signature's return type from a literal to its base (e.g., `3` → `number`).
/// Returns the original type unchanged if it is not a `Callable` or no
/// widening is needed.
///
/// This is a thin boundary wrapper that encapsulates solver `TypeData::Callable`
/// inspection so checker modules never touch `.lookup()` or `TypeData` directly.
pub(crate) fn widen_callable_literal_return_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    let Some(callable) = tsz_solver::type_queries::get_callable_shape(db, type_id) else {
        return type_id;
    };

    let mut any_changed = false;
    let new_call_sigs: Vec<_> = callable
        .call_signatures
        .iter()
        .map(|sig| {
            let widened = tsz_solver::widen_literal_type(db, sig.return_type);
            if widened != sig.return_type {
                any_changed = true;
                let mut new_sig = sig.clone();
                new_sig.return_type = widened;
                new_sig
            } else {
                sig.clone()
            }
        })
        .collect();

    if any_changed {
        let mut new_shape = (*callable).clone();
        new_shape.call_signatures = new_call_sigs;
        db.callable(new_shape)
    } else {
        type_id
    }
}

// ── Type construction wrappers ──

/// Create `type_id | undefined`. Used for optional chain call results.
pub(crate) fn union_with_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    db.union2(type_id, TypeId::UNDEFINED)
}

// ── Union / classifier wrappers ──

pub(crate) fn is_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_union_type(db, type_id)
}

pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

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
    tsz_solver::widen_type(db, type_id)
}

/// Widen a type for diagnostic display, preserving boolean literal intrinsics.
///
/// Like `widen_type` but keeps `true`/`false` literals so narrowed types
/// display correctly (e.g., `string | false` instead of `string | boolean`).
pub(crate) fn widen_type_for_display(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::widen_type_for_display(db, type_id)
}

/// Extract the element type from a rest-argument array/tuple type.
pub(crate) fn rest_argument_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::rest_argument_element_type(db, type_id)
}

/// Check if a type transitively references any type parameter whose name is in the given set.
pub(crate) fn references_any_type_param_named(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    names: &rustc_hash::FxHashSet<tsz_common::interner::Atom>,
) -> bool {
    tsz_solver::references_any_type_param_named(db, type_id, names)
}

/// Check if a type transitively contains a type parameter with the given name.
pub(crate) fn contains_type_parameter_named(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    tsz_solver::contains_type_parameter_named(db, type_id, name)
}

/// Check if a type transitively contains a specific `TypeId`.
pub(crate) fn contains_type_by_id(db: &dyn TypeDatabase, type_id: TypeId, target: TypeId) -> bool {
    tsz_solver::contains_type_by_id(db, type_id, target)
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

/// Check if a type represents an unresolved inference result (error, contains
/// infer types, or transitively references error).
pub(crate) fn is_unresolved_inference_result(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unresolved_inference_result(db, type_id)
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

/// Check if a type is a generic type application.
pub(crate) fn is_generic_application(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::query::is_generic_application(db, type_id)
}

// ── Literal/type extraction wrappers ──

pub(crate) use tsz_solver::LiteralValue;

/// Extract the literal value from a literal type.
pub(crate) fn literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<LiteralValue> {
    tsz_solver::literal_value(db, type_id)
}

/// Widen a literal type to its base type (e.g., `3` → `number`).
pub(crate) fn widen_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::widen_literal_type(db, type_id)
}

// ── Contextual/operation wrappers ──

/// Check whether a type is a "fresh" object literal type (for excess property checking).
pub(crate) fn is_fresh_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::relations::freshness::is_fresh_object_type(db, type_id)
}

/// Widen "fresh" object literal types to remove freshness tracking.
pub(crate) fn widen_freshness(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::relations::freshness::widen_freshness(db, type_id)
}

/// Re-export of the solver's iterator info type.
pub(crate) use tsz_solver::operations::iterators::IteratorInfo;

/// Get iterator/iterable info from a type.
pub(crate) fn get_iterator_info(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
    is_async: bool,
) -> Option<IteratorInfo> {
    tsz_solver::operations::get_iterator_info(db, type_id, is_async)
}

/// Collect all types recursively reachable from a root type.
pub(crate) fn collect_all_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<TypeId> {
    tsz_solver::visitor::collect_all_types(db, type_id)
}

// ── FunctionShape transformation helpers ──

/// Apply a `TypeSubstitution` to every type component in a `FunctionShape`.
///
/// Replaces type parameter references in parameter types, return type, this-type,
/// and type predicate type. Clears `type_params` since they are now resolved.
pub(crate) fn instantiate_function_shape(
    db: &dyn TypeDatabase,
    func: &FunctionShape,
    substitution: &tsz_solver::TypeSubstitution,
) -> FunctionShape {
    FunctionShape {
        params: func
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(db, param.type_id, substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect(),
        return_type: instantiate_type(db, func.return_type, substitution),
        this_type: func
            .this_type
            .map(|this_type| instantiate_type(db, this_type, substitution)),
        type_params: vec![],
        type_predicate: func.type_predicate.as_ref().map(|predicate| TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target,
            type_id: predicate
                .type_id
                .map(|tid| instantiate_type(db, tid, substitution)),
            parameter_index: predicate.parameter_index,
        }),
        is_constructor: func.is_constructor,
        is_method: func.is_method,
    }
}

/// Instantiate a generic function shape by substituting type parameters with
/// their defaults or constraints. Used for return-context matching where we
/// need a concrete shape but have no argument-driven substitution.
///
/// Returns the shape unchanged if it has no type parameters or no
/// defaults/constraints to apply.
pub(crate) fn instantiate_shape_to_defaults(
    db: &dyn TypeDatabase,
    func: &FunctionShape,
) -> FunctionShape {
    if func.type_params.is_empty() {
        return func.clone();
    }

    let mut substitution = tsz_solver::TypeSubstitution::new();
    for tp in &func.type_params {
        let Some(replacement) = tp.default.or(tp.constraint) else {
            continue;
        };
        substitution.insert(tp.name, replacement);
    }

    if substitution.is_empty() {
        return func.clone();
    }

    instantiate_function_shape(db, func, &substitution)
}

/// Replace parameter types at the given positions with a replacement type.
///
/// Used to sanitize binding-pattern parameters during generic inference:
/// destructured parameters contribute no inference candidates, so their
/// types are replaced with `unknown` to avoid polluting the constraint.
pub(crate) fn sanitize_params_at_positions(
    params: &[ParamInfo],
    positions: &[usize],
    replacement: TypeId,
) -> Vec<ParamInfo> {
    let mut result = params.to_vec();
    for &index in positions {
        if let Some(param) = result.get_mut(index) {
            param.type_id = replacement;
        }
    }
    result
}

/// Convert a slice of function parameters to tuple elements.
///
/// Each parameter's `type_id`, `optional`, `rest`, and `name` fields are
/// transferred directly.  Used when synthesizing a tuple type that mirrors
/// a parameter list (e.g. collecting remaining params for a rest argument).
pub(crate) fn params_to_tuple_elements(params: &[ParamInfo]) -> Vec<TupleElement> {
    params
        .iter()
        .map(|param| TupleElement {
            type_id: param.type_id,
            optional: param.optional,
            rest: param.rest,
            name: param.name,
        })
        .collect()
}

/// Sanitize binding-pattern parameters in a callable shape.
///
/// Like [`sanitize_params_at_positions`] but operates on a [`CallableShape`]:
/// each call signature's parameters at the given positions are replaced with
/// `replacement`.  Returns a new `CallableShape` ready for interning.
pub(crate) fn sanitize_callable_shape_binding_pattern_params(
    shape: &CallableShape,
    positions: &[usize],
    replacement: TypeId,
) -> CallableShape {
    let mut sanitized = shape.clone();
    sanitized.call_signatures = sanitized
        .call_signatures
        .iter()
        .map(|sig| {
            let mut new_sig = sig.clone();
            new_sig.params = sanitize_params_at_positions(&sig.params, positions, replacement);
            new_sig
        })
        .collect();
    sanitized
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

/// Check if a type is constructor-like (has construct signatures or is a constructor function).
pub(crate) fn is_constructor_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::data::is_constructor_like_type(db, type_id)
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
    db: &dyn tsz_solver::QueryDatabase,
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

pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
}

// ── Conditional type query ──

pub(crate) fn is_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_conditional_type(db, type_id)
}

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

pub(crate) fn type_contains_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_contains_string_literal(db, type_id)
}

// ── Number literal value extraction ──

pub(crate) fn number_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<f64> {
    tsz_solver::type_queries::get_number_literal_value(db, type_id)
}

// ── Same base literal kind comparison ──

pub(crate) fn are_same_base_literal_kind(db: &dyn TypeDatabase, a: TypeId, b: TypeId) -> bool {
    tsz_solver::type_queries::are_same_base_literal_kind(db, a, b)
}

// ── Contextual literal classification ──

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

/// Check if a type is a valid mapped type key constraint (keyof, string, number,
/// symbol, union of these, or a type parameter with such a constraint).
pub(crate) fn is_valid_mapped_type_key_type(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
) -> bool {
    let evaluator = tsz_solver::BinaryOpEvaluator::new(db);
    evaluator.is_valid_mapped_type_key_type(type_id)
}
