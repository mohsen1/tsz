use std::sync::Arc;

use tsz_common::interner::Atom;
use tsz_solver::def::DefId;
use tsz_solver::def::resolver::TypeResolver;
use tsz_solver::type_handles::Variance;
use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn compute_variance_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
    target_param: Atom,
) -> Variance {
    tsz_solver::relations::variance::compute_variance_with_resolver(
        db,
        resolver,
        type_id,
        target_param,
    )
}

pub(crate) fn compute_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    tsz_solver::relations::variance::compute_type_param_variances_with_resolver(
        db, resolver, def_id,
    )
}
