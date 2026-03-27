use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::super::common::{callable_shape_for_type, contains_type_parameters};

/// Check if a type is a bare type parameter (`TypeParameter` or `Infer`).
pub(crate) fn is_bare_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_type_parameter(db, type_id)
}

/// Get the base constraint of a type for TS2344 checking.
///
/// For `TypeParameter` with constraint: returns the constraint.
/// For `TypeParameter` without constraint: returns `UNKNOWN`.
/// For all other types (including `Infer`): returns the type unchanged.
pub(crate) fn base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::get_base_constraint_of_type(db, type_id)
}

/// Get the object and index types of an `IndexAccess` type.
pub(crate) fn index_access_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

/// Get the extends type and false type of a conditional type.
///
/// Returns `Some((extends_type, false_type))` if the type is a `Conditional`.
/// Used for TS2344 constraint checking: for `Extract<T, C>` (i.e., `T extends C ? T : never`),
/// the result is always a subtype of `C`, so if `C` satisfies the required constraint,
/// the TS2344 check should be skipped.
pub(crate) fn conditional_type_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    let cond_id = tsz_solver::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.get_conditional(cond_id);
    Some((cond.extends_type, cond.false_type))
}

/// Get all four components of a conditional type: check, extends, true, false.
///
/// Returns `Some((check_type, extends_type, true_type, false_type))` if the
/// type is a `Conditional`. Used for distinguishing true Extract patterns
/// (`T extends C ? T : never` where true_type == check_type) from general
/// conditional types with custom true branches.
pub(crate) fn full_conditional_type_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId, TypeId, TypeId)> {
    let cond_id = tsz_solver::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.get_conditional(cond_id);
    Some((
        cond.check_type,
        cond.extends_type,
        cond.true_type,
        cond.false_type,
    ))
}

// =========================================================================
// Type query wrappers — callable/this/primitive/union classification
// =========================================================================

/// Check if a type is callable (Function or Callable shape).
pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

/// Check if a type is a `this` type (visitor-based, handles Lazy indirection).
pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_this_type(db, type_id)
}

/// Check if a type is a primitive (string, number, boolean, bigint, etc.).
pub(crate) fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_primitive_type(db, type_id)
}

/// Check if a type is a union and return whether it has members.
pub(crate) fn has_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_union_members(db, type_id).is_some()
}

// =========================================================================
// Type parameter extraction for call validation
// =========================================================================

/// Extract type parameters from a callable type, selecting the overload
/// whose arity matches the provided type argument count.
///
/// Returns `None` when multiple overloads match or the type is not callable.
pub(crate) fn extract_type_params_for_call(
    db: &dyn TypeDatabase,
    callee_type: TypeId,
    type_arg_count: usize,
) -> Option<Vec<tsz_solver::TypeParamInfo>> {
    tsz_solver::type_queries::data::extract_type_params_for_call(db, callee_type, type_arg_count)
}

/// For callable types with overloads, returns the distinct type-parameter counts
/// accepted by different overloads. Used to emit TS2743 instead of TS2558.
pub(crate) fn overload_type_param_counts(
    db: &dyn TypeDatabase,
    callee_type: TypeId,
) -> Option<Vec<usize>> {
    tsz_solver::type_queries::data::overload_type_param_counts(db, callee_type)
}

// =========================================================================
// Index-key classification
// =========================================================================

/// Re-export `IndexKeyKind` so generic_checker doesn't import solver directly.
pub(crate) use tsz_solver::type_queries::IndexKeyKind;

/// Classify a type for index-key matching (string, number, literal, union, etc.).
pub(crate) fn classify_index_key(db: &dyn TypeDatabase, key_type: TypeId) -> IndexKeyKind {
    tsz_solver::type_queries::classify_index_key(db, key_type)
}

/// Check if a key type (and its `IndexKeyKind`) matches a string index signature.
///
/// Delegates to the solver's canonical implementation.
pub(crate) fn key_matches_string_index(
    db: &dyn TypeDatabase,
    key_type: TypeId,
    kind: &IndexKeyKind,
) -> bool {
    tsz_solver::type_queries::key_matches_string_index(db, key_type, kind)
}

/// Check if a key type (and its `IndexKeyKind`) matches a number index signature.
///
/// Delegates to the solver's canonical implementation.
pub(crate) fn key_matches_number_index(
    db: &dyn TypeDatabase,
    key_type: TypeId,
    kind: &IndexKeyKind,
) -> bool {
    tsz_solver::type_queries::key_matches_number_index(db, key_type, kind)
}

// =========================================================================
// Lazy/Application info extraction
// =========================================================================

