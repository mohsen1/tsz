use tsz_solver::{ObjectShape, SubtypeFailureReason, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::{AssignabilityEvalKind, ExcessPropertiesKind};

pub(crate) fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    tsz_solver::type_queries::classify_for_assignability_eval(db, type_id)
}

pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    tsz_solver::type_queries::classify_for_excess_properties(db, type_id)
}

pub(crate) fn are_types_overlapping_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags: u16 = 0;
    if strict_null_checks {
        flags |= tsz_solver::types::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    let policy = tsz_solver::RelationPolicy::from_flags(flags);
    tsz_solver::query_relation_with_resolver(
        db,
        env,
        left,
        right,
        tsz_solver::RelationKind::Overlap,
        policy,
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

pub(crate) struct AssignabilityFailureAnalysis {
    pub weak_union_violation: bool,
    pub failure_reason: Option<SubtypeFailureReason>,
}

pub(crate) fn analyze_assignability_failure_with_context(
    db: &dyn TypeDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
) -> AssignabilityFailureAnalysis {
    let mut checker = tsz_solver::CompatChecker::with_resolver(db, env);
    ctx.configure_compat_checker(&mut checker);
    AssignabilityFailureAnalysis {
        weak_union_violation: checker.is_weak_union_violation(source, target),
        failure_reason: checker.explain_failure(source, target),
    }
}
