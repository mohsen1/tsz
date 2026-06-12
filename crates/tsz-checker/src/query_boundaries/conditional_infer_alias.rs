use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::relations::subtype::TypeResolver;

pub(crate) fn application_base_def_id<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> Option<DefId> {
    tsz_solver::type_queries::conditional_infer_alias::application_base_def_id(db, resolver, base)
}

pub(crate) fn application_base_is_raw_conditional_alias<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> bool {
    tsz_solver::type_queries::conditional_infer_alias::application_base_is_raw_conditional_alias(
        db, resolver, base,
    )
}

pub(crate) fn application_base_uses_conditional_infer<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> bool {
    tsz_solver::type_queries::conditional_infer_alias::application_base_uses_conditional_infer(
        db, resolver, base,
    )
}
