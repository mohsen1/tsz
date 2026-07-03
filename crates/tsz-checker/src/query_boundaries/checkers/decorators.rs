use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{FunctionShape, ParamInfo, TypeId, TypeParamInfo};

pub(crate) fn decorator_global_type_ref(db: &dyn TypeDatabase, def_id: DefId) -> TypeId {
    db.lazy(def_id)
}

pub(crate) fn class_accessor_decorator_target_any(
    db: &dyn TypeDatabase,
    target_def: DefId,
) -> TypeId {
    let base = db.lazy(target_def);
    db.application(base, vec![TypeId::ANY, TypeId::ANY])
}

pub(crate) fn decorator_context_application(
    db: &dyn TypeDatabase,
    context_def: DefId,
    args: Vec<TypeId>,
) -> TypeId {
    let base = db.lazy(context_def);
    db.application(base, args)
}

pub(crate) fn method_decorator_value_type(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
) -> TypeId {
    db.function(FunctionShape {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    })
}

pub(crate) fn accessor_decorator_value_type(
    db: &dyn TypeDatabase,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
) -> TypeId {
    db.function(FunctionShape {
        type_params: Vec::new(),
        params,
        this_type,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    })
}

pub(crate) fn decorator_void_or_replacement_type(
    db: &dyn TypeDatabase,
    replacement_type: TypeId,
) -> TypeId {
    db.union2(TypeId::VOID, replacement_type)
}
