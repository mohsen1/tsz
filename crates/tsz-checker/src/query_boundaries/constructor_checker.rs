use tsz_solver::{CallSignature, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries_classifiers::ConstructorAccessKind;
pub(crate) use tsz_solver::type_queries_extended::{
    AbstractConstructorAnchor, ConstructorReturnMergeKind, InstanceTypeKind,
};

pub(crate) fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

pub(crate) fn construct_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_construct_signatures(db, type_id)
}

pub(crate) fn classify_for_instance_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> InstanceTypeKind {
    tsz_solver::type_queries::classify_for_instance_type(db, type_id)
}

pub(crate) fn classify_for_constructor_return_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorReturnMergeKind {
    tsz_solver::type_queries::classify_for_constructor_return_merge(db, type_id)
}

pub(crate) fn resolve_abstract_constructor_anchor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorAnchor {
    tsz_solver::type_queries_extended::resolve_abstract_constructor_anchor(db, type_id)
}

pub(crate) fn classify_for_constructor_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorAccessKind {
    tsz_solver::type_queries::classify_for_constructor_access(db, type_id)
}
