use tsz_solver::{ObjectShape, QueryDatabase, SubtypeFailureReason, TypeDatabase, TypeId};

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
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
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

pub(crate) fn is_assignable_with_overrides<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
) -> bool {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
    } = *inputs;
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_overrides(tsz_solver::RelationQueryInputs {
        interner: db.as_type_database(),
        resolver,
        source,
        target,
        kind: tsz_solver::RelationKind::Assignable,
        policy,
        context,
        overrides,
    })
    .is_related()
}

#[derive(Clone, Copy)]
pub(crate) struct AssignabilityQueryInputs<'a, R: tsz_solver::TypeResolver> {
    pub db: &'a dyn QueryDatabase,
    pub resolver: &'a R,
    pub source: TypeId,
    pub target: TypeId,
    pub flags: u16,
    pub inheritance_graph: &'a tsz_solver::InheritanceGraph,
    pub sound_mode: bool,
}

pub(crate) fn is_assignable_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        policy,
        context,
    )
    .is_related()
}

pub(crate) fn is_assignable_bivariant_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::AssignableBivariantCallbacks,
        policy,
        context,
    )
    .is_related()
}

pub(crate) fn is_subtype_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    class_check: Option<&dyn Fn(tsz_solver::SymbolRef) -> bool>,
) -> tsz_solver::RelationResult {
    let policy = tsz_solver::RelationPolicy::from_flags(flags);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::Subtype,
        policy,
        context,
    )
}

pub(crate) fn is_redeclaration_identical_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
    flags: u16,
    inheritance_graph: &tsz_solver::InheritanceGraph,
    sound_mode: bool,
) -> bool {
    let policy = tsz_solver::RelationPolicy::from_flags(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::RelationContext {
        query_db: Some(db),
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::query_relation_with_resolver(
        db,
        resolver,
        source,
        target,
        tsz_solver::RelationKind::RedeclarationIdentical,
        policy,
        context,
    )
    .is_related()
}

pub(crate) struct AssignabilityFailureAnalysis {
    pub weak_union_violation: bool,
    pub failure_reason: Option<SubtypeFailureReason>,
}

pub(crate) struct AssignabilityGateResult {
    pub related: bool,
    pub analysis: Option<AssignabilityFailureAnalysis>,
}

pub(crate) fn check_assignable_gate_with_overrides<R: tsz_solver::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::AssignabilityOverrideProvider,
    ctx: Option<&crate::context::CheckerContext<'_>>,
    collect_failure_analysis: bool,
) -> AssignabilityGateResult {
    let related = is_assignable_with_overrides(inputs, overrides);

    if !collect_failure_analysis || related {
        return AssignabilityGateResult {
            related,
            analysis: None,
        };
    }

    let analysis = ctx.map(|ctx| {
        analyze_assignability_failure_with_context(
            inputs.db.as_type_database(),
            ctx,
            inputs.resolver,
            inputs.source,
            inputs.target,
        )
    });

    AssignabilityGateResult { related, analysis }
}

pub(crate) fn analyze_assignability_failure_with_context<R: tsz_solver::TypeResolver>(
    db: &dyn TypeDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> AssignabilityFailureAnalysis {
    let mut checker = tsz_solver::CompatChecker::with_resolver(db, resolver);
    ctx.configure_compat_checker(&mut checker);
    AssignabilityFailureAnalysis {
        weak_union_violation: checker.is_weak_union_violation(source, target),
        failure_reason: checker.explain_failure(source, target),
    }
}
