use tsz_solver::{CallSignature, FunctionShape, TypeDatabase, TypeId};

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

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn union_members_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_type(db, type_id)
}

pub(crate) fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_tuple_type(db, type_id)
}

pub(crate) fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_type(db, type_id)
}

pub(crate) fn is_string_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        tsz_solver::type_queries::classify_for_literal_value(db, type_id),
        tsz_solver::type_queries::LiteralValueKind::String(_)
    )
}
