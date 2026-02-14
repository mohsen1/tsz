use tsz_solver::{CallableShape, TypeDatabase, TypeId};

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/state_type_analysis.rs"]
mod tests;
