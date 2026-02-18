use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn types_are_comparable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    tsz_solver::type_queries::types_are_comparable(db, source, target)
}

pub(crate) fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_like_type(db, type_id)
}
