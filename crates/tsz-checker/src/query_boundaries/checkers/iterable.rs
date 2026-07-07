use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{FunctionShape, TypeId};

pub(crate) use super::super::common::{
    call_signatures_for_type, is_string_type, is_this_type, union_members as union_members_for_type,
};
pub(crate) use tsz_solver::type_queries::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind,
};

pub(crate) fn classify_full_iterable_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> FullIterableTypeKind {
    tsz_solver::type_queries::classify_full_iterable_type(db, type_id)
}

pub(crate) fn classify_async_iterable_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AsyncIterableTypeKind {
    tsz_solver::type_queries::classify_async_iterable_type(db, type_id)
}

pub(crate) fn async_iterable_protocol_lookup_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::async_iterable_protocol_lookup_type(db, type_id)
}

pub(crate) fn classify_for_of_element_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ForOfElementKind {
    tsz_solver::type_queries::classify_for_of_element_type(db, type_id)
}

pub(crate) fn function_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn iterator_info_yield_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    is_async: bool,
) -> Option<TypeId> {
    tsz_solver::operations::get_iterator_info(db, type_id, is_async).map(|info| info.yield_type)
}

pub(crate) fn iterator_result_value_types(
    db: &dyn QueryDatabase,
    result_type: TypeId,
) -> (TypeId, TypeId) {
    tsz_solver::operations::extract_iterator_result_value_types(db, result_type)
}

pub(crate) fn tuple_element_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::union_or_single(db, members)
}

pub(crate) fn union_element_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn intersection_element_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_type(db, type_id)
}

pub(crate) fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_tuple_type(db, type_id)
}

pub(crate) fn is_string_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        tsz_solver::type_queries::classify_for_literal_value(db, type_id),
        tsz_solver::type_queries::LiteralValueKind::String(_)
    )
}
