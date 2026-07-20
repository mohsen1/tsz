use std::sync::Arc;

use tsz_common::interner::Atom;
use tsz_solver::TypeId;
use tsz_solver::construction::QueryDatabase;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::DefId;
use tsz_solver::def::resolver::TypeResolver;

pub(crate) use tsz_solver::type_handles::Variance;

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

/// Session-cached computed-variance lookup for a generic `DefId`.
///
/// Routes through the session-level `variance_cache` exposed by the
/// `QueryDatabase` so repeated references to the same generic do not rebuild a
/// fresh `VarianceComputer` and re-walk the lazy type graph. The mask is
/// computed with the supplied resolver on a miss and stored for reuse. The
/// declared-variance mask for a `DefId` is session-stable, so this is
/// behavior-preserving relative to the uncached entry point.
pub(crate) fn compute_type_param_variances_with_resolver_cached(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    query_db: &dyn QueryDatabase,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    tsz_solver::relations::variance::compute_type_param_variances_with_resolver_cached(
        db,
        resolver,
        Some(query_db),
        def_id,
    )
}

pub(crate) fn compute_actual_type_param_variances_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    def_id: DefId,
) -> Option<Arc<[Variance]>> {
    tsz_solver::relations::variance::compute_actual_type_param_variances_with_resolver(
        db, resolver, def_id,
    )
}

/// Solver-owned strict variance query for polymorphic `this` in a class member.
pub(crate) fn contains_this_type_in_strict_contravariant_position_with_resolver(
    db: &dyn TypeDatabase,
    resolver: &dyn TypeResolver,
    type_id: TypeId,
) -> bool {
    tsz_solver::relations::variance::contains_this_type_in_strict_contravariant_position_with_resolver(
        db, resolver, type_id,
    )
}
