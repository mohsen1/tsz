use tsz_solver::TypeId;

pub(crate) use super::common::{
    callable_shape_for_type, has_construct_signatures, is_symbol_or_unique_symbol, union_members,
};
pub(crate) use tsz_solver::type_queries::ConstructorCheckKind;

pub(crate) fn classify_for_constructor_check(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    tsz_solver::type_queries::classify_for_constructor_check(db, type_id)
}

pub(crate) fn has_function_shape(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn is_constructor_function_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id)
        .is_some_and(|shape| shape.is_constructor)
}

/// True when `type_id` is a surface type constructor over the canonical
/// polymorphic `this`, such as `this[]`, `readonly this[]`, `this | undefined`,
/// or `Foo & this`.
///
/// This deliberately walks only constructor surfaces. It does not inspect object
/// members, lazy class/interface bodies, or type-parameter constraints, because
/// those can mention `this` without making the receiver itself a `this`-relative
/// wrapper.
pub(crate) fn is_compound_this_relative_surface_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> bool {
    tsz_solver::type_queries::is_compound_this_relative_surface_type(db, type_id, this_type)
}
