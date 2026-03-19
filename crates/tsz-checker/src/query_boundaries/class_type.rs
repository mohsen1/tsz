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

/// Check if a type is type-parameter-like, including `BoundParameter` (de Bruijn
/// indexed type parameters used inside generic class bodies).
pub(crate) fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter_like(db, type_id)
}

/// Check if `undefined` is potentially assignable to the given type.
///
/// This mirrors tsc's `isTypeAssignableTo(undefinedType, type)` for the purposes
/// of TS2564 checking. In particular:
/// - `undefined` is assignable to `any`, `unknown`, `void`, `undefined`
/// - `undefined` is assignable to unions containing `undefined`
/// - `undefined` is assignable to unconstrained type parameters (constraint is `unknown`)
/// - `undefined` is assignable to type parameters whose constraint allows `undefined`
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

    // For type parameters (including BoundParameter from de Bruijn indexed
    // type params in generic class bodies): check the constraint.
    // BoundParameter arises when e.g. `class C<T> { foo: T; }` — the type of
    // `foo` is a BoundParameter, not a TypeParameter, inside the class body.
    if is_type_parameter_like(db, type_id) {
        // Unconstrained type parameter has implicit constraint `unknown`,
        // and `undefined` is assignable to `unknown`.
        let constraint = type_parameter_constraint(db, type_id);
        return match constraint {
            None => true, // unconstrained → implicit `unknown` → undefined assignable
            Some(c) => undefined_is_assignable_to(db, c), // check constraint recursively
        };
    }

    false
}
