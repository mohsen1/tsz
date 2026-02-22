use tsz_solver::{MappedTypeId, TypeDatabase, TypeId};

pub(crate) use super::common::{
    is_generic_type, lazy_def_id, object_shape_for_type as object_shape,
};
pub(crate) use tsz_solver::type_queries::{
    MappedConstraintKind, PropertyAccessResolutionKind, TypeResolutionKind,
};

pub(crate) fn application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

pub(crate) fn mapped_type_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<MappedTypeId> {
    tsz_solver::type_queries::get_mapped_type_id(db, type_id)
}

pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

pub(crate) fn classify_mapped_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> MappedConstraintKind {
    tsz_solver::type_queries::classify_mapped_constraint(db, type_id)
}

pub(crate) fn classify_for_type_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeResolutionKind {
    tsz_solver::type_queries::classify_for_type_resolution(db, type_id)
}

pub(crate) fn classify_for_property_access_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessResolutionKind {
    tsz_solver::type_queries::classify_for_property_access_resolution(db, type_id)
}

pub(crate) fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ConditionalType>> {
    tsz_solver::type_queries::get_conditional_type(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/state_type_environment.rs"]
mod tests;
