use tsz_solver::construction::TypeDatabase;
use tsz_solver::{CallSignature, TypeId};

pub(crate) fn construct_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    if let Some(signatures) = tsz_solver::type_queries::get_construct_signatures(db, type_id) {
        return Some(signatures);
    }
    let shape = tsz_solver::type_queries::get_function_shape(db, type_id)?;
    if !shape.is_constructor {
        return None;
    }
    Some(vec![CallSignature {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: shape.return_type,
        type_predicate: shape.type_predicate,
        is_method: shape.is_method,
    }])
}
