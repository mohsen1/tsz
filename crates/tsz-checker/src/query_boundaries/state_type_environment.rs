use tsz_solver::{MappedTypeId, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries_extended::{
    MappedConstraintKind, PropertyAccessResolutionKind, TypeResolutionKind,
};

pub(crate) fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}

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

pub(crate) fn object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
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

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
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
