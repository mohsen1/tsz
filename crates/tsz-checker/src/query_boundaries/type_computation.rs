use tsz_solver::TypeId;

pub(crate) fn evaluate_contextual_structure_with(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    tsz_solver::type_queries::evaluate_contextual_structure_with(db, type_id, evaluate_leaf)
}
