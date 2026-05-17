use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn is_intersection_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_intersection_type(db, type_id)
}

/// Returns true when `type_id` is an intersection whose members contain no raw
/// primitive (`number`, `string`, `boolean`). Branded primitive intersections
/// (`number & { __brand: T }`) are excluded; they are handled separately.
pub(crate) fn is_non_primitive_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_intersection_type(db, type_id)
        && !tsz_solver::type_queries::get_intersection_members(db, type_id).is_some_and(|members| {
            members
                .iter()
                .any(|&m| m == TypeId::NUMBER || m == TypeId::STRING || m == TypeId::BOOLEAN)
        })
}
