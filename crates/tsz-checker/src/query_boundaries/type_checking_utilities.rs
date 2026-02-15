use tsz_solver::{TupleListId, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::{TypeParameterConstraintKind, UnionMembersKind};
pub(crate) use tsz_solver::type_queries_extended::{
    ArrayLikeKind, ElementIndexableKind, IndexKeyKind, LiteralKeyKind, TypeQueryKind,
};

pub(crate) fn widened_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries_extended::get_widened_literal_type(db, type_id)
}

pub(crate) fn tuple_list_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TupleListId> {
    tsz_solver::type_queries_extended::get_tuple_list_id(db, type_id)
}

pub(crate) fn application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries_extended::get_application_base(db, type_id)
}

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn literal_key_kind(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralKeyKind {
    tsz_solver::type_queries_extended::classify_literal_key(db, type_id)
}

pub(crate) fn classify_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> ArrayLikeKind {
    tsz_solver::type_queries_extended::classify_array_like(db, type_id)
}

pub(crate) fn unwrap_readonly_for_lookup(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries_extended::unwrap_readonly_for_lookup(db, type_id)
}

pub(crate) fn classify_index_key(db: &dyn TypeDatabase, type_id: TypeId) -> IndexKeyKind {
    tsz_solver::type_queries_extended::classify_index_key(db, type_id)
}

pub(crate) fn classify_element_indexable(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ElementIndexableKind {
    tsz_solver::type_queries_extended::classify_element_indexable(db, type_id)
}

pub(crate) fn classify_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> TypeQueryKind {
    tsz_solver::type_queries_extended::classify_type_query(db, type_id)
}

pub(crate) fn is_invalid_index_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries_extended::is_invalid_index_type(db, type_id)
}

pub(crate) fn classify_for_type_parameter_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeParameterConstraintKind {
    tsz_solver::type_queries::classify_for_type_parameter_constraint(db, type_id)
}

pub(crate) fn classify_for_union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> UnionMembersKind {
    tsz_solver::type_queries::classify_for_union_members(db, type_id)
}
