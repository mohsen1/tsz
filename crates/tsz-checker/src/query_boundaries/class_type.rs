use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{
    callable_shape_for_type, construct_signatures_for_type, has_function_shape,
    intersection_members, is_generic_mapped_type, is_generic_type, object_shape_for_type,
};

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_includes_undefined(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

/// Check if `undefined` is potentially assignable to the given type.
///
/// This mirrors tsc's `isTypeAssignableTo(undefinedType, type)` for the purposes
/// of TS2564 checking. In particular:
/// - `undefined` is assignable to `any`, `unknown`, `void`, `undefined`
/// - `undefined` is assignable to unions containing `undefined`
///
/// TypeScript does NOT suppress TS2564 for naked type parameters, even when their
/// constraint is `any`, `unknown`, or includes `undefined`. Only the declared
/// property type itself matters here, not what a future instantiation might allow.
pub(crate) fn undefined_is_assignable_to(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY
        || type_id == TypeId::UNKNOWN
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
    {
        return true;
    }

    // Check if type directly includes undefined (e.g., string | undefined)
    if type_includes_undefined(db, type_id) {
        return true;
    }

    false
}
