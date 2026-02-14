use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

pub(crate) use tsz_solver::type_queries::ConstructorCheckKind;

pub(crate) fn is_direct_class_lazy_reference(checker: &CheckerState<'_>, type_id: TypeId) -> bool {
    let Some(def_id) = tsz_solver::type_queries::get_lazy_def_id(checker.ctx.types, type_id) else {
        return false;
    };
    let Some(sym_id) = checker.ctx.def_to_symbol.borrow().get(&def_id).copied() else {
        return false;
    };
    let Some(symbol) = checker.ctx.binder.get_symbol(sym_id) else {
        return false;
    };
    symbol.flags & tsz_binder::symbol_flags::CLASS != 0
}

pub(crate) fn first_construct_signature_return_type(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_construct_signatures(db, type_id)
        .and_then(|signatures| signatures.first().map(|sig| sig.return_type))
}

pub(crate) fn union_members(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn has_construct_signatures(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

pub(crate) fn classify_for_constructor_check(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    tsz_solver::type_queries::classify_for_constructor_check(db, type_id)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn should_report_accessor_mismatch(
    checker: &mut CheckerState<'_>,
    getter_type: TypeId,
    setter_type: TypeId,
    error_pos: NodeIndex,
) -> bool {
    checker.should_report_assignability_mismatch(getter_type, setter_type, error_pos)
}
