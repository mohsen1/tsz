use tsz_solver::{CallableShape, ObjectShapeId, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries_extended::{
    AbstractClassCheckKind, CallSignaturesKind, ClassDeclTypeKind, LazyTypeKind,
    StringLiteralKeyKind,
};

pub(crate) fn classify_for_abstract_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractClassCheckKind {
    tsz_solver::type_queries_extended::classify_for_abstract_check(db, type_id)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}

pub(crate) fn classify_for_lazy_resolution(db: &dyn TypeDatabase, type_id: TypeId) -> LazyTypeKind {
    tsz_solver::type_queries_extended::classify_for_lazy_resolution(db, type_id)
}

pub(crate) fn type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeParamInfo> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id)
}

pub(crate) fn classify_for_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> StringLiteralKeyKind {
    tsz_solver::type_queries_extended::classify_for_string_literal_keys(db, type_id)
}

pub(crate) fn string_literal_value(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_common::interner::Atom> {
    tsz_solver::type_queries_extended::get_string_literal_value(db, type_id)
}

pub(crate) fn classify_for_class_decl(db: &dyn TypeDatabase, type_id: TypeId) -> ClassDeclTypeKind {
    tsz_solver::type_queries_extended::classify_for_class_decl(db, type_id)
}

pub(crate) fn classify_for_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> CallSignaturesKind {
    tsz_solver::type_queries_extended::classify_for_call_signatures(db, type_id)
}

pub(crate) fn is_readonly_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_readonly_type(db, type_id)
}

pub(crate) fn object_shape_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<ObjectShapeId> {
    tsz_solver::type_queries::get_object_shape_id(db, type_id)
}
