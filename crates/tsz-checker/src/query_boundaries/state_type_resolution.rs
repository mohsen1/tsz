use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::lazy_def_id as get_lazy_def_id;
pub(crate) use super::common::{callable_shape_for_type, is_type_parameter};
pub(crate) use tsz_solver::type_queries::{
    BaseInstanceMergeKind, ConstructorTypeKind, SignatureTypeKind, StaticPropertySource,
};

pub(crate) fn is_object_with_index_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_with_index_type(db, type_id)
}

pub(crate) fn classify_for_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> SignatureTypeKind {
    tsz_solver::type_queries::classify_for_signatures(db, type_id)
}

pub(crate) fn classify_constructor_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorTypeKind {
    tsz_solver::type_queries::classify_constructor_type(db, type_id)
}

pub(crate) fn static_property_source(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> StaticPropertySource {
    tsz_solver::type_queries::get_static_property_source(db, type_id)
}

pub(crate) fn classify_for_base_instance_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> BaseInstanceMergeKind {
    tsz_solver::type_queries::classify_for_base_instance_merge(db, type_id)
}

pub(crate) fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/state_type_resolution.rs"]
mod tests;