/// Get the `DefId` of a `Lazy` type (visitor-based, handles indirection).
pub(crate) fn lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::visitor::lazy_def_id(db, type_id)
}

/// Check if a type is the boxed `Function` intrinsic by direct TypeId match.
pub(crate) fn is_boxed_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    db.get_boxed_type(tsz_solver::IntrinsicKind::Function)
        .is_some_and(|boxed_id| type_id == boxed_id)
}

/// Check if a `Lazy(DefId)` type corresponds to the boxed `Function` intrinsic.
pub(crate) fn is_boxed_function_def(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(def_id) = tsz_solver::visitor::lazy_def_id(db, type_id) {
        db.is_boxed_def_id(def_id, tsz_solver::IntrinsicKind::Function)
    } else {
        false
    }
}

/// Get the base `DefId` from an `Application` type's base type.
///
/// For `Application { base: Lazy(DefId), args: ... }`, returns the base DefId.
/// Used for coinductive heritage checking.
pub(crate) fn application_base_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    let app_id = tsz_solver::visitor::application_id(db, type_id)?;
    let app = db.type_application(app_id);
    tsz_solver::visitor::lazy_def_id(db, app.base)
}

/// Get the base `DefId` and type arguments from an `Application` type.
///
/// Returns `(Option<base_def_id>, args)` for coinductive/heritage checks.
pub(crate) fn application_base_def_and_args(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(Option<tsz_solver::def::DefId>, Vec<TypeId>)> {
    let app_id = tsz_solver::visitor::application_id(db, type_id)?;
    let app = db.type_application(app_id);
    let base_def = tsz_solver::visitor::lazy_def_id(db, app.base);
    Some((base_def, app.args.clone()))
}

// =========================================================================
// Array-like structural surface helpers
// =========================================================================

/// Re-export `ArrayLikeKind` so generic_checker doesn't import solver directly.
pub(crate) use tsz_solver::type_queries::ArrayLikeKind;

/// Classify whether a type is an array, tuple, or readonly array.
pub(crate) fn classify_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> ArrayLikeKind {
    tsz_solver::type_queries::classify_array_like(db, type_id)
}

/// Get the object shape of a type (for structural surface checks).
pub(crate) fn get_object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

/// Check if an object type has a named property.
pub(crate) fn has_property_by_name(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    tsz_solver::type_queries::find_property_in_object_by_str(db, type_id, name).is_some()
}

/// Extract the `MappedTypeId` if this type is a mapped type.
///
/// Used for TS2344 constraint checking: when the object part of an indexed
/// access resolves to a mapped type, the template type gives the value type
/// of the indexing operation (e.g., `{ [K in keyof T]: () => unknown }[M]`
/// yields `() => unknown`).
pub(crate) fn mapped_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::MappedTypeId> {
    tsz_solver::mapped_type_id(db, type_id)
}

/// Extract the template TypeId of a mapped type.
///
/// For `{ [K in keyof T]: SomeTemplate }`, returns `SomeTemplate`.
/// Used for TS2344 constraint checking where indexed access into
/// a mapped type yields the template type.
pub(crate) fn mapped_type_template(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let mapped_id = tsz_solver::mapped_type_id(db, type_id)?;
    Some(db.mapped_type(mapped_id).template)
}

/// Check if a mapped type's template is callable (has call/construct signatures).
///
/// Used for TS2344 constraint checking: when an indexed access into a mapped
/// type is checked against a callable constraint, the template type determines
/// whether the indexed value is callable.
pub(crate) fn is_mapped_template_callable(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> bool {
    tsz_solver::type_queries::is_mapped_template_callable(db, mapped_id)
}

/// Extract string/number index signature value types from a type.
///
/// Used for TS2344 constraint checking: when an indexed access into a type
/// parameter is checked against a callable constraint, a callable index
/// signature on the type parameter's constraint means the indexed value is
/// callable even if the constraint is not expressed as a mapped type.
pub(crate) fn index_signature_value_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> [Option<TypeId>; 2] {
    let Some(shape) = tsz_solver::type_queries::get_object_shape(db, type_id) else {
        return [None, None];
    };
    [
        shape.string_index.as_ref().map(|sig| sig.value_type),
        shape.number_index.as_ref().map(|sig| sig.value_type),
    ]
}

/// Check if a type is a generic type application (`TypeData::Application`).
///
/// Used for TS2344 constraint checking: Application types containing type
/// parameters (e.g., `Merge2<X>` where X has free type params) should defer
/// constraint checks to instantiation time, since the mapped type semantics
/// may differ from eagerly-resolved base constraints.
pub(crate) fn is_application_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}
