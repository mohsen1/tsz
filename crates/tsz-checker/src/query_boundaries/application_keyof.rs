use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::{CallSignature, TypeId, TypeParamInfo};

pub(crate) fn application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    super::common::application_info(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    super::common::call_signatures_for_type(db, type_id)
}

pub(crate) fn object_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> Option<SymbolId> {
    super::common::object_symbol(db, type_id)
}

pub(crate) fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::is_type_parameter_like(db, type_id)
}

pub(crate) fn lazy_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<DefId> {
    super::common::lazy_def_id(db, type_id)
}

pub(crate) fn type_param_info(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeParamInfo> {
    super::common::type_param_info(db, type_id)
}

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    super::common::union_members(db, type_id)
}

pub(crate) fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    super::common::keyof_inner_type(db, type_id)
}

pub(crate) fn string_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    super::common::string_literal_value(db, type_id)
}
