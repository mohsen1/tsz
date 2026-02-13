use tsz_solver::TypeId;
use tsz_solver::type_queries::{EvaluationNeeded, classify_for_evaluation};

pub(crate) fn should_evaluate_contextual_declared_type(
    db: &dyn tsz_solver::TypeDatabase,
    declared_type: TypeId,
) -> bool {
    matches!(
        classify_for_evaluation(db, declared_type),
        EvaluationNeeded::Conditional { .. }
            | EvaluationNeeded::Mapped { .. }
            | EvaluationNeeded::IndexAccess { .. }
    )
}
