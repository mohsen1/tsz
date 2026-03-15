use tsz_solver::TypeId;

/// Re-export of the solver's binary operation result type.
///
/// Wraps `tsz_solver::BinaryOpResult`.
/// This is the result enum returned by binary operation evaluation.
pub(crate) use tsz_solver::BinaryOpResult;

pub(crate) fn evaluate_contextual_structure_with(
    db: &dyn tsz_solver::QueryDatabase,
    type_id: TypeId,
    evaluate_leaf: &mut dyn FnMut(TypeId) -> TypeId,
) -> TypeId {
    tsz_solver::type_queries::evaluate_contextual_structure_with(db, type_id, evaluate_leaf)
}

pub(crate) fn evaluate_plus_chain(
    db: &dyn tsz_solver::QueryDatabase,
    operand_types: &[TypeId],
) -> Option<TypeId> {
    tsz_solver::BinaryOpEvaluator::new(db).evaluate_plus_chain(operand_types)
}
