use tsz_solver::TypeId;

pub(crate) use super::common::{callable_shape_for_type, has_construct_signatures, union_members};
pub(crate) use tsz_solver::type_queries_extended::ConstructorCheckKind;

pub(crate) fn classify_for_constructor_check(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    tsz_solver::type_queries_extended::classify_for_constructor_check(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}
