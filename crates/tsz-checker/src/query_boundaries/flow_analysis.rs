use tsz_solver::{QueryDatabase, TypeDatabase, TypeId};

pub(crate) use super::common::{
    LiteralValueKind, PredicateSignatureKind, array_element_type as get_array_element_type,
    call_signatures_for_type, classify_for_literal_value, classify_for_predicate_signature,
    contains_type_parameters, is_keyof_type, is_narrowing_literal, is_type_parameter_like,
    is_unit_type, stringify_literal_type, tuple_elements as tuple_elements_for_type,
    union_members as union_members_for_type,
};

pub(crate) fn union_types(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::union_or_single(db, members)
}

pub(crate) fn enum_member_domain(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::visitor::enum_components(db, type_id)
        .map(|(_def_id, members)| members)
        .unwrap_or(type_id)
}

pub(crate) fn are_types_mutually_subtype(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    tsz_solver::is_subtype_of(db, left, right) || tsz_solver::is_subtype_of(db, right, left)
}

pub(crate) fn is_assignable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    tsz_solver::query_relation(
        db,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::default(),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

pub(crate) fn is_assignable_strict_null(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::query_relation(
        db,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::from_flags(
            tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
        ),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

pub(crate) const fn is_compound_assignment_operator(operator_token: u16) -> bool {
    tsz_solver::is_compound_assignment_operator(operator_token)
}

pub(crate) const fn map_compound_assignment_to_binary(operator_token: u16) -> Option<&'static str> {
    tsz_solver::map_compound_assignment_to_binary(operator_token)
}

pub(crate) fn fallback_compound_assignment_result(
    db: &dyn TypeDatabase,
    operator_token: u16,
    rhs_literal_type: Option<TypeId>,
) -> Option<TypeId> {
    tsz_solver::fallback_compound_assignment_result(db, operator_token, rhs_literal_type)
}

pub(crate) fn widen_literal_to_primitive(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::widen_literal_to_primitive(db, type_id)
}

pub(crate) fn function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_return_type(db, type_id)
}

pub(crate) fn instance_type_from_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::instance_type_from_constructor(db, type_id)
}

pub(crate) fn is_promise_like_type(db: &dyn QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_promise_like(db, type_id)
}

pub(crate) fn are_types_mutually_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    types_are_subtype_with_env(db, env, left, right, strict_null_checks)
        || types_are_subtype_with_env(db, env, right, left, strict_null_checks)
}

pub(crate) fn is_assignable_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::from_flags(flags),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

/// Extract the `DefId` from a `Lazy(DefId)` type, if it is one.
/// Used by flow-control assignment to resolve lazy types via the `TypeEnvironment`.
pub(crate) fn get_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}

pub(crate) fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

/// If `type_id` is a promise-like application type, return the inner type argument.
/// Used by flow-control assignment to unwrap `await` RHS types.
pub(crate) fn unwrap_promise_type_argument(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if let Some((base, args)) = tsz_solver::type_queries::get_application_info(db, type_id)
        && (base == TypeId::PROMISE_BASE || tsz_solver::type_queries::is_promise_like(db, type_id))
    {
        return args.first().copied();
    }
    None
}

pub(crate) use tsz_solver::type_queries::flow::ExtractedPredicateSignature;

/// Re-export for flow narrowing: extract the predicate signature from a callable type.
pub(crate) fn extract_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ExtractedPredicateSignature> {
    tsz_solver::type_queries::flow::extract_predicate_signature(db, type_id)
}

/// Check if a type is only `false` or `never` (used for assertion-function detection).
pub(crate) fn is_only_false_or_never(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_only_false_or_never(db, type_id)
}

/// Get type parameter info (constraint, default, name) for a type parameter.
pub(crate) fn type_param_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeParamInfo> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id)
}

/// Evaluate an application type via the solver's `ApplicationEvaluator`.
///
/// This is the boundary entry point for flow-control code that needs to
/// evaluate generic application types (e.g., `Array<T>`) to their concrete
/// form. Callers should use this instead of constructing `ApplicationEvaluator`
/// directly.
pub(crate) fn evaluate_application_type(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::ApplicationEvaluator::new(db, env).evaluate_or_original(type_id)
}

fn types_are_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::RelationKind::Subtype,
        tsz_solver::RelationPolicy::from_flags(flags),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}
