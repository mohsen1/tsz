use tsz_solver::construction::TypeDatabase;
use tsz_solver::{CallSignature, FunctionShape, TypeId};

pub(crate) use super::super::common::{
    application_info, intersection_members, lazy_def_id, union_members,
};
pub(crate) use tsz_solver::type_queries::PromiseTypeKind;

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn function_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    tsz_solver::type_queries::classify_promise_type(db, type_id)
}

pub(crate) fn type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

pub(crate) fn promise_application_type(
    db: &dyn TypeDatabase,
    promise_base: TypeId,
    type_arg: TypeId,
) -> TypeId {
    db.application(promise_base, vec![type_arg])
}

pub(crate) fn await_contextual_operand_type(
    db: &dyn TypeDatabase,
    contextual: TypeId,
    promise_like: TypeId,
    promise: Option<TypeId>,
) -> TypeId {
    let mut members = vec![contextual, promise_like];
    if let Some(promise) = promise {
        members.push(promise);
    }
    db.union(members)
}

pub(crate) fn awaited_union_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn awaited_intersection_type(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.intersection(members)
}

pub(crate) fn thenable_callback_value_union(
    db: &dyn TypeDatabase,
    values: Vec<TypeId>,
) -> Option<TypeId> {
    match values.as_slice() {
        [] => None,
        [only] => Some(*only),
        _ => Some(db.union(values)),
    }
}

pub(crate) fn async_return_body_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}
